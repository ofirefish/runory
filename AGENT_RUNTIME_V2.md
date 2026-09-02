# Runory Agent Runtime V2

> Status: Architecture / Product / Implementation Baseline  
> Scope: iterative agent loop, tool use, interrupt/resume, durable state, right-panel timeline UI  
> Related: `AGENTS.md`, `AGENTIC.md`, `ARCHITECTURE.md`, `SECURITY.md`, `DESIGN.md`, `ROADMAP.md`  
> Priority: complete Runtime V2 before expanding Team / Marketplace / Cloud scope.

---

## 0. Current Architecture Decision — Command Proposal Runtime (2026-09-02)

本节是 Runtime V2 当前实现与后续开发的最高优先级基线。本文后续若仍描述 `Reason → Typed Tool → Observation`、Safe Read 自动执行或“禁止 generated shell”，均属于迁移前设计，在冲突处由本节取代。

对话式 Runtime V2 的主链为：

```text
AgentPanel
  → Tauri Agent Command
  → AgentRuntimeV2Service / AgentController
  → ContextManager
  → Reasoner
  → AgentDecision::CommandProposal { command, reason_summary }
  → Rust validation + conservative Risk / Mutability classification
  → exact Approval binding (run + command hash + target + session + policy snapshot)
  → AwaitingApproval
  → user Run / Cancel
  → AgentCommandExecutionService
  → ServerSessionManager interactive Terminal writer (exact command + Enter)
  → raw bytes continue only through Terminal Channel to xterm
  → bounded/redacted terminal capture stays in Rust as untrusted Observation
  → untrusted Observation
  → next Reasoner round
```

硬规则：

1. Reasoner 每轮只可生成一条非交互式命令，不能预生成未来 command queue。
2. 所有命令均须审批；“Safe / Read”只是 Rust 给 UI 的提示，不构成自动执行权限。
3. 模型不能决定 Risk、Mutability、Approval 或 Target；Rust 保守分类，无法可靠分类时为 `Unknown`。
4. Critical destructive command 在 production policy gate 中 fail-closed。
5. 审批后只执行已绑定的 exact command；command、target、session 或 policy snapshot 变化即失效。
6. React 不执行 Shell、不调用 `writeSsh`、不向 PTY 注入命令；仅渲染事件和发送审批动作。审批恢复后由 Rust Runtime 写入已绑定的交互式 PTY。
7. 命令失败或 Cancel 是 Observation，同一个 `AgentRun` 可以继续推理。
8. 写入或 `Unknown` 命令之后，Runtime 必须要求一次新的、经审批且成功的 ReadIntent 验证命令，之后才允许 `Final`。
9. 原始 Terminal 流只在 xterm 中显示；Rust 可把单次命令结果转换为最多 8 KiB、已脱敏的 `output_preview` 放入 AgentEvent。命令卡按 `Thinking → Awaiting approval → Completed/Failed + preview → Analyzing → 独立结论` 原位演进，禁止把完整 Terminal transcript 放入 React State。
9. `RunCompleted` 只能来自 `AgentDecision::Final`，并且没有待处理 Command、Observation、Approval、UserInput、ChangeSet 或 Verification。
10. Command 与输出均不得包含 Credential、Private Key、Passphrase、Vault Secret；输出进入模型前必须有界、脱敏并按 Untrusted Data 处理。

Typed Tool 模块不再是 V2 对话诊断的能力目录，也不要求为每个 Linux 操作新增 Tool。现有 Typed Tool 暂不物理删除，因为 Incident / Operations Pack / ChangeSet Preconditions / Verification / Rollback 等稳定子系统仍依赖它；这些子系统继续遵守原有 Registry、Policy 和审计边界。

Reasoner wire schema：

```json
{"action":"propose","command":"df -h","why":"Inspect filesystem usage"}
```

其它合法动作只有 `answer` 与 `clarify`。未知字段、混合动作、多行命令、NUL、超限内容或 Secret-like payload 必须由 Rust 拒绝并作为可恢复的决策错误处理。

---

## 1. Purpose

