# Codex Development Guide — Runory Agent Runtime V2

> Purpose: execute the Runtime V2 migration in small, reviewable stages without destabilizing SSH, Typed Tools, Policy, ChangeSet or Phase 10K context infrastructure.

> Architecture update (2026-09-02): the conversational Runtime V2 has migrated to `Reason → single CommandProposal → exact Approval → ServerSession exec → Observation → Reason`. Every command requires approval; Critical commands fail closed; mutating/unknown commands require a successful approved read verification before Final. The older AR2-B/AR2-F Typed-Tool-first and safe-read-auto-execute tasks below are historical and MUST NOT be implemented for the conversation path. Existing Typed Tools remain only where stable Incident, Operations Pack, ChangeSet, Verification or Rollback code depends on them. See `AGENT_RUNTIME_V2.md` §0.

---

## 1. Document merge

Add these files to the repository root:

```text
AGENT_RUNTIME_V2.md
CODEX_AGENT_RUNTIME_V2.md
```

Then semantically merge the related updates into current docs. Do not blindly overwrite newer project files.

Recommended priority:

```text
current user instruction
  ↓
AGENTS.md
  ↓
SECURITY.md
  ↓
AGENT_RUNTIME_V2.md
  ↓
AGENTIC.md
  ↓
ARCHITECTURE.md
  ↓
DESIGN.md
  ↓
ROADMAP.md
```

Runtime V2 supersedes older orchestration details only where they conflict with iterative / interruptible / resumable semantics. Existing Tool, Policy, Approval, ChangeSet, Verification, Rollback, Secret and Audit rules remain in force.

Recommended Git workflow:

```bash
git status
git checkout -b agent/runtime-v2-docs
# add and semantically merge documentation only
git diff
git add .
git commit -m "docs: define agent runtime v2"
```

Do not combine documentation migration and a large runtime rewrite in one PR.

---

# 2. First task: architecture audit only

Give Codex this prompt first:

```text
Runory is moving to Agent Runtime V2.

Do not modify business/runtime code yet.

Read completely:

AGENTS.md
SECURITY.md
AGENT_RUNTIME_V2.md
AGENTIC.md
ARCHITECTURE.md
DESIGN.md
ROADMAP.md

Then inspect the current implementation of:

- Agent runtime/orchestrator
- Plan & commands / command queue
- AI Agent right panel
- Incident engine
- Operations Packs
- Typed Tool Registry
- Risk Engine
- Policy Engine
- Approval
- ChangeSet
- Verification
- Rollback
- Phase 10K Context Manager
- persistence
- Tauri IPC / Channels

Target runtime:

User Goal
→ AgentController
→ Reasoner
→ AgentDecision
→ Tool
→ Policy/Risk
→ Execute or Interrupt
→ Observation
→ Working Facts
→ Reasoner
→ repeat

Required semantics:

- iterative Reason → Tool → Observation loop
- no fixed future command queue as runtime control
- safe read tools may auto-execute according to deterministic policy
- Tool failure becomes Observation, not automatic AgentRun failure
- Approval is a durable interrupt
- Reject becomes Observation and the run may continue
- Approve resumes the same AgentRun
- AwaitingApproval / AwaitingUser must be resumable
- React renders events and sends user actions; orchestration stays in Rust
- preserve existing Typed Tool / Policy / ChangeSet / Verification / Rollback security architecture
- do not expose or persist private chain-of-thought
- concise progress summaries are allowed

Do not implement Runtime V2 in this task.

Produce:

1. current implementation map
2. modules reusable unchanged
3. modules needing adapters
4. modules to deprecate
5. current command-queue coupling points
6. proposed Runtime V2 Rust module map
7. proposed React module map
8. proposed state machine
9. proposed AgentEvent schema
10. proposed persistence migration
11. backward compatibility risks
12. test gaps
13. exact AR2-A scope
14. exact files expected to change in AR2-A

Stop after the audit.
Do not begin AR2-A automatically.
```

