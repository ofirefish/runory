# Runory Roadmap

## Strategy

### Authorized increment — SSH local TCP tunnels

See `docs/SSH_TUNNELS.md`: independent tunnel management, saved loopback-only rules,
explicit start/stop on verified ServerSessions, TCP reachability checks and server
shortcuts. Reverse forwarding, SOCKS, jump hosts, public sharing and automatic
reconnection are outside this increment; existing SSH and Agent boundaries remain.

### Authorized increment — single-hop SSH jump hosts

See `docs/SSH_JUMP_HOSTS.md`: a target profile may reference one direct profile as
its jump host. Rust authenticates A, opens SSH `direct-tcpip` to B, verifies and
authenticates B independently, then exposes only B's normal ServerSession. Nested
jump routes, proxy chains, automatic fallback and background jump tunnels remain
outside this increment.

Runory 以稳定 SSH Core 为底座，逐步演化为 **AI-native Infrastructure Workspace**。

```text
Stable Remote Core
  ↓
Reusable Native Tools
  ↓
Read-only Agent Diagnosis
  ↓
ChangeSet / Approval / Verify / Rollback
  ↓
Skills
  ↓
MCP / Multi-system Context
```

不允许为了 Agent 重写已经稳定的 SSH Core，也不允许在安全边界未建立前直接开放 Autonomous Repair。

## Phase 0 — Foundation

交付：

- Tauri 2
- React + TypeScript + Vite
- Tailwind + shadcn/ui
- Rust + Tokio
- russh Spike
- xterm.js Spike
- Tauri Channel Streaming Spike
- i18n Foundation
- Design Tokens

### Five-platform SSH Spike

验证：

```text
Windows
macOS
Linux
Android
iOS
```

测试：
- handshake
- password auth
- PTY
- echo
- disconnect

之后锁定：
- Rust Toolchain
- russh Version
- Crypto Backend

## Phase 1 — SSH Core / v0.1

Status：Implemented

Profiles：
- CRUD
- Search
- Local Persistence

Groups：
- Create
- Rename
- Delete
- Collapse
- Reorder
- Move Host
- Ungrouped

Security：
- Credential Vault
- Private Key
- Passphrase
- Host Verification
- Changed-key Blocking

Terminal：
- PTY
- UTF-8
- ANSI
- Resize
- Search
- Copy/Paste
- Tabs
- Reconnect

UX：
- zh-CN / en-US
- Dark / Light / System
- Desktop Layout

Release：
- Windows
- macOS
- Linux validation

## Phase 2 — SFTP / Files

Status：Implemented

交付：
- russh-sftp
- Files Tab
- Directory Navigation
- Upload
- Download
- Rename
- Delete
- mkdir
- Refresh
- Transfer Queue
- Progress
- Cancel / Retry

Architecture：

```text
ServerSession
├── Terminal
└── SFTP
```

## Phase 3 — Server Dashboard

Status：Implemented

交付：
- CPU
- Memory
- Disk
- Uptime
- Network
- Process List
- Service Health

通过 SSH Exec Channel，无 Agent 优先。

## Phase 4 — Server Operations

Status：Implemented

交付：
- Docker
- Docker Logs
- Start/Stop/Restart
- PM2
- Nginx
- Config Test
- Reload
- Logs Browser

全部走 Typed Domain Service。

## Phase 5 — Deployment

Status：Implemented

交付：
- Git Setup
- Pull / Build / Restart
- Environment Config
- SSL Assistance
- Deployment History
- Basic Backup
- Basic Cron

## Phase 6 — Mobile Product

Status：Implemented（Android 工程可在 Windows/Android SDK 环境生成与验证；iOS 最终签名归档必须在 macOS + Xcode 完成）

从 Spike 进入 Production：

- iOS UX
- Android UX
- Mobile Terminal Extra Keys
- Vault Key Import
- Biometric Unlock
- Mobile SFTP File Picker
- App Store Packaging

## Phase 7 — AI Terminal

Status：Implemented（本地规则提供器；不连接云端模型，不自动执行命令）

