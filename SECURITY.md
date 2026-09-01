# Runory Security Architecture

> Implementation note: 本文同时记录当前已落地安全基线与 future architecture。标有 “Future Agentic” 的规则不表示对应 Agent Runtime 已实现；但一旦开发相关能力，这些规则立即构成强制安全边界。既有 SSH、SFTP、Operations、Deployment、Mobile、有限 AI 与 Cloud 安全边界继续有效。

## 1. Security Posture

Runory 是基础设施客户端，会保存/使用高价值 SSH Credential，并直接连接生产服务器，因此安全要求高于普通桌面 CRUD 产品。

安全默认值属于产品功能，而不是“高级设置”。

## 2. Threat Model

重点考虑：

- Credential Theft
- Private-key Leakage
- Compromised WebView Content
- Overpowered IPC
- MITM / Host Spoofing
- Host Key Changed
- Secret Logging
- Metadata Corruption
- Unsafe Vault Fallback
- Future Command Injection
- Remote Content Abusing Native Capability

## 3. Trust Model

### Lower Trust

- React/WebView
- User Input
- Remote SSH Output
- Terminal Escape Sequence
- Imported Paths
- Future AI Output

### Trusted

- Rust Core
- CredentialService
- KnownHostService
- Narrow Tauri Commands
- Secure Vault

## 4. Capabilities

遵循 Least Privilege。

不得随意暴露：

```text
shell
process
unrestricted filesystem
unrestricted HTTP
generic IPC execute
```

Production WebView 只加载 Bundled Local Content。

## 5. CSP

Production 禁止 Remote Script。

外部网页必须交给系统浏览器打开，不得给 External Web Content Runory Native Capability。

## 6. Secret Classification

Secrets：

```text
SSH Password
Private Key Passphrase
Private Key Content
Vault Master Secret
Future Tokens
```

Sensitive Infrastructure Metadata：

```text
Host/IP
Username
Group
Host Fingerprint
```

不要把 Infrastructure Metadata 当成“无敏感性 Telemetry”。

## 7. Credential Persistence

Profile 绝不能包含：

```text
password
passphrase
privateKeyContent
masterPassword
```

Remembered Credential：

```text
CredentialService
 ↓
CredentialVault
 ↓
Desktop: StrongholdCredentialVault + PlatformKeyStore
Mobile: PortableCredentialVault
```

Session-only 只存内存。

## 8. Vault UX

默认：

```text
Session Only
```

用户打开 Remember Securely 时：
- initialize / unlock vault
- 以 stable profile ID 存储
- 不提供“从前端读取记住密码”的通用 API
- SSH Connect 时由 Rust 内部获取

## 9. Secret Lifetime

尽可能：
- secret wrapper
- avoid Clone
- short scope
- zeroize after use
- no Debug formatting

## 10. Private Key

Desktop：
- React 选择文件路径
- Rust 读取文件内容
- Rust 解析
- Private Key Content 不返回 React

Mobile：
- Rust import to vault
- Profile 只保存不透明 keyId

## 11. Logging

使用 structured `tracing`。

允许：

```text
profile_id
session_id
host
port
auth_method
error_code
duration
```

禁止：

```text
password
passphrase
privateKeyContent
vault key
raw secret buffer
```

## 12. Host Verification

Mandatory。

Unknown：
- Trust Once
- Trust & Remember
- Cancel

Trusted Match：
- Continue

Changed：
- Block

绝不添加：

```text
AcceptAllHosts
SkipHostVerification
StrictHostKeyChecking=no
```

## 13. Fingerprint

使用标准 SHA256-style Fingerprint。

Known Host 至少绑定：
- Host
- Port
- Key Type
- Fingerprint

## 14. Host Verification Decision

Unknown Host 决策必须：
- 绑定单次 Connection Attempt
- 可 Cancel / Timeout
- 不可复用到别的 Host
- Trust Once 不写入
- Trust & Remember Atomic Persist

## 15. IPC

Good：

```text
ssh_connect(profile_id)
ssh_disconnect(session_id)
profile_update(profile)
known_host_remove(id)
```

Bad：

