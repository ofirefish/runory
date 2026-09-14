# Runory OpenSSH ProxyJump → JumpServer → Teleport → Boundary 开发方案

> 项目：Runory  
> 技术栈：Tauri 2 + React + Rust  
> 主题：Enterprise Bastion / Access Provider Framework  
> 目标：在不侵入 Terminal / SFTP / Agentic Runtime 的前提下，统一支持 OpenSSH ProxyJump、JumpServer、Teleport、HashiCorp Boundary，并为后续 CyberArk、BeyondTrust、StrongDM、国产堡垒机预留稳定扩展接口。

---

## 1. 背景

Runory 当前已经具备 SSH、SFTP、Terminal、Agentic Runtime、Tool Registry、Policy、ChangeSet、会话管理等基础能力。

随着企业使用场景增加，仅支持以下连接方式已经不足：

- Direct SSH；
- 静态 Host + Username + Password / SSH Key；
- 简单 JumpHost；
- 单一厂商堡垒机接入。

企业环境中的服务器访问通常还涉及：

- 多级 SSH 跳板；
- 堡垒机 API 鉴权；
- 动态资产发现；
- 动态账号发现；
- MFA / OTP / SSO；
- 短期 Token / SSH Certificate；
- 本地代理进程；
- `ProxyCommand`；
- Session Broker；
- 临时授权；
- 审计链路；
- 会话生命周期管理；
- Vendor CLI。

因此，Runory 不应为每种堡垒机单独修改 SSH Core，而应构建统一的 Enterprise Connection Framework。

---

## 2. 总体目标

第一阶段计划按以下顺序实现：

```text
OpenSSH ProxyJump
        ↓
JumpServer
        ↓
Teleport
        ↓
HashiCorp Boundary
```

通过这四种不同的连接模型验证 Runory 的统一连接架构。

四类 Provider 分别验证：

| Provider | 主要验证能力 |
|---|---|
| OpenSSH ProxyJump | 原生 SSH Tunnel / Multi-Hop |
| JumpServer | API Control Plane + Session Broker |
| Teleport | External CLI + Stdio Proxy + SSH Certificate |
| Boundary | External CLI + Local TCP Proxy + Logical Target |

最终目标是未来新增：

```rust
registry.register(
    Arc::new(CyberArkProvider::new())
);
```

时无需修改：

```text
SSH Core
Terminal
SFTP
Agentic Runtime
```

---

# 3. 核心架构原则

## 3.1 Provider 不负责实现 SSH

`BastionProvider` 的核心职责应定义为：

> 将厂商特有的认证、授权、资产发现、凭据获取、代理启动流程，转换为 Runory SSH Core 可以消费的统一连接描述。

Provider 不应提供：

```rust
connect_ssh()
open_terminal()
execute_command()
open_sftp()
```

这些职责仍然属于 SSH Core。

---

## 3.2 最终统一为 PreparedConnection

所有 Provider 最终输出：

```rust
pub struct PreparedConnection {
    pub logical_target: LogicalTarget,
    pub transport: TransportPlan,
    pub ssh: SshHandshakePlan,
    pub lifecycle: Option<ProviderSessionHandle>,
    pub audit: Option<AuditContext>,
}
```

其中：

```rust
pub enum TransportPlan {
    Tcp {
        host: String,
        port: u16,
    },

    SshJump {
        hops: Vec<JumpHop>,
        target_host: String,
        target_port: u16,
    },

    StdioProxy {
        command: CommandSpec,
    },

    LocalTcpProxy {
        command: CommandSpec,
        endpoint: LocalEndpointStrategy,
    },
}
```

Provider 与 Transport 对应关系：

| Provider | TransportPlan |
|---|---|
| Direct SSH | `Tcp` |
| OpenSSH ProxyJump | `SshJump` |
| JumpServer | `Tcp`，连接 KoKo |
| Teleport | `StdioProxy` |
| Boundary | `LocalTcpProxy` |

---

# 4. 总体调用链

```text
React UI
   │
   ▼
SessionManager
   │
   ▼
ConnectionResolver
   │
   ├──────── Direct
   │
   ├──────── ProxyJump
   │
   └──────── Bastion
              │
              ▼
        BastionRegistry
              │
       ┌──────┼──────────┬──────────┐
       ▼      ▼          ▼          ▼
    OpenSSH JumpServer Teleport  Boundary
       │      │          │          │
       └──────┴─────┬────┴──────────┘
                    ▼
            PreparedConnection
                    │
                    ▼
             TransportFactory
                    │
        ┌───────────┼────────────┐
        ▼           ▼            ▼
      TCP       SSH Tunnel   Process Proxy
                    │
                    ▼
                 SSH Core
                    │
          ┌─────────┼─────────┐
          ▼         ▼         ▼
       Terminal    SFTP     Agentic
```