Runory already has a mature Agentic safety foundation: SSH / ServerSession, Risk, Policy, Approval, ChangeSet, Verification, Rollback, Incident / Operations Packs, Phase 10K context optimization, multi-server controls and Audit. Typed Tools remain part of that foundation for structured subsystems, but are not the conversational Runtime's universal capability catalog.

The remaining weakness is orchestration. The existing Agent behaves too much like a command-plan wizard:

```text
User goal
  ↓
Generate Plan & commands
  ↓
Wait for confirmation per command
  ↓
Execute
  ↓
Continue next step
```

Runtime V2 replaces this with an iterative, interruptible, resumable Agent loop:

```text
User goal
  ↓
Reason about the next useful action
  ↓
Propose one command and wait for approval
  ↓
Observe real server state
  ↓
Update facts / hypotheses
  ↓
Reason again
  ↓
Need approval or user input?
  └── Command always interrupts → Checkpoint → User action → Resume same AgentRun
  ↓
Diagnose
  ↓
Propose ChangeSet
  ↓
Approve → Execute → Verify → Continue / Rollback / Complete
```

Product principle:

> The Agent chooses each next diagnostic command, but the user explicitly approves every server execution; Runory returns the result to the same run automatically.

---

# 2. Non-goals

Runtime V2 does not mean:

- direct or unapproved shell access for the model
- exposing or storing private chain-of-thought
- deleting Typed Tools that existing Incident / ChangeSet / Verification subsystems still require
- bypassing Policy / Risk / Approval
- bypassing ChangeSet / Verification / Rollback
- moving orchestration into React
- adding multi-agent orchestration
- adding new MCP providers, Skills, Operations Packs, Team, Marketplace or Cloud scope

Existing security rules remain authoritative.

---

# 3. Core Architecture

```text
                    User Goal
                       │
                       ▼
                Agent Controller
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
   Context Manager   Reasoner    Run Store
          │            │            │
          │            ▼            │
          │       AgentDecision      │
          │            │            │
          └──────┬─────┴─────┬──────┘
                 │           │
                 ▼           ▼
              Tool Call    AskUser / Final
                 │
                 ▼
            Tool Registry
                 │
                 ▼
          Policy / Risk Engine
                 │
        ┌────────┴─────────┐
        ▼                  ▼
     Allowed         Approval Required
        │                  │
        │              Interrupt
        │                  │
        │              Checkpoint
        │                  │
        │           Approve / Reject
        │                  │
        └──────────┬───────┘
                   ▼
              Tool Executor
                   │
                   ▼
               ToolResult
                   │
                   ▼
               Observation
                   │
                   ▼
              Working Facts
                   │
                   └────────────→ Reasoner
```

Required trust boundary:

```text
LLM / Reasoner
  ↓
AgentDecision
  ↓
Rust Agent Controller
  ↓
Tool Registry
  ↓
Policy / Risk / Approval
  ↓
Domain Service
  ↓
ServerSession
```

Forbidden:

```text
LLM → russh
LLM → CredentialVault
LLM → unrestricted shell
LLM → arbitrary filesystem
React → SSH execution
React → policy / approval semantics
```

---

# 4. Agent Controller

`AgentController` owns runtime orchestration.

Responsibilities:

- create / load / resume `AgentRun`
- build model context via the existing Context Manager
- invoke the Reasoner
- validate `AgentDecision`
- schedule safe ToolCalls
- coordinate safe parallel reads
- interrupt for approval or missing user input
- persist durable checkpoints
- convert ToolResult into Observation
- update Working Facts
- enforce time / token / tool budgets
- emit ordered AgentEvents
- coordinate completion, pause, failure and cancellation

The LLM chooses a desired next action; Runory determines whether and how it may happen.

---

# 5. Iterative Reason → Tool → Observe Loop

Logical loop:

