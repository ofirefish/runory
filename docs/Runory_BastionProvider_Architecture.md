# Runory BastionProvider 架构设计文档

> 目标：为 Runory 构建一个高度可扩展的堡垒机接入架构，使 Direct SSH、标准 SSH JumpHost、JumpServer，以及未来的 Teleport、HashiCorp Boundary、CyberArk、BeyondTrust、自研堡垒机等都能在不侵入 Terminal / SFTP / Agent Runtime 的前提下接入。

---

## 1. 背景与目标

Runory 当前核心能力围绕 SSH、SFTP、Agentic Runtime、Tool Registry、Policy、ChangeSet、会话管理等展开。

随着企业使用场景增加，仅支持以下连接方式已经不够：

- Direct SSH
- 普通 OpenSSH JumpHost / ProxyJump
- 静态 Host + Username + Password / Key

企业堡垒机通常具备更复杂的生命周期：

- 用户先认证堡垒机，而不是目标主机
- 支持密码、SSH Key、Token、MFA、OTP、扫码、浏览器 SSO
- 用户只能访问被授权的资产
- 一个资产可能暴露多个可用账号
- 目标凭据可能完全由堡垒机托管
- 堡垒机会记录命令、录像、文件传输
- 会话可能不是透明 TCP Tunnel
- 某些厂商通过 API、证书、代理协议或 WebSocket 建立连接
- 资产 IP 可能变化，但资产 ID 不变
- 登录流程可能要求用户多次交互

因此，Runory 不能把 JumpServer 仅仅实现成一个 `ProxyJump` 特例。

本设计的目标是：

1. 将“连接目标服务器”抽象为统一 Connection Provider 模型。
2. 将标准 JumpHost 与企业 Bastion 明确区分。
3. 建立厂商无关的 BastionProvider API。
4. 支持复杂 MFA / SSO / 用户交互认证状态机。
5. 支持资产发现、账号发现与服务器凭据托管。
6. Terminal、SFTP、Agent、自动化任务不感知底层堡垒机类型。
7. 新增一个堡垒机厂商时，不需要修改 Runory 核心 SSH 调用链。
8. 与 Runory Agentic V2 的 `AwaitingUser / AwaitingApproval / Resume` 模型自然结合。
9. 支持能力协商，不通过大量 `if provider == "xxx"` 实现。
10. 为未来企业版功能预留审计、Session Recording、证书、临时授权、动态凭证等能力。

---

# 2. 总体架构

建议将 Runory 的连接体系升级为以下结构：

```text
                    ┌────────────────────────────┐
                    │        UI / Commands       │
                    │ Terminal / SFTP / Agent    │
                    └─────────────┬──────────────┘
                                  │
                                  ▼
                       ┌────────────────────┐
                       │   SessionManager   │
                       └─────────┬──────────┘
                                 │
                                 ▼
                      ┌─────────────────────┐
                      │ ConnectionResolver  │
                      └──────────┬──────────┘
                                 │
              ┌──────────────────┼───────────────────┐
              │                  │                   │
              ▼                  ▼                   ▼
      ┌───────────────┐  ┌───────────────┐  ┌────────────────┐
      │ DirectProvider│  │JumpHostProvider│  │BastionProvider │
      └───────────────┘  └───────────────┘  └───────┬────────┘
                                                     │
                                            ┌────────┴────────┐
                                            │ BastionRegistry │
                                            └────────┬────────┘
                                                     │
                     ┌───────────────────────────────┼───────────────────────────────┐
                     │                               │                               │
                     ▼                               ▼                               ▼
             JumpServerProvider             TeleportProvider              BoundaryProvider
                     │
           ┌─────────┼───────────┐
           │         │           │
           ▼         ▼           ▼
          API       Auth        KoKo
           │                     │
           └──────────┬──────────┘
                      ▼
                Target Session
```

---

# 3. 核心设计原则

## 3.1 Bastion != JumpHost

这是整个架构最重要的边界。

普通 SSH JumpHost：

```text
Runory
  │ SSH
  ▼
Jump Host
  │ TCP forwarding
  ▼
Target:22
```

本质是：

```text
client -> jump host -> raw tcp -> target ssh
```

企业 Bastion：

```text
Runory
  │
  ▼
Bastion Authentication
  │
  ▼
Authorization
  │
  ▼
Asset Selection
  │
  ▼
Account Selection
  │
  ▼
Session Negotiation
  │
  ▼
Target
```

它可能根本不暴露目标服务器的：

- IP
- Port
- Password
- Private Key

因此两者必须是不同 Provider。

---

## 3.2 上层只面向 SessionManager

Terminal 不应：

```rust
connect_jumpserver(...)
```

Agent Tool 不应：

```rust
if jumpserver {
    ...
}
```

SFTP 不应：

```rust
match bastion_type {
    ...
}
```

统一调用：

```rust
session_manager.connect(host_id).await
```

由底层决定：

```text
Direct
JumpHost
JumpServer
Teleport
Boundary
...
```

---

## 3.3 资产 ID 优先，不依赖 IP

企业资产应使用：

```text
provider_asset_id
```

作为稳定标识。

错误：

```json
{
  "host": "10.10.20.31"
}
```

推荐：

```json
{
  "provider": "jumpserver",
  "asset_id": "e8456b57-xxxx-xxxx"
}
```

