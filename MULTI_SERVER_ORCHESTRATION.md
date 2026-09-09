# Runory Multi-server Orchestration

> Status: Approved product direction; implementation starts with target resolution and exact
> session binding. This document defines the architecture boundary for the remaining work.
>
> Related baselines: `AGENT_RUNTIME_V2.md`, `AGENTIC.md`, `SECURITY.md`, `ROADMAP.md`.

## 1. Purpose

Runory needs a general multi-server coordination framework that can support workflows such as:

- database source/replica and clustered deployments;
- rolling application deployment and upgrade;
- load balancer plus backend configuration;
- distributed service installation and configuration;
- fleet-wide diagnosis, drift detection and remediation;
- backup, restore, migration and disaster-recovery exercises.

The framework must not hard-code MySQL or any other product. Product-specific knowledge belongs
in optional Operations Packs or Skills. The orchestration runtime supplies reusable target,
dependency, execution, approval, verification and recovery semantics.

“General” does not mean an unrestricted batch shell. A task is supported when it can be expressed
through one or both of these existing safe execution paths:

1. iterative, non-interactive Runtime V2 command proposals, each bound to one exact target and
   explicitly approved; or
2. Native Typed Tools and versioned ChangeSets, with Fleet ChangeSet coordination for writes.

Critical commands remain blocked. Interactive password prompts, hidden target expansion and
unattended destructive operations are outside the framework.

## 2. Non-negotiable invariants

1. Rust is the orchestration authority. React only resolves suggestions, renders events and sends
   user actions.
2. Every target is an exact `(profile_id, session_id)` binding. A profile name or group name is
   never an execution authority.
3. A live Fleet run uses already connected and host-verified `ServerSession` instances. It cannot
   silently reconnect with stored credentials.
4. A model cannot add targets. Target selection is supplied by the user and revalidated by Rust.
5. Changes to target, session, role, topology, stage graph, command, ChangeSet version, policy or
   precondition invalidate the affected approval.
6. Investigation may use the existing read-only Typed Tool path. Multi-target writes use Fleet
   ChangeSets; they cannot be smuggled through an unreviewed plan or generic IPC.
7. Production execution defaults to `Sequential + Pause for Review`. Production `Parallel All`
   is rejected by Rust.
8. A successful exit code is not completion. Every mutating workflow has target-local and, where
   applicable, cross-target or service-level verification.
9. Secrets never enter model context, events, logs, persisted Fleet metadata or configuration
   previews. Secret material flows only through the Vault and transient Rust-owned inputs.
10. Terminal output, remote files, logs, HTTP bodies and MCP output are untrusted data. Only
    bounded, redacted observations enter reasoning.
11. A rollback claim must correspond to a real rollback implementation. Otherwise the step is
    visibly marked non-reversible before approval.
12. Persisted Fleet state is content-free metadata. Restart invalidates approval and converts
    in-progress activity to `Interrupted`; execution never resumes automatically.

## 3. User experience

### 3.1 Target mentions

The composer supports explicit server and group references:

```text
@db-primary
@"Database Primary"
@group:production-db
@db-primary#source
@group:replicas#replica
```

Rules:

- a bare mention resolves only a server profile;
- `@group:` resolves only a saved group;
- quoted names support spaces;
- `#role` assigns a workflow role such as `source`, `replica`, `leader`, `worker` or `canary`;
- profile and group matching is exact and case-insensitive;
- ambiguous names, multiple live sessions for one profile and disconnected group members are
  blocking validation errors;
- group expansion is shown to the user before the run starts;
- the resolved target chips display server, role and connection status;
- the current Terminal server is not silently added when explicit mentions are present.

The frontend may suggest and parse names, but submits resolved identifiers. Rust independently
checks every session-to-profile relationship before creating a Fleet run.

### 3.2 Review experience

Before a multi-server write, the UI presents:

