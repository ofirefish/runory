# Runory TeleportProvider 开发设计与实施指南

> 文档版本：v1.0  
> 目标环境：Runory（Tauri 2 + React）/ Windows / Teleport 18.11.x  
> 基准环境：`teleport.local:3080`，TLS Routing / multiplex，测试节点 `teleport-node01`

## 1. 目标

Runory 需要在现有堡垒机抽象体系中增加 Teleport Provider，使用户能够：

1. 配置 Teleport Cluster；
2. 通过 `tsh` 完成登录、MFA 和短期证书获取；
3. 自动发现可访问的 Teleport SSH Nodes；
4. 从 Runory 打开 SSH 终端；
5. 复用现有 PTY、SSH、SFTP、端口转发和会话管理能力；
6. 正确处理证书过期、重新登录、节点离线、权限不足、Proxy 不可达等情况；
7. 不在 Runory 数据库中保存 Teleport 用户密码、MFA 秘密或 Teleport 私钥。

核心原则：

> **Runory 不重新实现 Teleport 的认证、MFA、CA、短期证书和代理协议。Runory 负责 Provider 编排、用户体验、SSH/PTY/SFTP 生命周期；Teleport 协议能力交给官方 `tsh`。**

## 2. 已验证的 Golden Path

当前测试环境：

```text
Windows
├── Runory
├── tsh.exe 18.11.x
└── Docker Compose
    ├── teleport-server
    │   ├── Auth Service
    │   └── Proxy Service :3080
    └── teleport-node01
        └── SSH Service
```

服务端启用：

```yaml
auth_service:
  proxy_listener_mode: multiplex
```

统一入口：

```text
teleport.local:3080
```

已验证命令：

```powershell
tsh login --proxy=teleport.local:3080 --user=runory-test --insecure
tsh status
tsh ls
tsh ssh root@teleport-node01
```

开发调试时应先确认这组命令仍可工作，再判断是否为 Runory Provider 问题。

## 3. Teleport 与 Boundary 的差异

Boundary：

```text
Runory
  ↓
Boundary API / Session
  ↓
127.0.0.1:随机端口
  ↓
Runory SSH Engine
```

Teleport：

```text
Runory
  ↓
tsh login
  ↓
短期证书
  ↓
tsh proxy ssh
  ↓
Teleport Proxy
  ↓
Reverse Tunnel
  ↓
Teleport Node
```

因此不要把 TeleportProvider 简单复制为 BoundaryProvider。

## 4. 推荐总体架构

```text
                    Runory UI
                       │
                BastionProvider
                       │
       ┌───────────────┴───────────────┐
       │                               │
 BoundaryProvider               TeleportProvider
       │                               │
 LocalTcpTransport              StdioProxyTransport
       │                               │
 127.0.0.1:port                  tsh proxy ssh
       └───────────────┬───────────────┘
                       │
                   SSH Engine
                 ┌─────┴─────┐
                 │           │
                PTY         SFTP
```

建议新增：

```rust
pub enum BastionTransport {
    DirectTcp {
        host: String,
        port: u16,
    },
    LocalTcp {
        host: String,
        port: u16,
        session_id: String,
    },
    StdioProxy {
        executable: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        session_id: String,
    },
}
```

Boundary 返回 `LocalTcp`，Teleport 返回 `StdioProxy`。

## 5. Provider 接口

```rust
#[async_trait]
pub trait BastionProvider: Send + Sync {
    fn provider_type(&self) -> BastionProviderType;

    async fn probe(&self) -> Result<ProviderProbe>;
    async fn auth_status(&self) -> Result<AuthStatus>;

    async fn authenticate(
        &self,
        request: AuthenticateRequest,
        events: ProviderEventSink,
    ) -> Result<AuthResult>;

    async fn list_targets(
        &self,
        request: ListTargetsRequest,
    ) -> Result<Vec<BastionTarget>>;

    async fn open_transport(
        &self,
        request: OpenTransportRequest,
    ) -> Result<BastionTransport>;

    async fn logout(&self) -> Result<()>;
    async fn diagnose(&self) -> Result<ProviderDiagnostics>;
}
```