资产 IP 可由 Provider 动态解析。

---

## 3.4 不提取目标服务器密码

推荐：

```text
Runory
   │
   │ Bastion Identity
   ▼
JumpServer
   │
   │ Managed Credential
   ▼
Target
```

避免：

```text
JumpServer
   │ 返回 root password
   ▼
Runory
   │
   ▼
Target
```

原因：

- 减少敏感凭据暴露
- 保留堡垒机审计
- 保留账号托管
- 保留授权策略
- 保留会话录像
- 支持动态凭证

---

## 3.5 Provider 不直接操作 UI

错误：

```rust
JumpServerProvider::show_otp_dialog()
```

正确：

```text
Provider
   │
   ▼
AuthChallenge::Totp
   │
   ▼
Runtime
   │
   ▼
UI
```

用户输入：

```text
UI
  │
  ▼
Runtime
  │
  ▼
provider.continue_auth(...)
```

这样同一 Provider 能同时服务：

- Desktop GUI
- CLI
- Agent
- Remote API
- Headless automation

---

# 4. Connection Provider 总体抽象

建议第一层不是直接定义 Bastion，而是定义统一 Connection Provider：

```rust
#[async_trait]
pub trait ConnectionProvider: Send + Sync {
    fn id(&self) -> &'static str;

    async fn resolve(
        &self,
        ctx: &ConnectionContext,
        request: ConnectionRequest,
    ) -> Result<ResolvedConnection, ConnectionError>;
}
```

实现：

```text
DirectConnectionProvider
JumpHostConnectionProvider
BastionConnectionProvider
```

其中：

```text
BastionConnectionProvider
    ↓
BastionRegistry
    ↓
JumpServerProvider / TeleportProvider / ...
```

---

# 5. ConnectionRoute 模型

Runory Host 不应直接把所有 SSH 参数写死。

建议：

```rust
pub enum ConnectionRoute {
    Direct,

    JumpHost {
        jump_host_id: HostId,
    },

    Bastion {
        bastion_id: BastionId,
        provider: String,
        asset_id: String,
        account_id: Option<String>,
    },
}
```

示例：

```json
{
  "id": "host-prod-db-01",
  "name": "prod-db-01",
  "route": {
    "type": "bastion",
    "provider": "jumpserver",
    "bastion_id": "corp-jumpserver",
    "asset_id": "fa6d8f25-c8ba-4b01",
    "account_id": "root"
  }
}
```

---

# 6. BastionProvider 接口

建议核心接口：

```rust
#[async_trait]
pub trait BastionProvider: Send + Sync {

    fn id(&self) -> &'static str;

    fn display_name(&self) -> &'static str;

    fn capabilities(&self) -> BastionCapabilities;

    async fn probe(
        &self,
        endpoint: &BastionEndpoint,
    ) -> Result<BastionProbeResult, BastionError>;

    async fn start_auth(
        &self,
        ctx: &BastionContext,
        credential: &BastionCredential,
    ) -> Result<AuthStepResult, BastionError>;

    async fn continue_auth(
        &self,
        session: &AuthSession,
        response: AuthChallengeResponse,
    ) -> Result<AuthStepResult, BastionError>;

    async fn list_assets(
        &self,
        session: &AuthSession,
        query: AssetQuery,
    ) -> Result<AssetPage, BastionError>;

    async fn list_accounts(
        &self,
        session: &AuthSession,
        asset: &BastionAsset,
    ) -> Result<Vec<BastionAccount>, BastionError>;

    async fn connect(
        &self,
        session: &AuthSession,
        request: BastionConnectRequest,
    ) -> Result<BastionConnection, BastionError>;

    async fn disconnect(
        &self,
        connection: &BastionConnection,
    ) -> Result<(), BastionError>;
}
```

---

# 7. Capability 能力协商

不要通过：

```rust
if provider == "jumpserver"
```

决定行为。

使用 Capability。

```rust
bitflags! {
    pub struct BastionCapabilities: u64 {
        const SSH                  = 1 << 0;
        const SFTP                 = 1 << 1;
        const SCP                  = 1 << 2;
        const PORT_FORWARD         = 1 << 3;

        const ASSET_DISCOVERY      = 1 << 4;
        const ACCOUNT_DISCOVERY    = 1 << 5;

        const PASSWORD_AUTH        = 1 << 6;
        const KEY_AUTH             = 1 << 7;
        const TOKEN_AUTH           = 1 << 8;
        const MFA                  = 1 << 9;
        const BROWSER_SSO          = 1 << 10;

        const SESSION_RECORDING    = 1 << 11;
        const COMMAND_AUDIT        = 1 << 12;
        const FILE_AUDIT           = 1 << 13;

        const DYNAMIC_CREDENTIAL   = 1 << 14;
        const TEMP_ACCESS          = 1 << 15;

        const SESSION_RESUME       = 1 << 16;
        const AGENT_FORWARDING     = 1 << 17;
    }
}
```

UI 和 Runtime 根据 Capability 工作。

---

# 8. Bastion Endpoint

一个堡垒机实例：

```rust
pub struct BastionEndpoint {
    pub id: BastionId,
    pub provider: String,

    pub name: String,

    pub host: String,

    pub ports: BastionPorts,

    pub tls: Option<TlsOptions>,

    pub provider_config: serde_json::Value,
}
```

端口模型：