- exact targets and their roles;
- the stage/dependency graph;
- per-target ChangeSet version, risk and diff preview;
- execution strategy, batch/canary size and failure policy;
- target-local, cross-target and service-level verification contracts;
- rollback capability and non-reversible steps;
- policy and precondition status.

The Fleet Timeline is a projection of Rust events. It is not a second scheduler.

## 4. Architecture

```text
Composer mentions
      |
      v
Target resolver (UX) ---- catalog + connected tab metadata
      |
      v
Rust exact-target validation ---- ServerSessionManager
      |
      v
FleetCoordinator
      |---- FleetRun repository / checkpoints / audit metadata
      |---- Parent-child AgentRun ownership
      |---- Stage dependency scheduler
      |---- Fleet ChangeSet adapter
      `---- Verification aggregator
                 |
                 v
         Per-target Runtime V2
    Reason -> Action -> Approval -> Execute -> Observe
                 |
                 v
         ServerSessionManager / Typed Tools
```

The coordinator does not execute SSH itself. It schedules work against child runs that remain
bound to one `ServerSession`. This preserves the existing single-target command approval and PTY
execution invariants.

## 5. Domain model

### 5.1 Exact target

```text
FleetTargetBinding {
  profile_id
  session_id
  role?
  ordinal
}
```

`profile_id` identifies the inventory object; `session_id` identifies the live verified transport.
Both participate in approval identity. `role` is bounded metadata and never grants permission.

### 5.2 Fleet run

```text
FleetRun {
  id
  version
  user_goal
  exact_targets[]
  stages[]
  strategy
  failure_policy
  production
  state
  approval_binding?
  verification
  created_at
  updated_at
}
```

The full user goal, command text, file contents and output previews are not written to the
content-free recovery repository. User-visible live content remains in the bounded runtime event
store according to Runtime V2 rules.

### 5.3 Goal graph

The coordinator stores desired outcomes and dependencies, not a fixed future command queue:

```text
FleetStage {
  id
  summary
  target_selector
  depends_on[]
  execution_mode
  concurrency_limit
  completion_contract
  failure_policy?
}
```

Valid target selectors are exact target IDs or exact role names expanded from the immutable Fleet
target set. A stage cannot introduce a new target.

Stage graph requirements:

- acyclic;
- every dependency references an existing stage;
- every stage has at least one selected target;
- stage and target counts are bounded;
- completion of dependencies is checked by Rust;
- failed or cancelled dependencies block downstream stages unless an explicitly reviewed failure
  policy says otherwise.

### 5.4 Child runs

Each scheduled target owns a normal Runtime V2 child run:

```text
FleetChildRun {
  fleet_run_id
  fleet_version
  stage_id
  target_binding
  agent_run_id
  attempt
  state
}
```

The model receives the current stage goal, current target context, bounded relevant facts and
dependency outputs projected into structured evidence. It does not receive other servers' raw
terminal transcripts.

## 6. State machines

### 6.1 Fleet run

```text
Draft
  -> ValidatingTargets
  -> Investigating
  -> Planning
  -> AwaitingApproval
  -> Executing
  -> Verifying
  -> Succeeded

AwaitingApproval -> Cancelled
Executing -> PausedForReview | Failed | RollingBack | Cancelled
Verifying -> PausedForReview | Failed | RollingBack | Succeeded
RollingBack -> RolledBack | RollbackFailed
in-progress persisted state -> Interrupted on restart
```

### 6.2 Target/stage execution

```text
Pending -> Ready -> Running -> AwaitingApproval -> Running
Running -> Verifying -> Succeeded
Running | Verifying -> Failed | Cancelled
Succeeded -> RollbackPending -> RolledBack | RollbackFailed
```

Only Rust transitions state. UI-derived booleans have no scheduling meaning.

## 7. Planning and execution semantics

### 7.1 Investigation

The coordinator can fan out independent read-only Typed Tools within existing context and tool
budgets. Results are projected into target-bound facts:

```text
Fact {
  target_id
  source
  observed_at
  expires_at
  evidence_ref
  scalar_or_bounded_summary
}
```

Observation Cache remains target-bound. Verification and ChangeSet preconditions always bypass
cache and deduplication.

### 7.2 Iterative reasoning

A stage describes an outcome such as “replica is configured and following source”; it does not
contain a model-generated sequence of future shell commands. Each child run follows Runtime V2:

```text
Reason -> one CommandProposal -> exact approval -> execution -> observation -> Reason
```

Read-only Typed Tool calls may follow their existing Policy path. Command proposals always require
approval. Critical commands fail closed.

### 7.3 Writes

Multi-target state changes are converted to one ChangeSet per target and coordinated by the
existing `FleetExecutionService`:

```text
Fleet Draft
  -> exact fleet version + exact targets + per-target ChangeSet versions
