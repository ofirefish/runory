# Agent Runtime V2 — Architecture Audit & Migration Plan

> Date: 2026-09-02
> Type: 只读代码审计 + 迁移设计（本轮未修改任何业务/运行时代码）
> Baseline: `AGENT_RUNTIME_V2.md`（最高架构基线）、`AGENTS.md` / `SECURITY.md`（硬约束）

> Historical audit notice (2026-09-02): 本文记录迁移前审计事实。其“Typed Tool First / Safe Read 自动执行 / freeform shell 违规”等结论已被用户批准的 Command Proposal 架构取代。当前实现与安全边界以 `AGENT_RUNTIME_V2.md` §0 为准；本文不得作为回退对话主链的实现指令。
> Related: `CODEX_AGENT_RUNTIME_V2.md`、`AGENTIC.md`、`ARCHITECTURE.md`、`ROADMAP.md`

---

## 1. Executive Summary

1. **仓库当前并存四条 Agent 路径**，其中产品主路径（右侧 Context Panel 的 AI Agent）正是 V2 要废弃的「Plan & commands 整包命令队列 + 逐条 Approve & run / Edit / Skip + Continue next step」模式，且批准后的命令通过 `ssh_write` 直接写入 PTY，完全绕过 Typed Tool Registry / Policy / ChangeSet。
2. **最接近 V2 的骨架已经存在**：`agentic::runtime::AgentRuntimeService` 是一个真实的迭代 `Reason → Tool → Observation` 循环（`AgentDecision::{ToolCalls, Answer, Clarify, ProposeChange}`），读工具经 `NativeToolExecutionService` + Policy 自动执行，写提案进 ChangeSet。但它有三个硬缺口：远程模型 `decide()` 的 System Prompt 明确禁止 tool calls（迭代能力实际只在本地启发式 `local_turn` 生效）；没有 `AwaitingApproval` / `AwaitingUser` 运行时中断态；`AgentRun` 完全不持久化，不可恢复。
3. **安全基座可整体复用**：封闭 `NativeToolRegistry`（22 个 Tool，17 个只读）、`ToolDescriptor`（R0–R4 / Mutability / Scope / timeout）、双层 Policy（`AgentPolicyService` + `ToolPolicy`，含 policy version + hash）、版本绑定的 `ChangeSetService`（Approval / Precondition digest / 真实 Rollback / metadata-only 恢复）、Phase 10K `ObservationCache` / `ContextBudget` / `redact_secrets`、sanitized Audit。V2 是编排层替换，不是安全层重写。
4. **持久化缺口明确**：仓库无任何 SQLite 依赖；Agent 相关持久化全部是 JSON Repository（ChangeSet / Incident / Fleet / Policy / Audit / Model 配置），`AgentRun`、`AiAgentPlan`、`ChatRun` 只在内存。AR2-D 需要引入 SQLite（建议 `rusqlite` bundled）承载 `runory-agent.db`。
5. **未提交在制品（约 +3700 行）**：新的 `ai/chat.rs` + `AgentChatView` 是「逐条 propose → approve → observe」的人在环雏形，交互方向正确，但执行层是 freeform shell（`RemoteCommand::freeform`），违反 Typed Tool First 与 Policy 边界，不能作为 V2 基础，只能作为 UX 参考。建议在 AR2 轨道正式启动前冻结该路径的进一步扩张。
6. **建议按 `CODEX_AGENT_RUNTIME_V2.md` 的 AR2-A→I 顺序推进**；AR2-A 只新增 `src-tauri/src/agent/` 下的 V2 状态机 + 事件契约 + 内存仓库与单元测试，不改变任何现有行为。

---

## 2. Current Agent Architecture（真实代码现状）

### 2.1 四条并存路径总览

| 路径 | 入口 Command | Rust Service | 计划形态 | 执行通道 | 当前 UI 位置 |
|---|---|---|---|---|---|
| **A. Panel Plan（主路径）** | `ai_propose_plan`（`commands/ai.rs` L12–25） | `AiService`（`ai/service.rs`） | 一次性整包 `AiPlanProposal { summary, commands[] }`（最多 16 条） | **前端 `writeSsh` 写 PTY** | 右侧 `AgentPanel` |
| **B. Doctor Runtime** | `agent_doctor_run`（`commands/agentic.rs` L28–61）+ `Channel<AgentProgress>` | `AgentRuntimeService`（`agentic/runtime.rs`） | 迭代 `AgentDecision`，每轮 ≤4 个读 Tool | `NativeToolExecutionService` → `RemoteCommand::script` | Doctor Workspace / 面板遗留 `runDoctor` |
| **C. Legacy Typed Plan** | `ai_agent_*`（`commands/ai_agent.rs`） | `AiAgentService`（内存 HashMap） | 用户手选 Tool 的固定 step 队列，逐步审批 | `AiToolExecutor` → exec / SFTP / Ops | `AiAgentView` `mode="plan"` |
| **D. Chat（未提交 WIP）** | `ai_chat_start/approve/reject`（`commands/ai_chat.rs`） | `AiChatService`（`ai/chat.rs`） | 每轮 propose 一条 shell | `RemoteCommand::freeform` | `AgentChatView`（未接入主面板，入口悬空） |

### 2.2 主路径 A 的实际调用链（V2 要替换的对象）

```text
User Message（AgentComposer）
  ↓ AgentPanel.beginRun → runPlanOrDoctor（L341–343，固定 startPlan）
  ↓ proposePlan() = invoke("ai_propose_plan")
  ↓ commands/ai.rs::ai_propose_plan → CloudPolicyService.authorize(AiExecute)
  ↓ AiService.propose_plan → GatewayAssistantProvider.plan
  ↓ ModelGateway.complete_plan（model_gateway.rs L499–566，OpenAI-compatible /chat/completions，stream:false）
  ↓ parse_plan_lenient → AiPlanProposal { summary, commands[] }（一次性整包 Command Queue）
  ↓ validate_plan（Rust 重算 risk / requires_confirmation）
  ↓ React 渲染 AgentPlanCard（"Plan & commands" + "Awaiting confirmation"）
  ↓ 用户逐条 Approve & run / Edit / Skip
  ↓ Approve → writeSsh(sessionId, command bytes) —— 命令被“打字”进 PTY
  ↓ Continue next step → 前端收集 executed/skipped → startPlan(intent, exclude) → 再要一包 plan
```

关键违规点（对照 `AGENTS.md` §28 与 `AGENT_RUNTIME_V2.md`）：

- Observation 产生前预生成未来命令队列；
- 安全读取（df / systemctl status 等）也要求逐条人工确认；
- Approve 的副作用是 `ssh_write` 写终端，绕过 Tool Registry / Policy / Risk / ChangeSet；
- Skip 纯前端本地状态，无 Observation 回传；
- 「继续下一步」由 React 驱动（`continuePlan` + `exclude` + 硬编码 intent 文案）；
- 编排状态（`PlanCommandState` 状态机、`planBusy` 锁、60s 模型超时）全部在 React `useState`。

### 2.3 路径 B（Doctor Runtime）现状

- 状态机（`agentic/state.rs` L11–22）：`GatheringContext / Investigating / Diagnosing / NeedsInput / Succeeded / Failed / Cancelled / TimedOut / PolicyBlocked / BudgetExceeded`。**没有** `AwaitingApproval` / `AwaitingUser` / `Paused` 等可恢复中断态。
- 循环（`runtime.rs` L202–428）：`for _ in 0..budget.max_model_calls`，每轮 `ModelGateway.decide()` → `AgentDecision`；ToolCalls 经 `from_model_read_call`（只允许读 Tool）→ `execute_reads`（dedup + ObservationCache + 并行读）；ProposeChange → `ChangeSetService.draft` + `check_policy`。
- **硬缺口 1**：`model_gateway.rs::decide`（L287–376）远程 System Prompt 禁止 tool calls，只允许 answer/clarify；真正的 tool 选择靠 Local `local_turn` 关键词启发式。等于「迭代循环存在，但远程模型没接上」。
- **硬缺口 2**：进度只有 `Channel<AgentProgress>`（stage + tool 名），最终 `AgentRun` 一次性随 command 返回；无事件流、无持久化、无 seq。
- **硬缺口 3**：`NeedsInput`（Clarify）后由 UI 拼接文本重新发起**新的 run**，不是同一 run 的 Resume。
- 预算：`AgentBudget`（`state.rs` L43–80，默认 `max_model_calls=4`、`max_tool_calls=20`、token/time/cost）；取消经 `watch` + `agent_run_cancel`。

### 2.4 路径 D（未提交 chat WIP）定位

`AiChatService`（`ai/chat.rs`）：`probe_context`（uname/whoami/pwd）→ `complete_agent_turn` → `ChatDecision::{Propose, Answer, Question}` → `AiChatRunStatus::AwaitingApproval` → `ai_chat_approve` 执行 `RemoteCommand::freeform` → redact 输出进 history → 下一轮。

- ✅ 交互语义接近 V2：单步决策、Approve 后同一 run 继续、Reject 不结束任务、`AwaitingApproval` 已是显式状态。
- ❌ 执行层是 freeform shell + 本地 `chat_executable` 分类，不经 `NativeToolRegistry` / `AgentPolicyService` / ChangeSet，违反 AGENTS.md §26 规则 2/3；不落盘、无事件流、无 Channel。
- 结论：**只作为 UX/协议参考，不作为 V2 实现基座**；`complete_agent_turn` 的协议经验可迁移进 V2 Reasoner Protocol。

### 2.5 持久化现状

| 数据 | 机制 | 文件 |
|---|---|---|
| Model Provider 配置 | JSON Repository | `agent-model.json` |
| ChangeSet（content-free） | JSON | `agentic-change-sets.json` |
| Incident（content-free, v2 schema） | JSON | `agentic-incidents.json` |
| Fleet | JSON | `agentic-fleet-runs.json` |
| Agent Policy + Audit | JSON | `agent-policy.json` 等 |
| Tool / AI / Chat / MCP Audit | JSON（上限 2000 条） | `tool-audit.json` / `ai-audit.json` / `ai-chat-audit.json` / `mcp-audit.json` |
| **AgentRun / AiAgentPlan / ChatRun** | **仅内存** | — |

仓库内 **无 rusqlite / sqlx / sqlite 任何依赖**。

### 2.6 前端现状

- 面板结构：`Workspace` → `ContextPanel`（宽度 320–480，默认 360，`localStorage` + pointer 拖拽 resize，`onResize` → `fitTerminals()`）→ `ContextPanelTabs`（`[Agent] [Inspector]`）→ `AgentPanel`。
- 状态：对话/plan/审批状态全部在 `AgentPanel` 的 `useState<Record<string, ServerSessionModel>>`（按 profile.id 分桶）；Zustand 只管面板显隐与 tab；`agent-state.ts` 仅类型定义，无 store。
- 事件：唯一 Channel 是 doctor 的 `Channel<AgentProgress>`；running 圆点靠 `window` 自定义事件 `runory:agent-run` / `runory:agent-done`；plan/chat 全部是一次性 invoke。
- 旧 UI i18n 锚点：`contextPanel.planCardTitle`（Plan & commands）、`contextPanel.planConfirmLabel`（Awaiting confirmation）、`contextPanel.planApprove` / `planEdit` / `planSkip` / `planContinue`（Continue next step）/ `planStop`。