```rust
pub struct BastionPorts {
    pub api: Option<u16>,
    pub ssh: Option<u16>,
    pub web: Option<u16>,
}
```

JumpServer 示例：

```json
{
  "provider": "jumpserver",
  "host": "jump.example.com",
  "ports": {
    "ssh": 2222,
    "web": 443,
    "api": 443
  }
}
```

---

# 9. Credential 模型

只保存“用户访问堡垒机的身份”。

```rust
pub enum BastionCredential {
    Password {
        username: String,
        password_ref: SecretRef,
    },

    SshKey {
        username: String,
        key_ref: SecretRef,
        passphrase_ref: Option<SecretRef>,
    },

    Token {
        token_ref: SecretRef,
    },

    BrowserSso {
        username_hint: Option<String>,
    },

    ExternalAgent {
        username: String,
    },
}
```

目标服务器账号不是 Credential。

---

# 10. AuthSession

```rust
pub struct AuthSession {
    pub id: AuthSessionId,

    pub bastion_id: BastionId,

    pub provider: String,

    pub principal: BastionPrincipal,

    pub provider_state: ProtectedProviderState,

    pub expires_at: Option<DateTime<Utc>>,
}
```

说明：

`provider_state` 可用于保存：

- API Token
- Session Cookie
- SSH session state
- 临时证书
- Challenge ID
- 厂商专有 session identifier

禁止直接明文落盘敏感字段。

---

# 11. 认证状态机

企业认证绝不能只返回：

```text
OK / ERROR
```

建议：

```rust
pub enum AuthStepResult {
    Authenticated(AuthSession),

    Challenge(AuthChallenge),

    ExternalAction(ExternalAuthAction),
}
```

---

## 11.1 AuthChallenge

```rust
pub enum AuthChallenge {

    Password {
        id: String,
        message: String,
    },

    Totp {
        id: String,
        message: String,
    },

    SmsCode {
        id: String,
        message: String,
        masked_target: Option<String>,
    },

    Confirm {
        id: String,
        message: String,
    },

    Choice {
        id: String,
        message: String,
        choices: Vec<AuthChoice>,
    },

    Text {
        id: String,
        message: String,
        secret: bool,
    },
}
```

---

## 11.2 ExternalAuthAction

支持未来 SSO：

```rust
pub enum ExternalAuthAction {
    OpenBrowser {
        url: String,
        callback_uri: Option<String>,
    },

    DeviceCode {
        verification_uri: String,
        user_code: String,
        expires_in: Duration,
    },

    QrCode {
        payload: String,
    },
}
```

这样可以适配：

- OAuth
- OIDC
- SAML
- Teleport Browser Login
- JumpServer MFA
- 企业扫码认证

---

# 12. 统一 Bastion 状态机

```rust
pub enum BastionSessionState {
    Idle,

    Probing,

    Authenticating,

    AwaitingUser {
        challenge: AuthChallenge,
    },

    AwaitingExternalAuth {
        action: ExternalAuthAction,
    },

    Authenticated,

    DiscoveringAssets,

    SelectingAsset,

    DiscoveringAccounts,

    SelectingAccount,

    Connecting,

    Connected,

    Disconnected,

    Failed {
        error: BastionError,
    },
}
```

---

# 13. 与 Agentic Runtime 集成

Runory Agent 不应知道 JumpServer。

Agent 只表达：

```text
ssh_execute(
    host = "prod-db-01",
    command = "df -h"
)
```

流程：

```text
Agent Tool
   │
   ▼
SessionManager
   │
   ▼
ConnectionResolver
   │
   ▼
route = Bastion
   │
   ▼
JumpServerProvider
   │
   ├─ Auth OK
   │
   └─ MFA required
          │
          ▼
AgentRun::AwaitingUser
          │
          ▼
UI 输入 OTP
          │
          ▼
Resume AgentRun
          │
          ▼
建立会话
          │
          ▼
执行 df -h
```

建议在 AgentRun 的 pending operation 中保存：

```rust
pub struct PendingConnectionOperation {
    pub host_id: HostId,

    pub bastion_id: BastionId,

    pub provider: String,

    pub auth_session_id: Option<AuthSessionId>,

    pub challenge_id: Option<String>,

    pub continuation: ConnectionContinuation,
}
```

这样进程中断或 UI 切换后可恢复。

---

# 14. Asset 模型

不要复用普通 Host。

```rust
pub struct BastionAsset {
    pub provider: String,

    pub remote_id: String,

    pub name: String,

    pub address: Option<String>,

    pub platform: Option<String>,

    pub protocols: Vec<AssetProtocol>,

    pub node_path: Option<String>,

    pub labels: HashMap<String, String>,

    pub metadata: serde_json::Value,
}
```

关键字段：

```text
remote_id
```

必须来自远端堡垒机。

---

# 15. AssetProtocol

```rust
pub struct AssetProtocol {
    pub protocol: BastionProtocol,

    pub port: Option<u16>,

    pub enabled: bool,
}
```

```rust
pub enum BastionProtocol {
    Ssh,
    Sftp,
    Scp,
    Rdp,
    Vnc,
    Telnet,
    Custom(String),
}
```

Runory 初期只实现：

```text
SSH
SFTP
```

但模型允许扩展。

---

# 16. BastionAccount