```text
execute(anything)
read_file(any_path)
run_shell(any_command)
```

## 16. Input Validation

Frontend Zod 只是 UX。

Rust 必须再次 Validate：
- UUID
- Host
- Port
- Username
- Key Path
- Group Ref
- Enum
- Sort Order
- Session Existence

## 17. Terminal Output

Remote Output 是不可信数据。

不得把 Terminal Output 当：
- HTML
- Markdown
- App Command
- Native Instruction

Web Link 应走安全 External Browser。

## 18. Storage

Metadata：
- App Data
- Atomic Write
- no secret

Credential：
- Vault only

## 19. Errors

Rust 返回稳定 Error Code。

Error Context 不得含 Secret。

Technical Details 必须 Sanitized。

## 20. Panic Policy

SSH / Credential / Storage Core Path 避免：

```rust
unwrap()
expect()
panic!()
```

使用：

```rust
Result<T, AppError>
```

Malformed Remote Data 不得导致 App Crash。

## 21. Dependency Security

新增安全相关依赖前：
- maintenance
- license
- advisory
- mobile compile
- lockfile

及时跟进 Tauri、SSH、Crypto 安全版本。

## 22. Mobile Security

移动端额外考虑：
- App Sandbox
- OS Backup
- App Switcher Snapshot
- Clipboard Leakage
- Biometric Unlock

未来应考虑：
- App Switcher 隐私遮罩
- Face ID / Touch ID / Android Biometrics
- 不将 Secret 长时间放剪贴板

Phase 6 已建立以下基线：

- 进入后台后立即设置 App Switcher 隐私遮罩；返回时使用 iOS/Android 原生生物识别或设备凭据
- 生物识别只恢复当前存活会话，不保存、不推导、不向 React 返回 Vault Master Secret
- Desktop 使用 Stronghold；Android/iOS 使用 Argon2id + AES-256-GCM 的纯 Rust 加密仓库，两者实现相同 `CredentialVault` 接口
- Desktop 的 Vault 解锁 Secret 只写入 OS 原生安全存储（Windows Credential Manager、Apple Keychain、Linux Secret Service/keyutils），不写入 Runory Repository、日志或 React 状态
- 既有 Desktop Vault 只在用户成功输入一次原密码后迁移；运行时只读探测确认系统安全存储不可用时允许本次会话密码解锁，探测成功后若实际写入失败则立即重新锁定，不把“本次已解锁”误报为迁移成功
- 私钥导入由 Rust 读取，限制为普通文件、非空且最多 1 MiB；React 仅持有不透明 `key_id`
- 移动 SFTP 继续使用原生文件选择器与一次性、类型限定的 `LocalFileGrant`
- Desktop 进程重启后可从同一 OS 用户的系统安全存储自动解锁；免密码 UI 以运行时可用性而不是编译平台为准。Android/iOS 在移动 PlatformKeyStore 完成前仍必须重新输入 Vault 主密码

## 23. SFTP

Remote Path 是不可信输入。

下载必须防：
- Local Path Traversal
- 自动执行下载文件

Upload/Download 必须来自明确用户意图。

## 23.1 Remote Operations / Deployment

Phase 3–5 禁止暴露 raw SSH exec IPC。

必须：
- 每个 IPC command 对应明确业务动作
- 远端程序和脚本模板由 Rust Core 固定
- 动态参数经过 Rust 验证并作为安全 quoting 的位置参数传递
- Exec timeout、stdin 和 stdout/stderr 有硬上限
- Docker / PM2 / Nginx / Deployment 变更要求 UI 明确确认
- `.env` 内容仅经 stdin 传输，不记录、不进入 Zustand、不写入 Deployment History
- Cron 只允许预定义任务模板，不接受任意命令文本
- Remote output 按不可信纯文本展示

Deployment History 只允许记录：profile ID、operation、target path/domain、时间、结果和 stable error code，不记录环境值、Git Credential 或命令输出。

## 24. AI Terminal 与有限类型化计划执行

AI Output 永远是不可信输入。当前 Phase 7/8 实现是本地规则提供器与用户显式构造的有限类型化计划，不是 `AGENTIC.md` 定义的自主 Agent Runtime。