### 2.7 「固定命令队列 / Continue next step」耦合点清单（迁移拆除目标）

| # | 位置 | 符号 |
|---|---|---|
| 1 | `src-tauri/src/domain/ai.rs` L102–109 | `AiPlanProposal`（summary + commands[]） |
| 2 | `src-tauri/src/domain/ai.rs` L80–83 | `AiGenerateRequest.exclude`（“继续下一步”去重） |
| 3 | `src-tauri/src/agentic/model_gateway.rs` L499–566 | `complete_plan`（一次性整包 plan prompt） |
| 4 | `src-tauri/src/agentic/model_gateway.rs` L19–106 | `parse_plan_lenient` |
| 5 | `src-tauri/src/ai/service.rs` L132–150 / L209–244 | `plan` / `validate_plan`（≤16 条命令队列校验） |
| 6 | `src-tauri/src/commands/ai.rs` L12–25 | `ai_propose_plan` |
| 7 | `src/components/context-panel/agent/AgentPanel.tsx` L149–197 | `startPlan` |
| 8 | 同上 L199–224 | `planCommand`（approve/skip/edit + `writeSsh`） |
| 9 | 同上 L245–263 | `continuePlan`（前端推进队列） |
| 10 | 同上 L341–343 | `runPlanOrDoctor`（固定走 plan） |
| 11 | `src/components/context-panel/agent/AgentPlanCard.tsx` 全卡 | Plan & commands / Awaiting confirmation / Continue next step |
| 12 | `src/components/context-panel/agent/agent-state.ts` | `PlanCommand` / `PlanCommandState` 队列状态机 |
| 13 | `src-tauri/src/domain/ai_agent.rs` L97–121 + `ai/agent_service.rs` L40–77 | `AiAgentPlan` / `AiPlanStep` 固定 step 队列 |
| 14 | `src-tauri/src/commands/ai_agent.rs` | `ai_agent_step_approve/execute` 等逐步队列 IPC |
| 15 | `src-tauri/src/agentic/incident.rs` `plan_for`（~L965） | Operations Pack 固定探测序列（V2 中降级为 Playbook 提示） |

---

## 3. Runtime V2 Target Architecture（适配本仓库的 Rust 模块架构）

原则：**新增 `src-tauri/src/agent/`（V2 编排层），既有 `agentic/`、`tools/`、`policy/` 原地复用，不搬家、不改名**。

```text
src-tauri/src/agent/                  ← 全部新增（V2 编排层）
├── mod.rs
├── run.rs            AgentRun V2 领域对象（id / goal / targets / state / budget refs）
├── state.rs          AgentRunState V2 + 显式 transition 表 + 稳定错误码
├── event.rs          AgentEventType + AgentEventEnvelope（run_id/seq/ts/type/payload）
├── decision.rs       AgentDecision V2 协议 + 结构化校验（AR2-B）
├── controller.rs     AgentController：循环调度 / 中断 / 恢复 / 预算（AR2-B/C）
├── reasoner.rs       Reasoner trait + ModelGateway adapter（AR2-B）
├── checkpoint.rs     AgentCheckpoint 模型（AR2-C/D）
├── approval.rs       ApprovalRequest + exact binding + 失效（AR2-C）
├── facts.rs          WorkingFact（AR2-H）
├── artifact.rs       ToolArtifact（AR2-H）
├── repository.rs     RunStore / EventRepository trait + InMemory 实现（AR2-A）
└── sqlite.rs         SQLite 持久化实现 runory-agent.db（AR2-D）

复用（不动，或仅由 controller 调用）：
├── agentic/model_gateway.rs   Reasoner Provider 后端（需为 V2 打开 tool-call 协议）
├── agentic/context.rs         snapshot / compact / freshness / redact_secrets
├── agentic/optimization.rs    ObservationCache / dedup / projection
├── agentic/changes.rs         ChangeSetService（写路径唯一原语）
├── agentic/fleet.rs           多目标写（经 ChangeSet 复用）
├── agentic/incident.rs        Incident 域数据保留；plan_for 降级为 Playbook（adapter）
├── tools/*                    Registry / Descriptor / Execution / Audit / ToolPolicy
├── policy/*                   AgentPolicyService / PolicyEngine（version+hash）
├── skills/registry.rs         Playbook / Domain Knowledge 注入 context
└── mcp/gateway.rs             经 adapter 收束进统一 Observation/Policy 边界
```

数据流（目标）：

```text
User Goal → agent_run_start (IPC)
  → AgentController（Rust 权威状态）
     ├─ ContextManager（复用 10K snapshot/compact/facts）
     ├─ Reasoner（ModelGateway adapter）→ AgentDecision
     ├─ ToolCalls → from_model_read_call → AgentPolicyService
     │     ├─ Allow(R0/R1) → ToolAutoAuthorized → NativeToolExecutionService（并行安全读）
     │     └─ RequireApproval(R2+/写) → ApprovalRequest + Checkpoint + AwaitingApproval
     ├─ ProposeChangeSet → ChangeSetService.draft →（复用既有审批/执行/验证/回滚链）
     ├─ ToolResult → Observation → WorkingFacts →（回到 Reasoner）
     └─ 全程 emit AgentEvent → 持久化 + Tauri Channel → React AgentTimeline
```

---

## 4. Keep / Adapt / Deprecate Matrix

### Keep（原样复用）

| 模块 | 理由 |
|---|---|
| SSH Core / `ServerSessionManager` / `ssh/exec.rs` | V2 不触碰传输层 |
| `NativeToolRegistry` + 22 个 Typed Tool + 各执行器 | Reason→Tool 的执行后端 |
| `ToolDescriptor` / `RiskLevel(R0–R4)` / `Mutability` / `ToolScope` | V2 Tool 词汇表 |
| `NativeToolExecutionService`（Policy 门闸 + timeout + cancel + audit + `ToolExecutionAuthority`） | 写权限只能来自 ChangeSet 的机制保留 |
| `AgentPolicyService` / `PolicyEngine`（policy_version + policy_hash + `current_matches`） | Safe Read 自动化的确定性判定源 |
| `ChangeSetService`（版本绑定审批 / precondition digest / FilePatch 真实回滚 / metadata-only 恢复） | V2 写路径不变 |
| `FleetExecutionService` | 多目标写内核 |
| Phase 10K：`ObservationCache` / `ContextBudget` / `snapshot` / `compact` / `redact_secrets` / `AgentRunMetrics` | V2 Context 层直接挂接 |
| Skills Registry / Incident 域数据与持久层 / 各 sanitized Audit | 不因 V2 削弱 |
| `ContextPanel` 布局壳 / Tabs / 显隐 Zustand store / `AgentHeader` / `AgentComposer` / `InlineChangeSetCard` / `AgentModelSettings` | UI 壳保留 |

### Adapt（逻辑保留，接 V2 Adapter）

| 模块 | Adapter 内容 |
|---|---|
| `agentic/runtime.rs` `AgentRuntimeService` | 循环骨架/`execute_reads`/budget 逻辑迁移为 `AgentController` 的内核；旧 doctor IPC 保留至 AR2-I 后清理 |
| `agentic/planning.rs` `AgentDecision` | 语义映射到 V2：`Answer→Final`、`Clarify→AskUser`、`ProposeChange→ProposeChangeSet`；`ToolCalls` 补 `reason_summary`/`expected_observation` |
| `agentic/model_gateway.rs` | 收敛 `decide`/`complete_agent_turn` 为统一 Reasoner Protocol；**为远程模型打开受校验的 tool-call 输出**；后续增加流式 |
| `agentic/incident.rs` Operations Packs | `plan_for` 固定序列降级为 Playbook / Skill 提示（初始 Tool 候选 + 领域知识），Agent 可依 Observation 改道 |
| MCP `read_context` | 收束为统一 Observation 源，评估经 `PolicyEvaluation(source=Mcp)`，消除旁路 |
| `ActivityTimeline` / `AgentStatus`（React） | 改为消费 `AgentEvent` 流 |
| `AgentPanel`（React） | 剥离编排（startPlan/continuePlan/planCommand/writeSsh），改为 事件订阅 + 用户动作转发 |
| `ai/chat.rs` 协议经验 | `parse_decision` / redaction 经验并入 V2 Reasoner Protocol；服务本身不保留为执行路径 |

### Deprecate（V2 Release Gate 后的 Cleanup PR 删除；本轮与 AR2-A 均不删）

| 模块 | 替代物 |
|---|---|
| 路径 A：`ai_propose_plan` / `AiService.plan` / `AiPlanProposal` / `complete_plan` / `parse_plan_lenient` / `exclude` | V2 迭代循环 |
| `AgentPlanCard` + `PlanCommand(State)` + `continuePlan` + Approve→`writeSsh` | `AgentTimeline` + Approval Card |
| 路径 C：`AiAgentService` 固定 step 队列 + `ai_agent_step_*` IPC | Typed Tool + ChangeSet 循环 |
| 路径 D：`ai_chat_*` freeform shell 执行链 | V2（保留协议经验） |
| 悬空 `AgenticWorkspaceView` / `LegacyWorkspace` 双轨 Agent UX | 右栏单一 Agent + Expand Focus Workspace |
| i18n：`contextPanel.plan*` 系列 key | 新 timeline / approval keys |

---

## 5. AgentRun State Machine（V2）

采用 `AGENT_RUNTIME_V2.md` §10 状态全集，与现有 `agentic::state::AgentRunState` 的映射及新增项：

```text
Created ──start──▶ Running
Running ⇄ Reasoning ⇄ Acting ⇄ Observing        （运行子态，循环内切换）
Reasoning ──AskUser──────────▶ AwaitingUser      （durable interrupt，新增）
Acting   ──ApprovalRequired──▶ AwaitingApproval  （durable interrupt，新增）
AwaitingUser / AwaitingApproval ──resume──▶ Running（同一 run_id）
Reasoning ──diagnosis──▶ Diagnosed ──▶ PlanningChange ──▶ AwaitingApproval
AwaitingApproval ──approve──▶ ExecutingChange ──▶ Verifying
Verifying ──ok──▶ Running│Completed；──fail──▶ Running（继续调查）│ RollingBack
RollingBack ──▶ Running │ Failed
任意非终态 ──pause──▶ Paused ──resume──▶ Running
任意非终态 ──cancel──▶ Cancelled
预算/超时/不可恢复 ──▶ Failed（终态）
终态：Completed / Failed / Cancelled
```

关键语义（与旧实现的差异）：