```rust
loop {
    let context = context_manager.build(&run).await?;
    let decision = reasoner.next(context).await?;

    match decision {
        AgentDecision::ToolCalls(calls) => {
            controller.handle_tool_calls(&mut run, calls).await?;
        }
        AgentDecision::AskUser(question) => {
            controller.interrupt_for_user(&mut run, question).await?;
            break;
        }
        AgentDecision::ProposeChangeSet(proposal) => {
            controller.handle_change_proposal(&mut run, proposal).await?;
        }
        AgentDecision::Final(answer) => {
            controller.complete(&mut run, answer).await?;
            break;
        }
    }
}
```

The implementation may differ, but the semantics must not.

## 5.1 No fixed future command queue

Do not generate ten future commands and treat them as the workflow. A high-level approach may be shown to the user, but it is descriptive and mutable:

```text
Current approach

✓ Check filesystem usage
✓ Identify affected mount
● Locate major consumers
○ Determine root cause
○ Recommend remediation
```

It is not an executable queue.

## 5.2 Safe parallel reads

One Reasoner round may request multiple independent read-only Typed Tools, for example:

```text
system.disk_usage
filesystem.inode_usage
block_devices.list
```

The Controller may safely execute these in parallel according to Policy and resource impact metadata. The Agent then reasons from the combined observations.

---

# 6. AgentDecision Protocol

Recommended logical model:

```rust
enum AgentDecision {
    ToolCalls(Vec<ToolCallRequest>),
    AskUser(UserQuestion),
    ProposeChangeSet(ChangeProposal),
    Final(FinalResponse),
}
```

`ToolCallRequest` should contain:

```text
tool_name
arguments
target_ids
reason_summary
expected_observation
```

The model's claimed `risk`, `approval_required` or `allowed=true` is never authoritative. Runory recomputes these deterministically.

---

# 7. Tool Strategy

## 7.1 Typed Tool First

Priority:

```text
Typed domain tool
  ↓
typed generic diagnostic tool
  ↓
terminal.exec_readonly
  ↓
terminal.exec
```

Examples:

```text
system.disk_usage
filesystem.inode_usage
filesystem.top_consumers
block_devices.list
service.status
service.logs
nginx.test
nginx.config
docker.inspect
docker.logs
network.port_check
http.request
```

If `system.disk_usage` internally uses `df`, the Agent should still see a structured Tool, not parse terminal text.

## 7.2 Shell remains an escape hatch

`terminal.exec_readonly` is acceptable when no Typed Tool exists. `terminal.exec` remains higher risk and must never become the default orchestration path.

---

# 8. Risk and Resource Impact

Risk and resource impact are separate dimensions.

Recommended risk levels:

```text
R0  Safe structured read
R1  Normal diagnostic read
R2  Sensitive / broad / expensive read
R3  State-changing action
R4  Destructive / critical action
```

Recommended default behavior:

```text
R0 → auto execute
R1 → auto execute
R2 → policy dependent
R3 → approval
R4 → explicit approval or deny
```

A read can still be operationally expensive. Add Tool impact metadata such as:

```text
Low
Medium
HighIO
HighCPU
LongRunning
ExternalCost
```

Example:

```text
filesystem recursive scan /
Risk: R1/R2
Impact: HighIO
```

Policy may restrict this even though it does not mutate state.

---

# 9. Interrupt / Resume

Interrupt/resume is the defining Runtime V2 capability.

## 9.1 Approval interrupt

When a ToolCall requires approval:

```text
Running
  ↓
AwaitingApproval
```

The Controller must:

1. validate the ToolCall
2. compute Risk / Policy
3. create an `ApprovalRequest`
4. persist a durable checkpoint
5. emit `ApprovalRequired`
6. stop scheduling the pending risky action
7. wait

After approval:

```text
load checkpoint
  ↓
validate approval binding
  ↓
validate current preconditions
  ↓
execute exact approved action
  ↓
create observation
  ↓
resume Agent loop
```

Approval resumes the **same `AgentRun`**. It does not create a replacement run.

## 9.2 Ask-user interrupt

If required context is missing:

```text
AwaitingUser
```

Example:

> I found two configured sites on this server. Which one should I diagnose?

The answer becomes a run event, and the same run resumes.

## 9.3 Reject is an observation

