# Runory Architecture

> Current-state note: 本文的 SSH / SFTP / Operations / Deployment / Mobile / limited AI / Cloud 章节描述当前仓库基线。Phase 10A–10J 已在稳定 Core 之上增加 Agent Runtime、Native Tool Registry、版本化 ChangeSet、Approval / Verification / Rollback、Skills、只读 MCP、Multi-server Foundation、Production Hardening、Production Operations Packs、Incident Lifecycle 与 Production Qualification；无通用执行 IPC，现有稳定模块优先保留。

## 1. 架构目标

Runory 的核心规则：

> **React 是跨平台交互层，Rust 是跨平台基础设施引擎。**

架构必须支持：

- Secure Credential Boundary
- High-throughput Terminal Streaming
- 明确的 SSH Session Lifecycle
- Desktop + Mobile Core 复用
- SFTP / Server Ops 后续扩展
- 可测试 Domain Layer
- 最少的 Platform-specific Business Logic

## 2. 总体结构

```text
┌──────────────────────────────────────────────┐
│                  React UI                    │
│ Profiles Groups Sessions Terminal Settings  │
│                       │                      │
│                    xterm.js                  │
└───────────────────────┬──────────────────────┘
                        │
                Commands / Channels
                        │
┌───────────────────────▼──────────────────────┐
│                  Rust Core                   │
│ ProfileService       GroupService            │
│ CredentialService    KnownHostService        │
│ ServerSessionManager SSHService              │
└───────────────────────┬──────────────────────┘
                        │
                       russh
                        │
                        ▼
                   SSH Server
```

## 3. Trust Boundary

### React/WebView

按 Lower-trust Presentation Boundary 对待。

允许知道：
- Sanitized Profile
- Group
- Session ID
- Session Status
- Error Code
- Terminal Output
- 用户正在输入的 Transient Credential

不得拥有：
- Persisted Password
- Private Key Content
- Vault Internals
- Master Secret
- russh Handle
- Arbitrary FS
- Native Process Execution

### Rust

Trusted Native Boundary。

负责：
- SSH Transport
- Credential
- Key Reading
- Host Verification
- Session Lifecycle
- Persistence
- Capability-sensitive Operations

当前 React 还包含 Files、Dashboard、Operations、Deployment、Mobile、AI Terminal、有限 AI Plan 与 Cloud Settings；它们仍然只是 Interaction Layer。

## 4. Frontend Modules

```text
src/
├── app/
├── components/
│   ├── layout/
│   ├── ui/
│   └── common/
├── features/
│   ├── profiles/
│   ├── groups/
│   ├── sessions/
│   ├── terminal/
│   ├── files/
│   ├── dashboard/
│   ├── operations/
│   ├── deployment/
│   ├── mobile/
│   ├── ai/
│   ├── ai-agent/
│   └── settings/
├── hooks/
├── i18n/
├── lib/
│   ├── tauri/
│   ├── terminal/
│   └── supabase/
├── stores/
└── types/
```

`profiles`：
- Form
- Host Item
- Host Menu

`groups`：
- Group Tree
- CRUD Dialog
- Drag/Drop

`sessions`：
- Tabs
- Status
- Reconnect / Disconnect

`terminal`：
- xterm lifecycle
- fit/search addons
- input bridge
- stream binding

## 5. Rust Modules

```text
src-tauri/src/
├── commands/
├── agentic/
├── ai/
├── cloud/
├── credentials/
├── dashboard/
├── deployment/
├── domain/
├── groups/
├── known_hosts/
├── mcp/
├── operations/
├── profiles/
├── settings/
├── skills/
├── ssh/
├── storage/
├── tools/
├── transfers/
└── lib.rs
```

### Domain
只定义数据结构、状态和 AppError。

### Commands
保持 Thin Adapter：
1. deserialize/validate
2. call service
3. typed response/error

### Services
放 Business Logic。

### Repositories
隔离 Storage。

## 6. Domain

```text
HostGroup
ServerProfile
ServerSession
TerminalChannel
```

必须保持概念独立。

## 7. ServerSession

ServerSession 表示已认证 SSH Transport，而不是一个 Tab。

```text
ServerSession
├── TerminalChannel
├── TerminalChannel (future multi shell)
├── SFTPChannel
├── ExecChannel
└── TunnelChannel
```