---

# 5. BastionProvider 接口

建议定义：

```rust
#[async_trait]
pub trait BastionProvider: Send + Sync {
    fn provider_type(&self) -> BastionProviderType;

    fn capabilities(&self) -> BastionCapabilities;

    async fn probe(
        &self,
        config: &BastionConfig,
    ) -> Result<ProviderProbeResult>;

    async fn auth_status(
        &self,
        ctx: &ProviderContext,
    ) -> Result<AuthStatus>;

    async fn authenticate(
        &self,
        ctx: &ProviderContext,
    ) -> Result<AuthResult>;

    async fn continue_auth(
        &self,
        challenge: AuthResponse,
    ) -> Result<AuthResult>;

    async fn list_assets(
        &self,
        query: AssetQuery,
    ) -> Result<Vec<BastionAsset>>;

    async fn list_accounts(
        &self,
        asset: &BastionAsset,
    ) -> Result<Vec<BastionAccount>>;

    async fn prepare_connection(
        &self,
        request: ConnectRequest,
    ) -> Result<PreparedConnection>;

    async fn release(
        &self,
        session: ProviderSessionHandle,
    ) -> Result<()>;
}
```

---

# 6. ConnectionRoute

建议连接配置统一定义为：

```rust
pub enum ConnectionRoute {
    Direct,

    JumpHost {
        hops: Vec<JumpHop>,
    },

    Bastion {
        provider_id: BastionProviderId,
        asset_ref: Option<String>,
        account_ref: Option<String>,
    },
}
```

这样 Host 本身不需要知道 JumpServer、Teleport 或 Boundary 的技术细节。

---

# 7. Phase 0：Connection Framework

在实现具体 Provider 之前，先完成统一 Framework。

## 7.1 必须完成的模块

```text
ConnectionRoute
ConnectionResolver
PreparedConnection
TransportPlan
TransportFactory
BastionProvider
BastionRegistry
BastionCapabilities
Auth State Machine
BastionAsset
BastionAccount
ProviderSessionHandle
LogicalTarget
HostIdentityPolicy
MockBastionProvider
```

## 7.2 核心要求

- 现有 Direct SSH 保持兼容；
- Terminal 不感知 Provider；
- SFTP 不感知 Provider；
- Agent Runtime 不感知 Provider；
- Secret 不进入 React Store；
- Provider 可独立单元测试；
- Transport 可独立测试；
- 支持异步取消；
- 支持会话清理；
- 支持 Provider Authentication 中断和恢复。

---

# 8. Phase 1：OpenSSH ProxyJump

## 8.1 目标

实现原生 SSH ProxyJump 语义，而不是简单调用：

```bash
ssh -J user@bastion user@target
```

Runory 应自己完成：

```text
TCP Bastion
     ↓
SSH handshake
     ↓
Authenticate Bastion
     ↓
direct-tcpip(target:22)
     ↓
获得双向字节流
     ↓
SSH handshake Target
     ↓
Authenticate Target
     ↓
ServerSession
```

---

## 8.2 JumpHop 数据结构

```rust
pub struct JumpHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credential_ref: CredentialRef,
    pub host_key_policy: HostKeyPolicy,
}
```

第一版 UI 可以只支持一个 Jump Host，但 Core 应直接支持：

```text
Runory
  ↓
Jump A
  ↓
Jump B
  ↓
Jump C
  ↓
Target
```

---

## 8.3 HostKey 验证

堡垒机与目标服务器是两个独立 SSH Peer：

```text
jump.example.com
    fingerprint A

10.0.0.15
    fingerprint B
```

必须分别验证。

禁止因为目标通过 JumpHost 访问就跳过 Target HostKey 校验。

---

## 8.4 ProxyJump DoD

```text
Host
 ↓
ConnectionRoute::JumpHost
 ↓
连接 JumpHost
 ↓
JumpHost HostKey 验证
 ↓
JumpHost Auth
 ↓
direct-tcpip
 ↓
Target HostKey 验证
 ↓
Target Auth
 ↓
ServerSession
 ↓
Terminal / SFTP / Agent
```

