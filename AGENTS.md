# AGENTS.md — Runory Development Rules

本文件是 AI Coding Agent 在 Runory 仓库中的强约束。

除非用户明确改变架构，否则任何实现不得违反本文件。

## 1. Product

Runory 是 Local-first、跨平台 SSH / Infrastructure Management Client。

Runory 已从 Phase 1 的 SSH Core 演进到多阶段基础设施工作区。当前仓库中已经存在并允许维护的稳定基线包括：SSH Core、SFTP / Files、Dashboard、Typed Operations、Deployment、Mobile Product、有限本地 AI Terminal / 类型化计划执行，以及可选 Cloud Foundation。

这些既有能力不得因为早期 Phase 文档而被删除或大规模重构。完整 Agentic Infrastructure Workspace 仍是 future architecture，详见 `AGENTIC.md`。

目标平台：

```text
Windows
macOS
Linux
iOS
Android
```

Desktop 首发，Mobile 架构必须保持可行。

## 2. Mandatory Stack

Frontend：

```text
React
TypeScript
Vite
Tailwind CSS
shadcn/ui
Lucide
Zustand
Zod
React Hook Form
i18next / react-i18next
xterm.js
```

Core：

```text
Tauri 2
Rust
Tokio
russh
serde
thiserror
tracing
```

禁止引入：
- Electron
- ElectronEgg
- Next.js
- Node.js-only Infrastructure Core

## 3. Scope

Phase 1 的历史基线包括：

- Host Profile
- Group
- Search
- Password Auth
- Private Key Auth
- Passphrase
- Credential Vault
- Test Connection
- SSH Connect / Disconnect / Reconnect
- Host Key Verification
- Interactive PTY
- Multi-session / Tabs
- Theme
- zh-CN / en-US
- Desktop Build
- Android/iOS SSH Spike

当前代码中已经实现的 Phase 2–9 能力可以修复、测试和做小步演进，但现有代码不代表 `AGENTIC.md` 已完成。

未经用户明确要求和对应 Roadmap 边界，不得新增或扩大：

```text
Port Forwarding
Jump Host
Proxy
Billing
Remote Telemetry
完整 Agent Runtime
自主远程修复
Skills Runtime / Marketplace
MCP Client / Gateway
RDP
```

不得为了让目录或命名匹配 future 文档而重写稳定 SSH Core、`ServerSessionManager`、SFTP、Typed Operations 或现有 Repository。

## 4. Core Architecture

强规则：

> React = Interaction Layer  
> Rust = Infrastructure Engine

React 禁止实现：
- SSH
- TCP
- Credential Persistence
- Private-key Parsing
- Host Verification
- Arbitrary FS
- Process Execution

Rust 负责全部上述能力。

## 5. Secrets

`ServerProfile` 永远不能存：

```text
password
passphrase
privateKeyContent
vaultMasterSecret
```

Persisted Credential：

```text
CredentialService
 ↓
CredentialVault
```

UI 不得拥有“读取已记住密码明文”的 API。

Transient Secret 只有用户当前操作时可以一次性提交给 Rust。

Secret 永远不能出现在日志。

## 6. Host Verification

Mandatory。

禁止：

```text
accept_all_hosts
skip_host_verification
StrictHostKeyChecking=no
```

Unknown：
- Trust Once
- Trust & Remember
- Cancel

Changed：
- Block

## 7. Tauri Security

Least Privilege。

未经明确需求，不允许 Broad：
- shell
- filesystem
- process
- HTTP

禁止万能 API：

```text
execute_anything
read_any_file
run_shell
```

Command 必须 Business-specific。

## 8. IPC

Request/Response 使用 Command。

Terminal Output 使用 Tauri Channel。

禁止 Terminal Output 通过：
- Zustand
- React State
- generic event spam

禁止持久化 Terminal Output。

## 9. TerminalView

允许：
- Initialize xterm
- Addons
- Keyboard
- Resize
- Bind Channel
- Write Output

禁止：
- Connect SSH
- Access Vault
- Read Private Key
- Own russh Handle

## 10. Zustand

可以存：
- Display Profiles
- Groups
- Session UI Metadata
- Active Tab
- Settings

禁止存：
- Terminal Output
- Password
- Passphrase
- Private Key
- russh Handle