Provider 类型：

```rust
pub enum BastionProviderType {
    Direct,
    OpenSshProxyJump,
    JumpServer,
    Boundary,
    Teleport,
}
```

能力声明：

```rust
pub struct ProviderCapabilities {
    pub host_discovery: bool,
    pub interactive_auth: bool,
    pub local_tcp_proxy: bool,
    pub stdio_proxy: bool,
    pub sftp: bool,
    pub scp: bool,
    pub port_forwarding: bool,
}
```

## 6. TeleportProfile 数据模型

```rust
pub struct TeleportProfile {
    pub id: String,
    pub name: String,
    pub proxy_addr: String,
    pub teleport_user: Option<String>,
    pub cluster_name: Option<String>,
    pub tsh_path: Option<String>,
    pub insecure: bool,
    pub tsh_version: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
}
```

Node：

```rust
pub struct TeleportTarget {
    pub provider_profile_id: String,
    pub node_name: String,
    pub node_id: Option<String>,
    pub hostname: Option<String>,
    pub address: Option<String>,
    pub labels: BTreeMap<String, String>,
    pub os_login: Option<String>,
    pub cluster_name: String,
}
```

必须区分：

```text
Teleport 用户：runory-test
OS Login：root
```

UI 建议明确显示：

```text
Teleport 账号：runory-test
服务器登录用户：root
```

## 7. tsh 探测

Windows 查找顺序：

1. 用户显式配置路径；
2. Runory tools 目录；
3. `%PATH%`；
4. 可选的常见用户目录。

调用时禁止 Shell 字符串拼接：

```rust
let output = tokio::process::Command::new(tsh_path)
    .arg("version")
    .output()
    .await?;
```

版本策略建议：

```text
Cluster 18 / tsh 18 -> 推荐
Cluster 18 / tsh 17 -> 可兼容但提示升级
tsh major > Cluster major -> 强警告或阻止
```

## 8. Authentication

Runory 禁止保存：

```text
Teleport Password
OTP Seed
MFA Secret
Teleport Private Key
短期证书内容
```

`tsh login` 的身份存储由 Teleport 管理，默认位于：

```text
~/.tsh
```

Runory 只保存：

```text
proxy_addr
teleport_user
cluster_name
tsh_path
```

生产登录：

```powershell
tsh login --proxy=teleport.example.com:443 --user=jim
```

开发：

```powershell
tsh login --proxy=teleport.local:3080 --user=runory-test --insecure
```

`--insecure` 只允许出现在高级/开发设置，生产默认关闭。

`tsh login` 是交互流程，可能涉及：

```text
Password
OTP
WebAuthn
Browser MFA
SSO
Browser URL
```

MVP 推荐直接复用 Runory PTY：

```text
TeleportAuthSession
  ↓
tsh login
  ├── stdout
  ├── stderr
  ├── stdin
  └── browser
```

不要依赖具体英文提示文案驱动状态机。

认证状态：

```rust
pub enum AuthState {
    Missing,
    LoggedOut,
    Valid {
        user: String,
        cluster: String,
        valid_until: Option<DateTime<Utc>>,
        os_logins: Vec<String>,
    },
    Expired,
    Invalid,
}
```

连接前：

```text
auth_status()
  ├── Valid   -> connect
  ├── Expired -> 提示重新登录
  └── Missing -> 提示登录
```

## 9. Node Discovery

主命令：

```powershell
tsh ls --format=json
```

第一版不要过早绑定 Teleport CLI JSON Schema：

```rust
let value: serde_json::Value =
    serde_json::from_slice(&output.stdout)?;
```

增加 Adapter：

```text
tsh JSON
  ↓
TeleportResourceAdapter
  ↓
TeleportTarget
  ↓
Runory Host UI
```

在当前 18.11 环境保存脱敏 fixture：