Rejecting one action should usually not terminate the run.

The Agent receives a structured observation:

```text
User rejected ToolCall TC-1024.
```

It may then:

- find a safer alternative
- ask a clarifying question
- explain that further progress is blocked
- return a constrained recommendation

---

# 10. Durable State Machine

Recommended states:

```text
Created
Running
Reasoning
Acting
Observing

AwaitingApproval
AwaitingUser

Diagnosed
PlanningChange
ExecutingChange
Verifying
RollingBack

Paused
Completed
Failed
Cancelled
```

Key semantics:

- `ToolCall Failed` does not imply `AgentRun Failed`.
- `AwaitingApproval` and `AwaitingUser` are durable states.
- closing/reopening Runory must not lose a pending approval.
- connection loss should preserve the run when safe, even if Tool execution must pause.

---

# 11. Durable Persistence

Use SQLite for complex Agent runtime state rather than ordinary profile JSON.

Suggested database:

```text
runory-agent.db
```

Recommended tables:

```text
agent_runs
agent_events
agent_checkpoints
agent_messages
tool_calls
tool_results
observations
working_facts
approval_requests
tool_artifacts
```

## 11.1 Event ordering

Every persisted event contains:

```text
run_id
seq
event_type
timestamp
payload
```

`seq` is monotonic per run.

## 11.2 Checkpoint content

Persist only what is necessary to safely resume:

```text
run_id
event_cursor
pending_interrupt
working_fact_snapshot
context_reference
budget_state
policy_snapshot
change_set_reference
```

Never persist:

```text
SSH password
private key content
passphrase
Vault secret
cloud credential
model hidden reasoning
```

---

# 12. Agent Event Model

Runtime V2 is event-driven.

Suggested events:

```text
RunCreated
RunStarted
RunResumed
RunPaused
RunCancelled
RunCompleted
RunFailed

UserMessageAdded
AssistantMessageAdded
ReasoningStarted
ProgressUpdated

ToolRequested
ToolAutoAuthorized
ToolApprovalRequired
ToolStarted
ToolOutputChunk
ToolCompleted
ToolFailed

ObservationAdded
FactsUpdated

UserInputRequired
UserInputReceived
ApprovalGranted
ApprovalRejected
ApprovalInvalidated

DiagnosisUpdated
RootCauseIdentified

ChangeSetProposed
ChangeSetApproved
ChangeSetExecutionStarted
ChangeSetExecutionCompleted

VerificationStarted
VerificationCompleted
RollbackStarted
RollbackCompleted
```

The React UI should reconstruct the visible run from this event stream instead of rendering a precomputed `plan[]`.

---

# 13. Progress Summaries vs Private Reasoning

Allowed UI:

```text
Investigating nginx log locations…
/var/log/nginx does not exist.
Checking the systemd journal instead…
```

Do not show or persist full private chain-of-thought.

Persist and render only:

- progress summaries
- requested actions
- tool execution
- evidence
- observations
- execution-relevant decisions
- diagnosis
- approval / rejection
- ChangeSet / verification / rollback status

---

# 14. Tool Failure Semantics

Tool failure becomes an observation.

Supported categories should include stable error types such as:

```text
NotFound
CommandFailed
Timeout
PermissionDenied
ConnectionLost
Unsupported
MalformedResult
```

Example:

```text
Tool:
file.search("/var/log/nginx")

Result:
NotFound
```

The Agent can reason:

```text
Nginx may be logging through journald.
```

and continue with:

```text
service.logs("nginx")
```

Only mark the entire run failed when meaningful recovery is exhausted or runtime infrastructure is unrecoverable.

---

# 15. Working Facts

Maintain compact, evidence-backed facts:

```text
OS = Ubuntu 24.04
nginx installed = true
nginx service = active
/var/log/nginx exists = false
nginx config valid = true
nginx logging source = journald
upstream = 127.0.0.1:8000
port 8000 = closed
```

Each fact should include:

```text
key
value
source_event_id
target_id
observed_at
freshness / provenance
```