必须支持：

- Password；
- SSH Key；
- Agent / Key Store；
- Multi-Hop；
- HostKey Verification；
- Cancel；
- Connection Timeout；
- Tunnel Error Mapping。

---

# 9. Phase 2：JumpServer

## 9.1 架构模型

JumpServer 必须区分：

```text
Control Plane
+
Session Plane
```

流程：

```text
Runory
   │
   │ Access Key
   ▼
JumpServer Core API
   │
   ├─ Assets
   ├─ Accounts
   ├─ Permissions
   │
   └─ Connection Token
             │
             ▼
           KoKo
             │
             ▼
          Target
```

Access Key 不用于直接 SSH 登录目标服务器。

---

## 9.2 JumpServerProvider

```rust
pub struct JumpServerProvider {
    api: JumpServerApiClient,
    credential_store: Arc<CredentialStore>,
    version_adapter: JumpServerVersionAdapter,
}
```

配置：

```rust
pub struct JumpServerConfig {
    pub base_url: String,
    pub koko_host: Option<String>,
    pub koko_port: Option<u16>,
    pub access_key_id: String,
    pub access_key_secret_ref: SecretRef,
    pub tls_policy: TlsPolicy,
}
```

---

## 9.3 Secret 存储

数据库允许保存：

```text
Access Key ID
SecretRef
Provider ID
Base URL
KoKo 配置
```

禁止保存：

```text
Access Key Secret 明文
Connection Token
目标服务器密码
目标私钥
临时 SSH Credential
```

Secret 应存入 OS Credential Store，例如：

```text
Windows Credential Manager
macOS Keychain
Linux Secret Service
```

---

## 9.4 API Adapter

不要在 Provider 内散落：

```rust
"/api/v1/..."
```

建议：

```text
JumpServerProvider
       │
       ▼
JumpServerApi
       │
       ├── V3Adapter
       └── V4Adapter
```

启动时：

```text
probe
 ↓
version detection
 ↓
capability detection
 ↓
select adapter
```

所有 API Schema 应以用户实际 JumpServer 实例 `/api/docs/` 为准。

---

## 9.5 Connection 流程

```text
prepare_connection()
      │
      ├─ validate Access Key
      │
      ├─ validate asset permission
      │
      ├─ resolve account
      │
      ├─ create Connection Token
      │
      └─ PreparedConnection
               │
               ▼
        TransportPlan::Tcp
               │
               ▼
             KoKo
               │
               ▼
            SSH Core
```

Session 关闭：

```text
ServerSession close
       ↓
Provider.release()
       ↓
清理临时 Token / Session Context
```

---

## 9.6 JumpServer DoD

必须实现：

- Provider 配置；
- Access Key 安全保存；
- API 连通性检查；
- API 认证检查；
- Asset Discovery；
- Account Discovery；
- Connection Token；
- KoKo SSH；
- MFA 状态处理；
- Terminal；
- SFTP；
- Agentic Runtime；
- Session Cleanup；
- Secret Redaction；
- Provider Contract Test。

---

# 10. Phase 3：ExternalHelperManager

Teleport 和 Boundary 都需要 Vendor CLI，因此应先开发通用进程代理框架。

建议目录：

```text
src-tauri/src/helper/
    mod.rs
    process.rs
    stdio.rs
    local_proxy.rs
    lifecycle.rs
    version.rs
    redaction.rs
```

核心接口：

```rust
pub trait ExternalHelperManager {
    fn locate_binary(...);
    fn check_version(...);
    fn spawn(...);
    fn spawn_stdio_proxy(...);
    fn spawn_local_proxy(...);
    fn wait_ready(...);
    fn capture_stderr(...);
    fn terminate(...);
    fn kill_process_tree(...);
}
```

必须正确处理：

```text
用户关闭 Terminal Tab
用户关闭 Host Session
Runory Exit
SSH 失败
Provider Token 过期
Network Disconnect
Agent Cancel
Helper Crash
```

Windows 平台必须正确终止：

```text
tsh.exe
boundary.exe
```

及其整个子进程树。

---

# 11. Phase 4：Teleport

## 11.1 设计原则

第一阶段不建议在 Rust 内重新实现 Teleport 私有协议。

Runory 使用官方：

```text
tsh
```

作为认证和 Transport Helper。

总体链路：