Phase 1 可以一个 Session 一个 Terminal，但代码模型不得写死。

## 8. ServerSessionManager

概念：

```rust
ServerSessionManager {
    sessions: Arc<RwLock<HashMap<SessionId, SessionHandle>>>
}
```

SessionHandle：
- profile_id
- status
- russh handle
- terminal task/channel
- cancellation
- last_activity

任何 russh Object 不跨 IPC。

## 9. Session State Machine

```text
Idle
 ↓
Connecting
 ↓
VerifyingHost
 ↓
Authenticating
 ↓
OpeningShell
 ↓
Connected
 ├──→ Disconnected
 └──→ Error
```

所有 Transition 明确。

## 10. Connection Flow

```text
React
 ↓ ssh_connect(profileId)
Command
 ↓
ProfileService
 ↓
CredentialService
 ↓
SSHService
 ↓
russh handshake
 ↓
KnownHostService
 ↓
authentication
 ↓
PTY
 ↓
shell
 ↓
ServerSessionManager
 ↓
Tauri Channel
 ↓
xterm.js
```

## 11. IPC

### Command

用于 Request/Response：

```text
profile_list
profile_get
profile_create
profile_update
profile_delete

group_list
group_create
group_update
group_delete
group_reorder

ssh_test
ssh_connect
ssh_disconnect
ssh_reconnect
ssh_resize

known_host_remove
settings_get
settings_update
vault_unlock
vault_lock
```

### Channel

Terminal Output 必须用 Channel。

### Status

低频 Session State 可以用 Event 或单独的 Status Channel。

## 12. Terminal Output

```text
Remote Server
 ↓
russh Channel
 ↓
Rust Async Read
 ↓
Tauri Channel
 ↓
xterm.write()
```

Terminal Output 不得经过 React Global State。

## 13. Terminal Input

```text
Keyboard
 ↓
xterm.onData()
 ↓
small buffer
 ↓
ssh_write
 ↓
Rust
 ↓
russh channel
```

## 14. Resize

```text
ResizeObserver
 ↓
FitAddon.fit()
 ↓
cols/rows
 ↓
ssh_resize
 ↓
SSH window-change
```

## 15. Credential Architecture

```text
SSHService
   │
CredentialService
   ├── PlatformKeyStore (system-owned Vault unlock secret)
   └── CredentialVault trait
       ├── Desktop: StrongholdCredentialVault
       └── Mobile: PortableCredentialVault
```

SSHService 不知道 Secret 实际存储方式。

## 16. Key Source

```rust
enum KeySource {
    File { path: String },
    Vault { key_id: Uuid },
}
```

Desktop Phase 1：File  
Mobile Phase 6：Vault

## 17. Host Verification

KnownHostService：
- Fingerprint
- Lookup
- Persist Trust
- Detect Changed Key

禁止 Global Accept All。

## 18. Storage

Phase 1：

```text
JSON Repository
+
Atomic Write
```

文件：

```text
profiles.json
groups.json
known-hosts.json
settings.json
```

未来切 SQLite，只替换 Repository。

## 19. Atomic Write

```text
serialize
 ↓
write .tmp
 ↓
flush
 ↓
fsync
 ↓
rename
```

## 20. Capabilities

Frontend 不授予 Broad：
- shell
- process
- FS
- HTTP

Private Key File Picker 使用窄权限 Native Dialog；内容由 Rust 读取。

## 21. Cross-platform

Shared：
- React feature logic
- Rust domain
- SSH core
- repository contracts
- host verification
- error model
- i18n
- design token

Platform-aware：
- key import
- biometric unlock
- native picker
- mobile terminal toolbar
- window/package behavior

## 22. Mobile

```text
iOS / Android WebView
 ↓
React
 ↓
Tauri Bridge
 ↓
Rust Core
 ↓
russh
```

核心产品路径不允许依赖 Node.js。

## 23. Dependency Policy

新增 Rust dependency 前：
- maintenance
- license
- Rust version
- desktop compile
- mobile compile
- native dependency footprint

新增 frontend dependency：
- WebView compatibility
- no Node-only API
- avoid redundant framework

## 24. SFTP / Files（Implemented）

Phase 2 已在共享 `ServerSession` 上实现：