交付：
- Explain Command
- Generate Command
- Diagnose Output
- Propose Fix
- User Confirmation
- Session Context

所有命令建议均要求用户审核；确认后只插入交互式终端，不发送回车。高风险动作不会静默执行。

## Phase 8 — Limited Typed AI Plan

Status：Implemented（本地类型化计划、逐项审批、执行审计与最多 10 个活动会话审查）

这里的 “Implemented” 仅指现有 `AiAgentService` 有限计划执行，不表示 `AGENTIC.md` 的 Agent Runtime、ChangeSet / Verification / Rollback、Skills 或 MCP 已实现。

Tools：

```text
terminal.exec
file.read
file.write
system.metrics
process.list
docker.list
docker.restart
nginx.test
nginx.reload
```

增加：
- Plan
- Risk Classification
- Policy
- Approval
- Audit
- Multi-server Review

`terminal.exec` 仅允许 Rust 预定义只读预设；`file.read/write` 限制为 512 KiB UTF-8 文件。计划不携带可执行 raw shell，所有步骤均需单独批准，文件内容和工具输出不进入审计记录。

## Phase 9 — Optional Cloud

Status：Two-stage Production Promotion Gate Implemented（邮箱认证、Organization/RLS、Rust 端到端加密同步、tombstone、逐项冲突、成员角色、站内邀请、Access Policy、Audit 游标分页、Rust Typed Operation 强制、全设备持久绑定、新建/同步 Profile 自动纳管、短期令牌自动刷新、Ed25519 v2 签名离线策略、双 keyId 重叠轮换、真实 JWT/RLS/Edge 本地 E2E、五套双语 Auth 邮件模板、SMTP 配置验证工具、Postgres 17 Migration 重放、每日有界 Audit Retention Cron、63 项 pgTAP、Lint/Advisor 本地门禁、只读远端验证器、无秘密 schema v2 JSON 证据、完整应用源码 SHA-256、部署前候选批准与部署后 Production 复核；等待 Staging/Production 凭据执行）

Future opt-in Credential / Private Key E2EE Sync 的分阶段设计见 `docs/CLOUD_SYNC_SECURITY_ARCHITECTURE.md`。它不改变当前 Phase 9 状态，也不得在 device trust、recovery、migration 与外部安全审查门禁完成前上传凭据。

已交付：
- Encrypted Profile/Group Sync + Tombstone
- Team Inventory + In-app Invitation
- Access Policy / Audit Schema
- Organization + RLS

后续生产化：
- 在 Staging/Production 应用 Custom SMTP，并完成五类 Auth 邮件实际投递验证
- 远端 Migration / RLS / Advisor 验证
- 在 Staging/Production 配置策略签名 Secret 与编译期公钥，验证在线签发、五分钟离线窗口和密钥轮换发布流程
- 在远端 Staging/Production 核对 Audit Cron 首次运行与 Job History

Local-only 模式仍应保留。

## Phase 10 — Agentic Infrastructure Workspace Foundation

Status：Implemented — Phase 10A–10J 已按顺序安全门禁完成

10A 的最小首批边界：

- 复用现有 `ServerSessionManager`、SFTP、Dashboard、Operations 与 Deployment Service
- 建立 Rust Native Tool Adapter / Registry、`ToolDescriptor`、`ToolResult`、Risk 与 Mutability
- 第一批仅暴露只读 System / Service / Network / HTTP / Nginx 诊断 Tool
- 不接 MCP，不实现 Skills，不开放写 Tool，不做自动修复
- 不重构 SSH Transport、CredentialVault、Host Verification 或现有 IPC 数据流

### 10A — Native Tool Foundation

当前已完成：封闭 Registry、Descriptor / Result、Risk / Mutability / Scope、R0/R1 Policy Gate、Descriptor Timeout、Structured Error、sanitized Atomic Audit、绑定 `invocation_id` 的显式 Cancellation，以及七个只读 Tool：`system.info`、`system.disk`、`service.status`、`service.logs`、`network.port_check`、`http.request`、`nginx.test`。无通用 Tool IPC，真实 OpenSSH 集成测试覆盖同一 `ServerSession` 的隔离 Exec Channel、结构化 Nginx 校验、执行中取消与完成态 Audit。