- **ToolCall Failed ≠ AgentRun Failed**：Tool 失败产生 `ToolFailed` 事件 + Observation，run 回到 `Reasoning`。旧 doctor 已部分符合（`ToolResult.success=false` 是结构化结果），V2 把它固化为状态机规则。
- **Reject 是 Observation**：`ApprovalRejected` → `ObservationAdded("User rejected TC-xxx")` → `Reasoning`，不进入 `Failed`。
- 旧状态映射：`GatheringContext/Investigating/Diagnosing → Running(Reasoning/Acting/Observing)`；`NeedsInput → AwaitingUser`；`PolicyBlocked/BudgetExceeded/TimedOut → Failed`（带稳定 error_code），`Succeeded → Completed`。
- 非法迁移返回稳定错误码（建议 `AGENT_INVALID_TRANSITION`），不 panic。
- 重启恢复：`AwaitingApproval` / `AwaitingUser` / `Paused` 原样恢复；恢复的 `Running` 保守转为 `Paused`（`Interrupted` 原因码），**绝不静默续跑**（对齐 ChangeSet/Incident 既有恢复语义）。

---

## 6. Agent Event Model

### 6.1 Envelope

```rust
struct AgentEventEnvelope {
    run_id: Uuid,
    seq: u64,          // 单 run 内单调递增，由 Controller 在 run 写锁内分配
    timestamp_ms: u64,
    event_type: AgentEventType,
    payload: serde_json::Value,   // 类型化 payload，serde tag
}
```

### 6.2 事件类型（对齐 `AGENT_RUNTIME_V2.md` §12 全集）

Run 生命周期：`RunCreated / RunStarted / RunResumed / RunPaused / RunCancelled / RunCompleted / RunFailed`
消息：`UserMessageAdded / AssistantMessageAdded`
推理：`ReasoningStarted / ProgressUpdated`（仅 concise progress summary，禁止 CoT）
Tool：`ToolRequested / ToolAutoAuthorized / ToolApprovalRequired / ToolStarted / ToolOutputChunk / ToolCompleted / ToolFailed`
观察：`ObservationAdded / FactsUpdated`
用户输入：`UserInputRequired / UserInputReceived`
审批：`ApprovalGranted / ApprovalRejected / ApprovalInvalidated`
诊断：`DiagnosisUpdated / RootCauseIdentified`
ChangeSet：`ChangeSetProposed / ChangeSetApproved / ChangeSetExecutionStarted / ChangeSetExecutionCompleted`
验证/回滚：`VerificationStarted / VerificationCompleted / RollbackStarted / RollbackCompleted`

### 6.3 Ordering / Streaming / Persistence

- **Ordering**：`seq` 由 Controller 分配，持久化与 Channel 发送共用同一分配点；React 按 `(run_id, seq)` 排序渲染，重复 seq 幂等丢弃。
- **Streaming**：`agent_run_subscribe(run_id, after_seq, Channel<AgentEventEnvelope>)` —— 订阅时先回放 `> after_seq` 的已持久化事件，再实时追加。不重发全量运行时状态（对齐 `AGENT_RUNTIME_V2.md` §23）。Terminal Output 仍走独立 Channel，两条流不合并。
- **Persistence**：事件先落 `agent_events`（与 run 状态更新同事务），后发 Channel；崩溃后 UI 通过回放重建 Timeline。`ToolOutputChunk` 只持久化 sanitized 摘要引用（大输出进 ToolArtifact，AR2-H）。
- 载荷红线：事件 payload 经 `redact_secrets`；绝不包含 Credential / Private Key / Passphrase / Vault Secret / 模型隐藏推理。

---

## 7. AgentDecision Schema

### 7.1 V2 协议

```rust
enum AgentDecision {
    ToolCalls(Vec<ToolCallRequest>),   // 一轮可多个独立安全读
    AskUser(UserQuestion),
    ProposeChangeSet(ChangeProposal),  // 只产草稿，写入仍走 ChangeSetService
    Final(FinalResponse),
}

struct ToolCallRequest {
    tool_name: String,          // 必须命中 NativeToolName 封闭枚举
    arguments: serde_json::Value,
    target_ids: Vec<Uuid>,
    reason_summary: String,     // 展示用 progress summary，非 CoT
    expected_observation: String,
}
```

模型声明的 `risk` / `safe` / `approval_required` **一律不采信**；Rust 用 `ToolDescriptor` + `AgentPolicyService` 确定性重算（对齐现有 `validate_plan` 重算 risk、`from_model_read_call` 拒绝写 Tool 的做法）。

### 7.2 现有模型调用如何迁移

| 现状 | 迁移 |
|---|---|
| `planning::AgentDecision::{ToolCalls, Answer, Clarify, ProposeChange}` | 已同构：`Answer→Final`、`Clarify→AskUser`、`ProposeChange→ProposeChangeSet`；`ModelToolCall` 扩展为 `ToolCallRequest` |
| `model_gateway.decide`（远程禁 tool） | **移除“禁止 tool calls”限制**，System Prompt 改为输出 V2 JSON 决策（tool_calls / ask_user / propose_change_set / final），沿用 `response_format: json_object` + lenient 解析 + 结构化校验；Local provider 继续走 `local_turn` 启发式作为离线 fallback |
| `complete_agent_turn`（chat 单条 propose） | 协议合并进 `decide`；freeform shell propose 替换为 Typed ToolCall（无 Typed Tool 时后续经 `terminal.exec_readonly` fallback，AR2-F 之后） |
| `complete_plan`（整包 commands） | 不迁移，随路径 A 废弃 |
| 校验 | `agent/decision.rs` 做结构校验：tool 必须在 Registry、参数过 input_schema、每轮 ToolCalls ≤ `MAX_TOOL_CALLS_PER_TURN`、非法决策 → 记录事件并要求模型重试（计入预算） |

---

## 8. Interrupt / Resume Design

### 8.1 ApprovalRequired 中断（AR2-C）

现状缺口：不存在独立 `ApprovalRequest` 对象（审批状态内嵌在 ChangeSet），也没有可恢复中断态。V2 新增：

```rust
struct ApprovalRequest {
    id: Uuid,
    run_id: Uuid,
    subject: ApprovalSubject,      // ToolCall(tool_call_id) | ChangeSet(change_set_id, version)
    tool_name: String,
    arguments_hash: String,        // SHA-256(canonical json)，新增（现状没有）
    target_ids: Vec<Uuid>,
    risk: RiskLevel,
    policy_version: u64,           // 复用 PolicyEngine::seal
    policy_hash: String,
    precondition_ref: Option<String>, // 复用 ChangeSet precondition digest
    state: Pending | Granted | Rejected | Invalidated,
    created_at, decided_at,
}
```

流程：

```text
Reasoner 请求 R2+/写动作
  → Policy 判定 RequireApproval
  → 创建 ApprovalRequest + AgentCheckpoint（同事务持久化）
  → emit ToolApprovalRequired / ChangeSetProposed
  → run: Running → AwaitingApproval（停止调度该风险动作及后续轮次）
用户 Approve（agent_run_approve(run_id, approval_id)）
  → 校验 binding（arguments_hash / targets / policy current_matches / precondition digest 重查，绕过 cache）
  → 任一变化 → ApprovalInvalidated → 回 Reasoning 重新规划
  → 校验通过 → 执行精确批准动作 → ToolResult → Observation → Resume 同一 run
用户 Reject
  → ApprovalRejected → ObservationAdded("User rejected …") → Reasoning 继续（找替代 / 提问 / 说明受限）
```

ChangeSet 类审批直接复用 `ChangeSetService.approve(id, version)` 及其失效机制，`ApprovalRequest` 是它在 AgentRun 内的镜像引用，不产生第二个审批真相源。

### 8.2 AskUser 中断

`AgentDecision::AskUser` → emit `UserInputRequired` → `AwaitingUser` + Checkpoint。`agent_run_reply(run_id, text)` → `UserInputReceived` 事件 → 同一 run 回 `Reasoning`。替换现状「Clarify 后前端拼文本开新 run」。

### 8.3 Checkpoint 内容（AR2-C 定义，AR2-D 落盘）

```text
run_id / event_cursor(seq) / pending_interrupt(approval_id 或 question)
working_fact_snapshot / context_reference / budget_state
policy_snapshot(version+hash) / change_set_reference
```

禁止项沿用 SECURITY.md：无 Credential、无私钥、无 passphrase、无 Vault/云密钥、无模型隐藏推理。

---

## 9. Persistence Design（AR2-D）

### 9.1 选型

新增依赖 `rusqlite`（bundled feature，无系统依赖，桌面+移动均可编译）。理由（对照 AGENTS.md §18）：AgentRun 的「run 状态 + 事件 + checkpoint + approval 必须同事务一致」是 JSON Repository（整文件原子重写）无法胜任的；`ARCHITECTURE.md` §27 已明确建议 Agent 复杂状态用 SQLite。Host/Profile 等既有 JSON Repository 不迁移。

### 9.2 Schema（`runory-agent.db`）

```sql
CREATE TABLE schema_version (version INTEGER NOT NULL);

CREATE TABLE agent_runs (
  id TEXT PRIMARY KEY, goal_summary TEXT NOT NULL,      -- redacted
  target_profile_ids TEXT NOT NULL,                     -- json array
  state TEXT NOT NULL, failure_code TEXT,
  budget_json TEXT NOT NULL, metrics_json TEXT,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);

CREATE TABLE agent_events (
  run_id TEXT NOT NULL REFERENCES agent_runs(id),
  seq INTEGER NOT NULL, timestamp_ms INTEGER NOT NULL,
  event_type TEXT NOT NULL, payload TEXT NOT NULL,      -- sanitized json
  PRIMARY KEY (run_id, seq)
);

CREATE TABLE agent_messages (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL, role TEXT NOT NULL,
  content TEXT NOT NULL,                                 -- redacted; assistant 仅 progress/final
  created_at INTEGER NOT NULL
);

CREATE TABLE tool_calls (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL,
  tool_name TEXT NOT NULL, sanitized_arguments TEXT NOT NULL,
  arguments_hash TEXT NOT NULL, target_ids TEXT NOT NULL,
  risk TEXT NOT NULL, authorization TEXT NOT NULL,       -- auto | approval:<id>
  state TEXT NOT NULL, requested_seq INTEGER NOT NULL
);

CREATE TABLE tool_results (
  tool_call_id TEXT PRIMARY KEY REFERENCES tool_calls(id),
  success INTEGER NOT NULL, error_code TEXT,
  sanitized_summary TEXT NOT NULL, duration_ms INTEGER NOT NULL,
  artifact_ref TEXT, completed_at INTEGER NOT NULL
);

CREATE TABLE observations (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL,
  source_event_seq INTEGER NOT NULL, kind TEXT NOT NULL,
  sanitized_content TEXT NOT NULL, created_at INTEGER NOT NULL
);

CREATE TABLE approval_requests (
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL,
  subject_kind TEXT NOT NULL, subject_id TEXT NOT NULL, subject_version INTEGER,
  tool_name TEXT NOT NULL, arguments_hash TEXT NOT NULL,
  target_ids TEXT NOT NULL, risk TEXT NOT NULL,
  policy_version INTEGER NOT NULL, policy_hash TEXT NOT NULL,
  precondition_ref TEXT, state TEXT NOT NULL,
  created_at INTEGER NOT NULL, decided_at INTEGER
);

CREATE TABLE agent_checkpoints (
  run_id TEXT PRIMARY KEY REFERENCES agent_runs(id),
  event_cursor INTEGER NOT NULL, pending_interrupt TEXT,
  fact_snapshot TEXT NOT NULL, budget_state TEXT NOT NULL,
  policy_snapshot TEXT NOT NULL, change_set_ref TEXT,
  updated_at INTEGER NOT NULL
);

-- AR2-H 追加：working_facts / tool_artifacts
```