Commit the audit result before implementation.

---

# 3. AR2-A — Event Model + State Machine

Branch:

```bash
git checkout -b agent/ar2-a-runtime-events
```

Prompt:

```text
Implement AR2-A from AGENT_RUNTIME_V2.md.

Scope only:

- AgentRun V2 domain model
- AgentRunState
- AgentEvent envelope/payload model
- monotonic per-run event sequence
- valid state transition rules
- event repository abstraction
- unit tests

Do not implement:

- LLM/reasoner loop
- tool scheduling
- auto safe reads
- approval resume
- full SQLite migration
- timeline UI
- ChangeSet changes

Requirements:

1. Authoritative Rust state enum covering:
Created, Running, Reasoning, Acting, Observing,
AwaitingApproval, AwaitingUser,
Diagnosed, PlanningChange, ExecutingChange, Verifying, RollingBack,
Paused, Completed, Failed, Cancelled.

2. AgentEvent envelope:
run_id, seq, timestamp, event_type, payload.

3. seq monotonic per run.

4. invalid transitions return stable Rust errors.

5. keep current runtime working while V2 types are introduced; prefer adapters over destructive replacement.

6. do not persist hidden reasoning.

7. tests:
- valid transitions
- invalid transitions
- terminal states
- interrupt states
- event ordering
- serialization compatibility

Run cargo fmt, cargo clippy, cargo test, and frontend typecheck if shared types changed.

Report:
- changed files
- state/event model
- compatibility strategy
- tests
- known gaps
- exact AR2-B boundary

Stop after AR2-A.
```

Gate:

```text
V2 event/state contracts exist without changing current Agent behavior.
```

---

# 4. AR2-B — Iterative Reason → Tool → Observe Loop

Branch:

```bash
git checkout -b agent/ar2-b-iterative-loop
```

Prompt:

```text
Implement AR2-B.

Goal:
Introduce a read-only iterative Agent loop.

Flow:
Context
→ Reasoner
→ AgentDecision
→ read-only Typed Tool
→ ToolResult
→ Observation
→ Context
→ Reasoner
→ repeat

Implement:

1. AgentDecision protocol:
- ToolCalls
- AskUser
- Final

Do not add write/ChangeSet proposal yet.

2. AgentController loop.

3. Structural validation of Reasoner output.
Model risk/permission declarations are not authoritative.

4. Dispatch only existing read-only Typed Tools.

5. Independent safe reads may execute in parallel when current infrastructure supports it.

6. Tool failure becomes Observation.
Do not automatically fail the run.

7. enforce:
- max reasoner rounds
- tool call budget
- time budget
- cancellation

8. reuse Phase 10K Context Manager.
Do not create another context system.

9. keep the legacy runtime behind an adapter/feature flag while migration is incomplete if necessary.

Tests:
- two-round diagnosis
- tool failure then alternate tool
- AskUser reaches interrupt-ready state
- final answer
- cancellation
- budget exhaustion
- duplicate read compatibility with Phase 10K

Do not implement write actions, ChangeSet, approval resume, timeline replacement, new Operations Packs or MCP.

Stop after AR2-B.
```

Gate:

```text
A real ToolResult can change the Agent's next action.
```

---

# 5. AR2-C — Interrupt / Approval / Resume

Branch:

```bash
git checkout -b agent/ar2-c-interrupt-resume
```

Prompt:

