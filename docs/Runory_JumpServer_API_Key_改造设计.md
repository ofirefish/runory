# Runory JumpServer API Key 接入改造设计

> 文档用途：指导 Runory 增加 JumpServer API Key 接入能力，并将其纳入可扩展的 BastionProvider 架构。
>
> 目标平台：Runory（Tauri 2 + React + Rust）
>
> 文档版本：v1.0
>
> 日期：2026-09-10

---

## 1. 背景

Runory 当前的核心能力是通过 SSH 建立服务器会话，并在其上提供 Terminal、SFTP、Agentic Runtime、Tool Registry、Policy、ChangeSet 等能力。

现有 SSH 模式主要围绕“直接连接目标服务器”或“标准 SSH 堡垒机 / ProxyJump”展开。当服务器资产位于 JumpServer 后方时，Runory 不应绕过 JumpServer 直接获取目标服务器密码、私钥或真实登录凭据，而应使用 JumpServer 官方提供的控制面 API 与连接令牌机制完成授权连接。

JumpServer 的 Access Key 用于调用 REST API。Access Key 权限继承创建该 Key 的 JumpServer 用户权限。连接资产时，应进一步申请 Connection Token，再通过 JumpServer 的 SSH 接入组件（通常为 KoKo）进入目标资产。

因此，本次改造的核心不是“让 SSH 支持 API Key”，而是增加一个独立的 `JumpServerProvider`，让 API Key 工作在堡垒机控制面，最终仍向 Runory SSH Core 输出统一的可连接描述。

---

## 2. 改造目标

### 2.1 核心目标

1. Runory 支持配置 JumpServer 实例。
2. 支持使用 JumpServer Access Key ID + Access Key Secret 进行 API 认证。
3. 支持检测 JumpServer 连通性和认证状态。
4. 支持获取当前 API Key 用户有权访问的资产。
5. 支持获取资产可用账号和 SSH 协议信息。
6. 支持为目标资产创建 Connection Token。
7. 将 JumpServer 连接转换为 Runory 统一的 `PreparedConnection`。
8. 复用现有 SSH Session、PTY、Terminal、SFTP 和 Agentic Runtime。
9. Access Key Secret 不进入 React 持久化层，不以明文写入 SQLite 或配置文件。
10. 为未来支持 Teleport、CyberArk、其他堡垒机预留统一 Provider 接口。

### 2.2 非目标

本阶段不实现以下内容：

- 完整 JumpServer 管理后台功能；
- 创建、修改、删除 JumpServer 资产；
- 创建 JumpServer 用户和授权规则；
- 获取或保存目标服务器真实密码；
- 绕过 JumpServer 审计链路；
- 在 Runory 内重新实现 JumpServer 的权限系统；
- 将 JumpServer API 细节暴露给 Agentic Runtime。

---

## 3. 设计原则

### 3.1 控制面与数据面分离

JumpServer API Key 属于控制面凭据：

```text
AccessKey ID + AccessKey Secret
        │
        ▼
JumpServer REST API
        │
        ├── 查询当前用户
        ├── 查询授权资产
        ├── 查询授权账号
        └── 创建 Connection Token
```

真正的 SSH 数据通道仍由 Runory SSH Core 建立：

```text
Connection Token
        │
        ▼
JumpServer / KoKo
        │
        ▼
目标 Linux SSH
```

禁止设计为：

```text
API Key -> SSH Core -> 目标服务器
```

### 3.2 Provider 屏蔽厂商差异

SSH Core 不应该知道 JumpServer、Teleport、CyberArk 等产品名。

所有堡垒机 Provider 最终统一输出：

```text
PreparedConnection
```

### 3.3 长期密钥与短期会话凭据分离

长期保存：

```text
AccessKey ID
AccessKey Secret
```

运行时短期获取：

```text
Connection Token
```

Connection Token 默认只存在内存，不长期落盘。

### 3.4 Agentic Runtime 不感知堡垒机

Agent 层只应看到一个已经建立的 `SshSession`。

以下能力原则上无需因 JumpServer 而重构：

```text
Tool Registry
Policy
ChangeSet
AwaitingApproval
AwaitingUser
AgentRun
PTY
Terminal
```

---

## 4. 总体架构

```text
┌──────────────────────────────────────────────────────────┐
│                       Runory UI                          │
│                                                          │
│  Hosts        Bastion Providers       Terminal / SFTP   │
└────────────────────────────┬─────────────────────────────┘
                             │ Tauri Commands
                             ▼
┌──────────────────────────────────────────────────────────┐
│                      Runory Core                         │
│                                                          │
│  HostService                                             │
│      │                                                   │
│      ▼                                                   │
│  BastionProviderRegistry                                 │
│      │                                                   │
│      ├── DirectProvider                                  │
│      ├── StandardSshBastionProvider                      │
│      ├── JumpServerProvider                              │
│      ├── TeleportProvider       [future]                 │
│      └── CyberArkProvider       [future]                 │
│                                                          │
│                  PreparedConnection                      │
│                           │                              │
│                           ▼                              │
│                       SSH Core                           │
│                           │                              │
│                  ┌────────┴────────┐                     │
│                  ▼                 ▼                     │
│               Terminal           SFTP                    │
│                  │                                       │
│                  ▼                                       │
│            Agentic Runtime                               │
└──────────────────────────────────────────────────────────┘
```