```rust
pub struct BastionAccount {
    pub remote_id: Option<String>,

    pub username: String,

    pub display_name: Option<String>,

    pub privileged: bool,

    pub secret_managed_by_bastion: bool,

    pub metadata: serde_json::Value,
}
```

典型 JumpServer：

```text
username: root
secret_managed_by_bastion: true
```

Runory 不拿到 root 密码。

---

# 17. AssetQuery

企业资产通常上千台，不能一次全部加载。

```rust
pub struct AssetQuery {
    pub search: Option<String>,

    pub node: Option<String>,

    pub protocol: Option<BastionProtocol>,

    pub page: u32,

    pub page_size: u32,
}
```

返回：

```rust
pub struct AssetPage {
    pub items: Vec<BastionAsset>,

    pub page: u32,

    pub page_size: u32,

    pub has_more: bool,
}
```

---

# 18. BastionConnectRequest

```rust
pub struct BastionConnectRequest {
    pub asset: BastionAsset,

    pub account: BastionAccount,

    pub protocol: BastionProtocol,

    pub terminal: Option<TerminalOptions>,

    pub options: BastionConnectOptions,
}
```

```rust
pub struct TerminalOptions {
    pub term: String,

    pub cols: u16,

    pub rows: u16,
}
```

```rust
pub struct BastionConnectOptions {
    pub request_sftp: bool,

    pub request_port_forward: bool,

    pub agent_forwarding: bool,

    pub locale: Option<String>,
}
```

---

# 19. BastionConnection

不要固定返回 TCP Stream。

```rust
pub enum BastionConnection {

    SshTransport {
        transport: Box<dyn AsyncReadWrite>,
        metadata: BastionSessionMetadata,
    },

    Pty {
        channel: Box<dyn PtyChannel>,
        metadata: BastionSessionMetadata,
    },

    Tunnel {
        stream: Box<dyn AsyncReadWrite>,
        metadata: BastionSessionMetadata,
    },

    Native {
        handle: Box<dyn NativeBastionSession>,
        metadata: BastionSessionMetadata,
    },
}
```

原因：

未来厂商可能：

- SSH Gateway
- WebSocket PTY
- 临时证书 + Proxy
- 本地代理
- 专有协议

---

# 20. Session Metadata

```rust
pub struct BastionSessionMetadata {
    pub session_id: Option<String>,

    pub provider: String,

    pub asset_id: String,

    pub account: String,

    pub recording: bool,

    pub command_audit: bool,

    pub file_audit: bool,

    pub started_at: DateTime<Utc>,
}
```

UI 可以显示：

```text
JumpServer Session

Asset: prod-db-01
Account: root

Recording: ON
Command Audit: ON
File Audit: ON
```

---

# 21. SessionManager

最终所有上层组件只使用：

```rust
pub trait SessionManager {

    async fn connect(
        &self,
        host_id: HostId,
        intent: SessionIntent,
    ) -> Result<SessionHandle, SessionError>;

}
```

SessionIntent：

```rust
pub enum SessionIntent {
    Terminal,
    ExecuteCommand,
    Sftp,
    PortForward,
    AgentTool,
}
```

---

# 22. ConnectionResolver

流程：

```text
HostId
  │
  ▼
Load Host
  │
  ▼
ConnectionRoute
  │
  ├─ Direct
  │
  ├─ JumpHost
  │
  └─ Bastion
        │
        ▼
    BastionRegistry
        │
        ▼
    provider.connect()
```

伪代码：

```rust
match host.route {
    ConnectionRoute::Direct => {
        direct_provider.resolve(...).await
    }

    ConnectionRoute::JumpHost { .. } => {
        jump_provider.resolve(...).await
    }

    ConnectionRoute::Bastion {
        provider,
        ..
    } => {
        bastion_provider
            .resolve(provider, ...)
            .await
    }
}
```

---

# 23. BastionRegistry

```rust
pub struct BastionRegistry {
    providers: HashMap<String, Arc<dyn BastionProvider>>,
}
```

```rust
impl BastionRegistry {

    pub fn register(
        &mut self,
        provider: Arc<dyn BastionProvider>,
    ) {
        self.providers.insert(
            provider.id().to_string(),
            provider,
        );
    }

    pub fn get(
        &self,
        id: &str,
    ) -> Option<Arc<dyn BastionProvider>> {
        self.providers.get(id).cloned()
    }
}
```

注册：

```rust
registry.register(
    Arc::new(JumpServerProvider::new())
);

registry.register(
    Arc::new(TeleportProvider::new())
);
```

---

# 24. Provider 插件化

第一阶段可以静态编译：

```text
providers/
  jumpserver
  teleport
```

未来可升级：

```text
Bastion Plugin API
      │
      ├── native dynamic library
      ├── WASM provider
      └── sidecar RPC provider
```

建议长期优先考虑：

```text
WASM + capability sandbox
```

原因：

- 厂商插件隔离
- 更安全
- 易发布
- 容易版本控制
- 减少 ABI 问题

---

# 25. Provider Version Contract

```rust
pub struct ProviderManifest {
    pub id: String,

    pub name: String,

    pub provider_version: Version,

    pub bastion_api_version: Version,

    pub capabilities: BastionCapabilities,
}
```

例如：

```json
{
  "id": "jumpserver",
  "name": "JumpServer",
  "provider_version": "1.0.0",
  "bastion_api_version": "1.0.0"
}
```