```text
Implement AR2-C.

Goal:
Approval/user input becomes a real runtime interrupt instead of command queue confirmation.

Implement:

- AwaitingApproval
- AwaitingUser
- ApprovalRequest
- exact action binding
- reject-as-observation
- resume same AgentRun
- removal of runtime dependency on Continue next step

Approval binding includes:

run_id
tool_call_id
tool_name
arguments_hash
target_ids
risk
policy_version/hash
precondition reference where available

Rules:

- approval does not create a new AgentRun
- rejection does not automatically end the run
- changed arguments/targets invalidate approval
- Agent cannot execute the pending risky action while waiting
- existing Policy/Risk remains authoritative
- do not weaken R3/R4 constraints
- do not expose private reasoning

Full restart durability belongs to AR2-D, but V2 interrupt state must already serialize cleanly.

Tests:
- approval interrupt
- approve and resume same run_id
- reject and reason again
- AskUser and resume
- mutated args invalidate approval
- changed target invalidates approval
- cancel while awaiting approval

Stop after AR2-C.
```

Gate:

```text
Approve/reject pauses and resumes one logical AgentRun.
```

---

# 6. AR2-D — Durable Checkpoints + SQLite

Branch:

```bash
git checkout -b agent/ar2-d-durable-state
```

Prompt:

```text
Implement AR2-D.

Goal:
AgentRun survives application restart at interrupt boundaries.

Implement durable storage, preferably SQLite, for:

agent_runs
agent_events
agent_checkpoints
agent_messages
tool_calls
tool_results
observations
approval_requests

Add working_facts/tool_artifacts only if needed now; full integration is AR2-H.

Requirements:

1. schema migration/versioning
2. transactional persistence when run state/checkpoint/approval/event must remain consistent
3. recover AwaitingApproval, AwaitingUser, Paused
4. use conservative recovery for interrupted Running state
5. never persist SSH password, private key content, passphrase, Vault secret, cloud secret or hidden reasoning
6. sanitize persisted Tool input/output per SECURITY.md
7. startup discovers resumable runs but never silently executes a pending action

Tests:
- restart AwaitingApproval
- restart AwaitingUser
- checkpoint corruption handling
- schema migration
- no-secret persistence
- event replay order
- stale approval remains blocked until revalidated

Stop after AR2-D.
```

Gate:

```text
Close Runory → reopen → safely resume the pending AgentRun.
```

---

# 7. AR2-E — Right-panel Agent Timeline UI

Branch:

```bash
git checkout -b agent/ar2-e-timeline-ui
```

Prompt:

```text
Implement AR2-E.

This is a Presentation / Interaction refactor.

Runory already places AI Agent in the right-side Context Panel. Keep this placement:

[ AI Agent ] [ Inspect ]

Replace the current:

Plan & commands
AWAITING CONFIRMATION
per-command Approve/Edit/Skip queue
Continue next step

with:

AgentHeader
AgentTimeline
AgentComposer

Panel:
default ~420px
min 360px
max 600px
resizable

Add optional Expand / Focus Workspace for long runs.

Timeline renders AgentEvent types:

UserMessage
Progress
ToolActivity
Approval
Observation/Evidence
Diagnosis
ChangeSet
Verification
FinalAnswer

UX rules:

1. safe auto-executed read tools are compact rows, e.g.:
   ✓ Checked disk usage

2. expandable details may show tool, backend implementation detail if relevant, duration and sanitized result.

3. do not show a large card for every safe read.

4. approval-required actions get a prominent card with action, risk, reason, targets, verification, Reject/Approve.

5. remove Continue next step. Runtime continues automatically after observations.

6. remove Plan & commands as the primary Agent UI. A collapsible Current approach is allowed but is not executable.

7. never show private chain-of-thought. Show concise progress summaries only.

8. composer fixed at bottom: Ask Runory about this server...

9. panel collapse or Inspect tab switch does not stop/recreate the AgentRun.

10. panel resize must correctly trigger xterm fit/resize.

11. use existing i18n.

12. React remains presentation-only; do not move orchestration into frontend.

Do not change Tool/Policy/ChangeSet semantics.

Tests:
- timeline event rendering
- live event append
- approval card actions
- collapse/reopen
- run continues while hidden
- resize + xterm
- no Continue next step
- no command queue primary view

Provide screenshots in final report.
Stop after AR2-E.
```