JumpServerProvider 内部：

```text
JumpServerProvider
      │
      ├── JumpServerApiClient
      │      │
      │      ├── AccessKeyAuth
      │      ├── HttpSigner
      │      └── HTTP Client
      │
      ├── AssetResolver
      ├── AccountResolver
      ├── ConnectionTokenManager
      └── ErrorMapper
```

---

## 5. 建议目录结构

```text
src-tauri/src/
├── bastion/
│   ├── mod.rs
│   ├── provider.rs
│   ├── registry.rs
│   ├── models.rs
│   ├── error.rs
│   │
│   ├── direct/
│   │   └── provider.rs
│   │
│   ├── ssh_bastion/
│   │   └── provider.rs
│   │
│   └── jumpserver/
│       ├── mod.rs
│       ├── provider.rs
│       ├── client.rs
│       ├── auth.rs
│       ├── signer.rs
│       ├── models.rs
│       ├── assets.rs
│       ├── accounts.rs
│       ├── connection_token.rs
│       ├── version.rs
│       └── error.rs
│
├── credential/
│   ├── mod.rs
│   ├── store.rs
│   └── windows.rs
│
├── ssh/
├── sftp/
└── agentic/
```

前端建议：

```text
src/
├── features/
│   ├── bastion/
│   │   ├── BastionProviderList.tsx
│   │   ├── BastionProviderForm.tsx
│   │   ├── JumpServerForm.tsx
│   │   ├── JumpServerAssetPicker.tsx
│   │   ├── JumpServerAccountPicker.tsx
│   │   └── types.ts
│   │
│   └── hosts/
│       └── HostConnectionForm.tsx
```

---

## 6. BastionProvider 抽象

建议定义统一接口：

```rust
#[async_trait]
pub trait BastionProvider: Send + Sync {
    fn id(&self) -> &str;

    fn provider_type(&self) -> BastionProviderType;

    async fn health_check(&self) -> Result<BastionHealth, BastionError>;

    async fn list_assets(
        &self,
        query: Option<&str>,
    ) -> Result<Vec<BastionAsset>, BastionError>;

    async fn get_asset(
        &self,
        asset_id: &str,
    ) -> Result<BastionAsset, BastionError>;

    async fn list_accounts(
        &self,
        asset_id: &str,
    ) -> Result<Vec<BastionAccount>, BastionError>;

    async fn prepare_connection(
        &self,
        request: PrepareConnectionRequest,
    ) -> Result<PreparedConnection, BastionError>;

    async fn release_connection(
        &self,
        connection: &PreparedConnection,
    ) -> Result<(), BastionError>;
}
```

类型定义：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BastionProviderType {
    Direct,
    SshBastion,
    JumpServer,
    Teleport,
    CyberArk,
    Custom(String),
}
```

注意：

- 不要把 `create_jumpserver_token()` 放到顶层 Provider Trait；
- 不要让调用方依据 provider 类型自行拼 JumpServer API；
- 所有厂商差异都封装在 Provider 内。

---

## 7. PreparedConnection 统一连接描述

这是本次架构改造最重要的数据类型。

```rust
#[derive(Debug)]
pub struct PreparedConnection {
    pub provider_id: Option<String>,
    pub transport: ConnectionTransport,
    pub endpoint: ConnectionEndpoint,
    pub username: Option<String>,
    pub auth: PreparedAuth,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug)]
pub struct ConnectionEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug)]
pub enum ConnectionTransport {
    DirectSsh,
    SshProxyJump,
    JumpServer,
}

#[derive(Debug)]
pub enum PreparedAuth {
    Password {
        secret_ref: SecretRef,
    },
    PrivateKey {
        secret_ref: SecretRef,
        passphrase_ref: Option<SecretRef>,
    },
    EphemeralToken {
        token: SecretString,
    },
    ProviderSpecific {
        kind: String,
        secret: SecretString,
    },
}
```

约束：

1. `PreparedConnection` 生命周期以单次连接为主；
2. `EphemeralToken.token` 不序列化；
3. Debug 输出必须脱敏；
4. 不允许写入普通日志；
5. `metadata` 只存非敏感字段，例如 asset_id、provider_type、protocol。

---

## 8. JumpServer Provider 配置模型

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpServerConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth: JumpServerAuthConfig,
    pub org_id: Option<String>,
    pub verify_tls: bool,
    pub connect_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JumpServerAuthConfig {
    AccessKey {
        key_id: String,
        secret_ref: SecretRef,
    },

    PrivateToken {
        token_ref: SecretRef,
    },
}
```

即使第一阶段 UI 只开放 Access Key，也建议在模型层保留 `PrivateToken`。

原因：

- JumpServer 版本差异；
- Access Key POST 行为可能存在兼容性差异；
- 企业环境可能有自己的认证策略；
- 后续迁移无需修改 Provider 顶层模型。

---