```text
Runory SSH Core
      │
      │ stdin/stdout
      ▼
 tsh proxy ssh
      │
      ▼
Teleport Proxy
      │
      ▼
Teleport Node
```

对应：

```rust
TransportPlan::StdioProxy
```

---

## 11.2 TeleportProvider

```rust
pub struct TeleportProvider {
    tsh: TshClient,
    helper_manager: Arc<ExternalHelperManager>,
}
```

初始化：

```text
find tsh
 ↓
tsh version
 ↓
tsh status
```

必须支持版本检测与错误提示。

---

## 11.3 Asset Discovery

优先使用结构化输出：

```bash
tsh ls --format=json
```

处理：

```text
JSON
 ↓
TeleportNode
 ↓
BastionAsset
```

禁止解析终端表格格式。

---

## 11.4 Authentication

流程：

```text
tsh status
   │
   ├─ valid
   │   ↓
   │ continue
   │
   └─ expired / missing
         ↓
    AuthChallenge
         ↓
      tsh login
         ↓
 Browser / SSO / MFA
         ↓
       Resume
```

此处应与 Runory Agent Runtime 的：

```text
AwaitingUser
Resume
```

机制兼容。

---

## 11.5 prepare_connection

示例：

```rust
TransportPlan::StdioProxy {
    command: CommandSpec {
        executable: "tsh".into(),
        args: vec![
            "proxy".into(),
            "ssh".into(),
            "root@server01".into(),
        ],
        ..Default::default()
    },
}
```

数据流：

```text
tsh stdout ─────► SSH Core read

tsh stdin  ◄───── SSH Core write
```

SSH Core 不需要感知 Teleport。

---

# 12. Teleport Host Certificate 支持

Teleport 可能使用 SSH Certificate，因此 Runory HostKey 模型不能只支持：

```text
hostname → SHA256 fingerprint
```

建议升级为：

```rust
pub enum HostIdentityPolicy {
    KnownHost {
        hostname: String,
    },

    HostCertificateAuthority {
        ca: SshPublicKey,
        principals: Vec<String>,
    },

    ProviderManaged {
        provider_id: String,
        identity: ProviderHostIdentity,
    },
}
```

禁止使用：

```text
accept_all_host_keys = true
```

绕过证书验证。

---

# 13. Phase 5：HashiCorp Boundary

## 13.1 架构模型

Boundary 更接近：

```text
Identity
   ↓
Controller
   ↓
Authorize Session
   ↓
Worker
   ↓
Target
```

第一版建议复用 Boundary CLI 创建 authenticated local proxy。

链路：

```text
Runory
   │
   ├── spawn
   ▼
boundary connect
   │
   ▼
Boundary Worker
   │
   ▼
Target
   ▲
   │
127.0.0.1:random_port
   ▲
   │
SSH Core
```

对应：

```rust
TransportPlan::LocalTcpProxy
```

---

## 13.2 BoundaryProvider

```rust
pub struct BoundaryProvider {
    cli: BoundaryCli,
    helper_manager: Arc<ExternalHelperManager>,
}
```

MVP 第一版允许用户配置：

```text
Boundary Address
Target ID
Username
SSH Credential
```

先完成 Target SSH 连接，再继续实现完整发现能力。

---

## 13.3 第二阶段扩展

后续增加：

```text
Scope Discovery
Project Discovery
Target Discovery
Host Discovery
Credential Broker
Managed Credential
Session Metadata
```

---

# 14. PhysicalEndpoint 与 LogicalTarget 分离

这是 Boundary、Teleport、CyberArk 等场景必须具备的设计。

例如 Boundary 实际建立：

```text
127.0.0.1:47281
```

但目标服务器是：

```text
prod-db-01
Target ID: ttcp_xxxxx
```

不能把 HostKey 绑定到：

```text
127.0.0.1:47281
```

因此定义：

```rust
pub struct LogicalTarget {
    pub stable_id: String,
    pub display_name: String,
    pub host_identity_alias: Option<String>,
}
```

示例：

```rust
PreparedConnection {
    logical_target: LogicalTarget {
        stable_id: "ttcp_xxxxx".into(),
        display_name: "prod-db-01".into(),
        host_identity_alias: Some("ttcp_xxxxx".into()),
    },

    transport: TransportPlan::LocalTcpProxy {
        command: boundary_command,
        endpoint: LocalEndpointStrategy::Stdout,
    },

    ssh: ssh_plan,
    lifecycle: Some(session_handle),
    audit: None,
}
```