Phase 10A 到此停止。ModelProvider、AgentRun、Context Manager、UI Activity Timeline 和任何写操作属于后续阶段，不得因 Foundation 完成而自动扩大范围。

- Tool Registry、`ToolDescriptor`、`ToolResult`
- RiskLevel、Mutability、Scope
- Timeout / Cancellation、Structured Error
- 首批只读 `system.*`、`service.*`、`network.*`、`http.*`、`nginx.test`

### 10B — Read-only Server Doctor

Status：Implemented

- ModelProvider abstraction
- AgentRun state machine、Context Manager、Tool Router
- Secret Redaction、Prompt Injection Boundary
- Tool Budget、Timeout / Cancel
- Agent Activity Timeline、Diagnosis 与 Evidence

此阶段禁止 `file.patch`、`service.restart`、`nginx.reload`、arbitrary `terminal.exec` 和 automatic repair。

### 10C — ChangeSet Draft / Approved Repair

Status：Implemented（版本化草稿与精确版本审批；首批 `file.patch`、`service.reload/restart`、`nginx.reload` 经统一 Registry / Policy / Audit。`file.patch` 实现写后回读与真实反向补丁；无法可靠回滚的 Service/Nginx 动作明确标记不支持。）

先交付 ChangeSet model、version、Risk、Diff Preview、Verification Plan、Rollback Plan 与 Review UI，不执行写操作。其后才可按独立授权开放第一批 `file.patch`、`service.reload/restart`、`nginx.reload`，且必须经过 Policy、Approval、Snapshot、Execute、Verify、Audit 和真实 Rollback。

### 10D — Skills

Status：Implemented（四个 Built-in Skill、受控用户目录加载、Manifest/版本/Tool/Risk 审查、显式启用；Skill 不授予权限。）

实现 Manifest parsing、`SKILL.md` loading、Tool requirements、Risk ceiling 与 Built-in Registry。第一批只考虑 Nginx Doctor、Website Troubleshooter、Linux Service Doctor、Disk Space Doctor。

### 10E — MCP / External Context

Status：Implemented（只读 HTTP JSON-RPC；HTTPS/loopback endpoint、Vault Token、默认禁用、逐 Tool `readOnlyHint` 授权与 sanitized Audit；无通用 MCP Tool IPC。）

在前述安全链稳定后实现 MCP Client、Secure Configuration、Tool Adapter、Permission Review、per-tool enable/disable 与 Audit；第一版只读。Native Tools 管服务器，MCP 只扩展外部世界。

### 10F — Multi-server Agent

Status：Implemented（最多 10 个活动 Session、结构化版本/服务漂移、多目标 ChangeSet 草稿；每个 Target 保持独立 ChangeSet ID/Version 审批。）

最后再增加 Multi-target Context、Config / Version / Service Drift、Multi-server ChangeSet 与 target-specific approval。一次审批不得隐式扩大到未展示目标。

### 10G — Agentic Production Hardening

Status：Implemented