## 9. SecretRef 与凭据存储

### 9.1 禁止明文持久化

以下位置禁止保存 Access Key Secret：

```text
React localStorage
React persisted state
SQLite 明文字段
hosts.json
settings.json
日志
崩溃报告
Agent Context
Tool Call 参数
```

### 9.2 推荐存储位置

Windows 版本优先使用：

```text
Windows Credential Manager
```

数据库只保存：

```text
provider_id
provider_type
base_url
auth_type
access_key_id
secret_ref
org_id
verify_tls
```

例如：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    pub id: String,
}
```

凭据接口：

```rust
#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn put(
        &self,
        namespace: &str,
        key: &str,
        value: SecretString,
    ) -> Result<SecretRef, CredentialError>;

    async fn get(
        &self,
        secret_ref: &SecretRef,
    ) -> Result<SecretString, CredentialError>;

    async fn delete(
        &self,
        secret_ref: &SecretRef,
    ) -> Result<(), CredentialError>;
}
```

Credential key 推荐：

```text
runory/bastion/{provider_id}/jumpserver/access-key-secret
```

---

## 10. JumpServer Access Key HTTP Signature

JumpServer 官方 Access Key 认证采用 HTTP Header Signature。

当前官方示例的签名 Headers 包括：

```text
(request-target)
accept
date
```

算法：

```text
hmac-sha256
```

示意签名字符串：

```text
(request-target): get /api/v1/xxx/
accept: application/json
date: Thu, 10 Sep 2026 07:30:00 GMT
```

签名过程：

```text
signing_string
     │
     ▼
HMAC-SHA256(secret)
     │
     ▼
Base64
     │
     ▼
Authorization: Signature ...
```

### 10.1 Signer 接口

```rust
pub struct JumpServerSigner {
    key_id: String,
    secret: SecretString,
}

impl JumpServerSigner {
    pub fn sign(
        &self,
        method: &Method,
        path_and_query: &str,
        headers: &mut HeaderMap,
    ) -> Result<(), JumpServerError> {
        // 1. 保证 Accept 存在
        // 2. 生成 GMT Date
        // 3. 构建 signing string
        // 4. HMAC-SHA256
        // 5. Base64
        // 6. 写 Authorization Header
        Ok(())
    }
}
```

### 10.2 设计要求

- 所有 JumpServer API 请求必须走统一 signer；
- 不允许每个 API 方法独立实现签名；
- Path 必须包含 query string；
- HTTP method 参与 `(request-target)`；
- Date 必须使用标准 GMT HTTP 日期；
- POST 时要明确设置 `Content-Type: application/json`；
- Header 名大小写处理要与实际签名实现保持一致；
- 单元测试中加入官方示例或固定向量。

---

## 11. JumpServerApiClient

```rust
pub struct JumpServerApiClient {
    base_url: Url,
    http: reqwest::Client,
    auth: JumpServerAuth,
    org_id: Option<String>,
}
```

内部认证：

```rust
pub enum JumpServerAuth {
    AccessKey(JumpServerSigner),
    PrivateToken(SecretString),
}
```

统一请求入口：

```rust
impl JumpServerApiClient {
    async fn request(
        &self,
        method: Method,
        path: &str,
    ) -> Result<reqwest::RequestBuilder, JumpServerError> {
        // base_url + path
        // Accept: application/json
        // X-JMS-ORG
        // Date
        // Content-Type if required
        // AccessKey signature / PrivateToken
        todo!()
    }
}
```

业务方法：

```rust
impl JumpServerApiClient {
    pub async fn get_current_user(&self) -> Result<JumpServerUser, JumpServerError>;

    pub async fn get_version(&self) -> Result<Option<String>, JumpServerError>;

    pub async fn list_permitted_assets(
        &self,
        query: Option<&str>,
    ) -> Result<Vec<JumpServerAsset>, JumpServerError>;

    pub async fn get_asset(
        &self,
        asset_id: &str,
    ) -> Result<JumpServerAsset, JumpServerError>;

    pub async fn list_permitted_accounts(
        &self,
        asset_id: &str,
    ) -> Result<Vec<JumpServerAccount>, JumpServerError>;

    pub async fn create_connection_token(
        &self,
        request: CreateConnectionTokenRequest,
    ) -> Result<JumpServerConnectionToken, JumpServerError>;

    pub async fn expire_connection_token(
        &self,
        token_id: &str,
    ) -> Result<(), JumpServerError>;
}
```

> 注意：不同 JumpServer 版本中的资产权限、账号权限、Connection Token 请求字段和返回字段可能变化。开发时必须以目标实例 `https://<jumpserver>/api/docs/` 中的 OpenAPI 定义为准，不应仅凭文档示例硬编码长期契约。

---

## 12. 第一阶段 API 范围

第一阶段只做 Runory 必须使用的能力。

| 能力 | 用途 | 优先级 |
|---|---|---:|
| Current User / Profile | 验证 Access Key | P0 |
| Version / Server Info | 兼容性判断 | P1 |
| Permitted Assets | 资产选择 | P0 |
| Permitted Accounts | 账号选择 | P0 |
| Create Connection Token | 建立 SSH | P0 |
| Expire Connection Token | 主动释放 | P1 |