Runory Core 根据：

```text
bastion_api_version
```

判断兼容性。

---

# 26. 推荐目录结构

```text
src/
├── connection/
│   ├── mod.rs
│   ├── resolver.rs
│   ├── manager.rs
│   ├── route.rs
│   ├── errors.rs
│   │
│   ├── direct/
│   │   ├── mod.rs
│   │   └── provider.rs
│   │
│   ├── jump_host/
│   │   ├── mod.rs
│   │   ├── provider.rs
│   │   └── tunnel.rs
│   │
│   └── bastion/
│       ├── mod.rs
│       ├── provider.rs
│       ├── registry.rs
│       ├── capabilities.rs
│       ├── auth.rs
│       ├── asset.rs
│       ├── account.rs
│       ├── session.rs
│       ├── errors.rs
│       │
│       └── providers/
│           ├── jumpserver/
│           │   ├── mod.rs
│           │   ├── provider.rs
│           │   ├── api.rs
│           │   ├── auth.rs
│           │   ├── assets.rs
│           │   ├── accounts.rs
│           │   ├── koko.rs
│           │   ├── session.rs
│           │   ├── version.rs
│           │   └── errors.rs
│           │
│           ├── teleport/
│           └── boundary/
```

---

# 27. JumpServerProvider 设计

JumpServer Provider 建议拆成两条路径：

```text
JumpServerProvider
    │
    ├── API Plane
    │     ├ Auth
    │     ├ Assets
    │     ├ Accounts
    │     ├ Permissions
    │     └ Version
    │
    └── Session Plane
          └ KoKo SSH Gateway
```

不要让 REST API 和 KoKo SSH 实现互相耦合。

---

# 28. JumpServer API Client

```rust
pub struct JumpServerApiClient {
    endpoint: Url,

    http: reqwest::Client,

    auth: JumpServerApiAuth,
}
```

职责：

```text
probe version
login
refresh token
list authorized assets
list authorized accounts
query asset
query user permissions
```

不要负责 SSH 数据流。

---

# 29. JumpServer KoKo Client

```rust
pub struct KokoClient {
    endpoint: String,

    port: u16,

    ssh_client: Arc<dyn SshClient>,
}
```

职责：

```text
connect KoKo
authenticate JumpServer user
negotiate asset
negotiate target account
open PTY
open session
resize terminal
heartbeat
disconnect
```

---

# 30. JumpServer 登录建议

优先设计成可插拔流程。

可能形式：

```text
username/password
username/key
password + OTP
browser SSO
token
```

不要在核心 Provider 中假设：

```text
JumpServer = password login
```

---

# 31. JumpServer Asset Discovery

流程：

```text
Runory
  │
  ▼
JumpServer API
  │
  ▼
Authorized Assets
  │
  ▼
BastionAsset[]
```

资产同步模式建议支持：

### 动态查询

每次打开资产选择器：

```text
query remote assets
```

优点：

- 权限实时
- 不缓存敏感资产

### 本地索引

保存：

```text
asset_id
name
node
protocol
last_seen
```

不保存：

```text
target password
private key
```

推荐最终：

```text
remote source of truth + local lightweight cache
```

---

# 32. 资产缓存

建议：

```rust
pub struct CachedBastionAsset {
    pub provider: String,

    pub bastion_id: BastionId,

    pub remote_id: String,

    pub name: String,

    pub metadata: MinimalMetadata,

    pub synced_at: DateTime<Utc>,
}
```

默认 TTL：

```text
5 ~ 15 分钟
```

但 Provider 可覆盖。

---

# 33. Account Discovery

用户选择 Asset 后：

```text
JumpServer
   │
   ▼
Authorized Accounts
   │
   ▼
root
deploy
ubuntu
postgres
```

Runory UI：

```text
Asset
  prod-db-01

Account
  [ root ▾ ]
```

绝不显示密码。

---

# 34. UI 设计

连接类型：

```text
Connection Type

● Direct SSH

○ SSH Jump Host

○ Bastion
```

选择 Bastion：

```text
Bastion Provider

[ JumpServer ▼ ]
```

配置：

```text
Name
Corp JumpServer

Server
jump.company.com

SSH Port
2222

HTTPS Port
443

Authentication
Password + MFA
```

---

# 35. Asset Selector

不要使用普通 Host 表单。

推荐：

```text
Select Asset

Search assets...
--------------------------------

Production
  prod-web-01
  prod-web-02
  prod-db-01

Testing
  test-api-01
```

支持：

```text
Search
Node Tree
Protocol
Tag / Label
Favorite
Recent
```

---

# 36. MFA UI

状态驱动：

```text
JumpServer requires verification

One-time password

[             ]

[Verify]
```

UI 只渲染：

```text
AuthChallenge
```

不包含 JumpServer 特定逻辑。

---

# 37. Session UI

Terminal 顶部可以显示：

```text
prod-db-01
via Corp JumpServer

root
Recording ●
```

这样用户明确知道：

```text
当前会话被堡垒机审计
```

---

# 38. SFTP

SFTP 必须走同一 Session / Route 模型。

错误：

```text
Terminal -> Bastion
SFTP -> direct target IP
```

正确：

```text
Terminal
    │
    ├── SessionManager
    │
SFTP
    │
    └── SessionManager
            │
            ▼
         Bastion
```