Facts are not arbitrary long-term memory. Stale facts cannot silently satisfy Verification or ChangeSet preconditions.

---

# 16. Tool Artifacts

Large outputs must not be injected into model context in full.

Examples:

- 5000-line journal output
- full nginx config dump
- large Docker logs
- directory scan
- large HTTP response body

Store them as `ToolArtifact`.

Model context receives:

```text
summary
important structured findings
artifact reference
```

Optional internal tools:

```text
artifact.search
artifact.read
artifact.tail
```

This preserves evidence while keeping context bounded.

---

# 17. Phase 10K Context Integration

Do not build a second Context Manager.

Runtime V2 reuses:

- Context Budget
- Compaction
- Freshness
- Tool result projection
- Cache / invalidation
- Deduplication
- Secret redaction
- AgentRun metrics

Per reasoning round, provide only relevant:

```text
User Goal
Current Target Context
Relevant Conversation
Working Facts
Recent Observation
Active Incident / ChangeSet
Relevant Policy Summary
Available Tool Schemas
Budget State
Relevant Artifact References
```

Do not blindly include the full terminal history or raw tool output.

---

# 18. ChangeSet / Write Integration

All state-changing infrastructure actions continue through the existing safe path:

```text
Agent
  ↓
Change proposal
  ↓
ChangeSet
  ↓
Policy
  ↓
Approval
  ↓
Precondition validation
  ↓
Execute
  ↓
Verify
  ↓
Commit / Rollback
```

Runtime V2 must not introduce a write bypass.

---

# 19. Approval Binding

Approval binds the exact action.

Recommended binding:

```text
run_id
tool_call_id or change_set_id
tool_name
arguments_hash
target_ids
risk
policy_version
policy_hash
precondition_hash
```

If any execution-sensitive field changes, prior approval is invalid.

Example:

```text
Approved: nginx.reload
Changed:  nginx.restart
Result:   approval invalidated
```

---

# 20. Verification Is Part of the Agent Loop

Successful command exit code does not prove the problem is resolved.

Example:

```text
service.restart("myapp")
  ↓
service.status("myapp")
  ↓
network.port_check(8000)
  ↓
http.health_check(url)
```

If Verification fails, the Agent may continue investigation or propose Rollback. Final output must distinguish `command succeeded` from `problem verified resolved`.

---

# 21. Operations Packs / Skills Role

Do not delete existing Operations Packs. Reposition them from rigid orchestrators to adaptive playbooks / domain knowledge.

Example:

```text
Disk Incident Playbook

Useful checks:
- filesystem usage
- inode usage
- block devices
- top-level consumers
- logs
- Docker
- backups
```

The Agent may adapt after every observation. Playbooks must not force a fixed command list.

---

# 22. Single Agent First

Runtime V2 uses:

```text
1 Agent Controller
1 Reasoner
many Typed Tools
Skills / Playbooks
```

Do not add planner/reviewer/network/nginx subagents until production evidence demonstrates a need.

---

# 23. Tauri IPC

Use command/request IPC for:

```text
start_agent_run
send_agent_message
approve_agent_action
reject_agent_action
cancel_agent_run
pause_agent_run
resume_agent_run
load_agent_run
```

Use Tauri Channel for streaming:

```text
Rust AgentRuntime
  ↓
AgentEvent stream
  ↓
Tauri Channel
  ↓
React AgentTimeline
```

Do not resend the entire runtime state on every update.

---

# 24. Product Layout

Keep the current layout:

```text
Main Workspace                          Right Context Panel
────────────────────────────────────   ─────────────────────────
Terminal / Files / Dashboard           [ AI Agent ] [ Inspect ]
Operations / Deploy
```

The Agent remains in the right panel for normal work.

Recommended panel sizing:

```text
default 420px
min     360px
max     600px
```

Support Resize.

For complex Agent runs, provide `Expand` to open an Agent Focus Workspace while preserving the same `AgentRun`.

Current pre-V2 panel reference:

```text
docs/assets/agent/agent-panel-before-runtime-v2.png
```

---

# 25. Agent Panel Information Architecture