资产查询应优先使用“当前用户有权限访问的资产”语义，而不是管理员全量资产接口。

Connection Token 创建路径在常见 JumpServer v4 中通常位于 authentication API 下，但具体请求 schema 必须通过目标实例 `/api/docs/` 校验。

---

## 13. JumpServer 数据模型

### 13.1 Asset

```rust
#[derive(Debug, Clone)]
pub struct JumpServerAsset {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub platform: Option<String>,
    pub protocols: Vec<JumpServerProtocol>,
    pub comment: Option<String>,
}
```

### 13.2 Protocol

```rust
#[derive(Debug, Clone)]
pub struct JumpServerProtocol {
    pub name: String,
    pub port: Option<u16>,
}
```

### 13.3 Account

```rust
#[derive(Debug, Clone)]
pub struct JumpServerAccount {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub privileged: bool,
}
```

Runory 不保存账号密码。

### 13.4 Connection Token

```rust
pub struct JumpServerConnectionToken {
    pub id: Option<String>,
    pub token: SecretString,
    pub expires_at: Option<DateTime<Utc>>,
    pub endpoint: Option<ConnectionEndpoint>,
    pub username: Option<String>,
    pub raw_metadata: HashMap<String, Value>,
}
```

`raw_metadata` 只允许运行时使用，禁止直接日志输出。

---

## 14. Host 模型改造

如果当前 Host 模型接近：

```rust
Host {
    hostname,
    port,
    username,
    password,
}
```

建议升级为：

```rust
pub struct Host {
    pub id: String,
    pub name: String,
    pub group_id: Option<String>,
    pub connection: HostConnection,
}

pub enum HostConnection {
    DirectSsh {
        hostname: String,
        port: u16,
        username: String,
        auth: SshAuthConfig,
    },

    SshBastion {
        target: SshTarget,
        bastion_provider_id: String,
    },

    BastionAsset {
        provider_id: String,
        asset_id: String,
        account_id: Option<String>,
        protocol: String,
    },
}
```

JumpServer 类型 Host 持久化：

```text
provider_id
asset_id
account_id
protocol
```

不要复制保存：

```text
目标 IP
目标密码
目标私钥
Connection Token
```

每次连接前重新通过 `asset_id` 解析当前权限和连接信息。

---

## 15. JumpServerProvider 实现

```rust
pub struct JumpServerProvider {
    config: JumpServerConfig,
    client: JumpServerApiClient,
}
```

### 15.1 health_check

流程：

```text
读取 Secret
  ↓
构造 API Client
  ↓
调用 current-user/profile 类接口
  ↓
可选读取版本
  ↓
返回 BastionHealth
```

结构：

```rust
pub struct BastionHealth {
    pub reachable: bool,
    pub authenticated: bool,
    pub provider_version: Option<String>,
    pub username: Option<String>,
    pub asset_count: Option<u64>,
    pub warnings: Vec<String>,
}
```

### 15.2 list_assets

职责：

- 只返回当前用户已授权资产；
- 支持服务端查询时优先服务端过滤；
- 大量资产时支持分页；
- 转换为统一 `BastionAsset`；
- 过滤或标记不支持 SSH 的资产。

### 15.3 list_accounts

职责：

- 查询用户对指定资产有权使用的账号；
- 不返回账号密文；
- 区分普通账号、特权账号；
- UI 只展示授权账号。

### 15.4 prepare_connection

核心流程：

```text
PrepareConnectionRequest
          │
          ▼
校验 provider / asset
          │
          ▼
刷新资产权限
          │
          ▼
刷新账号权限
          │
          ▼
创建 Connection Token
          │
          ▼
解析 JumpServer SSH Endpoint
          │
          ▼
PreparedConnection
```

伪代码：

```rust
async fn prepare_connection(
    &self,
    req: PrepareConnectionRequest,
) -> Result<PreparedConnection, BastionError> {
    let asset = self.client.get_asset(&req.asset_id).await?;

    ensure_ssh_supported(&asset)?;

    let accounts = self
        .client
        .list_permitted_accounts(&req.asset_id)
        .await?;

    let account = resolve_account(accounts, req.account_id.as_deref())?;

    let token = self
        .client
        .create_connection_token(CreateConnectionTokenRequest {
            asset_id: req.asset_id.clone(),
            account_id: account.id.clone(),
            protocol: "ssh".into(),
        })
        .await?;

    map_token_to_prepared_connection(token, &self.config)
}
```

---

## 16. SSH Core 接入边界

SSH Core 增加统一入口：

```rust
pub async fn connect_prepared(
    prepared: PreparedConnection,
) -> Result<SshSession, SshError>;
```

调用方：

```text
HostService
    │
    ▼
BastionProvider.prepare_connection()
    │
    ▼
PreparedConnection
    │
    ▼
SshClient.connect_prepared()
```

SSH Core 禁止：

```text
if provider == JumpServer
if provider == Teleport
call_jumpserver_api()
```

厂商判断必须止于 Provider 层。

---

## 17. Connection Token 生命周期