- ChangeSet 副作用前持久化执行声明，阻止重复/并发执行。
- 仅持久化无内容 ChangeSet 元数据；重启后审批失效，执行中记录标记 `Interrupted`，不自动续跑。
- MCP 双协议兼容：`2026-07-28` 现代无状态 Streamable HTTP，以及 `2025-03-26`–`2025-11-25` 初始化/Session 协议。
- MCP 响应 ID、Content-Type、分页、SSE 最终响应和 512 KiB 流式上限校验。
- Agentic UI 拆为按需加载的 Doctor、ChangeSet、Integrations 与 Legacy 模块；MCP Tool 由用户显式选择并提供 JSON 对象参数。
- Fleet Approval 绑定 Fleet Version、精确 Target Session IDs 与逐 Target ChangeSet ID/Version；Target 修订自动失效审批。
- 支持 Sequential、Canary、Rolling Batch，以及仅非生产可用的 Parallel；默认 Sequential。
- 支持 Stop、Pause for Review、Continue、Rollback；默认失败暂停复核，继续时不重放已处理 Target。
- Fleet Verification 覆盖 target-local、cross-target 与可选 service-level verification。
- Multi-server Rollback 逐 Target 独立追踪 RolledBack / RollbackFailed，未执行 Target 保持 Pending。
- 确定性 incident fixture 覆盖 target selection、ChangeSet generation、exact approval、execution batching、verification、rollback 和 restart recovery。
- Fleet audit/recovery metadata 记录 Agent Run ID、Model、Tool Call Count、Target、Risk、Approval Binding、Execution、Verification、Rollback 与 Duration，不持久化远端内容。
- 不新增 Model Provider、Marketplace、Trusted Automation、R4 Tool 或 broad Tauri capability。

### 10H — Production Operations Packs

Status：Implemented

- 统一 Incident domain/state machine 与 Evidence-bound Root Cause
- Website、Nginx、Docker、Disk Full、Linux Service 五类 Pack
- DNS/TCP/TLS/HTTP、进程/端口/配置、目录/大文件、Docker inspect/logs 等 Native Typed Tool
- Repair Plan 只绑定既有 ChangeSet/Fleet；Docker restart 为 R3 精确审批 step
- Timeline/Evidence/Root Cause/Repair/ChangeSet/Verification 结构化 UI
- 五类 deterministic regression fixture；Disk 危险删除始终阻止

### 10I — Incident Lifecycle & Production Validation

Status：Implemented

- Incident 使用原子 JSON Repository 保存最多 2,000 条内容无关元数据；不持久化症状、日志、HTTP body、配置内容或操作员正文。
- 重启恢复为 `metadata-only`；调查/执行/验证中的记录统一标记 `Interrupted`，不自动续跑或重新执行 Tool。
- Website Pack 将 public endpoint 与 optional upstream endpoint 建模为独立 Typed TCP probe。
- 多目标调查生成确定性 scalar comparison，展示一致性与 drift，不把远端正文写入比较或 Audit。
- Operator Handoff、Resolved / Accepted Risk / False Positive closure 使用窄业务 Command；Resolved 必须已有成功 Verification，False Positive 只允许 Inconclusive。
- Audit Export 仅返回 evidence reference、target、risk、ChangeSet binding、verification、resolution、timeline 与 duration，并显式声明 content omitted。
- Incident UI 增加本机历史、恢复状态、跨目标比较、交接/关闭和脱敏导出展示。
- 回归测试覆盖持久化脱敏、恢复中断、public/upstream 分离、跨目标 drift、关闭门槛和 Audit 导出；真实 Docker OpenSSH fixture 增加 Operations Pack Typed Tool 集成测试。

Phase 10I 到此停止；不自动进入 Autonomous Remediation、Marketplace、Team/Cloud 或扩大 MCP/Shell 范围。

### 10J — Production Qualification & Fault Lab

Status：Implemented

- Incident metadata schema 升级为 v2，并对 v1 记录执行原子、content-free migration。
- Repository JSON 损坏、未知 schema、重复 Target、悬空 Evidence、非法 Comparison 或变更后的 ChangeSet target binding 均 fail-closed；原文件保持不变，不静默丢失 Audit。
- Incident 内存与持久层均验证 2,000 条硬上限，Timeline、Evidence 与 Comparison 同样有界。
- 新增三节点真实 Docker OpenSSH fault-lab：两个 active target、一个 failed target，由真实 Session + Native Typed Tool 生成跨目标 drift 与 evidence-bound root cause。
- Fault-lab 使用版本化 deterministic fixture；不暴露 Shell IPC，不改变 Tool Registry、Risk、Approval 或 Fleet 策略。
- Production qualification 同时复用 Phase 10G 的 exact approval / partial failure / rollback fixture 和 Phase 10I 的 content-free lifecycle tests。