Future Agentic 的强制执行链为：`AI Proposal → Typed Tool → Policy → Approval / ChangeSet → Domain Service → SSH`。

Phase 7 安全基线：

- 当前提供器完全在 Rust 本地运行，不访问网络，也不需要 API Key
- Terminal Context 是每个活动 `ServerSession` 最多 32 KiB 的易失尾部缓冲，不记录、不持久化、不原样返回 React
- Explain / Generate / Diagnose / Propose Fix 只返回结构化分析与不可信命令建议
- 所有建议（包括低风险）都必须经过确认弹窗；确认只插入文本，不发送回车
- AI 路径无权调用 unrestricted shell、`ExecChannel`、CredentialVault 或 SFTP 写操作
- 风险、用途、诊断和信号使用稳定 machine code，由 React 本地化

Phase 8 的有限工具仍必须逐工具经过 Policy、Approval、Domain Service 和审计；高风险动作必须可审查。

### 24.1 Phase 8 执行基线

Phase 8 已建立以下执行基线：

- 禁止 raw command IPC；`terminal.exec` 只有 Disk、Memory、Listening Ports、Recent Errors 四个 Rust 预设
- 每个步骤先写入 `IN_PROGRESS` 审计记录，再开始远端副作用；完成后原子更新成功或 stable error code
- 所有步骤都需逐项批准，批准状态绑定 `plan_id + step_id`，且步骤只能执行一次
- `file.read/write` 只允许最多 512 KiB 的 UTF-8 内容；写入使用远端临时文件并重命名
- `file.write` 内容不返回计划 UI、不进入日志或审计，执行完成或失败后从计划内存移除
- 放弃计划或离开 Agent 视图时显式 Discard，清除仍未执行的 Rust 内存工具载荷
- Audit 只记录 profile、tool、risk、target、时间、结果；不记录 Goal、输出、文件内容或凭据
- Multi-server Plan 最多 10 个活动 Session，不使用保存的离线凭据静默重连

### 24.2 Future Agentic Security

Phase 10A–10G 已实现封闭 Native Tool Registry、R0–R3 Policy、执行前后 sanitized Audit、显式 Cancellation、Read-only Server Doctor、版本化 ChangeSet / Approval / Verification / truthful Rollback、Skills、只读 MCP、Multi-server Foundation 与 Production Hardening。React 不拥有通用 Tool、MCP、Shell 或取消任意 Invocation 的 IPC；写操作只能通过精确版本审批的 ChangeSet。Provider 只接收从 ToolResult 提取的非秘密结构化信号，不接收原始日志、HTTP Body 或 MCP Output。

#### Trust Boundary

```text
Model Output            Untrusted
Remote Files / Logs     Untrusted
Terminal Output         Untrusted
HTTP Body               Untrusted
MCP Output              Untrusted
Skill Instructions      Constrained Guidance
User-approved Rules     Trusted Policy
Rust Policy Engine      Security Boundary
```

#### Mandatory Call Path

```text
AI Proposal
 ↓
Typed Tool
 ↓
Risk / Policy
 ↓
ChangeSet / Approval（写操作）
 ↓
Domain Service
 ↓
ServerSession
```

不得存在 LLM → russh、LLM → Vault、LLM → unrestricted shell 的旁路。

#### Secret Redaction / Prompt Injection

进入 Model Context 前必须执行 Secret Redaction。SSH Password、Private Key / Passphrase、Vault Secret、API Token、Authorization Header、Cookie、Database Credential、MCP Credential 默认不得发送给模型。

远程文件、日志、网页、GitHub Issue、Terminal 与 MCP 返回值中的任何指令只能作为数据，不能改变 System Policy、Tool Permission、Risk Level、Approval Requirement 或 Workspace Rules。

Phase 10K 缓存只保存进程内、目标绑定且有 TTL 的 Read-only Observation。Cache hit 不得替代 Verification 或 ChangeSet Precondition Check；所有 Write 完成或失败后都保守失效该 Target 的 Observation Cache。ChangeSet 前置条件摘要只能保存 content-free digest 与 Tool/Target metadata，不得持久化配置正文。