建议状态机：

```text
NotCreated
    │
    ▼
Creating
    │
    ├──── error ────> Failed
    │
    ▼
Ready
    │
    ▼
InUse
    │
    ├──── expires ──> Expired
    │
    └──── close ────> Releasing
                         │
                         ▼
                      Released
```

规则：

1. 尽量按会话创建 Connection Token；
2. 不默认持久化 token；
3. token 只保存在 Rust 内存；
4. 会话关闭时执行 best-effort expire；
5. 应用异常退出时依赖 JumpServer 自身 TTL 兜底；
6. token 过期时不要无限自动重试；
7. 自动重新创建 token 前必须重新确认资产/账号授权仍有效。

---

## 18. UI 改造

### 18.1 Settings > Bastion Providers

新增：

```text
Settings
└── Bastion Providers
      ├── Add Provider
      ├── Edit Provider
      ├── Test Connection
      └── Delete Provider
```

JumpServer 表单：

```text
┌──────────────────────────────────────────────┐
│ JumpServer                                   │
│                                              │
│ Name                                         │
│ [公司生产堡垒机                         ]   │
│                                              │
│ Base URL                                     │
│ [https://jump.example.com               ]   │
│                                              │
│ Authentication                              │
│ [Access Key ▼]                              │
│                                              │
│ AccessKey ID                                 │
│ [AKXXXXXXXXXXXXXXXX                     ]   │
│                                              │
│ AccessKey Secret                             │
│ [••••••••••••••••••••••                ]   │
│                                              │
│ Organization ID                              │
│ [optional                               ]   │
│                                              │
│ [x] Verify TLS                               │
│                                              │
│ [Test Connection]              [Save]        │
└──────────────────────────────────────────────┘
```

测试结果示例：

```text
Connected
JumpServer: 4.x
Authentication: OK
User: runory-service
Accessible assets: 37
```

### 18.2 新建 Host

Connection Type：

```text
Direct SSH
SSH Bastion
JumpServer
```

JumpServer 模式：

```text
Provider
[公司生产堡垒机 ▼]

Asset
[Search assets...]

Account
[deploy ▼]

Protocol
SSH
```

选中 Asset 后自动展示只读信息：

```text
Asset name
Address
Platform
SSH protocol/port
Provider
```

Runory 不要求用户输入该资产密码。

---

## 19. Tauri Command 设计

前端不要直接访问 JumpServer。

新增命令：

```rust
#[tauri::command]
async fn bastion_save_provider(...)

#[tauri::command]
async fn bastion_test_provider(...)

#[tauri::command]
async fn bastion_list_assets(...)

#[tauri::command]
async fn bastion_list_accounts(...)

#[tauri::command]
async fn bastion_delete_provider(...)
```

建立连接继续走统一 Session 命令：

```rust
#[tauri::command]
async fn ssh_connect(host_id: String, ...)
```

而不是新增：

```text
jumpserver_ssh_connect
```

`ssh_connect()` 内部通过 HostConnection 自动选择 Provider。

---

## 20. 错误模型

```rust
pub enum BastionError {
    Network(String),
    Tls(String),
    AuthenticationFailed,
    PermissionDenied,
    AssetNotFound,
    AccountNotAvailable,
    ProtocolNotSupported,
    ConnectionTokenFailed(String),
    ConnectionTokenExpired,
    ProviderVersionUnsupported(String),
    ApiChanged(String),
    CredentialUnavailable,
    InvalidConfiguration(String),
    RateLimited,
    Internal(String),
}
```

JumpServer HTTP 错误映射示例：

| HTTP | Runory Error | UI |
|---:|---|---|
| 400 | InvalidConfiguration / TokenFailed | 请求参数或版本不兼容 |
| 401 | AuthenticationFailed | Access Key 无效或签名失败 |
| 403 | PermissionDenied | 当前用户无权限 |
| 404 | AssetNotFound / ApiChanged | 资产不存在或 API 路径变化 |
| 429 | RateLimited | 请求过于频繁 |
| 5xx | Internal / ApiChanged | JumpServer 服务异常 |

UI 错误信息禁止直接展示包含 secret/token 的响应体。

---

## 21. 日志与脱敏

允许记录：

```text
provider_id
provider_type
base_url host
HTTP method
API path
status code
asset_id
account_id
session_id
latency
error category
```

禁止记录：

```text
AccessKey Secret
Authorization Header
完整 HTTP Signature
Private Token
Connection Token
目标服务器密码
目标服务器私钥
Cookie
```

建议实现：

```rust
pub struct Redacted<T>(pub T);
```

所有 Secret 类型禁止派生默认 `Debug`，或者自定义 Debug：

```text
SecretString(**REDACTED**)
```

---

## 22. SFTP 兼容

SFTP 不应单独重新做 JumpServer 登录。

推荐：

```text
Host
  ↓
Provider.prepare_connection()
  ↓
SSH Transport
  ├── Terminal channel
  └── SFTP subsystem/channel
```

如果 Runory 当前 Terminal 和 SFTP 分别建立独立 SSH 连接，需要评估：