---

# 15. BastionCapabilities

禁止大量出现：

```rust
if provider == "jumpserver" {
}
```

必须使用 Capability Model：

```rust
pub struct BastionCapabilities {
    pub asset_discovery: bool,
    pub account_discovery: bool,

    pub password_auth: bool,
    pub public_key_auth: bool,

    pub mfa: bool,
    pub browser_sso: bool,

    pub short_lived_credentials: bool,
    pub ssh_certificate: bool,

    pub sftp: bool,
    pub session_recording: bool,

    pub managed_credentials: bool,
    pub access_request: bool,

    pub external_helper: bool,
    pub multi_hop: bool,
}
```

能力矩阵：

| Capability | ProxyJump | JumpServer | Teleport | Boundary |
|---|---:|---:|---:|---:|
| Asset Discovery | × | ✓ | ✓ | ✓ |
| Account Discovery | × | ✓ | 部分 | 视配置 |
| MFA / SSO | × | ✓ | ✓ | ✓ |
| Managed Credential | × | ✓ | ✓ | ✓ |
| SSH Certificate | ✓ | 可选 | ✓ | 可选 |
| Audit | × | ✓ | ✓ | ✓ |
| External Helper | × | × | ✓ | ✓ |
| Multi-Hop | ✓ | 内部实现 | Cluster | Worker |
| Short-Lived Access | × | ✓ | ✓ | ✓ |

---

# 16. Auth State Machine

认证流程不能只返回：

```text
success / failed
```

需要支持：

```rust
pub enum AuthResult {
    Authenticated(AuthContext),

    Challenge(AuthChallenge),

    Failed(AuthError),
}
```

Challenge 类型：

```rust
pub enum AuthChallenge {
    Password,
    Otp,
    Totp,
    BrowserSso {
        url: String,
    },
    DeviceCode {
        code: String,
        verification_url: String,
    },
    UserApproval,
    TouchSecurityKey,
}
```

与 Agentic V2：

```text
Running
   ↓
AwaitingUser
   ↓
Resume
```

兼容。

---

# 17. TransportFactory

```rust
pub struct TransportFactory;

impl TransportFactory {
    pub async fn open(
        plan: &TransportPlan,
        ctx: &TransportContext,
    ) -> Result<Box<dyn AsyncTransport>> {
        // ...
    }
}
```

统一输出：

```rust
pub trait AsyncTransport:
    AsyncRead + AsyncWrite + Send + Unpin
{
}
```

SSH Core 只看到：

```text
AsyncRead + AsyncWrite
```

而不知道底层来自：

```text
TCP Socket
SSH direct-tcpip
Teleport tsh
Boundary Local Proxy
```

---

# 18. BastionRegistry

```rust
pub struct BastionRegistry {
    providers: HashMap<BastionProviderType, Arc<dyn BastionProvider>>,
}
```

注册：

```rust
registry.register(
    BastionProviderType::OpenSsh,
    Arc::new(OpenSshProvider::new()),
);

registry.register(
    BastionProviderType::JumpServer,
    Arc::new(JumpServerProvider::new()),
);

registry.register(
    BastionProviderType::Teleport,
    Arc::new(TeleportProvider::new()),
);

registry.register(
    BastionProviderType::Boundary,
    Arc::new(BoundaryProvider::new()),
);
```

未来：

```rust
registry.register(
    BastionProviderType::CyberArk,
    Arc::new(CyberArkProvider::new()),
);
```

无需改动 SSH Core。

---

# 19. 推荐 Rust 目录结构

```text
src-tauri/src/
│
├── connection/
│   ├── mod.rs
│   ├── route.rs
│   ├── resolver.rs
│   ├── prepared.rs
│   ├── transport.rs
│   └── session_manager.rs
│
├── bastion/
│   ├── mod.rs
│   ├── provider.rs
│   ├── registry.rs
│   ├── capabilities.rs
│   ├── auth.rs
│   ├── asset.rs
│   ├── account.rs
│   └── session.rs
│
├── transport/
│   ├── mod.rs
│   ├── tcp.rs
│   ├── ssh_jump.rs
│   ├── stdio_proxy.rs
│   └── local_proxy.rs
│
├── providers/
│   ├── openssh/
│   │   ├── mod.rs
│   │   └── provider.rs
│   │
│   ├── jumpserver/
│   │   ├── mod.rs
│   │   ├── api.rs
│   │   ├── auth.rs
│   │   ├── assets.rs
│   │   ├── accounts.rs
│   │   ├── token.rs
│   │   └── provider.rs
│   │
│   ├── teleport/
│   │   ├── mod.rs
│   │   ├── tsh.rs
│   │   ├── auth.rs
│   │   ├── assets.rs
│   │   └── provider.rs
│   │
│   └── boundary/
│       ├── mod.rs
│       ├── cli.rs
│       ├── auth.rs
│       ├── targets.rs
│       └── provider.rs
│
└── helper/
    ├── mod.rs
    ├── process.rs
    ├── stdio.rs
    ├── local_proxy.rs
    ├── version.rs
    └── redaction.rs
```