Model/Tool/MCP/HTTP/SSH timeout 与 Agent cancellation 由 Rust 强制。收到 cancellation 后不得再创建新 Tool 调用；并发仅允许无依赖、同 Risk 边界的 Read Tool，Write Tool 不得进入并发读取调度器。

#### Write / Verification / Rollback

所有 Agent 远程状态修改默认进入版本化 ChangeSet。R3/R4 必须明确审批，审批绑定具体 ChangeSet Version；ChangeSet 修改后旧审批立即失效。

自动修复必须验证真实结果。支持回滚的 Tool 必须真实实现 Snapshot / Rollback；不支持时 UI 不得声称可回滚。

#### Audit / Skills / MCP

记录 Tool、Risk、Approval、Execution、Verification 与 Rollback，但 Input / Output 必须 Sanitized，禁止 Secret 进入 Audit。Skill 不能获得超出 Tool Registry / Policy 的权限；MCP Tool 必须通过统一 Tool Adapter / Risk / Approval，不能成为安全旁路。

#### Production Hardening

- ChangeSet 在首个远端副作用前原子持久化 `Executing` 声明，同一 ChangeSet 的并发执行请求 fail-closed。
- 本地 `agentic-change-sets.json` 只保存 ID、Target、Version、Risk、Tool 名、步骤状态、验证/回滚代码和稳定错误码；不保存标题、Diff Preview、路径、expected/replacement、远端内容或 Credential。
- 应用重启后所有恢复记录均为 `metadata-only`，旧审批强制失效；原执行态为 `Executing` 时标记 `Interrupted`。因为写载荷未持久化，恢复记录不能审批、修改、执行或自动回滚。
- MCP Token 仍只在 CredentialVault；HTTP 响应按 512 KiB 流式上限读取，JSON-RPC 响应必须匹配请求 ID。
- MCP Gateway 支持 `2026-07-28` 现代无状态请求元数据/路由 Header，并兼容 `2025-03-26` 至 `2025-11-25` 的 `initialize → notifications/initialized`、Protocol Header 与内存 Session ID。Session ID 不持久化。
- 只读 Tool 必须显式逐项启用并由用户在 Doctor 中选择。带 `x-mcp-header` 的 Tool 在当前未实现安全 Header 映射前不进入可用列表；Deprecated HTTP+SSE 长连接不在当前边界。
- Fleet Approval 同时绑定 Fleet Version、完整且精确的 Target Session ID 集合，以及每个 Target 的 ChangeSet ID / Version；任一 Target ChangeSet 修订会自动失效 Fleet Approval。
- Multi-target write 默认 `Sequential + Pause for Review`。Production Parallel All 在 Rust 校验层拒绝；Canary / Rolling Batch 不能越过已审批 Target 集合。
- Pause 后显式继续只执行仍为 Pending 的 Target；成功、失败、待执行和回滚状态分别保留，不允许因重试抹平历史。
- Fleet Verification 分为 target-local、cross-target 与可选 service-level；验证失败同样进入 Failure Policy，不得仅凭写 Tool exit code 宣称 Fleet 成功。
- `Rollback` 按完成顺序逆序逐 Target 调用原 ChangeSet 的真实 rollback；包含不支持回滚的已执行步骤时如实标记 Rollback Failed。

#### Production Operations Packs

- Incident 调查只接受结构化 Pack 参数与最多 10 个已连接 Session，不接受 raw shell。
- DNS/TLS/进程/监听/配置/磁盘/Docker 探针均为 Registry 内 R1 Typed Tool；配置预览和日志有界且执行 Secret Redaction。
- Root Cause 必须包含 Evidence ID；证据不足时只能返回 Inconclusive。
- Incident 绑定 ChangeSet/Fleet 时重新验证精确 Target 集合与 Version，不能扩大执行范围。
- Disk Pack 不包含删除 Tool。Docker restart 为 R3、无回滚声明，并在成功后重新读取容器状态。

#### Incident Lifecycle