```text
ServerSession
├── Terminal
└── SFTP
```

尽量复用同一 SSH Transport/Auth。

## 25. Operations / Deployment（Implemented）

Phase 3–5 实际实现保持：

```text
ServerSession (authenticated SSH transport)
├── TerminalChannel
├── SftpChannel
└── ExecChannel (one isolated channel per typed operation)
    ├── DashboardService
    ├── OperationsService
    └── DeploymentService
```

`ExecChannel` 只接受 Rust Core 构造的 `RemoteCommand`。IPC 不接受 raw command、shell fragment 或任意路径读写请求。固定脚本通过 `sh -c` 执行，动态值只作为经过 POSIX quoting 的位置参数传递。

Phase 5 的部署历史继续通过 Repository + Atomic JSON Write 持久化；环境变量值只作为 transient stdin 发送，不进入历史记录。

## 25.1 Mobile Product

Phase 6 继续复用同一 Rust Core，不建立 JavaScript SSH 或移动端旁路：

```text
iOS / Android React Interaction Layer
 ├── Adaptive Navigation / Safe Area
 ├── Terminal Extra Keys
 ├── Privacy Overlay + Native Biometric Prompt
 └── Native File Picker Intent
              ↓ typed IPC
Rust Core
 ├── CredentialService
 │    ├── Desktop PlatformKeyStore: Credential Manager / Keychain / Secret Service
 │    └── CredentialVault
 │         ├── Desktop: StrongholdCredentialVault
 │         ├── Mobile: PortableCredentialVault (Argon2id + AES-256-GCM)
 │         └── private-key:{key_id}
 ├── LocalFileGrantService (single-use upload/download grants)
 └── ServerSessionManager
      ├── TerminalChannel
      ├── SftpChannel
      └── ExecChannel
```

移动私钥导入由 Rust 打开系统文件选择器、读取和校验最多 1 MiB 的私钥，再直接写入移动端加密 CredentialVault。React 只接收不透明 `key_id` 和显示名称。移动端仓库使用 Argon2id 派生密钥和 AES-256-GCM 认证加密，避免 Android/iOS 交叉构建依赖原生 libsodium；Desktop 继续使用 Stronghold。Desktop 的不透明 Vault 解锁 Secret 由 Rust 写入当前 OS 用户的 Credential Manager、Keychain 或 Secret Service；旧 Vault 首次成功解锁后迁移，以后可在启动时自动解锁。Rust 会先执行只读运行时探测，因为编译进应用的系统存储后端在某些登录会话中仍可能不可用；此时 UI 回退到本次会话密码解锁。Android/iOS 仍保留主密码回退，直到移动 PlatformKeyStore 门禁完成；生物识别不向 React 返回 Vault Master Secret。

## 26. AI Terminal

AI 不能直接访问 russh：

```text
Local Assistant Provider
 ↓
Typed AI Analysis Tool
 ↓
Policy / Approval
 ↓
Terminal Insert (no newline)
 ↓
User explicitly runs command
```

Phase 7 使用可替换的 Rust `AiAssistantProvider`，当前实现为确定性、本地、可审计的规则提供器，不需要网络、账户或 API Key。React 只提交用户意图并渲染结构化结果；风险、用途、诊断代码和安全命令模板均由 Rust 生成。

`ServerSession` 为诊断维护最多 32 KiB 的易失终端尾部上下文。该缓冲不进入 Zustand、React State、日志或持久化仓库，IPC 只返回诊断和建议，不返回原始上下文。AI 建议不能调用 `ExecChannel`；用户确认后仅通过现有 `TerminalChannel` 插入文本，且不附加回车。

### 26.1 有限类型化 AI Plan（Implemented）

Phase 8 不把 Phase 7 的文本建议升级成任意 Shell，而是增加独立的、由用户构造工具步骤的类型化执行链：

```text
React Plan Builder
 ↓ AiPlanRequest (typed tools only)
AiAgentService (Rust, in-memory plan)
 ↓ Policy risk + per-step approval
AiToolExecutor
 ├── DashboardService
 ├── OperationsService
 ├── bounded SftpChannel read/write
 └── fixed TerminalPreset → ExecChannel
 ↓
ServerSession
```