---

# 20. 前端设计

前端不要根据厂商写大量特殊页面。

建议 Provider 配置统一使用：

```text
ProviderList
   ↓
ProviderEditor
   ↓
Dynamic Provider Schema
```

Provider 暴露：

```rust
ProviderFormSchema
```

例如 OpenSSH：

```text
Host
Port
Username
Credential
HostKey Policy
```

JumpServer：

```text
Base URL
KoKo Host
KoKo Port
Access Key ID
Access Key Secret
TLS Policy
```

Teleport：

```text
Proxy Address
Cluster
Profile
Path to tsh
```

Boundary：

```text
Boundary Address
Scope / Project
Target ID
Path to boundary CLI
```

---

# 21. Provider UI 能力协商

资产列表页面：

```text
if capabilities.asset_discovery
    显示“浏览资产”
else
    显示手动目标配置
```

账号列表：

```text
if capabilities.account_discovery
    自动获取账号
else
    用户输入 username
```

这样 UI 不需要知道具体 Provider 品牌。

---

# 22. 数据模型

建议：

```text
bastion_providers
bastion_provider_settings
host_connection_routes
provider_asset_cache
provider_account_cache
```

Provider 表：

```sql
CREATE TABLE bastion_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

注意：

`config_json` 中只能存普通配置和 `SecretRef`，不能保存 Secret 明文。

---

# 23. Secret 管理

统一定义：

```rust
pub struct SecretRef {
    pub namespace: String,
    pub key: String,
}
```

例如：

```text
runory/bastion/js-prod/access-key-secret
runory/ssh/host-123/private-key-passphrase
```

日志必须统一 Redaction：

```text
Authorization
Access Key Secret
Password
Connection Token
SSH Private Key
OTP
Session Token
Teleport Identity
Boundary Token
```

禁止在：

```text
tracing
console.log
React State
SQLite
Crash Report
Agent Conversation
```

中出现明文 Secret。

---

# 24. Provider Session Lifecycle

统一定义：

```rust
pub struct ProviderSessionHandle {
    pub provider_id: String,
    pub session_id: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub helper_process: Option<HelperProcessId>,
}
```

SessionManager：

```text
prepare_connection
        ↓
open transport
        ↓
SSH handshake
        ↓
ServerSession
        ↓
Terminal / SFTP / Agent
        ↓
close
        ↓
transport shutdown
        ↓