- `agentic-incidents.json` 只保存结构化生命周期元数据，不保存症状、ToolResult、日志、HTTP body、配置预览、路径内容或操作员正文。
- 恢复记录标记 `metadata-only`；原状态为 Investigating / Correlating / Executing / Verifying 时转换为 `Interrupted`，不得自动续跑。
- Cross-target comparison 只允许确定性标量（状态码、布尔、版本/平台标识、使用率等），禁止以原始 Remote Content 作为持久差异。
- Resolved closure 必须绑定成功 Verification；Accepted Risk 必须有显式操作员依据；False Positive 只允许 Evidence 不足的 Inconclusive Incident。
- Incident Audit Export 与 Repository 使用相同 content-free 边界，并通过 `contentOmitted=true` 明确声明省略原始内容。
- Handoff / Closure / Export 是窄业务 IPC，不提供任意文件写入、Shell 或 Tool 执行能力。

#### Production Qualification

- Incident Repository 在构造内存状态前验证 schema、数量上限、Target 唯一性、Evidence 引用闭包、Comparison Target 集合及 ChangeSet exact binding。
- v1→v2 migration 只重写已通过验证的 content-free metadata；损坏 JSON 或结构篡改不得被自动覆盖、截断或静默忽略。
- Fault injection 仅存在于 Docker OpenSSH 测试镜像，通过固定 fixture 文件改变 Typed Service 状态；生产 Registry 与 IPC 不包含 fault/chaos 开关。
- 多目标真实测试仍执行 Mandatory Host Verification，并为每个端点扫描和绑定独立 fingerprint。

## 25. Supabase Cloud Foundation

跨设备 Credential / Private Key 同步的 future proposal 见 `docs/CLOUD_SYNC_SECURITY_ARCHITECTURE.md`。在其 device trust、recovery、migration 与 production gate 完整落地前，当前 Phase 9 的凭据不上传边界不得放宽。