Replace:

```text
Plan & commands
AWAITING CONFIRMATION
Approve/Edit/Skip command queue
Continue next step
```

with:

```text
Agent Header
  ↓
Agent Timeline
  ↓
Composer
```

Timeline renders:

```text
User Message
Progress Summary
Tool Activity
Approval
Observation / Evidence
Diagnosis
ChangeSet
Verification
Final Answer
```

---

# 26. Example: Disk Diagnosis UX

User:

```text
Check disk usage and diagnose issues.
```

Expected UI:

```text
You
Check disk usage and diagnose issues.

● Investigating disk usage...

✓ Checked filesystem usage
  / 94%

✓ Checked inode usage
  / 27%

● Root filesystem usage is high.
  Looking for major consumers...

✓ /var uses 48 GB

● Inspecting /var...

✓ Docker uses 41.8 GB

Root cause
Docker container logs use 18.4 GB.

Proposed fix
Configure log rotation and safely reclaim old logs.

R3 · Approval required

[Review & approve]
```

No manual click is required for the safe diagnostic chain.

---

# 27. Tool Activity UI

Auto-executed safe tools are compact:

```text
✓ Checked disk usage
```

Expandable detail may show:

```text
Tool
system.disk_usage

Backend command
sanitized / implementation detail if relevant

Duration
83 ms

Result
sanitized structured output
```

Do not use a large command card for every R0/R1 read.

---

# 28. Approval Card

Only a real interrupt should dominate the panel:

```text
┌──────────────────────────────────┐
│ APPROVAL REQUIRED                │
│ R3 · Write                       │
│                                  │
│ Restart myapp.service            │
│                                  │
│ Why                              │
│ The app is stopped and nginx     │
│ cannot reach its upstream.       │
│                                  │
│ Verification                     │
│ service.status                   │
│ network.port_check               │
│ http.health_check                │
│                                  │
│ [Reject]              [Approve]  │
└──────────────────────────────────┘
```

For complex ChangeSets, use `[Review ChangeSet]` and open the existing wide review UI.

---

# 29. Raw Command Editing

For Typed Tools, approve the semantic action rather than a shell string:

```text
Restart myapp.service
```

not:

```text
systemctl restart myapp
[Edit]
```

If an explicit shell fallback is used and editing is allowed, editing creates a new action and must trigger revalidation/reapproval.

---

# 30. Composer / Pause / Stop

Composer remains fixed at bottom:

```text
Ask Runory about this server...
```

Behavior:

```text
Enter       send
Shift+Enter newline
```

User controls:

```text
Pause
Stop
```

- `Pause` prevents new reasoner/tool scheduling while preserving resumable state.
- `Stop` cancels the run according to cancellation rules.
- Collapsing the panel does not pause or cancel the run.

---

# 31. React Responsibilities

React may manage presentation state:

```text
active context tab
panel width
expanded timeline items
composer draft
scroll position
```

React must not own:

```text
AgentRun state machine
pending approval truth
Tool scheduler
Policy decisions
SSH execution
Reasoner loop
Checkpoint semantics
```

Authoritative runtime state remains in Rust + persistence.

---

# 32. Suggested Rust Layout

Adapt to the current repository. Do not move stable modules simply to match this diagram.

```text
src-tauri/src/agent/
├── runtime/
│   ├── controller.rs
│   ├── state.rs
│   ├── decision.rs
│   ├── scheduler.rs
│   └── resume.rs
├── reasoner/
│   ├── provider.rs
│   ├── protocol.rs
│   └── validation.rs
├── events/
│   ├── event.rs
│   ├── emitter.rs
│   └── repository.rs
├── checkpoint/
│   ├── checkpoint.rs
│   └── repository.rs
├── context/
│   ├── manager.rs
│   ├── facts.rs
│   ├── observations.rs
│   └── artifacts.rs
├── approval/
├── tools/
├── changeset/
└── persistence/
    └── sqlite.rs
```

---

# 33. Suggested React Layout