provider.release()
```

所有异常路径也必须执行 release。

建议使用 RAII / Drop Guard + 显式 async close 双保险。

---

# 25. Error Model

不要直接把各厂商 stderr 输出到 UI。

统一定义：

```rust
pub enum BastionError {
    ProviderUnavailable,
    AuthenticationRequired,
    AuthenticationFailed,
    AuthorizationDenied,
    AssetNotFound,
    AccountNotAvailable,
    TokenExpired,
    HelperMissing,
    HelperVersionMismatch,
    ProxyStartupFailed,
    TransportFailed,
    HostKeyRejected,
    CertificateRejected,
    SessionExpired,
    Cancelled,
    Unsupported,
    Internal,
}
```

同时保留：

```rust
ProviderDiagnostic {
    provider_code,
    safe_message,
    debug_context,
}
```

`debug_context` 必须经过 Redaction。

---

# 26. 开发阶段规划

## P0 — Connection Framework

目标：建立厂商无关的连接架构。

任务：

```text
BAS-001 ConnectionRoute
BAS-002 ConnectionResolver
BAS-003 PreparedConnection
BAS-004 TransportPlan
BAS-005 TransportFactory
BAS-006 BastionProvider
BAS-007 BastionRegistry
BAS-008 BastionCapabilities
BAS-009 Auth State Machine
BAS-010 LogicalTarget
BAS-011 HostIdentityPolicy
BAS-012 MockBastionProvider
BAS-013 Session Lifecycle
BAS-014 Provider Contract Test
```

---

## P1 — OpenSSH ProxyJump

任务：

```text
BAS-101 JumpHop Model
BAS-102 SSH Tunnel Transport
BAS-103 direct-tcpip
BAS-104 Multi-Hop
BAS-105 Bastion HostKey
BAS-106 Target HostKey
BAS-107 Password Auth
BAS-108 SSH Key Auth
BAS-109 Cancel / Timeout
BAS-110 Terminal Integration
BAS-111 SFTP Integration
BAS-112 Agent Integration
```

---

## P2 — JumpServer

任务：

```text
BAS-201 JumpServer Provider Skeleton
BAS-202 Secure Access Key Storage
BAS-203 Version Probe
BAS-204 API Adapter
BAS-205 Auth Check
BAS-206 Asset Discovery
BAS-207 Account Discovery
BAS-208 Connection Token
BAS-209 KoKo Transport
BAS-210 MFA
BAS-211 Session Release
BAS-212 Terminal Test
BAS-213 SFTP Test
BAS-214 Agent Test
```

---

## P3 — External Helper Framework

任务：

```text
BAS-301 Binary Locator
BAS-302 Version Probe
BAS-303 Process Manager
BAS-304 Stdio Proxy
BAS-305 Local TCP Proxy
BAS-306 Ready Detection
BAS-307 stderr Capture
BAS-308 Secret Redaction
BAS-309 Process Tree Cleanup
BAS-310 Cancel Integration
```

---

## P4 — Teleport

任务：

```text
BAS-401 Teleport Provider Skeleton
BAS-402 tsh Locator
BAS-403 tsh Version
BAS-404 tsh Status
BAS-405 tsh Login
BAS-406 Browser SSO
BAS-407 Asset Discovery
BAS-408 Stdio Proxy Transport
BAS-409 Host Certificate Validation
BAS-410 Terminal Test
BAS-411 SFTP Test
BAS-412 Agent Test
```

---

## P5 — Boundary

任务：

```text
BAS-501 Boundary Provider Skeleton
BAS-502 CLI Locator
BAS-503 Auth State
BAS-504 Target ID Connection
BAS-505 Local Proxy Detection
BAS-506 Logical Target
BAS-507 HostIdentity Alias
BAS-508 Session Lifecycle
BAS-509 Scope Discovery
BAS-510 Target Discovery
BAS-511 Credential Broker
BAS-512 Terminal Test
BAS-513 SFTP Test
BAS-514 Agent Test
```

---

# 27. 推荐实际实施顺序

严格按照：

```text
P0
Connection Framework
      ↓
P1
OpenSSH ProxyJump
      ↓
P2
JumpServer
      ↓
P3
External Helper Framework
      ↓
P4
Teleport
      ↓
P5
Boundary
```

不要：

```text
ProxyJump 做一套
JumpServer 做一套
Teleport 临时 subprocess
Boundary 再写另一套 subprocess
```

每完成一个 Provider，都必须反向验证 Framework 是否仍然保持厂商无关。

---

# 28. Contract Test

最终建立统一 Provider Contract Test：

| Provider | Terminal | SFTP | Agent | HostKey | Cancel | Cleanup |
|---|---:|---:|---:|---:|---:|---:|
| Direct SSH | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| ProxyJump | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| JumpServer | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Teleport | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Boundary | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

---

# 29. 故障测试矩阵

## OpenSSH ProxyJump

```text
JumpHost 不可达
JumpHost 密码错误
JumpHost SSH Key 错误
JumpHost HostKey 改变
Target 不可达
Target HostKey 改变
中间 Hop 断线
Tunnel Timeout
```

## JumpServer

```text
Access Key 无效
Access Key 被删除
API 不可达
API Version 不兼容
资产无权限
账号无权限
Connection Token 失败
Connection Token 过期
KoKo 不可达
MFA Cancel
```

## Teleport

```text
tsh 不存在
tsh 版本不兼容
Teleport Proxy 不可达
登录过期
SSO Cancel
MFA Failed
Node 无权限
SSH Certificate 过期
tsh helper crash
```

## Boundary

```text
boundary CLI 不存在
Boundary Controller 不可达
Auth Token 失效
Target 无权限
Worker 不可达
Local Proxy 启动失败
Local Port 读取失败
Session Expired
Helper Crash
```

## 通用

```text
关闭 Terminal Tab
关闭 Host
Runory Exit
Agent Cancel
Network Disconnect
Sleep / Wake
Provider Session Expired
```

---

# 30. 安全要求

必须遵守：

1. 不绕过堡垒机审计链路；
2. 不主动获取目标服务器托管密码；
3. Secret 不进入 React 持久化状态；
4. Secret 不写 SQLite 明文；
5. 临时 Token 不长期缓存；
6. SSH HostKey 不因代理存在而关闭验证；
7. Teleport SSH Certificate 必须验证；
8. Boundary 本地随机端口不能作为 Host Identity；
9. Agent Runtime 不得读取 Provider Secret；
10. Tool Registry / Policy / ChangeSet 不因堡垒机功能而绕过。

---

# 31. Agentic Runtime 集成

Agent Runtime 不应调用：

```text
JumpServer API
Teleport tsh
Boundary CLI
```

Agent 只调用已有 SSH Tool：

```text
ssh.exec
file.read
file.write
service.inspect
process.list
...
```

底层：

```text
Agent
 ↓