### 9.3 Versioning / 事务边界 / 重启恢复

- `schema_version` + 顺序 migration；migration 失败 fail-closed，保留原文件（对齐 Incident v1→v2 迁移纪律）。
- 事务边界：`(run 状态变化 + 对应事件 + checkpoint + approval 状态)` 单事务；事件 append 成功后才向 Channel 发送。
- 重启恢复：启动扫描非终态 run → `AwaitingApproval`/`AwaitingUser`/`Paused` 恢复原态；`Running/Acting` 保守转 `Paused(Interrupted)`；**启动时绝不自动执行任何 pending 动作**；恢复的 Approval 在使用前必须重新通过 binding + precondition + `policies.current_matches` 校验，否则 `ApprovalInvalidated`。
- 旧数据迁移压力为零：现状 `AgentRun` 本就不落盘，无历史 run 需要转换；ChangeSet / Incident JSON 文件保持原格式不动。
- 禁止持久化：SSH password / 私钥 / passphrase / Vault Secret / API Secret / 模型隐藏推理（复用 `redact_secrets` + 字段级不序列化双保险）。

---

## 10. Context Integration（复用 Phase 10K，不建第二套）

| 10K 能力 | V2 挂接点 |
|---|---|
| `ContextBudget` / `snapshot()`（item/byte/token 三重预算） | `AgentController` 每轮 Reasoning 前构建模型上下文的唯一入口 |
| `compact()` → `ContextFact` | 长 run 压缩；`WorkingFact`（AR2-H）作为 `ContextFact` 的超集扩展（+ `source_event_id` / `target_id` / `observed_at` / `freshness` / `provenance`），不另起炉灶 |
| `ObservationCache`（target+source TTL；写后 `invalidate_target`） | `execute_reads` 原样复用；ChangeSet precondition / Verification 继续绕过 cache（既有不变式） |
| dedup（`duplicate_calls`） | 循环内重复读判定复用 |
| `redact_secrets` | 事件 payload、持久化、模型上下文三处统一调用 |
| `AgentRunMetrics` | 扩展 V2 观测字段（reasoner_rounds / approval_interrupts / rejections / recovery_attempts / time_to_first_observation 等） |
| `ModelProvider / ModelCapability` 路由 | Reasoner adapter 复用，不改变 Tool 权限 |

每轮上下文组成（对齐 V2 §17）：Goal + Target Context + 相关对话 + Working Facts + 最近 Observation + 活动 Incident/ChangeSet + Policy 摘要 + 可用 Tool Schema + 预算状态 + Artifact 引用。不注入完整终端历史 / 完整日志。

修复一个现状缺口：目前 `ObservationCache` 只被 Doctor/Incident 使用，主路径 A/C/D 全部旁路——V2 收敛后所有读观察自然进入统一缓存边界。

---

## 11. Tool / Policy Integration（Safe Read 自动执行）

### 11.1 进入循环的三级通道

```text
Typed Tool（NativeToolName 封闭枚举，22 个，优先）
  ↓ 无匹配
terminal.exec_readonly（尚不存在——AR2-F 新增，需命令策略筛查 + R1/R2）
  ↓
terminal.exec（现状仅 Phase 8 预设枚举；V2 中保持最高限制，默认审批）
```

### 11.2 自动执行判定（全部在 Rust）

```text
ToolCallRequest
  → from_model_read_call（写 Tool 直接拒绝，只能走 ProposeChangeSet）
  → AgentPolicyService.evaluate（PolicyDecision）
      Allow            → emit ToolAutoAuthorized → 执行（R0/R1 默认命中既有规则）
      RequireApproval  → ApprovalRequest + AwaitingApproval（R2 视 Policy，R3/R4 必审批）
      Deny             → emit 事件 + Observation（policy-denied），run 继续 Reasoning
  → NativeToolExecutionService（ToolPolicy + timeout + cancel + audit）
```

模型/React 均无权声明 safe；判定输入只有 `ToolDescriptor.risk_level/mutability` + PolicySet。同轮多个无依赖只读 Tool 复用 `execute_reads` 的同 Risk 边界并行调度。

### 11.3 现状 Tool 缺口（AR2-F 范围，本轮不实现）

- 缺 `filesystem.inode_usage`（df -i）、`block_devices.list`（lsblk）；`filesystem.top_consumers` 可由既有 `system.directory_usage`（du）+ `system.large_files`（find -size）归并覆盖。
- `ToolDescriptor` 无 resource impact 元数据 → 新增 `impact: Low|Medium|HighIO|HighCPU|LongRunning|ExternalCost` 字段，Policy 可据此限制高 IO 扫描。
- 无独立 Tool 错误分类 → 在 ToolResult 错误码之上定义映射（`NotFound / CommandFailed / Timeout / PermissionDenied / ConnectionLost / Unsupported / MalformedResult`），复用既有 `EXEC_TIMED_OUT` / `SFTP_NOT_FOUND` / exit 90–92 约定，供 Observation 结构化表述。

---

## 12. ChangeSet Integration（AR2-G）

写路径完全复用既有链路，V2 只增加事件与回环：

```text
AgentDecision::ProposeChangeSet
  → Rust 校验 evidence / target / risk / policy（复用 planning.rs 既有校验）
  → ChangeSetService.draft（版本化）→ emit ChangeSetProposed
  → ApprovalRequest(subject=ChangeSet(id, version)) → AwaitingApproval + Checkpoint
Approve
  → ChangeSetService.approve(id, version)（policy_snapshot / step 审批语义不变）
  → execute：Executing 声明先落盘 → capture 的 precondition digest 绕过 cache 重查
     → 变化 → ApprovalInvalidated → 回 Reasoning 重新诊断（不自动覆盖）
  → 执行步骤事件：ChangeSetExecutionStarted/Completed
  → Verification（继续强制；exit 0 ≠ resolved）→ VerificationStarted/Completed
     → 失败 → Observation → 继续调查或 propose Rollback（RollbackStarted/Completed）
  → 成功 → invalidate_target（ObservationCache）→ Observation 回 Reasoner → 继续/Complete
```

不变式保持：写 Tool 无 `ToolExecutionAuthority` 必然 `Blocked`；审批绑定 exact version；revise/重启即失效；Fleet 写入经 `FleetExecutionService` 且默认 Sequential + Pause for Review；Rollback 只在真实支持时声明。**V2 不新增任何写旁路**（同时废弃路径 A 的 `writeSsh` 写旁路——这是 V2 使安全性净提升的一点）。

---

## 13. UI Migration Plan（AR2-E）

### 13.1 目标结构（右栏保持 `[ AI Agent ] [ Inspect ]`）

```text
ContextPanel（保留；宽度调整为 default 420 / min 360 / max 600，现为 360/320/480）
└── AgentPanel（改造为纯渲染 + 动作转发）
    ├── AgentHeader          保留（+ run 状态徽标 / Pause / Stop / Expand）
    ├── AgentTimeline        新增（由 AgentConversation 演化，按 AgentEvent 渲染）
    │   ├── UserMessageEvent / ProgressEvent（紧凑 ●/✓ 行）
    │   ├── ToolEvent（紧凑行，可展开 tool/耗时/sanitized result）
    │   ├── ApprovalEvent（大卡：risk · 动作 · why · verification · Reject/Approve）
    │   ├── ObservationEvent / DiagnosisEvent
    │   ├── ChangeSetEvent（复用 InlineChangeSetCard，事件驱动）
    │   ├── VerificationEvent / FinalEvent
    │   └── CurrentApproach（可折叠进度表示，非执行队列）
    └── AgentComposer        保留（Enter 发送 / Shift+Enter 换行）
```

### 13.2 组件处置