支持 `terminal.exec`、`file.read`、`file.write`、`system.metrics`、`process.list`、`docker.list`、`docker.restart`、`nginx.test`、`nginx.reload`。其中 `terminal.exec` 的 IPC 只接受 `AiTerminalPreset` 枚举，不接受命令字符串；Docker 与 Nginx 调用既有 Domain Service。

一个计划最多覆盖 10 个活动 Session、包含 12 个工具模板。每个展开后的步骤拥有独立状态和审批，不能批量越过 Policy。计划只在内存中保存；`file.write` 内容不进入公开 Plan，执行后从内存移除。`ai-audit.json` 通过 Repository + Atomic Write 持久化最多 2000 条执行元数据，不记录 Goal、文件内容、命令输出或 Terminal Context。

该 Phase 8 实现本身不是 `AGENTIC.md` 所定义的 Agent Runtime，也不包含统一 Tool Registry、版本化 ChangeSet、Skills 或 MCP；这些能力由后续 Phase 10 模块通过 Adapter 复用稳定服务，而不是改名或替换本节与 SSH Core。

## 26.2 Optional Cloud Foundation

Phase 9 使用 Supabase Auth 邮箱登录与 Postgres RLS，但 Local-only 始终是默认可用路径：

跨设备 Credential / Private Key 同步与设备密钥信封仍不是当前基线；其兼容演进提案见 `docs/CLOUD_SYNC_SECURITY_ARCHITECTURE.md`。在该文档的迁移与安全门禁完成前，本节下述“凭据永不上传”规则继续强制生效。

```text
React Auth UI → supabase-js (publishable key only)
                         ↓ JWT
Supabase Data API → GRANT + RLS → Organization-scoped opaque sync objects
```

生产 Auth 邮件由部署工具通过固定 Supabase Management API Auth Configuration endpoint 配置。五套双语 HTML 模板在仓库内接受静态校验，本地 Supabase 使用同一模板与 Inbucket；Hosted Project 的 SMTP Host/User/Password 和 Management Access Token 只从部署环境读取，不进入 Vite、React、Rust Core 或版本库。配置写入后必须重新读取并核对所有非秘密字段与模板。

WebView 不持久化 Supabase Session，`persistSession=false`，避免 Refresh Token 写入 localStorage。Profile/Group 清单在 Rust 使用 Argon2id 派生的 AES-256-GCM 密钥加密，并将 Organization ID 绑定为 AAD；Supabase 只接收不透明密文。同步格式主动移除 Credential、Private Key、Vault Secret、Terminal Output、`key_source` 与 `last_connected_at`。

每个 Organization 在本机维护只含 UUID 与删除时间的 `cloud-sync-state.json`。只有已同步记录随后在本机消失时才生成 tombstone，因此首次同步和不完整快照不会被误判为删除。v2 加密快照携带 tombstone，同时继续兼容读取 v1。拉取先生成 Rust 内存 Preview；新增与远端较新项自动应用，本地较新、同版本差异、远端删除冲突由用户逐项选择。选择绑定预览时的对象类型、UUID 与本地时间戳，Apply 时在同一写锁内重新校验，避免 TOCTOU 覆盖。数据库写入通过 revision RPC 原子比较，防止多设备静默覆盖。

Team UI 通过受限 RPC 创建、接受和撤销站内邀请。邀请邮箱统一规范化，七天过期；只有 Owner/Admin 可创建和查看成员邮箱，接受者必须使用 Supabase Auth 中已经验证且完全匹配的邮箱。邀请变更写入 Cloud Audit。

Governance UI 允许 Owner/Admin 调整非 Owner 成员角色、移除成员、维护 Access Policy，并使用 `(occurred_at, id)` keyset cursor 分页读取 Audit。成员和策略表对 `authenticated` 撤销直接写权限，所有变化由带业务不变量和 Audit 写入的 RPC 完成。

用户可把本设备持久绑定到所选组织；该绑定覆盖设备上的全部 Profile，新建或云同步进入的 Profile 会立即继承组织治理。Rust 将 Organization ID、全设备作用域标记和用于状态展示/清理的 Profile UUID 快照原子写入 `cloud-policy-bindings.json`；Publishable Key、Access Token 与过期时间只存在内存。Profile 创建、删除和同步 Apply 后会刷新 UUID 快照，但安全判断以全设备作用域为准，不存在异步刷新空窗。应用重启后绑定仍有效但没有凭据：只有剩余有效期内的已验证签名决策可以继续使用，其他动作 fail-closed，直到邮箱用户重新登录。React 监听 Supabase `SIGNED_IN`、`TOKEN_REFRESHED` 和 `SIGNED_OUT`，只负责把新的短期令牌送入 Rust 或锁定内存凭据，不参与策略判断。