```text
src/features/agent/
├── AgentPanel.tsx
├── AgentHeader.tsx
├── AgentTimeline.tsx
├── AgentComposer.tsx
├── AgentFocusWorkspace.tsx
├── events/
│   ├── UserMessageEvent.tsx
│   ├── ProgressEvent.tsx
│   ├── ToolEvent.tsx
│   ├── ApprovalEvent.tsx
│   ├── DiagnosisEvent.tsx
│   ├── ChangeSetEvent.tsx
│   ├── VerificationEvent.tsx
│   └── FinalEvent.tsx
└── store/
    └── agentViewStore.ts
```

`agentViewStore` is presentation-only.

---

# 34. Migration Mapping

## Keep

- SSH / ServerSession
- Typed Tool Registry
- Risk Engine
- Policy Engine
- Approval binding
- ChangeSet
- Verification
- Rollback
- Phase 10K Context Manager
- MCP
- Skills
- Incident domain data
- Audit
- Multi-server safety controls

## Reposition

- Operations Packs → adaptive playbooks
- Typed Plan → internal/debug representation only
- diagnosis workflow → dynamic Agent loop

## Replace

- fixed Plan & Commands queue
- manual confirmation for ordinary safe reads
- `Continue next step`
- command-queue orchestrator
- frontend-owned orchestration state

---

# 35. Migration Track

Do not implement Runtime V2 in one giant PR.

```text
AR2-A  AgentEvent + AgentRun State Machine
AR2-B  Iterative Reason → Tool → Observe Loop
AR2-C  Interrupt / Approval / Resume
AR2-D  Durable Checkpoints + SQLite
AR2-E  Right-panel Agent Timeline UI
AR2-F  Safe Read Automation + Typed Tool Coverage
AR2-G  ChangeSet / Write Integration
AR2-H  Facts / Context / ToolArtifact Integration
AR2-I  Recovery / Regression / Production Hardening
```

Detailed Codex prompts are in `CODEX_AGENT_RUNTIME_V2.md`.

---

# 36. Acceptance Scenarios

## A. Dynamic nginx diagnosis

User:

```text
Find why nginx cannot start.
```

Expected:

1. safe checks run automatically
2. expected log path does not exist
3. Tool returns NotFound
4. AgentRun does not fail
5. Agent chooses journald / another valid source
6. Agent continues until a diagnosis or meaningful blocker

## B. Approval interrupt

Agent proposes service restart.

Expected:

1. state → `AwaitingApproval`
2. exact action persisted
3. user approves
4. same `run_id` resumes
5. action executes
6. Verification runs
7. Agent continues/completes

## C. Reject

Expected:

1. rejected action never executes
2. run remains alive
3. rejection becomes Observation
4. Agent finds a safer alternative or explains the limitation

## D. App restart while awaiting approval

Expected:

1. Runory closes/reopens
2. Agent history reloads
3. pending interrupt is restored
4. approval is still usable only if its binding/preconditions remain valid

## E. Tool timeout

Expected:

1. timeout event emitted
2. timeout becomes Observation
3. Agent may choose a different safe strategy
4. no duplicate unsafe execution

## F. Verified remediation

Expected:

1. write goes through ChangeSet
2. execution succeeds
3. Verification is mandatory
4. only verified outcome is called resolved

## G. Stale approval

After approval, target/config changes externally.

Expected:

1. precondition fails
2. approval invalidates
3. action does not execute
4. Agent re-diagnoses/re-plans/re-requests approval

## H. Disk diagnosis

Expected:

1. disk/inode/device reads auto-run
2. Agent narrows to the affected mount
3. Agent drills into relevant directories only
4. no command queue is displayed
5. user is interrupted only for risky remediation or required input

---

# 37. Required Tests

```text
unit/
  runtime_state_machine
  event_ordering
  decision_validation
  approval_binding
  checkpoint_serialization
  fact_freshness

integration/
  iterative_read_loop
  tool_failure_recovery
  approval_interrupt_resume
  rejection_continue
  ask_user_resume
  changeset_verify
  restart_recovery

security/
  no_secret_checkpoint
  no_policy_bypass
  no_skill_bypass
  no_mcp_bypass
  approval_argument_mutation
  stale_precondition

ui/
  timeline_render
  approval_card
  panel_resize
  panel_collapse_run_survives
  reconnect_state
  no_continue_next_step
```