```powershell
tsh ls --format=json > tests/fixtures/teleport/tsh-ls-v18.json
```

建立 Golden Fixture 单测。

Labels 必须完整保存，用于搜索、过滤、分组和未来策略。

## 10. SSH 连接方案

最终生产实现优先使用：

```powershell
tsh proxy ssh
```

`tsh ssh` 用于：

```text
Golden Path
诊断
Fallback
```

### 10.1 方案 A：OpenSSH ProxyCommand

如果 Runory 当前是 `ssh.exe + PTY`，优先采用：

```powershell
tsh config --proxy=teleport.local:3080
```

把 stdout 保存到：

```text
%LOCALAPPDATA%\Runory\teleport\profiles\<profile-id>\ssh_config
```

不要污染用户全局 `~/.ssh/config`。

连接：

```powershell
ssh.exe -F <runory-ssh-config> root@<teleport-node-host>
```

优点：

- 改动现有 SSH 引擎小；
- Teleport 管理 IdentityFile / CertificateFile / CA / ProxyCommand；
- OpenSSH 处理 ProxyCommand。

### 10.2 方案 B：StdioProxyTransport（长期推荐）

如果 SSH Core 可接受 `AsyncRead + AsyncWrite`：

```text
SSH Engine
  ↓
child.stdin/stdout
  ↓
tsh proxy ssh
  ↓
Teleport Proxy
  ↓
Node
```

Rust：

```rust
let mut child = Command::new(&profile.tsh_path)
    .arg("proxy")
    .arg("ssh")
    .arg(format!("--cluster={}", cluster))
    .arg(format!("{}@{}", os_login, node_name))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true)
    .spawn()?;
```

**关键要求：**

```text
stdout = SSH transport
stderr = tsh diagnostics
```

严禁合并 stderr 到 stdout，否则会污染 SSH 字节流。

如果当前 SSH Core 写死 `TcpStream`，建议重构为：

```rust
trait SshTransport:
    AsyncRead
    + AsyncWrite
    + Unpin
    + Send
{}
```

使下列传输都可接入：

```text
TcpStream
Boundary LocalTcpStream
Teleport StdioProxyTransport
```

### 10.3 方案 C：直接 tsh ssh

```powershell
tsh ssh root@teleport-node01
```

只作为 MVP/Fallback，不建议成为长期主实现，因为会绕过 Runory 原生 SSH/SFTP Core。

## 11. 推荐版本路线

V1：

```text
tsh login
tsh ls --format=json
tsh config
OpenSSH ProxyCommand
```

V2：

```text
Generic SshTransport
StdioProxyTransport
tsh proxy ssh
```

V2 完成后，Terminal、SFTP、Forwarding 都可以复用 Runory SSH Core。


## 12. Tauri Backend API

建议暴露：

```rust
#[tauri::command]
async fn teleport_probe(
    profile: TeleportProfileInput,
) -> Result<ProviderProbeDto>;

#[tauri::command]
async fn teleport_auth_status(
    profile_id: String,
) -> Result<AuthStatusDto>;

#[tauri::command]
async fn teleport_login(
    profile_id: String,
) -> Result<String>;

#[tauri::command]
async fn teleport_cancel_login(
    auth_session_id: String,
) -> Result<()>;

#[tauri::command]
async fn teleport_list_nodes(
    profile_id: String,
) -> Result<Vec<TeleportTargetDto>>;

#[tauri::command]
async fn teleport_connect(
    request: TeleportConnectRequestDto,
) -> Result<String>;

#[tauri::command]
async fn teleport_logout(
    profile_id: String,
) -> Result<()>;

#[tauri::command]
async fn teleport_diagnose(
    profile_id: String,
) -> Result<TeleportDiagnosticsDto>;
```

## 13. Frontend UI

### 13.1 Teleport Profile

```text
堡垒机 / Teleport

名称：
[公司 Teleport]

Proxy：
[teleport.example.com:443]

Teleport 账号：
[jim]

tsh.exe：
[C:\Tools\Teleport\tsh.exe] [自动检测]

高级：
[ ] 允许不安全 TLS（仅开发环境）

[测试连接]
[登录]
```