---

# 39. Port Forward

Port Forward 应由 Capability 控制：

```rust
if provider.capabilities()
    .contains(PORT_FORWARD)
{
    enable_port_forward_ui();
}
```

否则禁用。

不能假定所有堡垒机支持：

```text
-L
-R
-D
```

---

# 40. Agent Tool 集成

Agent Tools 保持：

```text
ssh_execute
ssh_read_file
ssh_write_file
sftp_upload
sftp_download
```

Tool 不加入：

```text
jumpserver_execute
teleport_execute
```

这是一个重要架构边界。

---

# 41. Policy 集成

Policy 输入增加 Route Context：

```rust
pub struct ExecutionContext {
    pub host_id: HostId,

    pub connection_route: ConnectionRouteSummary,

    pub bastion_metadata: Option<BastionPolicyContext>,
}
```

例如：

```text
provider = jumpserver
recording = true
target_account = root
asset_tags = ["production"]
```

Policy 可以：

```text
production + root + write
    ↓
require approval
```

---

# 42. ChangeSet 集成

堡垒机不应绕过 ChangeSet。

流程：

```text
Agent
  │
  ▼
Tool Registry
  │
  ▼
Policy
  │
  ▼
ChangeSet
  │
  ▼
SessionManager
  │
  ▼
Bastion
  │
  ▼
Target
```

不要出现旧模式：

```text
Agent
  ↓
PTY type command
  ↓
Target
```

---

# 43. Error Model

统一错误：

```rust
pub enum BastionError {

    Network,

    AuthenticationFailed,

    AuthenticationExpired,

    MfaRequired,

    PermissionDenied,

    AssetNotFound,

    AccountNotAllowed,

    ProtocolUnsupported,

    CapabilityUnavailable,

    SessionRejected,

    SessionExpired,

    ProviderUnavailable,

    ProviderVersionUnsupported,

    ProviderProtocolError,

    Timeout,

    Cancelled,

    Internal,
}
```

同时保留：

```rust
provider_error_code: Option<String>
provider_message: Option<String>
```

用于排障。

---

# 44. Retry 策略

不要所有错误都自动重试。

### 可重试

```text
Network
Timeout
ProviderUnavailable
TokenRefresh
```

### 不自动重试

```text
AuthenticationFailed
PermissionDenied
AccountNotAllowed
MFA invalid
```

避免触发企业安全锁定策略。

---

# 45. Secret 管理

所有秘密统一走 Runory SecretStore：

```rust
pub trait SecretStore {
    async fn get(
        &self,
        reference: &SecretRef
    ) -> Result<SecretValue>;
}
```

禁止：

```text
SQLite plaintext password
JSON plaintext token
日志打印 token
```

Windows 可以使用：

```text
Windows Credential Manager / DPAPI
```

跨平台后可抽象：

```text
OS Keychain
```

---

# 46. 日志规范

禁止日志：

```text
password
private key
access token
cookie
otp
authorization header
```

建议：

```text
provider=jumpserver
bastion=corp
asset_id=xxx
account=root
session_id=xxx
state=connected
```

---

# 47. Telemetry

可记录：

```text
provider
connection duration
auth duration
failure category
session type
feature usage
```

禁止上传：

```text
hostname
asset name
username
command
IP
credential
```

除非用户明确开启企业诊断。

---

# 48. Provider Contract Test

每个 Provider 都必须通过统一测试：

```text
ProviderContract
├── probe
├── auth
├── challenge
├── asset pagination
├── account discovery
├── connect
├── disconnect
├── cancel
├── session expiry
├── network failure
└── unsupported capability
```

---

# 49. MockBastionProvider

强烈建议第一阶段就实现：

```rust
MockBastionProvider
```

它支持：

```text
fake assets
fake accounts
fake MFA
fake timeout
fake permission denied
fake session expiry
```

这样前端、Runtime 和 Provider Framework 可以在 JumpServer 适配完成前独立开发。

---

# 50. 示例 Mock 流程

```text
User → Connect

MockProvider
    ↓
Challenge::Totp

UI
    ↓
123456

MockProvider
    ↓
Authenticated

list_assets
    ↓
prod-db-01

list_accounts
    ↓
root

connect
    ↓
MockSession
```

---

# 51. Provider 生命周期

```text
Created
   │
   ▼
Probe
   │
   ▼
Ready
   │
   ▼
Auth
   │
   ▼
Authenticated
   │
   ▼
Discover
   │
   ▼
Connect
   │
   ▼
Active
   │
   ▼
Disconnect
```

生命周期对象不应永久绑定 UI 页面。

---

# 52. Session 生命周期

```text
Pending
   │
   ▼
Connecting
   │
   ▼
Active
   │
   ├── Suspended
   │
   └── ReauthRequired
   │
   ▼
Closing
   │
   ▼
Closed
```

---

# 53. Reauthentication

企业 session 可能过期。

Provider 应返回：

```text
AuthExpired
```

SessionManager 决定：

```text
pause session
request user auth
resume if provider supports
```

不能直接把 Terminal 销毁。

---

# 54. 多窗口 Session 复用

建议未来支持：

```text
1 Bastion Auth Session
        │
        ├── SSH prod-web-01
        ├── SSH prod-db-01
        └── SFTP prod-web-01
```

但不要默认复用目标 Session。