Rust 在 Connect、SFTP Read/Write、Operate、Deploy、AI Execute 命令边界取权威决策。未配置签名公钥的开发构建继续调用固定 `evaluate_access_policy` RPC，并在网络不可用时 fail-closed。生产构建固定 Ed25519 验证公钥后改用 `evaluate-access-policy` Edge Function：Function 只转发调用者 JWT，以用户 RLS 身份调用同一 RPC，再用仅存在 Supabase Secret 的私钥签署包含 Organization、Profile、Action、Decision、签发时间和过期时间的决策包。

Rust 在接受在线结果和每次离线复用时都重新验证完整签名、keyId、上下文绑定、最多 300 秒 TTL 与 30 秒未来时钟偏差。生产构建最多固定四个 Ed25519 公钥，v2 决策把 keyId 纳入签名以支持重叠轮换。只把签名决策包原子写入 `cloud-policy-bindings.json`，不缓存裸布尔值、Token 或私钥。网络失败、令牌缺失或重启后仅可在签名包未过期时复用精确的 Organization/Profile/Action 决策；未知 keyId、过期、篡改、错误上下文、超长 TTL、未知版本或没有编译期公钥一律 fail-closed。SSH Transport、TerminalChannel 与 SftpChannel 的结构不变。

`private.prune_audit_records(retention_days)` 提供 30–3650 天的特权保留函数，不向客户端开放。Migration 启用 Supabase Cron 并创建 `runory-audit-retention-daily`：每天 03:17 UTC 清理最多 10,000 条超过 180 天的 Audit，使用 `(occurred_at, id)` 索引与 `FOR UPDATE SKIP LOCKED` 避免无界删除和并发等待。`cron` schema 不授予 Data API 客户端 USAGE。

## 26.3 Future Agentic Extension

详细规范见 `AGENTIC.md`。未来新增结构位于现有 Domain Service 之上，Agent 不得成为 SSH Core 的旁路：

Phase 10A–10G 已落地在稳定 Domain Service 之上：`NativeToolExecutionService` 强制执行 Policy、审批上下文、执行前后 Audit、Timeout 与 Cancellation；`AgentRuntimeService` 只从结构化非秘密信号生成 Diagnosis，原始远端内容保留为 Untrusted Evidence；ChangeSet 审批绑定精确 Version。Skills 只能约束调查流程，MCP 第一版仅接受逐项启用的只读 Tool，并使用 Vault Token 与独立 sanitized Audit。Registry 不作为通用 Tauri Tool command 暴露。

Production Hardening 增加两个不扩大权限的边界：`ChangeSetService` 在副作用前持久化无内容执行元数据，恢复时强制失效审批并把未完成事务标记为 `Interrupted`；`McpGateway` 以双协议适配器支持现代 `2026-07-28` 无状态 Streamable HTTP 和旧版初始化会话，Session ID 仅驻留内存。Agentic 主视图与 Doctor / ChangeSet / Integrations / Legacy 子视图按需加载，React 仍只通过业务 IPC 请求 Rust Runtime。

Phase 10G 的 Multi-server 写入继续复用单目标 ChangeSet，不新建旁路执行器：

```text
FleetExecutionService
 ├── Approval Binding (fleet version + exact targets + target ChangeSet versions)
 ├── Strategy (Sequential / Parallel / Canary / Rolling Batch)
 ├── Failure Policy (Stop / Pause / Continue / Rollback)
 ├── Fleet Verification (target-local / cross-target / service-level)
 └── content-free Fleet Audit / Recovery Metadata
              ↓
ChangeSetService (per target)
              ↓
NativeToolExecutionService → Policy / Audit → existing Domain Service
```

Production Parallel All 由 Rust 拒绝，默认 `Sequential + Pause for Review`。每个 Target 的 Pending / Executing / Succeeded / Failed / RolledBack / RollbackFailed 独立保存；重启后 Fleet 审批失效且执行中状态变为 Interrupted。