状态：

```text
● 已登录
Cluster: teleport.local
User: runory-test
证书有效期：11h 32m
```

或者：

```text
● 需要重新登录
```

### 13.2 Node 列表

```text
Teleport Nodes

搜索...

teleport-node01
development · runory · node01
OS Login: [root ▼]

[连接]
```

Labels 使用 Tag 显示。

### 13.3 Authentication UI

推荐直接嵌入 PTY：

```text
正在登录 Teleport
────────────────────

<tsh login 交互输出>

Password:
OTP:

────────────────────
[取消]
```

未来可以识别登录 URL 并提供：

```text
[在浏览器中完成认证]
```

但业务逻辑不要依赖 CLI 英文文案。

## 14. Provider 事件模型

```rust
pub enum ProviderEvent {
    StateChanged(ProviderState),

    AuthOutput {
        session_id: String,
        stream: OutputStream,
        data: String,
    },

    LoginSucceeded {
        profile_id: String,
    },

    LoginRequired {
        profile_id: String,
        reason: String,
    },

    TargetListUpdated {
        profile_id: String,
        count: usize,
    },

    ProxyStarted {
        session_id: String,
    },

    ProxyDiagnostic {
        session_id: String,
        level: DiagnosticLevel,
        message: String,
    },

    ProxyExited {
        session_id: String,
        exit_code: Option<i32>,
    },
}
```

## 15. Provider 状态机

```text
Unconfigured
   ↓
Ready
   ↓
LoggedOut
   ↓
Authenticating
   ├── Failed
   ↓
Authenticated
   ↓
Discovering
   ↓
Available
   ↓
Connecting
   ├── ReauthRequired
   ├── NodeOffline
   ├── PermissionDenied
   ↓
Connected
   ↓
Disconnected
```

不要把所有问题统一显示成“连接失败”。

## 16. 错误模型

```rust
pub enum TeleportErrorKind {
    TshNotFound,
    TshVersionMismatch,

    ProxyDnsFailure,
    ProxyConnectionRefused,
    ProxyTlsFailure,

    NotAuthenticated,
    CertificateExpired,
    MfaRequired,
    AuthenticationFailed,

    ClusterMismatch,
    NodeNotFound,
    NodeOffline,
    OsLoginNotAllowed,
    AccessDenied,

    ProxyProcessFailed,
    ProxyProcessExited,
    SshHandshakeFailed,

    InvalidCliOutput,
    UnsupportedCliVersion,

    Cancelled,
    Timeout,
}
```

UI 应给出可行动信息。

例如：

```text
Teleport 登录已过期
请重新登录 teleport.example.com

[重新登录]
```

而不是：

```text
exit code 1
```

stderr 可以有限分类，但必须同时保留原始诊断信息：

```text
结构化错误分类 -> UX
原始 stderr     -> Diagnostics
```

## 17. Process Supervisor

TeleportProvider 会创建：

```text
tsh login
tsh ls
tsh status
tsh proxy ssh
ssh.exe
```

必须统一管理：

```rust
pub struct ManagedProcess {
    pub id: Uuid,
    pub kind: ManagedProcessKind,
    pub child: Child,
    pub started_at: Instant,
    pub profile_id: String,
    pub session_id: Option<String>,
}
```

Runory 退出时必须结束：

```text
所有 tsh login
所有 tsh proxy ssh
所有 ssh.exe
```

Windows 长期建议使用 Job Object：

```text
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
```

防止应用异常退出后残留后台进程。

MVP 可先 `child.kill().await`，但必须统一通过 Supervisor 管理。

## 18. 超时策略

建议：

```text
probe             5s
status            5s
list nodes       10s
proxy spawn      10s
ssh handshake    15s
interactive login 不使用短超时
```

Interactive Login 只能由：

```text
用户取消
CLI 退出
全局较长超时
```

终止。

## 19. SSH Session 完整流程