---

# 38. Observability

Track per run:

```text
reasoner_rounds
model_calls
tool_calls
auto_authorized_tools
approval_interrupts
rejections
tool_failures
recovery_attempts
context_tokens
artifact_bytes
time_to_first_observation
time_to_diagnosis
time_to_resolution
verification_result
rollback_result
```

Never log secrets or hidden reasoning.

---

# 39. Runtime V2 Release Gate

Runtime V2 is complete only when:

- [ ] Safe read tools auto-execute according to Policy.
- [ ] No fixed future command queue drives the Agent.
- [ ] Tool failure does not automatically fail the AgentRun.
- [ ] Reject does not automatically fail the AgentRun.
- [ ] Approval interrupts/resumes the same durable AgentRun.
- [ ] AwaitingApproval survives application restart.
- [ ] AgentEvents are persisted/replayable.
- [ ] UI is event/timeline based.
- [ ] `Continue next step` is removed.
- [ ] Typed Tool remains preferred over shell.
- [ ] Writes still use ChangeSet / Policy / Approval.
- [ ] Verification remains mandatory.
- [ ] Approval binding and stale preconditions pass regression tests.
- [ ] No secret is stored in Agent persistence.
- [ ] Phase 10K Context Budget / Freshness remains effective.
- [ ] Production regression scenarios pass.

---

# 40. Final Product Principle

> Runory investigates by itself, adapts when reality differs from its assumptions, pauses only when it genuinely needs the user's decision, and then safely resumes the same task.

The Agent should feel like a:

```text
persistent infrastructure operator
```

not an:

```text
AI command generator
```

---

# Appendix A — Current Code Reality (2026-09 Audit)

Full audit: `docs/agent-runtime-v2-audit.md`. Key deltas between this baseline and the repository as audited:

1. Four parallel Agent paths coexist. The right-panel product path is `ai_propose_plan` → `AiPlanProposal { commands[] }` → per-command Approve/Edit/Skip → approved commands written to the PTY via `ssh_write` → frontend `Continue next step` re-proposes with an `exclude` list. This is the primary replacement target.
2. `agentic::runtime::AgentRuntimeService` already implements an iterative decision loop (`AgentDecision::{ToolCalls, Answer, Clarify, ProposeChange}`) over the Typed Tool Registry, Policy and ChangeSet — but the remote `ModelGateway::decide` prompt currently forbids tool calls, there is no `AwaitingApproval` / `AwaitingUser` runtime state, and `AgentRun` is never persisted.
3. The uncommitted `ai/chat` path is a human-in-the-loop single-step prototype with an explicit `AwaitingApproval` status, but it executes freeform shell outside the Tool Registry / Policy / ChangeSet boundary. It is UX/protocol reference only, not a Runtime V2 foundation.
4. The security foundation is reusable as-is: closed `NativeToolRegistry` (22 tools, 17 read-only), `ToolDescriptor` with R0–R4 / Mutability / Scope, two-layer Policy with `policy_version` + `policy_hash`, version-bound ChangeSet approval with precondition digests and metadata-only recovery, Phase 10K ObservationCache / ContextBudget / secret redaction, sanitized audit.
5. Known gaps to fill during AR2 tracks: no SQLite anywhere (AR2-D introduces `runory-agent.db`); no standalone `ApprovalRequest` object or `arguments_hash` binding (AR2-C); no resource-impact tool metadata and no `filesystem.inode_usage` / `block_devices.list` / `terminal.exec_readonly` tools (AR2-F); MCP `read_context` does not pass the unified Agent Policy evaluation (adapter work); `agentic/runtime.rs` and `agentic/state.rs` have zero tests.
6. Current right panel constants (`ContextPanel.tsx`): width 320–480, default 360. Runtime V2 target is 360–600, default 420 (AR2-E).