1. 能否共享同一个 SSH Session；
2. 若不能共享，Connection Token 是否可复用；
3. 若 Connection Token 为一次性，应为 SFTP 独立创建一个新的 token；
4. 不要假定 token 一定可多次使用。

---

## 23. Agentic Runtime 兼容

Agentic Runtime 的正确边界：

```text
AgentRun
   ↓
Tool Registry
   ↓
Policy
   ↓
ChangeSet
   ↓
SSH Session
   ↓
Bastion / Target
```

禁止 Agent 直接获得：

```text
AccessKey ID
AccessKey Secret
Connection Token
JumpServer Private Token
```

Agent Tool 只能使用已经建立的 Session，例如：

```text
ssh.exec
ssh.read
sftp.read
sftp.write
```

这样 JumpServer 接入不会扩大 Agent 的凭据权限边界。

---

## 24. 兼容性策略

JumpServer API 可能随版本变化。

建议增加：

```rust
pub struct JumpServerCapabilities {
    pub version: Option<String>,
    pub access_key_auth: bool,
    pub list_assets: bool,
    pub list_accounts: bool,
    pub connection_token: bool,
    pub expire_connection_token: bool,
}
```

连接测试过程中完成 Capability Probe：

```text
Base URL
   ↓
认证测试
   ↓
版本探测
   ↓
关键 API 探测
   ↓
Capabilities
```

不要只写：

```rust
if version >= "4.10" { ... }
```

更推荐能力探测，因为私有部署、LTS 分支和 API 回补可能导致版本号不足以准确反映能力。

---

## 25. API 版本变化处理

建议将 API path 收口：

```rust
pub struct JumpServerApiRoutes {
    pub current_user: &'static str,
    pub permitted_assets: &'static str,
    pub permitted_accounts: &'static str,
    pub connection_token: &'static str,
}
```

Provider 不应散落字符串：

```rust
client.get("/api/v1/...")
```

便于后续处理：

```text
JumpServer v3
JumpServer v4
JumpServer future
```

开发/测试时必须读取实际部署实例的：

```text
https://<jumpserver>/api/docs/
```

确认请求字段、分页格式、账号字段和 Connection Token 返回结构。

---

## 26. 安全策略

### 26.1 最小权限

推荐在 JumpServer 中创建专门给 Runory 使用的用户，例如：

```text
runory-service
```

只授权必要资产、必要账号和必要操作。

不要使用超级管理员 Access Key 作为日常 Runory 凭据。

### 26.2 Secret Rotation

Runory 应支持：

```text
Edit Provider
   ↓
Replace AccessKey Secret
   ↓
保存新 Credential
   ↓
Test
   ↓
成功后删除旧 Credential
```

避免先删除旧 Secret 导致配置完全不可用。

### 26.3 TLS

生产环境默认：

```text
verify_tls = true
```

允许关闭 TLS 校验仅用于内部测试，并在 UI 显示明显警告。

### 26.4 URL 限制

对 `base_url` 做：

- URL parse；
- 默认要求 HTTPS；
- 禁止嵌入 userinfo/password；
- 标准化 trailing slash；
- API 路径使用 URL join，避免字符串拼接漏洞。

---

## 27. 缓存策略

可以缓存：

```text
资产列表
资产非敏感 metadata
平台信息
protocol 信息
Provider version/capabilities
```

建议 TTL：

```text
30 ~ 120 秒
```

不要缓存：

```text
Connection Token
AccessKey Secret
账号 Secret
Authorization Header
```

每次正式连接前必须重新确认关键授权信息，不能因为 UI 缓存中仍存在资产就认为当前仍有权限。

---

## 28. 并发与会话管理

同一个 JumpServerProvider 可同时为多个 Host 创建连接。

建议：

```text
JumpServerProvider
       │
       ├── shared reqwest::Client
       ├── shared Credential reference
       └── per-session ConnectionToken
```

禁止：

```text
一个全局 Connection Token
供所有 SSH Session 共享
```

Session metadata：

```rust
pub struct BastionSessionMetadata {
    pub provider_id: String,
    pub asset_id: String,
    pub account_id: Option<String>,
    pub token_id: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
}
```

其中 `token_id` 可以用于 release，但 token value 不进入 metadata persistence。

---

## 29. 请求超时与重试

建议：

```text
connect timeout: 5~10s
request timeout: 15~30s
```

自动重试仅针对：

```text
DNS 临时失败
TCP reset
502
503
504
```

不自动重试：

```text
401
403
400
Connection Token 创建后的未知结果
```

特别是 POST Connection Token，如果服务端可能已经创建成功而客户端没有收到响应，不能简单无限重放。

---

## 30. 前端状态设计

```ts
type BastionProviderSummary = {
  id: string;
  name: string;
  type: 'jumpserver' | 'ssh-bastion';
  baseUrl?: string;
  authType?: 'access-key' | 'private-token';
  maskedKeyId?: string;
  health?: ProviderHealth;
};
```

Secret 只在用户编辑输入时短暂存在：

```ts
type JumpServerSecretInput = {
  accessKeySecret?: string;
  replaceExistingSecret?: boolean;
};
```

保存成功后立即：