```text
用户点击连接
   ↓
load TeleportProfile
   ↓
auth_status()
   ├── Expired -> Re-login
   └── Valid
   ↓
resolve target
   ↓
validate OS login
   ↓
spawn tsh proxy ssh
   ↓
wait proxy alive
   ↓
attach SSH Engine
   ↓
SSH handshake
   ↓
PTY open
   ↓
Connected
```

## 20. OS Login

Teleport User 和 Linux OS Login 分离。

`tsh status` 可用于确认当前允许的 Logins。

Runory 应：

1. 缓存允许的 OS Logins；
2. 连接时提供 Dropdown；
3. 只有一个时自动选择；
4. 权限变化后重新校验。

UI：

```text
服务器登录用户
[root ▼]

root
ubuntu
deploy
```

## 21. SFTP

长期推荐：

```text
Teleport
  ↓
SSH Transport
  ↓
Runory SSH Session
  ├── PTY
  └── SFTP
```

SFTP 层不应该知道 Teleport。

Provider 只负责：

```text
如何到达目标
```

SSH Core 负责：

```text
连接后的 SSH 协议能力
```

因此不要在 TeleportProvider 内另做一套 `tsh scp` 作为主 SFTP 实现。

`tsh scp` 仅作为 fallback。

## 22. Port Forwarding

Transport 建立后，端口转发仍归 SSH Engine：

```text
Local Forward
Remote Forward
Dynamic SOCKS
```

V1 若直接走 `tsh ssh`，可暂时使用其 `-L` 能力；V2 应统一回 Runory SSH Core。

## 23. Session Recording

Teleport 服务端可能开启会话录制。

Runory 不应：

- 假定所有 Cluster 都已开启 Recording；
- 在客户端伪造“已审计”状态；
- 承诺服务端未配置的审计能力。

未来可独立增加 Teleport Session Audit 功能。

## 24. Security Requirements

### 必须

- 使用 argv 调用子进程，不拼接 Shell 字符串；
- 不保存 Teleport Password；
- 不保存 OTP；
- 不复制 `.tsh` 私钥进 Runory DB；
- stderr 做敏感信息过滤；
- `--insecure` 默认关闭；
- 校验 Proxy 地址；
- App 退出清理 child；
- 不把环境变量和 identity material 写入日志。

### 禁止

错误：

```rust
Command::new("cmd.exe")
    .arg("/c")
    .arg(format!("tsh proxy ssh {}", user_input))
```

正确：

```rust
Command::new(tsh_path)
    .arg("proxy")
    .arg("ssh")
    .arg(format!("--cluster={}", cluster))
    .arg(format!("{}@{}", login, node));
```

## 25. Logging

建议日志分类：

```text
[TeleportProvider]
[TeleportAuth]
[TeleportDiscovery]
[TeleportProxy]
```

示例：

```text
INFO  profile connected
INFO  discovered 12 nodes
INFO  proxy process started session=...
WARN  certificate expires soon
ERROR proxy exited code=...
```

禁止打印：

```text
password
token
private key
identity raw content
```

## 26. Diagnose 功能

建议 UI 增加：

```text
Teleport -> Diagnose
```

诊断结果：

```text
Teleport Diagnostics

tsh.exe                 OK
Version                 18.11.0
Proxy                   teleport.local:3080
DNS                     OK
TCP                     OK
TLS                     INSECURE (development)
Authentication          OK
Cluster                 teleport.local
User                    runory-test
Certificate             valid
Nodes                   1

Result: READY
```

这可以显著降低“Teleport 连不上”的排查成本。

## 27. 开发测试命令

```powershell
tsh version
```

```powershell
tsh login `
  --proxy=teleport.local:3080 `
  --user=runory-test `
  --insecure
```

```powershell
tsh status
```

```powershell
tsh ls --format=json
```

```powershell
tsh ssh root@teleport-node01
```

```powershell
tsh config --proxy=teleport.local:3080
```

```powershell
tsh proxy ssh `
  --cluster=teleport.local `
  root@teleport-node01