Session State 使用：

```text
idle
connecting
verifying-host
authenticating
opening-shell
connected
disconnected
error
```

禁止多个 Boolean 拼连接状态。

## 11. Domain Model

保持分离：

```text
HostGroup
ServerProfile
ServerSession
TerminalChannel
```

不得把 ServerSession 永久建模为一个 Terminal Tab。

当前 SFTP / Exec 依附 ServerSession；未来新增 Channel 继续遵守这一边界。

## 12. Persistence

Phase 1 Metadata 使用 JSON Repository。

业务 Service 不得到处 raw fs read/write。

必须通过 Repository。

Write 必须 Atomic。

未来 SQLite 不得要求重写 SSHService。

## 13. Private Key

不得只建模 Desktop Path。

必须支持概念：

```text
File
Vault
```

Desktop：File  
Mobile：Vault（Phase 6 已实现导入与不透明 `key_id`）

## 14. Rust Errors

核心尽量避免：

```rust
unwrap()
expect()
panic!()
```

用 Typed Result。

跨 IPC 返回 Stable Error Code，不返回已翻译文案。

错误不能包含 Secret。

## 15. i18n

所有用户可见 UI 文案必须 i18n。

第一阶段：

```text
zh-CN
en-US
```

禁止 React 中硬编码可见中文/英文。

不翻译：
- User Server Name
- Group Name
- Username
- Path
- Terminal

“Ungrouped”是系统虚拟文案，要翻译。

Rust 不管理 UI Translation。

## 16. UI

遵循 `DESIGN.md`。

使用：
- shadcn/ui
- Lucide
- Design Token

禁止增加第二套完整 UI Framework。

避免：
- Excessive Gradient
- Neon Hacker Style
- Random Shadow
- Fixed Chinese-width Layout

## 17. Mobile

Core 不得假设：
- `~/.ssh` 永远存在
- Arbitrary Path 可访问
- Desktop Window API 永远存在
- Mouse 存在
- Physical Keyboard 存在

Mobile UI 可以与 Desktop 不同，但 Core 共用。

## 18. Dependencies

新增 Dependency 前：

1. 为什么必须
2. 现有 Stack 是否已能解决
3. Maintenance
4. License
5. Rust Core 是否 Mobile Compile
6. 不为“以后可能用”提前安装

JS Package Manager：

```text
pnpm
```

只保留：
- pnpm-lock.yaml
- Cargo.lock

不要混 npm/yarn lock。

## 19. Testing

SSH Integration 必须使用真实 Docker OpenSSH。

必须覆盖：

```text
Password Auth
Private Key Auth
Encrypted Key
Wrong Password
Wrong Key
Wrong Passphrase
Unknown Host
Changed Host
PTY
Resize
Disconnect
Reconnect
```

Frontend：
- Connection Form
- Group UI
- Session State
- Error Localization
- i18n Completeness

## 20. Logging

使用 `tracing`。

允许：
- profile_id
- session_id
- host
- port
- auth_method
- error_code
- duration

禁止：
- password
- passphrase
- private key data
- vault key

## 21. Code Organization

禁止 Giant File。

优先拆：
- domain
- service
- repository
- command adapter
- feature UI

Command Thin，Component Presentation-oriented。

## 22. Naming

Good：

```text
ServerSessionManager
KnownHostService
CredentialVault
```

Bad：

```text
UtilsManager
CommonService
Helper2
doStuff
```

## 23. Comments

Comment 用于解释：
- Security Invariant
- Protocol Behavior
- Non-obvious Workaround
- Architecture Decision

不要注释显而易见代码。

## 24. Quality

TypeScript：
- strict
- 禁止随意 any

Rust：
- cargo fmt
- cargo clippy
- typed errors

Feature 完成前：
- tests
- lint/type-check
- i18n completeness
- no broad capability
- no secret logs

## 25. Scope Discipline

架构可以为未来扩展，但当前 PR / Task 只能实现被明确授权的最小范围。`AGENTIC.md` 是 Agent / Tool / Skill / MCP 的详细设计基线，不是一次性开发授权。

禁止以“Agentic Foundation”为理由在一个任务中同时实现：

```text
Agent Runtime + Skills + MCP + 自动修复 + Marketplace
```