Gate:

```text
The UI feels like a live Agent run, not a command wizard.
```

---

# 8. AR2-F — Safe Read Automation + Typed Tool Coverage

Branch:

```bash
git checkout -b agent/ar2-f-safe-read-tools
```

Prompt:

```text
Implement AR2-F.

Goal:
Runory investigates without approval spam for ordinary diagnostics.

Use deterministic Rust Policy/Risk behavior:

R0 → auto execute
R1 → auto execute
R2 → Policy decides
R3 → approval
R4 → explicit approval or deny

Add/normalize resource impact metadata:
Low, Medium, HighIO, HighCPU, LongRunning, ExternalCost.

Migrate common diagnostics toward Typed Tools where reasonable:

df -h       → system.disk_usage
df -i       → filesystem.inode_usage
lsblk       → block_devices.list
du patterns → filesystem.top_consumers

Do not remove terminal.exec_readonly fallback.

Requirements:
- Typed Tool First
- safe parallel reads where independent
- no R0/R1 confirmation spam
- broad/high-IO scans remain policy-controlled
- audit auto-authorized actions
- UI receives auto-authorized/start/completed events

Tests:
- disk diagnosis reaches remediation proposal with zero clicks for safe reads
- high-IO scan policy restriction
- Typed Tool preferred over shell
- shell fallback when Typed Tool unavailable
- auto read cannot mutate remote state

Stop after AR2-F.
```

Gate:

```text
"Check disk usage" autonomously drills into the cause until a risky action or missing input is reached.
```

---

# 9. AR2-G — ChangeSet / Write Integration

Branch:

```bash
git checkout -b agent/ar2-g-changeset-write
```

Prompt:

```text
Implement AR2-G.

Goal:
Runtime V2 proposes and safely executes remediation through the existing ChangeSet path.

Required path:

Agent Decision
→ Change Proposal
→ ChangeSet
→ Policy
→ Approval
→ Preconditions
→ Execute
→ Verify
→ Commit / Rollback
→ Observation
→ Agent continues

Implement:
- Change proposal decision/event
- existing ChangeSet reuse
- approval interrupt
- exact binding
- precondition revalidation
- execution events
- verification events
- rollback events
- post-verification observation back into the loop

If verification fails:
- do not mark resolved
- Agent may investigate further or propose rollback
- preserve complete audit trail

Tests:
- approved successful remediation
- verification failure
- rollback
- stale precondition
- approval invalidation
- multi-target binding
- no direct write bypass

Stop after AR2-G.
```

Gate:

```text
Diagnose → approval → fix → verify → continue all occur within one AgentRun.
```

---

# 10. AR2-H — Facts / Context / Artifact Integration

Branch:

```bash
git checkout -b agent/ar2-h-context-facts
```

Prompt:

```text
Implement AR2-H.

Goal:
Long multi-round Agent runs stay bounded and evidence-grounded.

Reuse Phase 10K Context Manager.

Integrate:
- WorkingFact
- Observation provenance
- ToolArtifact
- Artifact references/search/read
- Context snapshots
- Freshness
- Compaction
- Budgeting
- Cache invalidation

WorkingFact contains:
key/value, source event/tool, target, observed_at, freshness/provenance.

Large outputs:
- store as ToolArtifact
- send summary/structured findings to model
- retain artifact reference

Do not inject full logs, full terminal history or large config trees unless explicitly needed.

Writes invalidate relevant cached facts.
Verification cannot rely solely on stale facts.

Tests:
- 5000-line logs do not enter full model context
- artifact search/read
- fact freshness expiry
- write invalidation
- compacted run retains correct diagnosis
- secret redaction for artifact summaries/persistence

Stop after AR2-H.
```

Gate:

```text
Long runs stay efficient without losing evidence or correctness.
```

---