Phase 10J 是发布资格与测试基础设施阶段，不增加运行时 Agent 能力。完成后停止，不自动进入下一阶段。

### 10K — Agent Performance / Cost / Context Optimization

Status：Implemented

- 统一 `ContextSource / ContextItem / ContextSnapshot / ContextBudget / ContextFreshness`，只选择任务相关且有界的上下文。
- 长运行将 Observation 压缩为保留 Evidence、Target、Risk、Approval、ChangeSet 与 Verification 引用的结构化事实。
- Tool Result 支持字段投影、分页、tail、时间范围声明与最大结果大小；超限结果退化为安全摘要。
- 只读 Observation Cache 绑定 target、source、observed-at 与 TTL；写入后按目标失效。Precondition / Verification 不读取缓存。
- 同一运行的调查 Read Tool 去重；相同 Risk 边界的无依赖只读 Tool 并行，Write Tool 仍完全串行并走 ChangeSet。
- ChangeSet 审批时捕获 target-local Typed Tool 前置条件摘要；执行前绕过缓存重新检查，变化时使审批失效并要求重新诊断、规划和审批。
- `ModelProvider / ModelCapability / ModelTask` 建立路由抽象，但不改变 Tool、Risk 或 Approval 权限。
- Agent Budget 覆盖 model/tool calls、input/output token、时间与可选成本；超限安全停止并返回当前调查状态。
- AgentRunMetrics 记录 token、context、cache/dedup、MCP、并行批次、时延、验证/回滚与可用成本信息。
- 五类 Incident fixture 的 deterministic before/after estimate 保持 diagnosis、approval、verification 与 audit 覆盖不下降。

Phase 10K 到此停止；不自动进入 Phase 11。

RDP、Marketplace、Signed Skill Package、Runory Managed AI 等仍是未排期 Future，必须由用户另行授权。

## Agent Runtime V2 Migration Track

在继续 Phase 11 之前，优先完成 Agent Runtime V2。该 Track 不增加新的 Agent 产品范围，而是替换旧的固定 Plan / Command Queue orchestration。

```text
AR2-A  AgentEvent + AgentRun State Machine
AR2-B  Iterative Reason → Single Command Proposal → Observe Loop
AR2-C  Exact Command Approval / Cancel / Resume
AR2-D  Durable Checkpoints + SQLite
AR2-E  Right-panel Agent Timeline UI
AR2-F  ServerSession Command Execution + Bounded Observation
AR2-G  Mutating Command Verification + Existing ChangeSet Integration
AR2-H  Facts / Context / ToolArtifact Integration
AR2-I  Recovery / Regression / Production Hardening
```

Runtime V2 Release Gate 通过前：

- 暂停新增 Team / Marketplace / Cloud 范围。
- 不新增无必要的 Operations Pack / MCP / Skill。
- 不删除稳定 Agent 安全底座。
- 不继续扩展固定命令队列作为主运行模式。
- 不再以“覆盖所有 Linux 能力”为目标扩展 V2 Typed Tool catalog；现有 Incident / ChangeSet / Verification Tool 保留。
- V2 每条命令均须审批，Critical fail-closed，写入/Unknown command 后须审批只读验证。

详细规范见 `AGENT_RUNTIME_V2.md` 与 `CODEX_AGENT_RUNTIME_V2.md`。

## Agentic Release Gate

任何 Agent Write 能力上线前必须满足：

- [x] Tool Registry 与 Risk Model 稳定
- [x] Secret Redaction 与 Prompt Injection Boundary
- [x] ChangeSet Versioning 与 Approval Binding
- [x] Verification Engine
- [x] Sanitized Audit
- [x] Rollback capability truthfully represented
- [x] Production server regression tests

## Release Rule

任何 Phase 不得通过削弱以下能力换开发速度：

- Credential Isolation
- Host Verification
- Capability Boundary
- Typed Domain Service
- Local-first
- Tool Risk Classification
- Human Approval for high-risk changes
- Verification
- Auditability