- 客户端只允许 `sb_publishable_*` Key；禁止 service-role/secret key 进入 Vite 环境
- Email Password Session 只驻留当前进程，关闭 `localStorage` 持久化
- `anon` 对业务表无权限；`authenticated` 只有显式最小 GRANT
- 所有 public 业务表强制启用 RLS，Organization membership 是租户隔离依据
- 授权不读取 `user_metadata`；Owner/Admin/Operator/Viewer 来自数据库成员表
- RLS 中的 `auth.uid()` 使用标量子查询并为成员、外键和时间分页列建立索引
- Supabase 只允许保存未来由 Rust 生成的不透明密文；Credential、Key、Vault、Terminal Output 永不上传
- Local-only 模式不需要账号，Supabase 未配置或不可用时不影响 SSH Core
- 同步口令只通过一次性 Tauri Command 进入 Rust，不写入 React State、Zustand、日志或 Supabase
- Cloud Snapshot 使用 Argon2id + AES-256-GCM；Organization ID 进入 AAD，密文不能跨 Organization 重放
- `authenticated` 无权直接 INSERT/UPDATE 同步对象；唯一写入 RPC 重新校验 Organization Role，并通过 `expected_revision` 原子更新，revision 不匹配返回冲突
- 拉取 Preview 只在 Rust 内存短暂保存，显式应用/取消或离开页面后清除；本地较新与冲突项不自动覆盖
- 邀请接受的 `security definer` RPC 已撤销 PUBLIC/anon 执行权，只接受与 Auth 已验证邮箱匹配的未过期邀请
- tombstone 仅记录已知 UUID 和删除时间，不包含服务器名称、地址或凭据；首次同步不会凭“远端缺失”推断删除
- 冲突决策绑定 `kind + id + expected_local_updated_at`，Apply 在 Rust 写锁内重新比较，默认始终保留本地
- `authenticated` 无权直接创建、更新或删除邀请；创建、接受、撤销均通过重新校验角色/邮箱并写 Audit 的受限 RPC
- 成员邮箱列表只向对应 Organization 的 Owner/Admin 返回；所有 `security definer` RPC 均固定空 `search_path` 并撤销 PUBLIC/anon EXECUTE
- 成员表与 Access Policy 表对客户端撤销 INSERT/UPDATE/DELETE；Owner/Admin 能力由 RPC 重新判断，Admin 不能修改或移除 Owner/Admin
- Audit 使用 Organization + `(occurred_at, id)` 游标分页，单页强制限制在 1–100 条，不接受无界客户端读取
- Audit 清理由 private 特权函数完成，客户端没有 EXECUTE；保留天数被限制为 30–3650 天，单批最多 10,000 条并使用 `SKIP LOCKED`
- Access Policy 由 Rust 在类型化操作命令边界调用固定 Supabase RPC；浏览器层不做允许/拒绝判断
- 持久策略文件只包含 Organization/Profile UUID 与全设备作用域标记；Publishable Key、access token 与过期时间仅存 Rust 内存，不落盘、不记录日志
- 退出登录只清除短期凭据，不会悄悄移除持久绑定；再次登录或 `TOKEN_REFRESHED` 时恢复内存凭据
- 组织绑定覆盖本设备全部 Profile；新建和云同步 Profile 自动纳管，安全判断不依赖 UUID 快照刷新，因此没有授权空窗
- 未绑定设备保持 Local-only；已绑定设备上的 Profile 只允许权威在线结果或未过期的 Ed25519 签名决策，其他未登录、拒绝、令牌过期、网络失败或无效响应均 fail-closed
- 离线策略包绑定版本、keyId、Organization、Profile、Action、Decision、签发时间和过期时间，最大 TTL 为 300 秒；Rust 每次使用都验证签名与上下文，缓存中不保存裸允许值
- 策略签名私钥只存在 Supabase Edge Function Secret；Rust 二进制最多固定四个带 keyId 的 Ed25519 公钥，React 无权替换信任根或参与验签
- 密钥轮换必须先发布同时信任新旧 keyId 的客户端，再切换 Edge Function active key；未知 keyId 始终 fail-closed
- 当前决策采用 deny-overrides；某动作没有 allow 策略时默认允许，一旦定义 allow 则只有匹配 allow 的 Profile 可执行；空 selector 作用于全部 Profile，`profileIds` selector 可限定 Profile
- Supabase Auth 邮件部署只访问固定 Management API endpoint；Access Token 与 SMTP 密码只从被忽略的 `.env` 或 CI Secret 读取，不打印、不进入 Vite Bundle
- 邮件模板禁止 Script 与远端资源，必须包含 Supabase `ConfirmationURL`，单个模板限制为 64 KiB；生产部署后重新读取并验证非秘密 Auth 配置
- Runory Organization 邀请仍为数据库内站内邀请；客户端不会为了发送邮件而持有 Service Role Key
- Supabase pgTAP 门禁覆盖最终 GRANT、RLS 跨组织隔离、直接 DML 拒绝、Security Definer ACL/search path、deny-overrides、审计、Cron schema 隔离、有界保留批次与关键索引
- 远端发布门禁只执行固定只读 SQL 和 Management API List/GET；Project Link、Migration Push、Function Deploy、Secret Set 与 Auth Apply 永远是分离的显式发布动作
- 远端验证只比较 Edge Secret 名称，不读取或输出 Secret 值；数据库探针不把密码或动态 SQL 放入命令参数
- 机器可读发布证据只包含公开 Project Ref、版本、布尔结果、Cron 状态与源码 SHA-256；禁止包含 CLI 原始响应、Token、JWT、连接串、SMTP 密码或签名私钥
- Production 部署前必须用新鲜 Staging `release_ready` 对比本地 `tooling_ready` 候选；部署后再与 Production `release_ready` 复核。同一 Project、额外字段、过期证据、Migration 或应用源码摘要漂移全部 fail-closed

## 26. v0.1 Security Checklist

- [ ] Profile JSON 无 plaintext credential
- [ ] Logs 无 secret
- [ ] 无 unrestricted FS
- [ ] 无 unrestricted shell
- [ ] Host Verification mandatory
- [ ] Changed Key blocks
- [ ] Production CSP reviewed
- [ ] Remote URL 无 privileged capability
- [ ] Vault lock/unlock tested
- [ ] Wrong key/passphrase tested
- [ ] Network failure tested
- [x] JSON corruption recovery tested（Incident metadata fail-closed and source preserved）
- [ ] Dependency audit reviewed