分两层：

```text
AuthSession
TargetSession
```

---

# 55. 并发控制

Provider 可声明：

```rust
pub struct ProviderLimits {
    pub max_auth_sessions: Option<u32>,
    pub max_target_sessions: Option<u32>,
}
```

避免厂商限制导致大量异常。

---

# 56. Cancel 模型

所有长操作需要：

```rust
CancellationToken
```

例如：

```rust
list_assets(ctx, cancel)
connect(ctx, cancel)
authenticate(ctx, cancel)
```

用户关闭窗口时能立即中止。

---

# 57. Timeout

统一：

```rust
pub struct BastionTimeouts {
    pub probe: Duration,
    pub auth: Duration,
    pub discovery: Duration,
    pub connect: Duration,
    pub io_idle: Option<Duration>,
}
```

Provider 可以覆盖。

---

# 58. Event Bus

建议 Connection 层发布事件：

```rust
pub enum ConnectionEvent {

    StateChanged,

    AuthChallenge,

    AssetSelected,

    AccountSelected,

    SessionConnected,

    SessionDisconnected,

    SessionWarning,

    Error,
}
```

UI / Agent / Logging 都订阅事件。

避免：

```text
Provider -> UI
Provider -> Agent
Provider -> Logger
```

强耦合。

---

# 59. JumpServer MVP

第一阶段建议仅实现：

```text
✓ JumpServer instance config
✓ SSH KoKo connection
✓ Password auth
✓ OTP challenge
✓ Asset selection
✓ Account selection
✓ Interactive terminal
✓ resize
✓ disconnect
✓ session metadata
```

暂不实现：

```text
SFTP
Port Forward
API full sync
Browser SSO
Session resume
```

先跑通最小闭环。

---

# 60. JumpServer Phase 2

```text
✓ REST API integration
✓ Asset discovery
✓ Account discovery
✓ local cache
✓ SFTP
✓ permission metadata
✓ session recording metadata
```

---

# 61. JumpServer Phase 3

```text
✓ Browser SSO
✓ token refresh
✓ port forwarding
✓ advanced MFA
✓ session resume
✓ diagnostics
```

---

# 62. 开发实施阶段

## Phase A：核心抽象

实现：

```text
ConnectionRoute
ConnectionProvider
ConnectionResolver
BastionProvider
BastionRegistry
Capabilities
AuthChallenge
BastionAsset
BastionAccount
BastionConnection
```

不连接任何真实堡垒机。

---

## Phase B：Mock Provider

实现：

```text
MockBastionProvider
```

验证：

```text
UI
Runtime
AwaitingUser
Resume
Asset selection
Account selection
SessionManager
```

---

## Phase C：JumpServer MVP

实现：

```text
JumpServer Provider
KoKo client
Password
OTP
SSH terminal
```

---

## Phase D：JumpServer API

实现：

```text
Asset Discovery
Account Discovery
Permission
Caching
```

---

## Phase E：SFTP / Agent

全部统一走：

```text
SessionManager
```

禁止直接连接目标服务器。

---

## Phase F：第二个 Provider

建议第二个适配：

```text
Teleport
```

目的不是功能覆盖，而是验证：

```text
BastionProvider 抽象是否真正厂商无关
```

如果第二个 Provider 需要修改核心接口，说明第一版抽象不够好。

---

# 63. 数据库迁移

原 Host：

```text
host
port
username
password
jump_host
```

升级：

```text
hosts
connection_routes
bastion_instances
bastion_asset_bindings
credential_refs
```

---

# 64. bastion_instances

```sql
CREATE TABLE bastion_instances (
    id TEXT PRIMARY KEY,

    provider TEXT NOT NULL,

    name TEXT NOT NULL,

    endpoint_json TEXT NOT NULL,

    credential_ref TEXT,

    created_at INTEGER NOT NULL,

    updated_at INTEGER NOT NULL
);
```

---

# 65. host route

```sql
ALTER TABLE hosts
ADD COLUMN route_type TEXT;

ALTER TABLE hosts
ADD COLUMN route_config_json TEXT;
```

推荐过渡阶段不要立即删除旧字段。

---

# 66. Asset Binding

```sql
CREATE TABLE bastion_asset_bindings (
    host_id TEXT PRIMARY KEY,

    bastion_id TEXT NOT NULL,

    provider TEXT NOT NULL,

    remote_asset_id TEXT NOT NULL,

    remote_account_id TEXT,

    cached_name TEXT,

    last_synced_at INTEGER
);
```

---

# 67. Migration

原：

```text
jump_host_id != null
```

迁移：

```text
route =
JumpHost {
    jump_host_id
}
```

原普通服务器：

```text
route = Direct
```

---

# 68. Backward Compatibility

第一阶段：

```text
Old Host Model
     ↓
Compatibility Adapter
     ↓
ConnectionRoute
```

第二阶段才移除旧逻辑。

避免一次性重构影响稳定性。

---

# 69. 安全边界

Provider 必须遵守：

```text
Provider
  ├─ 不绕过 Policy
  ├─ 不直接执行 Agent 命令
  ├─ 不访问 ChangeSet
  ├─ 不操作 UI
  ├─ 不持久化 plaintext secret
  └─ 不自行修改 Host
```

Provider 只是：

```text
身份认证 + 资源解析 + 会话建立
```

---