Tool Registry
 ↓
Policy
 ↓
ServerSession
 ↓
SSH Core
 ↓
PreparedConnection
 ↓
Provider Transport
```

这样不会破坏当前 Agentic V2 安全模型。

---

# 32. Definition of Done

整个项目完成的标准不是：

> 四种堡垒机都能打开一个 Terminal。

而是：

```text
用户选择 Host
      ↓
ConnectionResolver
      ↓
自动选择 Route / Provider
      ↓
Provider 完成认证与授权
      ↓
PreparedConnection
      ↓
TransportFactory
      ↓
SSH Core
      ↓
ServerSession
      ↓
Terminal / SFTP / Agentic
      ↓
Session Close
      ↓
Transport Cleanup
      ↓
Provider Release
```

同时做到：

```text
Direct SSH      不退化
ProxyJump       正常
JumpServer      正常
Teleport        正常
Boundary        正常
```

且未来新增 Provider 时：

```text
SSH Core       0 changes
Terminal       0 changes
SFTP           0 changes
Agent Runtime  0 changes
```

达到这一标准，才能说明 Runory 的 Enterprise Bastion Provider Framework 架构真正稳定。

---

# 33. Codex 实施约束

将本设计交给 Codex 时，应明确以下要求：

1. 先阅读现有 SSH、SessionManager、SFTP、Agentic Runtime 实现；
2. 禁止为了实现 Provider 重写稳定 SSH Core；
3. 必须先完成 P0 Framework；
4. Provider 特殊逻辑禁止进入 SSH Core；
5. 禁止出现大量 `if provider == ...`；
6. 使用 Capability Model；
7. 所有 Secret 使用 SecretRef；
8. 所有日志必须 Redaction；
9. 数据库变更必须提供 migration；
10. Direct SSH 必须向后兼容；
11. 每个 Phase 完成后运行全部现有测试；
12. 每个 Provider 必须运行 Contract Test；
13. JumpServer API Schema 以实际实例 `/api/docs/` 为准；
14. Teleport 优先复用 `tsh`；
15. Boundary 第一阶段优先复用官方 CLI；
16. 不允许通过关闭 HostKey 验证解决证书问题；
17. 不允许堡垒机功能绕过 Tool Registry / Policy / ChangeSet；
18. 所有 helper process 必须支持 Cancel 和 Cleanup；
19. Windows 必须处理整个 helper process tree；
20. 每个实现阶段必须补充单元测试、集成测试和失败场景测试。

---

# 34. 最终建议

Runory 不应把该功能定义为：

```text
JumpServer Support
```

也不应定义为：

```text
Bastion SSH Support
```

更合适的产品与架构定义是：

```text
Enterprise Connection Framework
```

OpenSSH ProxyJump、JumpServer、Teleport 和 Boundary 是第一批 Provider。

它们分别覆盖：

```text
SSH Tunnel
API Broker
Stdio Proxy
Local Proxy
Dynamic Credential
SSH Certificate
MFA / SSO
Asset Discovery
Session Lifecycle
Audit-aware Connection
```

这四种模式完成之后，Runory 才具备继续接入 CyberArk、BeyondTrust、StrongDM、传统国产堡垒机和企业自研访问网关的稳定基础。