| 处置 | 组件 |
|---|---|
| 保留 | `ContextPanel` / `ContextPanelTabs` / `context-panel-store` / `AgentHeader` / `AgentComposer` / `InlineChangeSetCard` / `DiagnosisCard` / `EvidenceList` / `ProposedFixCard` / `AgentModelSettings` |
| 改造 | `AgentPanel`（剥离编排）、`AgentConversation → AgentTimeline`（事件渲染）、`ActivityTimeline` / `AgentStatus`（事件驱动）、`agent-state.ts`（换为 Run/Event 视图模型 + 新 `agentViewStore` 仅存展示态：展开项/滚动位置/面板宽度/草稿） |
| 废弃 | `AgentPlanCard` 整卡、`PlanCommand(State)`、`continuePlan` / `startPlan` / `planCommand` / Approve→`writeSsh`、`AiAgentView` plan 模式队列、悬空 `AgenticWorkspaceView` 双轨、`AgentChatView`（保留为参考或并入 timeline 后删除） |
| 新增 | `AgentTimeline` + events/* 渲染件、`agent_run_subscribe` 前端绑定（`Channel<AgentEventEnvelope>` + after_seq 回放）、Expand → Agent Focus Workspace（同一 AgentRun） |

### 13.3 行为规则

- 面板折叠 / 切到 Inspect / 应用重启 均不终止 AgentRun（订阅重放恢复 Timeline）；
- Resize 继续触发 `fitTerminals()`；
- 删除 `contextPanel.plan*` 主交互 i18n key，新增 timeline / approval keys（zh-CN + en-US 同步）；
- React 不再持有：命令队列、planBusy 调度锁、60s 编排超时、意图路由启发式（`inferDiagnosticInputs` 移入 Rust 或废弃）。

---

## 14. Backward Compatibility / Migration Risks

| 风险 | 评估 | 缓解 |
|---|---|---|
| 旧 persisted runs | **无风险**：AgentRun 从未持久化 | 无需数据迁移 |
| 旧 ChangeSet / Incident / Fleet JSON | 保持原格式与恢复语义 | V2 只读引用，不改 schema |
| 四路径共享 `ModelGateway` | 改 `decide` 协议可能影响 Doctor/chat 现行为 | Reasoner adapter 走新方法/版本化 prompt，旧方法在清理 PR 前保持不动 |
| 未提交 WIP（ai/chat 等 +3700 行） | 与 V2 轨道方向部分重叠，易造成第五条路径 | 建议先决策：提交并冻结 或 收纳入 AR2 分支；AR2-A 前明确基线 |
| 前端状态迁移 | `ServerSessionModel` useState 与事件流模型不兼容 | AR2-E 一次性替换渲染层；期间旧面板保持可用（adapter/feature flag） |
| 审批双层语义（AgentPolicy `RequireApproval` vs ToolPolicy Blocked） | 语义漂移风险 | V2 明确：AgentPolicy 决定是否进审批 UI；ToolPolicy + Authority 决定能否执行；文档化于 approval.rs |
| Approval binding 增强（新增 arguments_hash） | 与现有 ChangeSet version 绑定叠加 | ApprovalRequest 是镜像引用，ChangeSet 仍是唯一写审批真相源 |
| 多目标安全 | Fleet 约束不得因 V2 放松 | Fleet 只经 ChangeSet 集成，Production Parallel All 拒绝规则不动 |
| i18n key 删除 | `contextPanel.plan*` 被引用于测试/文档 | 清理 PR 统一处理，AR2-E 前不删 |
| MCP 旁路收束 | `read_context` 行为变化可能影响 Doctor 外部上下文 | adapter 保持只读 + 逐项启用语义，仅补 Policy 评估记录 |
| SQLite 新依赖 | 移动端编译 / 维护面 | `rusqlite` bundled 纯 C 内嵌，五平台可编译；仅 agent 域使用，不动既有 Repository |
| 旧 IPC 兼容 | `ai_propose_plan` / `ai_agent_*` / `ai_chat_*` 在 AR2-E 完成前仍被 UI 使用 | 全程保留至 Release Gate 后的 Cleanup PR |

---

## 15. Test Gaps & Test Plan

### 15.1 现状缺口

- `agentic/runtime.rs` 与 `agentic/state.rs` **零测试**（循环、预算、状态迁移全裸奔）。
- 无状态机迁移测试、无事件排序测试、无 interrupt/resume 测试、无重启恢复测试、无 approval-binding-mutation 测试（ChangeSet version 层有，ToolCall 层无）。
- 前端约 19 个测试全为纯函数（`inferDiagnosticInputs` 等）；`AgentPanel` / `AgentPlanCard` / `AgentConversation` / `AgentChatView` 无组件测试；无 `vite.config.ts` test 段。
- 集成层：Docker OpenSSH fixture 覆盖 Tool/ChangeSet/Incident，但无「迭代循环 + tool 失败改道 + 审批恢复」端到端场景。

### 15.2 V2 必备测试（对齐 `AGENT_RUNTIME_V2.md` §37）

```text
unit/        runtime_state_machine · event_ordering · decision_validation
             approval_binding(arguments/target/policy mutation) · checkpoint_serialization · fact_freshness
integration/ iterative_read_loop · tool_failure_recovery(NotFound→改道) · approval_interrupt_resume
             rejection_continue · ask_user_resume · changeset_verify · restart_recovery(AwaitingApproval)
security/    no_secret_checkpoint · no_policy_bypass · no_skill_bypass · no_mcp_bypass
             approval_argument_mutation · stale_precondition
ui/          timeline_render · approval_card · panel_resize · panel_collapse_run_survives
             reconnect_state · no_continue_next_step
```

AR2-A 先交付 unit 前两类（state machine + event ordering + serialization）。

---

## 16. AR2-A Exact Scope（原始计划；实施结果见 §19，个别细节以 §19 为准）

**目标**：在不改变任何现有 Agent 行为的前提下，落地 V2 的两份契约——AgentRun 状态机与 AgentEvent 模型。

**做**：

1. `AgentRunStateV2` 权威枚举（§5 全集，经实际计数为 16 态，早期版本误写 17）+ 显式合法迁移表 + 终态/中断态判定 + 非法迁移返回稳定错误码 `AGENT_INVALID_TRANSITION`（typed error，不 panic）。
2. `AgentRun` V2 领域对象（id / goal_summary / target_profile_ids / state / failure_code / budget 引用 / created_at / updated_at）。
3. `AgentEventEnvelope`（run_id / seq / timestamp / event_type / payload）+ §6.2 全部事件类型 + serde 序列化契约（camelCase，与前端未来共享）。
4. 单 run 内 `seq` 单调分配器（写锁内分配，可注入时钟便于测试）。
5. `AgentEventRepository` / `AgentRunStore` trait + InMemory 实现（SQLite 属 AR2-D）。
6. 单元测试：合法迁移全覆盖 / 非法迁移拒绝 / 终态不可再迁移 / 中断态可恢复语义 / seq 单调与并发分配 / envelope serde 往返 / payload 禁 secret 字段结构约束。
7. `lib.rs` 仅声明 `mod agent;`——不注册任何 Tauri command、不 manage state、不接 UI。

**不做**（AR2-B+）：Reasoner 循环、Tool 调度、自动安全读、审批 Resume、SQLite、Timeline UI、ChangeSet 变更、旧路径删除。

**门禁**：`cargo fmt` / `cargo clippy` / `cargo test` 通过；现有全部测试不回归；前端零改动。

---

## 17. AR2-A Exact File Plan

### 新增

```text
src-tauri/src/agent/mod.rs
src-tauri/src/agent/state.rs        AgentRunStateV2 + transitions + errors（含 #[cfg(test)]）
src-tauri/src/agent/run.rs          AgentRun V2 domain（含 #[cfg(test)]）
src-tauri/src/agent/event.rs        AgentEventType + Envelope + seq 分配（含 #[cfg(test)]）
src-tauri/src/agent/repository.rs   RunStore / EventRepository trait + InMemory（含 #[cfg(test)]）
```

### 修改（最小侵入）

```text
src-tauri/src/lib.rs                仅追加 `mod agent;` 模块声明
```

### AR2-A 明确不能碰

```text
src-tauri/src/agentic/*             （runtime / planning / model_gateway / state / changes / incident / fleet / optimization / context）
src-tauri/src/ai/*                  （service / agent_service / chat / policy / tools）
src-tauri/src/commands/*            （不新增/不修改任何 IPC）
src-tauri/src/tools/* · policy/* · mcp/* · skills/* · ssh/* · credentials/*
src/**                              （全部前端，含 AgentPanel / AgentPlanCard / i18n）
src-tauri/Cargo.toml                （SQLite 依赖属 AR2-D，AR2-A 不加依赖）
```

---

## 18. Documentation Changes（本轮已执行）

1. 新建本文件 `docs/agent-runtime-v2-audit.md`。
2. `AGENT_RUNTIME_V2.md` 追加 “Appendix A — Current Code Reality (2026-09 Audit)”：记录四路径现状、Registry/Policy/ChangeSet 可复用结论、Tool 与持久化缺口，指向本审计文档。未修改任何稳定业务代码，也未为匹配文档而改代码。

---

## 19. AR2-A Implementation Result（已实施，代码验证事实）

实施日期：2026-09-02。以下内容全部经编译与单元测试验证，是后续 AR2-B+ 的真实契约基线。

### 19.1 实际文件

```text
新增  src-tauri/src/agent/mod.rs           模块入口 + 公开 re-export
新增  src-tauri/src/agent/state.rs         AgentRunStateV2 + 迁移规则 + AgentStateError
新增  src-tauri/src/agent/run.rs           AgentRun V2 领域对象 + seq 分配
新增  src-tauri/src/agent/event.rs         AgentEvent（35 类）+ AgentEventEnvelope
新增  src-tauri/src/agent/repository.rs    AgentRunStore / AgentEventRepository trait + InMemory 实现
修改  src-tauri/src/lib.rs                 仅追加 3 行 `pub mod agent;` 声明（含注释）
```

未触碰：`agentic/*`、`ai/*`、`commands/*`、`tools/*`、`policy/*`、`mcp/*`、`skills/*`、`ssh/*`、`Cargo.toml`、`Cargo.lock`、全部前端。旧 Agent 四路径行为零变化，Runtime V2 尚无任何生产调用方。

### 19.2 状态机（16 态，实际计数）

`AgentRunStateV2`：`Created / Running / Reasoning / Acting / Observing / AwaitingApproval / AwaitingUser / Diagnosed / PlanningChange / ExecutingChange / Verifying / RollingBack / Paused / Completed / Failed / Cancelled`。

分类（每个状态恰属一类，有测试保证）：

- Terminal：`Completed / Failed / Cancelled`，无任何出边。
- Interrupt（可恢复，`can_resume() == is_interrupt()`）：`AwaitingApproval / AwaitingUser / Paused`。
- Active：其余 9 态；`Created` 单独为初始态。

超出任务规范最小集的迁移决策（均取更严格/更安全方向）：

1. `Cancelled` / `Failed` 从所有非终态统一可达（用户 Stop / 不可恢复运行时错误）。
2. `Paused` 仅可从读/推理阶段进入（`Running / Reasoning / Acting / Observing / Diagnosed / PlanningChange`）；`ExecutingChange / Verifying / RollingBack` 进行中的副作用必须完成或失败，禁止中途暂停。
3. 增加 `AwaitingApproval → ExecutingChange`：已审批 ChangeSet 直接恢复进入执行（对应本审计 §8/§12 流程图）。
4. 非法迁移返回 typed `AgentStateError { from, to }`，稳定码 `AGENT_INVALID_TRANSITION`，不 panic、不静默。

### 19.3 AgentRun 领域对象

字段（全部私有，序列化协议共 5 个稳定字段）：`id: Uuid / state / created_at_epoch_ms / updated_at_epoch_ms / next_event_seq`。§16 计划中的 `goal_summary / target_profile_ids / budget` 等按任务规范“只建立稳定核心”原则推迟到需要它们的 AR2 阶段。

领域行为：`transition_to(next)` 强制走迁移校验（字段私有使 `run.state = …` 不可能）；`next_event_sequence()` 从 1 起按 run 单调分配（`&mut self` 独占 + 饱和加法），时间戳单调不回退。

### 19.4 事件契约（35 类，实际计数）

`AgentEvent` 为封闭 typed enum（35 个变体，非 String type + untyped JSON）；serde 生成稳定线格式 `{"type":"...","payload":{...}}`。**序列化命名为 snake_case**（如 `awaiting_approval`、`tool_completed`），此处修正 §16 早期计划中的 camelCase 描述——AR2-A 任务规范明确要求 snake_case 作为持久化/IPC 协议。

`AgentEventEnvelope { run_id, seq, timestamp_epoch_ms, event }`；`seq` 是唯一排序键，时间戳不参与排序。Payload 最小化：id 用 `Uuid`，摘要用用户可见 `summary`；无 `chain_of_thought` 等隐藏推理字段（有序列化测试守护）。

示例：

```json
{"run_id":"…","seq":7,"timestamp_epoch_ms":1725000000000,
 "event":{"type":"progress_updated","payload":{"summary":"Checking nginx service logs"}}}
```

### 19.5 Repository 契约

- `AgentRunStore`：`insert`（拒绝重复 id，`AGENT_RUN_ALREADY_EXISTS`）/ `get` / `save`（拒绝未知 run，`AGENT_RUN_NOT_FOUND`，杜绝静默 upsert）。
- `AgentEventRepository`：`append`（同 run `seq` 必须严格递增，违规拒绝 `AGENT_EVENT_SEQUENCE_INVALID`，绝不重排）/ `events_after(run_id, seq)`（仅返回 `seq >` 的有序尾部）/ `all_events`。
- `InMemoryAgentRunStore` / `InMemoryAgentEventRepository`：`RwLock<HashMap>` 线程安全；锁中毒 fail-closed（`AGENT_STORE_UNAVAILABLE`）。SQLite（AR2-D）实现相同 trait。

### 19.6 测试与门禁（实际执行结果）

- `cargo fmt --check`：通过。
- `cargo clippy --all-targets --all-features -- -D warnings`：`agent/` 模块 0 告警；全仓失败 5 处，全部位于开发前已存在的未提交 WIP（`ai/agent_service.rs:286`、`ai/service.rs:199-200`、`agentic/model_gateway.rs:728`、`lib.rs` DragDrop 块），属基线问题，AR2-A 按规则未修改这些文件。
- `cargo test --lib`：209 通过 / 0 失败 / 16 ignored；其中 `agent::` 新增 36 个测试（state 15 / run 8 / event 6 / repository 7），覆盖初始态、合法/非法迁移、终态封锁、中断往返、Tool 失败回路、seq 单调与并发、乱序拒绝、serde 往返与稳定命名、CoT 字段禁令、多 run 隔离。

### 19.7 与规范文档的已验证差异

1. 状态实际计数 16（本文早期版本与口头描述中的“17 态”为计数笔误，语义无变化）。
2. 事件类型实际计数 35（AR2-A 任务规范 §13 列表逐一核对）。
3. V2 协议序列化采用 snake_case（覆盖 §16 计划中 camelCase 的早期设想）。

---

## 20. AR2-B Implementation Result（已实施，代码验证事实）

实施日期：2026-09-02。只读迭代循环（`CODEX_AGENT_RUNTIME_V2.md` §4）已落地并通过门禁：**真实 ToolResult 能改变 Agent 下一步动作**（测试 `tool_result_changes_the_next_action`：同一 Reasoner 面对“磁盘 94%”与“磁盘 12%”两种观察产生不同后续动作）。

### 20.1 实际文件

```text
新增  src-tauri/src/agent/decision.rs    AgentDecision 协议 + 结构化校验 + PreparedToolCall
新增  src-tauri/src/agent/reasoner.rs    Reasoner trait + ReasonerInput/Observation/BudgetStatus
新增  src-tauri/src/agent/dispatch.rs    ToolDispatcher trait + ToolOutcome + NativeReadDispatcher
新增  src-tauri/src/agent/controller.rs  AgentController 迭代循环 + RunBudget + RunOutcome
修改  src-tauri/src/agent/mod.rs         模块注册与 re-export
修改  src-tauri/src/agent/run.rs         now_epoch_ms 放宽为 pub(super)（1 行）
```

`agentic/*`、`ai/*`、`tools/*`、`policy/*`、`commands/*`、`ssh/*`、`Cargo.toml/lock`、全部前端继续零修改；旧四路径行为不变，V2 仍无生产调用方（无 IPC、无 UI、无 LLM 接线）。

### 20.2 AgentDecision 协议与校验（decision.rs）

- `AgentDecision::{ToolCalls(Vec<ToolCallRequest>), AskUser{question}, Final{summary}}`；serde 采用与事件契约一致的 `type`/`payload` snake_case。按 §4 规范，AR2-B 刻意没有 `ProposeChangeSet` 变体。
- 模型声明的 risk/safe/approval **一律不采信**。校验双层强制只读：Rust 侧 `ToolDescriptor.mutability == Read`（权威）+ `NativeToolInvocation::from_model_read_call`（结构上不存在写分支）。任一层回归都无法悄悄打开写路径。
- 其余规则：tool 名必须命中封闭 Registry；每轮 ≤ `MAX_TOOL_CALLS_PER_TURN = 4`；`reason_summary`（≤300B）/question（≤2KB）/final（≤16KB）trim + `redact_secrets` + 非空。
- 非法决策返回 typed `DecisionError`，稳定码 `AGENT_DECISION_INVALID`；错误本身不携带模型可控文本。

### 20.3 AgentController 循环（controller.rs）

`Context → Reasoner → AgentDecision → 校验 → 只读 Tool → ToolOutcome → Observation → Context → …`

- **Tool 失败 = Observation**：`ToolFailed` 事件 + Observation 回灌 Reasoner，run 不失败（测试：service.logs 失败后改用 docker.logs 并 Completed）。
- **非法决策 = Observation**：记 `ObservationAdded` + 消耗一轮预算后继续（模型可重试）；被拒决策永不到达 dispatcher（测试用 panic dispatcher 守护）。
- **AskUser → `AwaitingUser`**：durable interrupt，run 持久化于该态、`can_resume()==true`；Resume 引擎属 AR2-C，AR2-B 在中断边界停止。
- **预算与取消**（Rust 确定性执行，Reasoner 只能看到不能改）：`RunBudget{max_reasoner_rounds:8, max_tool_calls:20, time_budget_ms:300_000}`；耗尽 → `RunFailed`，稳定码 `AGENT_BUDGET_EXCEEDED` / `AGENT_TIME_BUDGET_EXCEEDED`；取消经 `watch::Receiver<bool>` → `Cancelled`。
- 每轮上下文经 Phase 10K `agentic::context::snapshot`（goal 走 `user_context` 红act，观察以 `UntrustedRemoteData` 信任级进入），不存在第二套 context 系统（测试 `reasoner_receives_the_phase_10k_context_snapshot`）。
- 事件按 AR2-A 契约发射并连续编号（RunCreated → UserMessageAdded → RunStarted → ReasoningStarted → ProgressUpdated/ToolRequested/ToolAutoAuthorized/ToolStarted → ToolCompleted|ToolFailed → ObservationAdded → … → RunCompleted|RunFailed|RunCancelled|UserInputRequired），`seq` 无缝隙（测试守护）。
- 状态轨迹全程走 AR2-A 迁移表：`Created→Running→Reasoning→Acting→Observing→Reasoning→…`；AR2-B 结构上到不了 `AwaitingApproval`（无写路径）。

### 20.4 Reasoner 与 Dispatcher 边界

- `Reasoner` trait（async-trait）：输入 goal/round/observations/budget + Phase 10K `ContextSnapshot`；输出未验证 `AgentDecision`。**ModelGateway 适配器有意延后**：现行远程 `decide()` 的 System Prompt 禁止 tool calls，升级协议必须改 `agentic/model_gateway.rs`（冻结中的未提交 WIP），适配器与 V2 模型协议阶段一起交付。
- `ToolDispatcher` trait：每个 call 恰好一个 `ToolOutcome`（sanitized DTO，legacy `ToolResult` 不泄入 V2 公共契约）。
- `NativeReadDispatcher`（生产实现，暂 dormant，比照 `tools` 模块先例标注）：Phase 10K `ObservationCache` get → 真实 `NativeToolExecutionService::execute`（Policy→Risk→Audit 原样在内）→ 成功后 cache put；批内相同 invocation 只执行一次（10K dedup 语义），独立读经 `join_all` 并行。缺 session / policy 拦截都是结构化失败 Observation，不 panic（测试验证：cache 命中 + 批内去重 + `SESSION_NOT_FOUND` 结构化失败）。

### 20.5 测试与门禁（实际执行结果）

- `cargo fmt --check` 通过；`cargo test --lib` **232 通过 / 0 失败 / 16 ignored**（AR2-B 新增 23：decision 7 / dispatch 2 / controller 14），覆盖 §4 要求全部场景：两轮诊断、ToolResult 改变下一步（门禁）、Tool 失败换替代工具、AskUser 达到可恢复中断、直接 Final、取消、三种预算耗尽、非法决策恢复、Reasoner 故障、事件 seq 连续、10K cache/dedup 兼容、Phase 10K 上下文送达。
- `cargo clippy --all-targets --all-features -- -D warnings`：`agent/` 模块 0 告警；全仓仍余 5 处既有 WIP 基线错误（同 §19.6，未变化）。

### 20.6 AR2-C 就绪面

Resume 引擎可基于：`AwaitingUser` 持久化中断（run + 事件已在 store 中）、`RunOutcome::AwaitingUser` 边界、`UserInputReceived`/`RunResumed` 事件契约、`AwaitingApproval` 迁移边（`Acting→AwaitingApproval→Acting|ExecutingChange|Reasoning`）。AR2-C 需新增：ApprovalRequest 模型（exact binding + 失效）、Checkpoint、`resume` 入口在同一 run 上续跑循环。

---

## 21. AR2-C Implementation Result（已实施，代码验证事实）

实施日期：2026-09-02。审批/用户输入成为真实 Runtime 中断，Approve/Reject 在同一 `run_id` 上暂停与恢复（门禁测试 `approve_resumes_the_same_run_id_and_executes` / `reject_becomes_an_observation_and_reasons_again`）。

### 21.1 实际文件

```text
新增  src-tauri/src/agent/approval.rs     ApprovalRequest + arguments_hash + validate_pending_approval
新增  src-tauri/src/agent/checkpoint.rs   AgentCheckpoint + PendingInterruptRef + BudgetCheckpoint
新增  src-tauri/src/agent/gate.rs         AuthorizationGate + PolicyAuthorizationGate + Auto/Fn gate
修改  src-tauri/src/agent/controller.rs   中断/恢复 API + 授权分流 + checkpoint 写入
修改  src-tauri/src/agent/repository.rs   ApprovalStore / CheckpointStore / PendingToolCallStore + InMemory
修改  src-tauri/src/agent/reasoner.rs     ReasonerInput.user_replies（AskUser 恢复回灌）
修改  src-tauri/src/agent/mod.rs          模块注册与 re-export
```

未触碰：`agentic/*`、`ai/*`、`commands/*`、`tools/*`（只读调用，无修改）、`policy/*`（只读调用）、前端、Cargo.toml。旧四路径行为不变；V2 仍无 IPC/UI 接线。

### 21.2 ApprovalRequest 与 exact binding

`ApprovalRequest { id, run_id, tool_call_id, tool_name, arguments_hash, target_ids, risk, policy_version, policy_hash, precondition_ref, state, created_at, decided_at }`。`arguments_hash` = SHA-256(canonical tool_name + typed invocation Debug)；绑定不含原始 arguments。恢复时 `validate_pending_approval` 校验：Pending 态、tool_call_id、arguments_hash、target_ids、policy snapshot 全匹配；任一漂移 → `ApprovalInvalidated` + Observation + 回 Reasoning（不执行）。

失效码：`APPROVAL_ARGUMENTS_CHANGED` / `APPROVAL_TARGETS_CHANGED` / `APPROVAL_POLICY_CHANGED`。

### 21.3 AgentCheckpoint（可序列化，AR2-D 落盘预备）

`AgentCheckpoint { run_id, state, event_cursor, pending_interrupt, budget, target_ids, created_at }`；`pending_interrupt` 为 `Approval { approval_id }` 或 `UserInput { question }`。不含 credential/arguments/CoT（有 serde 测试守护）。写入点：`AwaitingApproval` / `AwaitingUser` 中断时。

### 21.4 Controller 中断/恢复 API

- `run_to_interrupt()` — 驱动至 Completed/Failed/Cancelled 或 `AwaitingUser`/`AwaitingApproval`。
- `resume_with_user_input(answer)` — `AwaitingUser → Reasoning`，emit `UserInputReceived` + `RunResumed`，同一 run 续跑。
- `approve(approval_id)` — 绑定校验通过后执行唯一 pending call，emit `ApprovalGranted` + `ToolStarted/Completed`，同一 run 续跑。
- `reject(approval_id)` — emit `ApprovalRejected` + Observation（"User rejected …"），回 Reasoning 续跑，不 Failed。
- `cancel()` — 非终态（含 `AwaitingApproval`）→ `Cancelled`，不新建 run。

授权分流（`AuthorizationGate`）：`Auto` → 原 AR2-B 自动读；`RequireApproval` → 创建 Approval + checkpoint + `AwaitingApproval`（pending call 存 `PendingToolCallStore`，中断期间不执行）；`Blocked` → Observation。生产 gate：`PolicyAuthorizationGate`（复用 `AgentPolicyService`）；测试 gate：`FnAuthorizationGate` / `AutoAuthorizationGate`。

### 21.5 测试与门禁（实际执行结果）

- `cargo fmt --check` 通过。
- `cargo test --lib` **248 通过 / 0 失败**；`agent::` **75 通过**（AR2-C 新增约 16：controller 7 项 CODEX §5 场景 + approval/checkpoint/gate/repository 扩展）。
- `cargo clippy`：`agent/` 0 告警；全仓余 5 处既有 WIP 基线（同 §19.6/§20.5）。

### 21.6 AR2-D 就绪面

InMemory store trait 已对齐 SQLite 需实现的五接口（runs/events/approvals/checkpoints/pending_calls）；checkpoint/approval  serde 契约稳定。AR2-D 引入 `rusqlite` + 事务边界 `(run + events + checkpoint + approval)` 即可，无需改 Controller 编排语义。

---

## 22. AR2-D Implementation Result（已实施，代码验证事实）

实施日期：2026-09-02。`runory-agent.db` SQLite 持久化落地：中断边界可跨进程重启恢复；启动扫描可恢复 run，**绝不静默执行 pending 动作**。

### 22.1 实际文件

```text
新增  src-tauri/src/agent/sqlite.rs       SqliteAgentDatabase + schema v1 + recovery + 五 Store impl
修改  src-tauri/src/agent/mod.rs          注册 sqlite 模块与导出
修改  src-tauri/src/agent/repository.rs   新增 StoreCorrupt / MigrationFailed / PersistenceFailed 错误码
修改  src-tauri/src/agent/run.rs          from_persisted 构造器（持久化重建）
修改  src-tauri/src/agent/decision.rs     PreparedToolCall 可 serde（pending call 落盘）
修改  src-tauri/src/tools/registry.rs     NativeToolInvocation 增加 Serialize/Deserialize（最小必要）
修改  src-tauri/Cargo.toml + Cargo.lock   新增 rusqlite 0.32（bundled）
```

未改：`agentic/*`（除 tools 一行 serde）、`ai/*`、`commands/*`、`lib.rs` setup（尚不 manage DB——无 IPC 调用方）、前端。旧四路径行为不变。

### 22.2 Schema（version = 1）

表：`schema_version`、`agent_runs`、`agent_events`、`agent_messages`、`tool_calls`、`tool_results`、`observations`、`approval_requests`、`agent_checkpoints`、`pending_tool_calls`。Migration 失败 fail-closed（`AGENT_STORE_MIGRATION_FAILED`）；损坏 JSON fail-closed（`AGENT_STORE_CORRUPT`）。

### 22.3 事务与恢复

- `persist_interrupt_bundle(run, events, checkpoint, approval?, pending_call?)` 单事务写入。
- `recover_on_startup()`：`AwaitingApproval` / `AwaitingUser` / `Paused` / `Created` 原样保留；其余非终态（Running/Acting/…）保守改为 `Paused`（`interrupted=true`）；**不执行任何 pending tool**。
- 陈旧 Approval 保持 `Pending`；执行前仍须 `validate_pending_approval`（含 policy revalidation）。

### 22.4 无 Secret 持久化

事件 JSON / pending call JSON / 消息内容写前经 `redact_secrets`；pending call 额外拒绝含 `password`/`passphrase`/`private_key`/`vault_master` 的载荷。测试扫描表内容守护。

### 22.5 测试与门禁（实际执行结果）

- `cargo fmt` / `cargo test --lib`：**256 通过 / 0 失败**（AR2-D 新增 8：`agent::sqlite`）。
- 覆盖：schema migration、AwaitingApproval/AwaitingUser 重启恢复、active→Paused、checkpoint 损坏 fail-closed、事件 replay 顺序、no-secret、stale approval 仍 blocked until revalidated。
- `cargo clippy`：`agent/` 0 告警；全仓余既有 WIP 基线。

### 22.6 AR2-E 就绪面

SQLite store 实现与 InMemory 相同 trait，Controller 可直接注入 `Arc<SqliteAgentDatabase>`。下一步（AR2-E）接 Timeline UI + IPC 时：在 `lib.rs` setup 打开 `app_data_dir/runory-agent.db`、启动调用 `recover_on_startup`、订阅事件回放 `events_after`。

---

## 23. AR2-E Implementation Result（已实施，代码验证事实）

实施日期：2026-09-02。右侧 Context Panel 改为 Runtime V2 Timeline 主交互；新增 `agent_v2_*` IPC；Plan & commands / Continue next step 不再是主路径。

### 23.1 实际文件

```text
新增  src-tauri/src/agent/broadcast.rs          事件持久化 + live broadcast
新增  src-tauri/src/agent/routing.rs            目标路由启发式移入 Rust
新增  src-tauri/src/agent/reasoner_planning.rs  PlanningReasoner（复用 local_turn / gateway）
新增  src-tauri/src/agent/session_dispatch.rs   Session 绑定 ToolDispatcher / PolicyGate
新增  src-tauri/src/agent/service.rs            AgentRuntimeV2Service
新增  src-tauri/src/commands/agent_v2.rs        start/subscribe/approve/reject/reply/cancel
新增  src/types/agent-v2.ts
新增  src/lib/tauri/agent-v2.ts
新增  src/components/context-panel/agent/AgentTimeline.tsx
新增  src/components/context-panel/agent/AgentApprovalCard.tsx
新增  src/components/context-panel/agent/agent-timeline-utils.ts
新增  src/components/context-panel/agent/agent-view-store.ts
改造  AgentPanel / AgentHeader / ContextPanel 宽度(420/360/600)
改造  lib.rs setup：打开 runory-agent.db + recover_on_startup + manage V2 service
```

### 23.2 UX / 架构不变式

- React 只渲染 `AgentEventEnvelope` 并转发用户动作；无 Plan 命令队列、无 Continue next step、无 `writeSsh` 旁路。
- 安全读仍经 Typed Tool + Policy；审批中断绑定同一 `run_id`。
- 面板折叠 / Inspect 切换不取消订阅语义：订阅按 `after_seq` 回放；run 继续在 Rust 侧。
- Envelope / AgentRun IPC 字段使用 camelCase；事件 `type`/`payload` 仍为 snake_case。

### 23.3 测试

- `cargo test --lib agent::`：agent 模块单测通过（含 routing / sqlite / controller）。
- `vitest`：`agent-timeline-utils.test.ts` 覆盖事件追加、审批态、终态推导。
- 旧 `AgentPlanCard` / `AgentConversation` 文件保留但不再被右侧主面板引用（Cleanup PR 删除）。

---

## 24. AR2-F Implementation Result（已实施，代码验证事实）

实施日期：2026-09-02。Safe Read 自动化与 Typed Tool 覆盖；R0/R1 低影响读自动执行，HighIO 扫描受 Policy 约束需审批；磁盘诊断可自动下钻至审批边界或缺输入为止。

### 24.1 实际文件

```text
改造  src-tauri/src/tools/descriptor.rs       ResourceImpact 元数据 + 3 新只读 Tool
新增  src-tauri/src/tools/terminal.rs           terminal.exec_readonly allowlist
改造  src-tauri/src/tools/incident.rs          filesystem.inode_usage / block_devices.list
改造  src-tauri/src/tools/registry.rs          执行路径 + from_model_read_call
改造  src-tauri/src/policy/model.rs            PolicyCondition.resource_impact
改造  src-tauri/src/policy/service.rs          默认 high-io-scan-approval 规则
改造  src-tauri/src/agent/gate.rs              resource_impact 传入 Policy 评估
新增  src-tauri/src/agent/tool_error.rs        Observation 错误分类
改造  src-tauri/src/agentic/planning.rs        local_disk_followup + inode/lsblk/readonly 路由
改造  src-tauri/src/agentic/optimization.rs   新 Tool cache key
改造  src-tauri/src/tools/execution.rs         sanitized_input + policy resource_impact
```

### 24.2 Tool / Policy 语义

| 能力 | 实现 |
|---|---|
| `system.disk` | R0 / Low → 默认 Auto |
| `system.directory_usage` / `system.large_files` | R1 / HighIO → 默认 RequireApproval |
| `filesystem.inode_usage` | R0 / Low（df -i） |
| `block_devices.list` | R0 / Low（lsblk） |
| `terminal.exec_readonly` | R1 / Medium；封闭 allowlist，无 Typed Tool 时 fallback |
| 磁盘下钻 | `local_disk_followup`：disk ≥85% 且缺 du/find 证据时并行请求 directory_usage + large_files |

### 24.3 Gate 行为

- R0/R1 + Low/Medium：Policy Allow → `ToolAutoAuthorized` → `ToolStarted` → `ToolCompleted`（零点击安全读）。
- HighIO：默认 Policy 要求审批 → `ToolApprovalRequired`；下钻在审批边界停止（符合「直到 risky action 或缺输入」）。
- Tool 失败经 `tool_error::classify_tool_error` 结构化进入 Observation，Run 继续。

### 24.4 测试

- `cargo test --lib`：**269 passed**（含 disk follow-up、typed-vs-shell、high-io policy、terminal allowlist、gate auto/approval）。
- `vitest` agent panel：**9 passed**（AR2-E UI 未改，仍兼容）。
- 未进入 AR2-G（ChangeSet 写集成）。

---

## 25. ChangeSet Integration（AR2-G）交付记录

### 25.1 Rust 闭环

| 组件 | 职责 |
|---|---|
| `agent/decision.rs` | `AgentDecision::ProposeChangeSet` + 结构校验 |
| `agent/changeset.rs` | `ChangeSetExecutor` trait + `SessionChangeSetExecutor`（复用 `ChangeSetService`） |
| `agent/controller.rs` | `propose_change_set` → `AwaitingApproval` → `approve_change_set` → execute → verify → rollback → Observation → 继续 Reasoning |
| `agent/approval.rs` | `change_set_id` / `change_set_version` 绑定 + `validate_pending_change_set_approval` |
| `agent/dispatch.rs` | 成功读 Tool 的 `tool_result` 写入 `evidence`，供 `change_proposal_is_evidence_bound` |
| `agent/reasoner_planning.rs` | `PlanningAgentDecision::ProposeChange` → `ProposeChangeSet` |
| `agentic/changes.rs` | `verify_execution()` + `verification_plan_satisfied()` |
| `agent/sqlite.rs` | schema v2：`approval_requests.change_set_id/version` |

### 25.2 事件链（同一 `AgentRun`）

```text
nginx.test (evidence) → ProposeChangeSet → ChangeSetProposed
  → ToolApprovalRequired → AwaitingApproval
Approve → ChangeSetApproved → ChangeSetExecutionStarted/Completed
  → VerificationStarted/Completed
  → （失败时）RollbackStarted/Completed → ObservationAdded → Reasoning 继续
```

### 25.3 前端

- `AgentTimeline` 渲染 ChangeSet / Verification / Rollback 事件行。
- `AgentApprovalCard` 在 `change_set_proposed` 后识别 ChangeSet 审批（`kind: change_set`）。
- i18n：`contextPanel.timeline.changeSet*` / `verification*` / `rollback*`（zh-CN + en-US）。

### 25.4 测试

- `cargo test --lib`：**271 passed**（含 `change_set_proposal_parks_for_approval_and_executes_after_grant`、`change_set_verification_failure_triggers_rollback_and_continues`）。
- `vitest` agent timeline utils：**+1** ChangeSet 审批识别测试。
- **未进入 AR2-H**。

---

## 26. Facts / Context / ToolArtifact Integration（AR2-H）交付记录

### 26.1 组件

| 组件 | 职责 |
|---|---|
| `agent/facts.rs` | `WorkingFact` / `WorkingFactSet`；从 `ToolResult` 提取紧凑事实；TTL freshness；写后 `invalidate_target` |
| `agent/artifact.rs` | `ToolArtifact` + `ArtifactStore`；大输出落库；`search` / `read` / `tail`；summary + 引用进上下文 |
| `agent/controller.rs` | `record_outcome` 提取事实并 emit `FactsUpdated`；大 `sanitized_data` 存为 artifact；`build_context` 注入 fresh facts + artifact refs（compact 之后） |
| `agent/checkpoint.rs` | `fact_snapshot` JSON；SQLite checkpoint 读写；`restore_facts_from_checkpoint` |
| `AgentTimeline` | 渲染 `facts_updated` |

### 26.2 不变式

- 模型上下文只收到：summary / 结构化 finding / artifact 引用，不注入完整日志或大配置树。
- WorkingFact 不是 Verification / ChangeSet precondition 的权威来源；写成功后目标事实失效，须重新观察。
- Artifact 内容经 `redact_secrets`；checkpoint / 事件不持久化 Secret。
- 复用 Phase 10K `snapshot` / `compact` / freshness，不另建 Context Manager。

### 26.3 测试

- `cargo test --lib`：含 `large_tool_output_becomes_artifact_not_full_model_context`、`write_invalidates_working_facts_for_target`、artifact search/read/tail、fact freshness。
- **未进入 AR2-I**。

---

## 27. Recovery / Regression / Production Hardening（AR2-I）交付记录

### 27.1 组件

| 组件 | 职责 |
|---|---|
| `agent/metrics.rs` | `RunMetrics`：reasoner/tool/approval/recovery/latency/verification/rollback 计数；无 Secret、无 private reasoning |
| `agent/controller.rs` | 全路径埋点；终端态 `persist_metrics()` → SQLite `metrics_json`；`pause()` / `resume()` |
| `agent/regression.rs` | Release Gate 回归场景 A–H（内存 harness，无需 Docker） |
| `agent/service.rs` | `pause` / `resume` 生产服务入口（rehydrate 同一 `run_id`） |
| `commands/agent_v2.rs` | `agent_v2_run_pause` / `agent_v2_run_resume` IPC |
| `agent/sqlite.rs` | `AgentRunStore::save_metrics`；`terminal_run_metrics_persist_to_sqlite` |

### 27.2 Release Gate 验收表（对照 `AGENT_RUNTIME_V2.md` §39）

| 门禁项 | 状态 | 证据 |
|---|---|---|
| Tool failure 是 Observation，Run 可继续 | ✅ | `scenario_a` / `scenario_g`；`tool_failure_becomes_an_observation_and_an_alternate_tool_runs` |
| Approval interrupt → 同一 `run_id` resume | ✅ | `scenario_b` / `scenario_d`；`approve_resumes_the_same_run_id_and_executes` |
| Reject 不执行、Run 继续 | ✅ | `scenario_c`；`reject_becomes_an_observation_and_reasons_again` |
| 崩溃后 pending approval 可恢复 | ✅ | `sqlite::awaiting_approval_survives_reopen` + `scenario_d` rehydrate approve |
| Stale approval（参数/策略漂移）不执行 | ✅ | `scenario_f`；`mutated_arguments_invalidate_approval_on_resume` |
| Tool timeout → recovery observation | ✅ | `scenario_g`（`recovery_attempts >= 1`） |
| Budget 耗尽 fail-closed | ✅ | `scenario_h`；round/tool/time budget controller tests |
| 崩溃 Running → `Paused`，不自动续跑 | ✅ | `sqlite::active_running_state_is_conservatively_paused` |
| Crash-recovered `Paused` → resume 同一 run | ✅ | `scenario_e` |
| ChangeSet verify/rollback metrics | ✅ | `approve_change_set` 路径 `record_verification` / `record_rollback` |
| Metrics 终端持久化 | ✅ | `transition` 终端态写 `metrics_json` |
| Pause / Resume IPC | ✅ | `agent_v2_run_pause` / `agent_v2_run_resume` |
| 事件回放 `run_paused` / `run_resumed` | ✅ | Timeline utils 已有类型；controller emit |
| Multi-server target binding（§36-I） | ⏭️ 刻意不在 V2 单目标范围 | Fleet 属 Phase 10G |
| Docker 端到端 SSH 回归 | ⏭️ 既有 integration_tests ignored fixture | 不重复扩 V2 scope |
| 旧 plan queue / doctor IPC 删除 | ⏭️ Cleanup PR（Release Gate 后） | 本阶段不删 |

### 27.3 回归场景映射（§36）

| 场景 | 测试 |
|---|---|
| A 日志缺失 →  alternate source | `scenario_a_log_not_found_agent_chooses_alternate_source` |
| B Approval interrupt resume | `scenario_b_approval_interrupt_resume_same_run` |
| C Reject continue | `scenario_c_reject_continues_safely` |
| D 重启 rehydrate approve | `scenario_d_rehydrate_after_approval_interrupt` |
| E Crash Paused resume | `scenario_e_crash_paused_run_resumes_same_run` |
| F Stale approval | `scenario_f_stale_approval_does_not_execute` |
| G Tool timeout recovery | `scenario_g_tool_timeout_recovery` |
| H Budget exhaustion | `scenario_h_budget_exhaustion_fails_closed` |

### 27.4 剩余风险 / 非目标

- **Pause 竞态**：外部 `pause` 需取得 controller 锁；drive 进行中会阻塞至下一 interrupt，非即时抢占。
- **Artifact / Metrics 进程内**：`InMemoryArtifactStore` 与 metrics 内容为 restart 后不重放；run 级 `metrics_json` 在终端态持久化。
- **前端 Pause 按钮**：`AgentHeader` Pause/Resume + `agent_v2_run_pause/resume` 已接线；Timeline 渲染 `run_paused`/`run_resumed`。
- **Benchmark 基线**：`RunMetrics` 字段齐全，未在本轮跑固定 hardware benchmark（无 CI 门禁数值）。

### 27.5 测试

- `cargo test --lib`：**288 passed**（+11 vs AR2-H：regression ×8、metrics ×2、sqlite metrics ×1）。
- **AR2-I 完成**；**Cleanup PR（§28）已完成**。

---

## 28. Cleanup PR（Release Gate 后）交付记录

### 28.1 已移除（路径 A / C / D / Doctor 悬空 UI）

| 类别 | 删除项 |
|---|---|
| 前端 Plan 队列 | `AgentPlanCard`、`AgentConversation`、`agent-state`、InlineChangeSet 等旧 Panel 组件 |
| 悬空 Agentic Workspace | `AgenticWorkspaceView`、`DoctorWorkspace`、`IncidentWorkspace`（独立页）、`LegacyWorkspace`、`IntegrationsWorkspace` |
| 悬空 ai-agent | `features/ai-agent/*`、`lib/tauri/ai-agent.ts`、`lib/tauri/ai-chat.ts`、`types/ai-agent.ts` |
| Rust IPC | `ai_propose_plan`、`ai_analyze_recent`、`ai_agent_*`、`ai_chat_*`、`agent_doctor_run`、`agent_multi_doctor_run`、`agent_run_cancel` |
| Rust 服务 | `AiAgentService`、`AiChatService`、`ai/agent_service.rs`、`ai/chat.rs` 及关联 policy/tools/audit |

### 28.2 保留（生产仍在用）

- **V2 Agent**：`agent_v2_*`（含 pause/resume）
- **ChangeSet / Fleet / Policy / MCP / Skills / Incident / Model**：`agentic.ts` + `ChangeSetWorkspace`
- **AI 助手 Tab**：`ai_explain/generate/diagnose/propose_fix`（非 plan 队列）
- **`AgentRuntimeService` 内核**：仍注册于 Tauri，供 Incident 等内部路径；Doctor IPC 已取消注册

### 28.3 前端 V2 补强

- `agent-v2.ts`：`pauseAgentV2Run` / `resumeAgentV2Run`
- `AgentHeader`：Running → Pause；Paused → Resume
- `AgentPanel`：paused 时禁用 Composer
- i18n：移除全部 `contextPanel.plan*`；新增 `pause` / `resume` / `timeline.resumed`

### 28.4 Release Gate 勾选（`AGENT_RUNTIME_V2.md` §39）

| 项 | 状态 |
|---|---|
| 无固定命令队列驱动 Agent | ✅ |
| `Continue next step` 已移除 | ✅ |
| UI 为 Event Timeline | ✅ |
| Approval 同一 durable run | ✅ V2 |
| Typed Tool / ChangeSet 写路径 | ✅ 未削弱 |
| 旧 plan IPC 已拆除 | ✅ |

### 28.5 测试

- `cargo test --lib`：**279 passed**（Cleanup 移除 legacy ai-agent 单测 −9；V2 regression 仍全绿）
- `vitest`：`agent-timeline-utils` 含 paused 状态测试