# 70. 不建议的设计

## 错误 1

```rust
enum ConnectionType {
    Direct,
    ProxyJump,
    JumpServer,
    Teleport,
    CyberArk,
}
```

问题：

每增加厂商都修改 Core。

---

## 错误 2

```rust
connect_jumpserver()
connect_teleport()
connect_boundary()
```

上层迅速失控。

---

## 错误 3

把所有 Provider 都要求返回：

```rust
TcpStream
```

会限制未来协议。

---

## 错误 4

让 JumpServer API 获取目标密码。

这会破坏堡垒机的安全边界。

---

## 错误 5

把 MFA 写在 JumpServerProvider UI。

会导致 GUI 与 Provider 强耦合。

---

# 71. 推荐最终调用方式

Terminal：

```rust
let session = session_manager
    .connect(
        host_id,
        SessionIntent::Terminal
    )
    .await?;
```

Agent：

```rust
let session = session_manager
    .connect(
        host_id,
        SessionIntent::AgentTool
    )
    .await?;
```

SFTP：

```rust
let session = session_manager
    .connect(
        host_id,
        SessionIntent::Sftp
    )
    .await?;
```

三者都不关心：

```text
Direct
JumpHost
JumpServer
Teleport
```

---

# 72. 目标架构评价标准

满足以下条件，可认为 BastionProvider 架构设计成功：

### 核心解耦

新增 Provider 不修改：

```text
Terminal
Agent Tool
SFTP UI
SessionManager public API
Policy
ChangeSet
```

### 可交互认证

支持：

```text
Password
OTP
MFA
Browser Login
Choice
Confirmation
```

### 不泄漏目标凭据

Runory 能通过 JumpServer 登录：

```text
root@server
```

但 Runory 不知道：

```text
root password
```

### Provider 可测试

所有 Provider 可通过：

```text
Provider Contract Test
```

### 第二厂商验证

Teleport 或 Boundary Provider 可以在不重构 Core 的情况下实现。

---

# 73. 推荐 Runory 最终模块关系

```text
                 ┌──────────────────┐
                 │      Agent       │
                 └────────┬─────────┘
                          │
                 ┌────────▼─────────┐
                 │   Tool Registry  │
                 └────────┬─────────┘
                          │
                 ┌────────▼─────────┐
                 │      Policy      │
                 └────────┬─────────┘
                          │
                 ┌────────▼─────────┐
                 │    ChangeSet     │
                 └────────┬─────────┘
                          │
              ┌───────────▼────────────┐
              │     SessionManager     │
              └───────────┬────────────┘
                          │
              ┌───────────▼────────────┐
              │   ConnectionResolver   │
              └───────────┬────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        │                 │                 │
        ▼                 ▼                 ▼
     Direct           JumpHost           Bastion
                                            │
                                            ▼
                                    BastionRegistry
                                            │
                      ┌─────────────────────┼────────────────────┐
                      │                     │                    │
                      ▼                     ▼                    ▼
                 JumpServer            Teleport             Boundary
                      │
                      ▼
                  SSH / API
                      │
                      ▼
                    Target
```

---

# 74. 最终建议

Runory 的 Bastion 支持不要设计成一个“JumpServer 功能”。

应该把它定义为：

```text
Enterprise Connection Framework
```

JumpServer 只是：

```text
第一个 BastionProvider
```

核心层需要长期稳定的只有：

```text
ConnectionRoute
ConnectionResolver
SessionManager
BastionProvider
BastionRegistry
Auth State Machine
Capability Model
Asset / Account Model
Session Model
```

厂商差异全部留在 Provider 内。

如果未来加入：

```text
JumpServer
Teleport
HashiCorp Boundary
CyberArk
BeyondTrust
StrongDM
自研堡垒机
```

Runory Core 最理想的状态应该是：

```text
0 changes
```

最多只需要：

```rust
registry.register(
    Arc::new(NewProvider::new())
);
```

这才说明 BastionProvider 抽象真正达到了目标。

---

# 75. 建议第一批开发任务

可以直接拆成以下工程任务：

```text
BAS-001
新增 ConnectionRoute

BAS-002
实现 ConnectionResolver

BAS-003
定义 BastionProvider trait

BAS-004
实现 BastionCapabilities

BAS-005
实现 AuthChallenge 状态机

BAS-006
实现 BastionAsset / Account

BAS-007
实现 BastionConnection

BAS-008
实现 BastionRegistry

BAS-009
实现 MockBastionProvider

BAS-010
Runtime 接入 AwaitingUser

BAS-011
SessionManager 接入 Bastion Route

BAS-012
JumpServerProvider skeleton

BAS-013
JumpServer KoKo SSH

BAS-014
JumpServer MFA

BAS-015
JumpServer Asset Discovery

BAS-016
JumpServer Account Discovery

BAS-017
Terminal 集成测试

BAS-018
Agent Tool 集成测试

BAS-019
SFTP 集成

BAS-020
Provider Contract Test
```

推荐从：

```text
BAS-001 ~ BAS-010
```

先完成核心 Framework。

此时甚至不需要 JumpServer 环境，就可以验证架构是否正确。

然后再做：

```text
BAS-012 ~ BAS-020
```

真实厂商接入。

---

**文档版本：v1.0**  
**目标项目：Runory**  
**架构主题：Enterprise Bastion Provider Framework**