Fleet Approval
  -> durable execution claim
Sequential / Canary / Rolling Batch / non-production Parallel
  -> target-local verification
Cross-target + optional service-level verification
  -> failure policy
```

A conversational command may diagnose or perform an explicitly approved isolated action, but it
cannot be used to bypass a structured subsystem's Fleet ChangeSet boundary.

## 8. Execution strategies

### Sequential

One target at a time. This is the production default.

### Canary

Run an explicitly displayed subset first, verify it, pause when required, then continue only to
the already approved remaining targets.

### Rolling batch

Run bounded batches. The next batch starts only after the current batch satisfies its completion
contract.

### Parallel

Allowed only for non-production runs and only for independent stages. Rust enforces concurrency
limits. Production `Parallel All` is rejected.

## 9. Failure handling

Supported policies:

- `Stop`: stop scheduling new work; preserve completed targets.
- `PauseForReview`: default; wait for an explicit user decision.
- `Continue`: continue only with independent, already-approved targets.
- `Rollback`: reverse completed reversible targets in reverse completion order.

Failure never expands scope, changes roles or retries a mutating action automatically. Retry is a
new attempt against the same exact binding after fresh precondition and approval checks.

## 10. Verification contracts

### Target-local

Checks the desired state on each server: service state, configuration validation, port state,
version or an application-specific read assertion.

### Cross-target

Compares deterministic structured values such as role, version, cluster identifier, membership,
replication state or health boolean. Raw remote contents are not persisted as drift.

### Service-level

Tests the externally observable behavior when explicitly configured, for example an HTTP health
endpoint or a database connection through a load balancer.

A mutating or unknown command requires a fresh, separately approved read-only verification command
before its child run can become Final. Structured ChangeSets keep their existing verification path.

## 11. Approval identity

Fleet approval binds at least:

```text
fleet_run_id
fleet_version
exact (profile_id, session_id, role) target set
stage graph digest
execution strategy and batch parameters
failure policy
per-target ChangeSet id/version
policy version/hash
precondition digests
verification contract digest
```

Any execution-sensitive change invalidates the old approval. UI confirmation text is not authority;
the Rust binding is authority.

## 12. Persistence and audit

Persisted orchestration metadata may contain IDs, versions, states, risks, timestamps, stable error
codes, durations and content digests. It must not contain:

- commands or terminal transcripts;
- file paths, configuration bodies or diffs;
- credentials, environment values or Vault material;
- raw HTTP, MCP or log output;
- model private reasoning.

Audit events record scheduling and authorization decisions with exact target identity. Recovery is
metadata-only and never permits approval, resume, execution or rollback of an old live payload.

## 13. Security and policy

- Mention text is untrusted display input.
- Roles do not grant access or imply root privileges.
- Every child run uses the effective Policy of its own target.
- Fleet risk is at least the maximum target ChangeSet risk.
- One denied target does not weaken policy for another target.
- A model cannot select a different session for a target.
- Credentials are acquired by Rust services only when the approved business operation needs them.
- No generic `execute_on_targets`, `ssh_write_many` or frontend PTY injection IPC is introduced.
- Existing Incident, Operations Pack, ChangeSet, Verification and Rollback Typed Tool boundaries
  remain authoritative.

## 14. Limits for the first production version

- 2–10 live targets;
- Linux/macOS SSH targets; Windows orchestration requires typed PowerShell domain services later;
- one live Session per mentioned profile unless the user selects an exact session explicitly;
- non-interactive commands only;
- bounded stage and retry counts;
- no autonomous reconnect;
- no stored sudo-password injection;
- no unattended Critical action;
- no dynamic inventory expansion during a run.

## 15. Example: MySQL source/replica

```text
@mysql-01#source @group:mysql-replicas#replica
Build a MySQL 8 GTID replication topology. Investigate first and show all changes.
```

An Operations Pack can produce this goal graph:

```text
S1: preflight all targets
S2: prepare source             depends on S1
S3: verify source              depends on S2
S4: prepare replicas           depends on S3
S5: join replicas              depends on S4
S6: verify every replica       depends on S5
S7: verify topology/service    depends on S6
```

Passwords and replication credentials are Vault-owned inputs. The model sees opaque references or
redacted facts only. Product-specific semantics live in the MySQL pack; the coordinator remains
generic.

## 16. Delivery plan

### M1 — Exact targets and mentions

- [x] Specify mention grammar and target-binding invariants.
- [x] Add a pure frontend parser/resolver with ambiguity and connection validation.
- [x] Add Rust exact `(profile_id, session_id, role)` validation IPC.
- [x] Add unit tests for duplicate, ambiguous, disconnected and over-limit input.
- [ ] Add accessible autocomplete and target chips.

### M2 — Fleet run domain

- [x] Add `FleetRun`, `FleetStage`, `FleetChildRun` and state transition validation.
- [x] Add content-free SQLite metadata repository and restart recovery.
- [x] Create an immutable exact-target/stage Graph Digest for later approval binding.
- [ ] Bind Graph Digest and exact targets to an approval/version transition.
- [ ] Add Fleet event envelopes and bounded history projection.

### M3 — Coordinator and child Runtime V2

- [ ] Create parent-child AgentRun ownership.
- [ ] Route each child to its exact Session dispatcher and target Policy.
- [ ] Schedule acyclic stages with bounded concurrency.
- [ ] Pause and cancel without leaking work into other targets.

### M4 — Multi-target investigation

- [ ] Fan out allowed read Tools under per-run budgets.
- [ ] Aggregate target-bound structured facts.
- [ ] Add role, version, service and configuration-digest comparisons.
- [ ] Prevent raw cross-target transcript aggregation.

### M5 — Fleet ChangeSet integration

- [ ] Convert target write proposals to independent ChangeSets.
- [ ] Reuse `FleetExecutionService` approval/execution/recovery semantics.
- [ ] Bind graph, target roles and verification contract into approval identity.
- [ ] Surface real diff, risk, rollback and precondition review.

### M6 — Fleet UI

- [ ] Add topology/stage view and per-target status.
- [ ] Add Sequential/Canary/Rolling controls within Policy.
- [ ] Add Pause/Continue/Stop/Rollback actions.
- [ ] Keep UI event-driven and orchestration-free.

### M7 — Qualification

- [ ] Add 3–10 node Docker OpenSSH integration fixtures.
- [ ] Cover target/session mismatch, disconnect, cancellation and restart interruption.
- [ ] Cover partial failure, approval invalidation, verification failure and rollback failure.
- [ ] Add a deterministic MySQL-compatible fixture only after the generic framework passes.

## 17. Completion criteria

The framework is complete only when:

- a user can select multiple connected servers through exact mentions;
- Rust proves every target/session binding before creating a run;
- the coordinator schedules goal stages without frontend orchestration;
- all target actions follow Runtime V2 or Typed Tool/ChangeSet safety paths;
- production writes use versioned Fleet approval;
- local and cross-target verification determine completion;
- partial failure and recovery preserve truthful per-target state;
- restart never resumes old approval or side effects;
- real Docker OpenSSH multi-node tests cover the safety-critical paths;
- no secrets, terminal transcripts or remote configuration bodies are persisted.