```ts
setAccessKeySecret('');
```

不要把 Secret 放入 Zustand/Redux 持久化 store。

---

## 31. 数据库迁移

建议表：

```sql
CREATE TABLE bastion_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    config_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

`config_json` 可包含：

```json
{
  "base_url": "https://jump.example.com",
  "auth": {
    "type": "access_key",
    "key_id": "AKXXXXXXXX",
    "secret_ref": "credential://..."
  },
  "org_id": null,
  "verify_tls": true
}
```

禁止 `config_json` 中出现：

```json
{
  "access_key_secret": "..."
}
```

Host 表增加：

```text
connection_type
provider_id
remote_asset_id
remote_account_id
remote_protocol
```

或者统一收口到 versioned `connection_json`。

---

## 32. 配置版本化

建议所有 Provider 配置带 schema version：

```json
{
  "schema_version": 1,
  "provider_type": "jumpserver",
  "base_url": "..."
}
```

这样后续扩展：

```text
v1 Access Key
v2 Private Token
v3 OIDC / Device Auth
```

不会造成配置迁移失控。

---

## 33. 单元测试

必须覆盖：

### Signer

- GET request-target；
- POST request-target；
- query string；
- Date header；
- Accept header；
- HMAC-SHA256；
- Base64；
- Secret 不出现在 Debug。

### ApiClient

- 200 JSON；
- 401；
- 403；
- 404；
- 429；
- 500；
- malformed JSON；
- pagination；
- timeout。

### Provider

- health check；
- asset mapping；
- account mapping；
- SSH protocol check；
- connection token mapping；
- release token；
- 权限被撤销。

---

## 34. 集成测试

推荐准备独立 JumpServer 测试环境：

```text
Runory Test
    │
    ▼
JumpServer
    │
    ├── asset-linux-01
    ├── asset-linux-02
    └── restricted-linux
```

账号：

```text
runory-test
```

授权：

```text
asset-linux-01 -> deploy
asset-linux-02 -> ops
restricted-linux -> no access
```

至少验证：

1. API Key 成功认证；
2. 看不到未授权资产；
3. 能正确获取允许账号；
4. 可生成 Connection Token；
5. Terminal 能正常连接；
6. Terminal resize 正常；
7. PTY UTF-8 正常；
8. SFTP 可用；
9. Agentic read tool 可用；
10. ChangeSet/approval 流程不受影响；
11. 连接关闭后正确清理；
12. 撤销 JumpServer 授权后 Runory 新连接失败。

---

## 35. 安全测试

必须专门测试：

```text
日志泄漏
错误弹窗泄漏
React DevTools 泄漏
SQLite 泄漏
Crash dump 泄漏
Debug 格式泄漏
HTTP tracing 泄漏
Agent Context 泄漏
```

搜索测试构建产物：

```text
AccessKeySecret 的测试值
ConnectionToken 的测试值
Authorization: Signature
```

确认不应出现在日志和持久化文件中。

---

## 36. 开发阶段划分

### Phase 1：Provider 基础架构

完成：

```text
BastionProvider
BastionProviderRegistry
PreparedConnection
HostConnection
统一错误模型
```

验收：Direct SSH 行为不能回归。

### Phase 2：Credential Store

完成：

```text
SecretRef
Windows Credential Manager backend
Secret CRUD
Debug redaction
```

### Phase 3：JumpServer API 基础

完成：

```text
JumpServerConfig
AccessKey auth
HTTP signer
JumpServerApiClient
health_check
```

### Phase 4：资产浏览

完成：

```text
list_assets
asset search
pagination
list_accounts
UI AssetPicker
UI AccountPicker
```

### Phase 5：SSH 连接

完成：

```text
create Connection Token
PreparedConnection mapping
SSH Core connect_prepared
Terminal end-to-end
```

### Phase 6：SFTP 与生命周期

完成：

```text
SFTP
Connection Token release
expiry handling
reconnect strategy
```

### Phase 7：稳定性与安全

完成：

```text
compatibility probe
rate limiting
retry policy
logging redaction
security tests
integration tests
```

---

## 37. 推荐实施顺序

```text
1. BastionProvider Trait
        ↓
2. PreparedConnection
        ↓
3. HostConnection 重构
        ↓
4. CredentialStore
        ↓
5. JumpServerSigner
        ↓
6. JumpServerApiClient
        ↓
7. health_check
        ↓
8. list_assets
        ↓
9. list_accounts
        ↓
10. create_connection_token
        ↓
11. SSH connect_prepared
        ↓
12. JumpServer UI
        ↓
13. SFTP
        ↓
14. Error/Retry/Compatibility
        ↓