Phase 10H 增加 `IncidentService`，不增加执行旁路。Operations Pack 经 `NativeToolExecutionService` 调查并聚合 Evidence；Repair Plan 只能绑定既有精确版本 ChangeSet/Fleet，再进入原 Approval/Execute/Verify/Rollback 链。Incident 不持有 Credential。

Phase 10I 为 `IncidentService` 增加本机 Repository，但只保存 content-free lifecycle metadata。运行时 Evidence 保留完整 `ToolResult`；持久层与 Audit Export 仅保留 Evidence ID、Target、Tool source、success/error code 与确定性 scalar comparison。恢复记录不能还原远端正文，进行中状态 fail-safe 转为 `Interrupted`。

Phase 10J 不增加生产 Command。它把 Incident Repository schema 提升到 v2，对 v1 做受控迁移，并在加载前验证完整 target/evidence/comparison/ChangeSet 引用图。任何损坏均保留原文件并 fail-closed。三节点 OpenSSH fault-lab 位于测试边界，生产 Runtime 仍只看到普通 `ServerSession`。

Phase 10K 在 Agent Runtime 内增加性能控制面，不增加 Tool 权限：

```text
Task Intent → Context Budget / Freshness → Context Snapshot
                                      ↓
Read Plan → Dedup → target-scoped TTL Cache → risk-grouped Parallel Reads
                                      ↓
Projection / Compaction → ModelProvider Capability → Diagnosis
                                      ↓
AgentRunMetrics / sanitized Tool Audit

Approved ChangeSet → uncached Typed Preconditions → sequential Write
                  changed └→ invalidate approval + stop
                  success └→ invalidate target observation cache → Verify
```

Cache 与 Dedup 只服务 Investigation Observation。Precondition 与 Verification 始终重新执行 Native Typed Tool；任何 Model 路由都不能改变 Tool Registry、Risk、Approval 或 target scope。

```text
Native Typed Investigation
          ↓
Live Incident (bounded remote evidence)
          ├── exact ChangeSet/Fleet binding → existing execution pipeline
          ├── Operator Handoff / guarded Closure
          └── content-free Repository / Sanitized Export
```

```text
Model Provider
 ↓
Agent Runtime
 ↓
Native / MCP Tool Adapter
 ↓
Tool Registry
 ↓
Policy / Risk
 ↓
Approval / ChangeSet（写操作）
 ↓
existing Domain Service
 ↓
ServerSession
```

Agentic Core 的目标边界包括 Agent Runtime、Context Manager、Tool Registry、Policy Engine、Approval Engine、ChangeSet Engine、Verification / Rollback、Audit Recorder、Skill Engine，以及后置的 MCP Client。

服务器核心能力保持 Native Typed Tool：`system.*`、`service.*`、`file.*`、`network.*`、`http.*`、`nginx.*`、`docker.*`。MCP 用于扩展服务器之外的上下文，不替代 SSH / SFTP Core。

任何 Agent 产生的远程状态修改默认遵循：`Diagnosis → Plan → ChangeSet → Approval → Execute → Verify → Commit / Rollback`。第一阶段 Agent 只做 Read-only Server Doctor，不具备远程写能力。

推荐目录是目标边界而不是迁移要求；不得为了匹配目录树批量移动现有模块，也不得直接把现有 `AiAgentService` 改名冒充完整 Agent Runtime。

## 27. Architecture Invariants

1. Secret 不进入 Profile JSON。
2. Terminal Output 不进入 React Global State。
3. UI 不拥有 SSH Transport。
4. Host Verification 不得全局关闭。
5. Rust 返回 Stable Error Code，UI 负责翻译。
6. Service 不直接依赖 JSON。
7. ServerSession 不等于永久的一对一 Terminal Tab。
8. Core 不依赖 Node.js。
9. Capability Least Privilege。
10. AI / Agent 必须经过 Tool Registry、Policy 和正常 Domain Service。
11. Agent 不拥有 Credential / Vault Secret。
12. 所有 Agent Write Operation 默认进入 ChangeSet。
13. Skill / MCP 不得绕过 Tool Permission。
14. Remote / MCP / Terminal Content 都属于 Untrusted Data。