即使 Future 功能已经出现在文档或设计稿中，也不得用 Mock Data 假装已经实现。Agentic 能力必须建立在稳定 SSH / SFTP / Domain Service 上，并按 `ROADMAP.md` 分阶段交付。

## 26. Agentic Hard Rules

当前 Phase 7/8 的本地规则分析和有限类型化计划执行继续独立保留。Phase 10A–10J 已建立 Rust Agent Runtime、受控 Native Tool、受审批写 Tool、ChangeSet / Verification / Rollback、Skills、只读 MCP、Multi-server Foundation、Production Hardening、五类 Production Operations Pack、Incident Lifecycle 与 Production Qualification。ChangeSet 和 Incident 只持久化无内容元数据，重启后旧审批失效且绝不自动续跑；MCP 同时支持现代无状态协议与初始化型 Streamable HTTP。仍不存在通用 Tool/MCP/Shell IPC；所有调用必须来自 Rust Runtime 或版本绑定的 ChangeSet。

Phase 10G Fleet 写入默认 Sequential + Pause for Review；Production Parallel All 必须在 Rust 拒绝。Fleet Approval 绑定 exact target IDs 和每个 target ChangeSet version，任何 target/version 变化都必须失效审批。Target execution / verification / rollback 状态独立可追踪，失败后不得自动扩大 target 范围。

Phase 10H Incident Root Cause 必须引用真实 Tool Evidence。Website/Nginx/Docker/Disk/Service Pack 只能编排 Registry 内的 Native Typed Tool；Repair 必须绑定现有单目标 ChangeSet 或 Fleet ChangeSet。Disk Pack 不得生成或执行自动删除动作。

Phase 10I Incident Repository 与 Audit Export 只能包含 content-free lifecycle metadata。恢复记录必须标记 `metadata-only`，进行中的状态转换为 `Interrupted`；Resolved closure 必须已有成功 Verification，且不得因 Handoff、Closure 或 Export 扩大 Tool/Target 权限。

Phase 10J Production Qualification 不新增生产执行能力。Incident metadata migration 必须先通过完整引用校验；损坏记录 fail-closed 且不得覆盖原文件。Fault Lab 只能存在于测试 fixture，不能通过生产 IPC 或 Tool Registry 启用。

Phase 10K Context / Cache / Model / Budget 优化不得改变 Tool Permission、Risk、Approval 或 Target Scope。Observation Cache 只允许 Read Tool，并绑定 Target、Source、Observed-at 与 TTL；Write 后必须失效相关 Target Cache。Verification 与 ChangeSet Precondition 必须绕过 Cache 和 Dedup。审批后前置条件变化必须停止并失效审批，不得自动覆盖或继续写入。

未来 Agentic 开发必须同时遵循 `AGENTIC.md` 与 `SECURITY.md`：

1. LLM 不得直接访问 russh、CredentialVault、任意 Shell 或任意 Filesystem。
2. Agent 只能调用 Rust Tool Registry 中注册的 Typed Tool。
3. 有 Typed Tool 时不得用 Shell 绕过 Risk / Policy。
4. Tool 必须声明 Risk、Mutability、Scope 和 Approval Policy。
5. 默认 Read-only。所有远程状态修改必须进入 ChangeSet。
6. R3/R4 操作必须明确审批；审批绑定 ChangeSet Version。
7. 修复流程必须包含 Verification。
8. 支持 Rollback 时必须真实实现；不支持时不得向 UI 声称可回滚。
9. Credential / Secret 不得进入 Model Context、Memory、Audit。
10. Remote File、Terminal Output、HTTP Body、MCP Output 均视为 Untrusted Data。
11. Skill 不能提升 Tool 权限，也不能绕过 Approval。
12. MCP Tool 必须经过统一 Tool Registry / Policy，不得成为旁路。
13. Agent Tool Activity 必须可审计。
14. 不持久化模型隐藏推理；只持久化必要的可审计结果与结构化状态。

## 27. Task Is Not Done If

- UI 能跑但绕过 Architecture
- Host Verification 被削弱
- Secret 写入 Profile JSON
- Terminal Output 进入 React State
- UI 有硬编码可见字符串
- 测试/Lint 不通过
- 新增无必要依赖
- 没有理由地只支持单桌面平台