15. Integration + Security Test
```

不要先从 UI 开始，也不要先在现有 SSH Connect 函数里堆 JumpServer 条件分支。

---

## 38. 验收标准

### P0 必须通过

- [ ] 可新增 JumpServer Provider；
- [ ] Access Key Secret 安全保存；
- [ ] 可 Test Connection；
- [ ] 能识别认证失败；
- [ ] 能列出当前用户授权资产；
- [ ] 不能显示未授权资产；
- [ ] 能列出授权账号；
- [ ] 能创建 Connection Token；
- [ ] 能通过 JumpServer 打开目标 SSH Terminal；
- [ ] API Key / Token 不出现在日志；
- [ ] Direct SSH 无功能回归；
- [ ] Agentic Runtime 无需厂商特殊分支。

### P1 建议通过

- [ ] SFTP；
- [ ] Connection Token 主动释放；
- [ ] Provider version/capability probe；
- [ ] 分页；
- [ ] Asset 搜索；
- [ ] Credential rotation；
- [ ] Private Token 认证接口预留；
- [ ] TLS warning；
- [ ] 兼容性错误提示。

---

## 39. 明确禁止的实现方式

### 禁止 1：SSH 配置直接增加 API Key

错误：

```rust
SshConfig {
    hostname,
    username,
    password,
    jumpserver_api_key,
}
```

原因：混淆 API 控制面与 SSH 数据面。

### 禁止 2：React 直接请求 JumpServer

错误：

```text
React -> fetch(JumpServer API)
```

原因：Secret 会进入 WebView/JS 环境，安全边界扩大。

正确：

```text
React -> Tauri Command -> Rust -> JumpServer
```

### 禁止 3：保存目标主机密码

Runory 通过 JumpServer 时不主动提取目标主机真实 Secret。

### 禁止 4：SSH Core 判断 JumpServer

错误：

```rust
if host.provider_type == JumpServer {
    // ...
}
```

正确：

```text
Provider -> PreparedConnection -> SSH Core
```

### 禁止 5：长期保存 Connection Token

Connection Token 是会话凭据，不作为 Host 静态认证信息。

---

## 40. 最终目标架构

```text
                         Runory
                           │
                           ▼
                    HostConnection
                           │
                           ▼
                 BastionProviderRegistry
                           │
             ┌─────────────┼──────────────┐
             │             │              │
             ▼             ▼              ▼
          Direct       SSH Bastion    JumpServer
                                         │
                                         ▼
                                 Access Key API Auth
                                         │
                                         ▼
                                  Asset / Account
                                         │
                                         ▼
                                Connection Token
             │             │              │
             └─────────────┼──────────────┘
                           ▼
                  PreparedConnection
                           │
                           ▼
                       SSH Core
                           │
                ┌──────────┼──────────┐
                ▼          ▼          ▼
             Terminal     SFTP      Agentic
```

未来新增：

```text
TeleportProvider
CyberArkProvider
BoundaryProvider
CustomProvider
```

只需要实现：

```text
BastionProvider
      ↓
PreparedConnection
```

Runory 上层业务无需随厂商扩展反复重构。

---

## 41. Codex 实施约束

将本设计交给 Codex 实现时，应明确要求：

1. 先检查现有 Runory SSH、Host、SFTP、Agentic 结构，不允许盲目重写；
2. 最大化复用当前稳定 SSH Session；
3. 先建立 Provider 边界，再实现 JumpServer；
4. 不允许把 JumpServer 特殊逻辑散落进 SSH Core；
5. 不允许改变现有 Agentic 安全模型；
6. 所有 Secret 必须脱敏；
7. 每个 Phase 完成后运行现有测试；
8. 对数据库变更提供 migration；
9. 对已有 Host 配置保持向后兼容；
10. 所有 JumpServer API schema 以测试实例 `/api/docs/` 为准；
11. 对不确定 API 行为建立 adapter/capability probe，不进行静默猜测；
12. 不得为了 JumpServer 支持绕过 Tool Registry、Policy 或 ChangeSet。

---

## 42. Definition of Done

本次 JumpServer API Key 改造完成的定义不是“API 能调通”，而是：

```text
用户在 Runory 中配置一个 JumpServer Provider
        ↓
安全保存 Access Key
        ↓
Runory 获取用户授权资产
        ↓
用户选择资产和授权账号
        ↓
Runory 动态申请 Connection Token
        ↓
建立 JumpServer 审计链路内的 SSH 会话
        ↓
Terminal / SFTP / Agentic 正常工作
        ↓
Session 关闭并清理短期凭据
```

同时满足：

```text
SSH Core 不知道 JumpServer
Agentic Runtime 不知道 API Key
React 不持有长期 Secret
数据库不保存明文 Secret
目标服务器真实密码不进入 Runory
```

达到以上条件后，才视为本次架构改造完成。

---

## 43. 参考资料

开发时优先参考目标 JumpServer 实例自身的 OpenAPI：

```text
https://<jumpserver-host>/api/docs/
```

JumpServer 官方文档：

- REST API / Authentication：`https://docs.jumpserver.org/zh/v4/dev/rest_api/`
- User Profile / Access Keys / Connection Token：`https://docs.jumpserver.org/zh/v4/manual/user/profile/`
- Admin Profile / Access Keys / Connection Token：`https://docs.jumpserver.org/zh/v4/manual/admin/profile/`
- Environment parameters / Connection Token TTL：`https://docs.jumpserver.org/zh/v4/manual/env/`

> API endpoint、请求字段、分页格式和 Connection Token schema 均应以目标实例的 `/api/docs/` 为最终依据。