# 11. AR2-I — Recovery / Regression / Production Hardening

Branch:

```bash
git checkout -b agent/ar2-i-production-hardening
```

Prompt:

```text
Implement AR2-I and execute the Runtime V2 Release Gate.

Do not add new product features.

Cover:
- tool timeout recovery
- model timeout/retry policy
- SSH disconnect/reconnect
- application crash/restart
- approval recovery
- AskUser recovery
- cancellation
- pause/resume
- duplicate action prevention
- event replay consistency
- stale preconditions
- policy change during interrupt
- budget exhaustion
- invalid model decision
- malformed tool result
- audit completeness

Regression fixtures:

1. nginx expected log directory missing → Agent chooses another source.
2. disk usage diagnosis → safe reads auto-run and adapt.
3. service restart → interrupt → approve → resume → verify.
4. user rejects action → Agent continues safely.
5. Runory restarts while approval is pending.
6. target state changes after approval → action must not execute.
7. tool timeout → safe alternative.
8. verification failure after successful command.
9. multi-server action retains target-specific approval binding.

Benchmark:
- reasoner rounds
- tool calls
- approval count
- time to diagnosis
- context tokens
- unnecessary command count

Verify every Release Gate item in AGENT_RUNTIME_V2.md.

Do not add multi-agent, Team/RBAC, Marketplace, new MCP providers or new Operations Packs.

Final report:
1. release-gate table
2. regression results
3. remaining risks
4. performance metrics
5. migration status from old runtime
6. obsolete runtime components safe to remove
7. recommended follow-up

Stop after AR2-I.
```

---

# 12. Cleanup PR after V2 release gate

Only after AR2-I passes should Codex remove obsolete architecture.

Potential cleanup:

```text
fixed command plan queue
Continue next step state
per-command form workflow
legacy plan execution orchestrator
duplicated frontend runtime state
legacy command-card components
```

Before deletion:

1. search all references
2. ensure persisted-run migration does not depend on them
3. run full regression suite
4. preserve compatibility migration when needed
5. document breaking changes

Use a separate cleanup PR.

---

# 13. Rules to prepend to every AR2 prompt

```text
Agent Runtime V2 development rules:

- AGENTS.md and SECURITY.md are hard constraints.
- AGENT_RUNTIME_V2.md defines the orchestration baseline.
- Preserve existing SSH / Typed Tool / Policy / ChangeSet / Verification architecture.
- Model must not directly access russh, Vault, arbitrary shell or arbitrary filesystem.
- Do not expose or persist private chain-of-thought.
- Show concise progress summaries only.
- Tool failure is an Observation, not automatically a run failure.
- Reject is an Observation, not automatically a run failure.
- Approval interrupts/resumes the same AgentRun.
- Safe read automation is deterministic and Policy-controlled.
- Write actions continue through ChangeSet.
- Verification is mandatory.
- React never owns runtime orchestration.
- Do not implement later AR2 phases unless explicitly requested.
```

---

# 14. Recommended order

```text
Architecture Audit
  ↓
AR2-A
  ↓
AR2-B
  ↓
AR2-C
  ↓
AR2-D
  ↓
AR2-E
  ↓
AR2-F
  ↓
AR2-G
  ↓
AR2-H
  ↓
AR2-I
  ↓
Cleanup PR
```

Do not parallelize B/C/D before the shared AgentRun and AgentEvent contracts stabilize. AR2-E may begin once A-D contracts are stable.

---

# 15. Product acceptance statement

Runtime V2 is successful when a user can say:

```text
Please investigate why nginx is failing.
```

and Runory can:

```text
inspect safely
→ encounter an unexpected environment
→ adapt
→ keep investigating
→ interrupt for a risky action
→ wait
→ resume after approval
→ execute the exact approved action
→ verify the real outcome
→ continue if verification fails
→ return an evidence-backed final result
```

without forcing the user to manually advance every diagnostic step.