```

## 28. 单元测试

### Config

```text
parse_proxy_address()
validate_tsh_path()
profile_serialization()
```

### CLI Output

```text
parse_tsh_version()
parse_tsh_status()
parse_tsh_ls_v18()
```

### Command Builder

```text
build_login_command()
build_list_nodes_command()
build_proxy_ssh_command()
```

重点做命令注入测试：

```text
node = "server; rm -rf /"
```

必须作为单独 argv，绝不能被 Shell 执行。

### Error Classification

```text
classify_certificate_expired()
classify_access_denied()
classify_node_not_found()
classify_proxy_dns_failure()
```

## 29. 集成测试矩阵

当前 Docker Compose 环境可作为固定 Integration Fixture。

| 场景 | 预期 |
|---|---|
| 正常 Cluster | READY |
| tsh 不存在 | TshNotFound |
| Proxy 域名错误 | ProxyDnsFailure |
| Proxy 停止 | ProxyConnectionRefused |
| 未登录 | NotAuthenticated |
| 登录有效 | Authenticated |
| Node 在线 | 能发现 |
| Node 停止 | NodeOffline / 消失 |
| 无 OS Login 权限 | AccessDenied |
| tsh proxy ssh | SSH handshake 成功 |
| Runory 关闭 | tsh/ssh 子进程全部退出 |

## 30. 手工验收

### T01 Provider 探测

```text
Given tsh.exe 可用
When 添加 Teleport Provider
Then 显示 Teleport 版本
```

### T02 登录

```text
When 点击“登录”
Then 打开认证 PTY
And 用户完成 Password/MFA
And 状态变成“已登录”
```

### T03 Node Discovery

```text
When 登录完成
Then UI 显示 teleport-node01
And Labels 正确
```

### T04 SSH

```text
When root@teleport-node01 点击连接
Then 打开 Runory Terminal
And hostname 返回 teleport-node01
```

### T05 Session Cleanup

关闭 Tab：

```text
SSH session closed
tsh proxy child terminated
```

### T06 Re-login

证书过期：

```text
连接前检测
-> 提示重新登录
-> 登录后重新连接
```

### T07 SFTP

```text
SSH connected
-> 打开 SFTP
-> ls
-> upload
-> download
```

必须走同一 SSH Transport 体系。

## 31. 开发阶段

### Phase TP-1：Provider Skeleton

实现：

```text
TeleportProfile
TeleportProvider
probe()
diagnose()
```

验收：

```text
能检测 tsh.exe 和版本
```

### Phase TP-2：Authentication

实现：

```text
tsh login PTY
tsh status
AuthState
logout
```

验收：

```text
Runory 内完成 MFA 登录
```

### Phase TP-3：Discovery

实现：

```text
tsh ls --format=json
TeleportTarget
labels
```

验收：

```text
Runory 显示 teleport-node01
```

### Phase TP-4：SSH MVP

实现：

```text
tsh config
OpenSSH ProxyCommand
Runory PTY
```

验收：

```text
Runory 能打开 root@teleport-node01
```

### Phase TP-5：Native Transport

重构：

```text
SSH Core
TcpStream
   ↓
Generic SshTransport
```

增加：

```text
StdioProxyTransport
```

验收：

```text
Runory SSH Engine 直接通过 tsh proxy ssh 建链
```

### Phase TP-6：SFTP

验收：

```text
Teleport 节点支持完整 SFTP
```

### Phase TP-7：UX / Recovery

实现：

```text
Expired
Re-login
Node Offline
Permission denied
Proxy down
Process cleanup
Diagnostics
```

## 32. Definition of Done

TeleportProvider V1 必须满足：

- [ ] 自动检测 `tsh.exe`
- [ ] 显示 tsh 版本
- [ ] 支持配置 Proxy
- [ ] 支持 Teleport User 登录
- [ ] 支持 Password/MFA/Browser 交互
- [ ] 不保存 Teleport 密码
- [ ] 能检测认证过期
- [ ] 能运行 `tsh ls --format=json`
- [ ] 能显示 Node + Labels
- [ ] 能选择 OS Login
- [ ] 能通过 Teleport 建立 SSH
- [ ] Terminal 可正常 Resize
- [ ] Disconnect 能杀掉 proxy child
- [ ] Runory 退出无残留 tsh.exe
- [ ] 错误信息可定位
- [ ] 开发环境支持 insecure
- [ ] 生产默认禁止 insecure

V2：

- [ ] `StdioProxyTransport`
- [ ] SFTP
- [ ] Port Forwarding
- [ ] Session Recovery
- [ ] 多 Teleport Profile
- [ ] 多 Cluster / Trusted Cluster
- [ ] 自动化集成测试

## 33. 不建议做的事情

不要自己实现：

```text
Teleport CA 请求
SSH Cert 签发
MFA Protocol
Teleport 私钥管理
```

这些能力属于 `tsh`。

不要把 Teleport 简化为传统 ProxyJump：

```text
ssh -> jump-host -> host
```

Teleport 是：

```text
Teleport Identity
   ↓
Proxy / TLS
   ↓
Reverse Tunnel
   ↓
Teleport Node
```

不要要求用户手工污染全局 `~/.ssh/config`。

Runory 应使用 `tsh config` 生成自己的 Provider 专用 SSH 配置。

## 34. 未来扩展

SSH Provider 完成后可继续：

```text
Teleport Database Access
Teleport Kubernetes Access
Teleport Desktop Access
Teleport Application Access
Teleport Access Requests
Teleport Session Audit
Teleport Machine Identity / tbot
```

CI、自动化 Agent 或无人值守任务不应复用人的 `tsh login` 身份，应单独评估 Teleport Machine & Workload Identity / `tbot`。

## 35. 最终推荐架构

```text
                         Runory
                            │
                     BastionProvider
                            │
       ┌────────────────────┼────────────────────┐
       │                    │                    │
   ProxyJump            Boundary             Teleport
       │                    │                    │
       │               Local TCP             tsh identity
       │                    │                    │
       │                    │             tsh proxy ssh
       │                    │                    │
       └────────────────────┼────────────────────┘
                            │
                     SshTransport
                            │
                     Runory SSH Core
                    ┌───────┼────────┐
                    │       │        │
                   PTY     SFTP    Forwarding
```

核心结论：

> **Provider 负责“如何到达目标”，SSH Core 负责“到达之后如何使用 SSH”。**

这样可以同时兼容：

```text
OpenSSH ProxyJump
JumpServer
Teleport
Boundary
未来其他堡垒机
```

而不会把任何一家堡垒机产品的实现细节渗透进 Runory 的终端、SFTP 和 Agent 层。

## 36. 建议的第一批开发任务

```text
TP-001 TeleportProfile 数据模型
TP-002 tsh 二进制探测与版本检查
TP-003 Teleport authentication PTY
TP-004 tsh status / AuthState
TP-005 tsh ls JSON Adapter
TP-006 Teleport Node 列表 UI
TP-007 OS Login 选择
TP-008 tsh config 管理
TP-009 OpenSSH ProxyCommand SSH MVP
TP-010 ProcessSupervisor
TP-011 Provider Diagnostics
TP-012 Error Classification
TP-013 Integration Tests
TP-014 SshTransport 泛化
TP-015 StdioProxyTransport
TP-016 SFTP over Teleport
```

建议先完成 `TP-001 ~ TP-009`，形成可用的 TeleportProvider MVP，再做 SSH Core 的 transport 泛化。

## 37. 官方参考

- Teleport `tsh` 使用说明  
  https://goteleport.com/docs/connect-your-client/teleport-clients/tsh/

- Teleport `tsh` CLI Reference  
  https://goteleport.com/docs/reference/cli/tsh/

- OpenSSH / `tsh config` 参考  
  https://goteleport.com/docs/connect-your-client/third-party/vscode/
