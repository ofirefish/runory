# Runory Server 信息云端同步与账号设计

> Status: Product and architecture design. 本文定义下一阶段实现边界，但不代表已部署到 Production。
>
> Baseline: 复用 Phase 9 已有 Supabase Auth、Organization/RLS、Rust AES-256-GCM 清单加密、Revision、Tombstone、冲突预览和部署门禁。凭据同步仍受 `CLOUD_SYNC_SECURITY_ARCHITECTURE.md` 的 M0–M5 门禁约束。

## 1. 决策摘要

本阶段把当前偏工程化的「可选云服务」整理为面向个人用户的账号与 Server 信息同步功能：

- `runory.app` 承载官网、账号确认、密码重置和隐私/服务条款页面。
- Supabase 采用 Free Plan，Production/Staging API 使用各自的 `https://<project-ref>.supabase.co`；`runory.app` 不代理 Supabase API。
- Supabase Auth 首期支持邮箱 + 密码、邮箱确认、忘记密码和退出登录；不新增自建密码系统。
- Supabase Postgres 保存账号公开资料、Workspace/RLS 元数据和加密同步对象。
- Supabase Storage 使用私有 `avatars` Bucket 保存头像；头像与 Server 加密清单完全分离。
- Server Profile、Group 和 Tombstone 在 Rust 中加密后才可上传。云端不得获得 Server 名称、地址、用户名或拓扑明文。
- 密码、Passphrase、私钥内容、私钥本机路径、Credential Vault Secret、Known-host 信任、Terminal/SFTP 内容永不进入本阶段同步。
- Local-first 不变：不注册、不登录、云端离线、退出登录或关闭同步时，SSH/SFTP 和本地 Repository 继续完整工作。
- 当前快照同步作为兼容基线；自动逐对象同步必须复用既有 E2EE Device/Vault 设计的 Gate M2，不另造一套弱化协议。

## 2. 产品范围

### 2.1 本阶段交付

1. 账号注册、邮箱确认、登录、忘记密码、退出登录。
2. 账号资料：展示名、头像、只读邮箱。
3. 每个账号自动拥有一个 Personal Workspace；现有 Team Organization 保持兼容。
4. 显式开启的 Server 信息云端同步。
5. 首次同步预览、冲突处理、删除确认、同步状态和错误恢复。
6. Windows/macOS/Linux 可用，并保持 iOS/Android 的 Deep Link、Secure Storage 和触控 UI 可行。

### 2.2 不在本阶段

- SSH 密码、私钥 Passphrase、私钥内容或任意 Credential 同步。
- 把账号密码当作同步加密密钥。
- 服务端明文搜索 Server、远程连接测试或云端 SSH。
- 自动同步 Known-host 后直接信任远端主机。
- 社交登录、企业 SSO、Billing、Marketplace。
- Server 数据 Web 管理后台或浏览器端解密。
- 用 Realtime、Edge Function 或 Service Role 绕过 RLS/Revision/Device Trust。

## 3. 数据分类

| 数据 | 云端形态 | 默认行为 |
|---|---|---|
| Auth 邮箱、密码哈希、会话 | Supabase Auth 管理 | 注册/登录需要 |
| 展示名、头像路径 | Postgres 明文元数据 | 用户可编辑 |
| 头像文件 | Supabase Storage 私有对象 | 用户显式上传 |
| Profile/Group | Rust E2EE 密文 | 用户显式开启同步 |
| Tombstone/Revision/对象大小/时间 | 无内容同步元数据 | 同步协议需要 |
| SSH 密码/密钥口令 | 不上传 | 禁止 |
| 私钥内容与 `KeySource::File.path` | 不上传 | 禁止 |
| `lastConnectedAt`、终端输出、SFTP 内容 | 不上传 | 设备本地 |
| Known-host 信任 | 不上传 | 设备本地；Changed 必须 Block |

头像和展示名不是加密清单的一部分。它们用于账号和团队成员 UI，因此服务端可见；UI 必须在上传前说明这一点。

## 4. Server 字段同步契约

同步对象继续使用本地 UUID 作为逻辑身份，但 UUID、对象类型和内容都应放在 E2EE Payload 内；进入未来 Gate M2 后，云端索引只使用由 Vault Index Key 派生的 opaque lookup ID。

| `ServerProfile` 字段 | 同步 | 说明 |
|---|---:|---|
| `id` | 是 | 加密 Payload 内逻辑 ID |
| `name` | 是 | 敏感基础设施元数据，必须加密 |
| `host` | 是 | IP/域名必须加密 |
| `port` | 是 | 与 Host 一起加密 |
| `username` | 是 | 必须加密 |
| `groupId` | 是 | 只引用同 Workspace 的 Group |
| `authMethod` | 是 | 仅同步方法枚举，不同步凭据 |
| `connectionRoute` | 是 | Jump Host 仅引用同步集合内 Profile ID |
| `sortOrder` | 是 | 维持用户排列 |
| `createdAt`、`updatedAt` | 是 | 展示/迁移用途，不作为唯一并发权威 |
| `keySource` | 否 | 不同步 Vault ID 或本机路径；新设备显示“需要配置密钥” |
| `lastConnectedAt` | 否 | 设备行为数据 |
| `osDistribution` | 否 | 当前为设备探测结果，避免把过期探测值当配置 |

`HostGroup` 同步 `id/name/sortOrder/collapsed/createdAt/updatedAt`。删除使用 Tombstone，不能用「拉取结果中缺少对象」推断删除。

跨设备恢复 Private-key Profile 时：

- 保留 `authMethod=privateKey`；
- 不创建伪造或空的 `keySource`；
- Profile 标记为 `credential-setup-required`，用户在本机重新选择 File 或导入 Vault；
- 未完成本机凭据绑定前允许编辑，禁止发起 SSH 连接；
- Jump Host 引用缺失时标记 `route-setup-required`，不得悄悄改成 Direct。

## 5. 账号与会话

### 5.1 注册与登录

首期 Auth Flow：

```text
邮箱 + 密码注册
  → Supabase 发送确认邮件
  → https://runory.app/auth/confirm
  → 确认成功，返回 Runory 登录
  → 登录后获取短期 Access Token
```

- 必须启用邮箱确认、Custom SMTP、密码强度、限流和生产 CAPTCHA。
- `raw_user_meta_data` 只可用于展示名初始化提示，绝不能用于 RLS 或角色判断。
- 角色只来自 `organization_members` 和受控 `app_metadata`；JWT Claim 过期时按服务端/RLS 当前状态 fail-closed。
- 账号密码与同步/Vault 密钥完全独立。修改账号密码不重新加密 Server 数据。
- `persistSession=false` 保持不变，Access/Refresh Token 不得写入 WebView `localStorage`。
- Desktop 的“保持登录”由业务专用 Rust Command 与 Platform Keychain/Keystore 实现；启动恢复后 Session 只存在于 Supabase 内存客户端，退出登录清除系统记录。移动端在 Platform Keystore 门禁完成前保持进程内会话。

### 5.2 邮件确认与密码重置

- Auth Site URL: `https://runory.app`。
- 精确允许 `https://runory.app/auth/confirm` 与 `https://runory.app/auth/reset`，生产环境不使用宽泛通配符。
- Desktop/Mobile 的 `runory://auth/callback` Deep Link 作为后续 OAuth/一键返回 App 的附加入口，不替代 HTTPS 页面。
- 重置页面完成 `verifyOtp/exchangeCodeForSession` 后只允许更新当前 Auth 用户密码，不接触同步密钥。
- 邮件发件人建议 `Runory <no-reply@runory.app>`，配置 SPF、DKIM、DMARC 并关闭邮件服务商的链接跟踪。

## 6. 账号资料数据库

不直接从 Data API 暴露 `auth.users`。新增最小 `public.user_profiles`：

```sql
create table public.user_profiles (
  id uuid primary key references auth.users(id) on delete cascade,
  display_name text not null check (char_length(display_name) between 1 and 64),
  avatar_path text,
  avatar_version bigint not null default 0 check (avatar_version >= 0),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);
```

约束：

- 不复制邮箱到 `user_profiles`，避免双写和过期信息。
- `avatar_path` 只保存对象路径，不保存短期 Signed URL。
- 不用可能阻塞注册的复杂 Auth Trigger。邮箱确认后第一次登录，通过固定 `ensure_my_profile` RPC 或受 RLS 保护的 INSERT 创建资料行。
- `display_name` 是展示信息，不参与授权。
- 新表在 Data API 未自动暴露时显式 `GRANT`；无论是否暴露都必须启用 RLS。

RLS：

- 用户可 SELECT 自己及共享组织成员的资料；INSERT/UPDATE 只通过固定 RPC，客户端没有直接 DML Grant。表仍保留 ownership RLS 作为纵深防御，UPDATE Policy 同时使用 `USING` 与 `WITH CHECK`。
- 组织成员只可 SELECT 与自己至少共享一个 Organization 的成员资料。
- `anon` 无任何 `user_profiles` 权限。
- 不允许客户端 DELETE 资料行；账号删除由受控账号生命周期处理。

如共享组织判断需要 `SECURITY DEFINER` Helper，它必须位于 `private` Schema、固定空 `search_path`、先检查 `auth.uid()`、只返回 Boolean，并撤销 `PUBLIC/anon` 执行权限。不得用 `SECURITY DEFINER` 修补普通 RLS 错误。

## 7. Personal Workspace

复用 `organizations`，增加兼容字段：

```text
kind = personal | team
```

- 既有行 Migration 默认 `team`，不改变当前组织和成员权限。
- 每个 Owner 最多一个 `personal` Workspace，使用 `(owner_id) WHERE kind='personal'` 唯一部分索引。
- `ensure_personal_workspace()` 在已验证用户首次登录后幂等创建 Personal Workspace 和 Owner Membership。
- UI 对 Personal Workspace 使用 i18n 系统名称“我的云端服务器 / My cloud servers”，不依赖数据库内固定中文或英文名称。
- Personal Workspace 不开放邀请；团队分享仍走 `team` Organization。
- 所有同步读写继续绑定 exact Workspace ID，绝不从当前 UI 选择猜测目标。

## 8. 头像 Storage

Bucket 设计：

```text
bucket: avatars
public: false
max file size: 2 MiB
allowed MIME: image/jpeg, image/png, image/webp
object path: <auth.uid()>/<random-object-id>.webp
```

- 禁止 SVG、HTML 和任意可执行/活动内容。
- App 端解码后裁剪为正方形，最大 512×512，再编码为 WebP；Bucket MIME/大小限制仍作为服务端第二道门禁。
- 每次上传使用新随机对象名，不使用 `upsert`，避免 CDN 缓存陈旧和 Storage UPDATE 权限扩大。
- 上传新对象成功后再 CAS 更新 `user_profiles.avatar_path/avatar_version`；更新失败则尽力删除新对象。
- 资料更新成功后删除旧对象；删除失败只产生无内容清理任务，不回滚已成功头像。
- 下载使用短期 Signed URL（建议 1 小时），只在内存缓存；数据库不保存 Signed URL。

Storage RLS：

- INSERT：Bucket 为 `avatars` 且第一段目录等于当前 `auth.uid()`。
- SELECT：对象属于自己，或对象 Owner 与当前用户共享 Organization。
- DELETE：仅对象 Owner。
- UPDATE：不授予，因为流程不覆盖既有对象。
- `anon` 无 Bucket 访问，客户端永远不持有 Service Role Key。

账号删除前必须先删除该用户拥有的 Storage Objects；否则 Supabase 会拒绝删除仍拥有对象的 Auth User。删除账号也不能立即撤销已经签发的 JWT，因此 Access Token 保持短寿命，敏感 RPC 校验有效 Session。

## 9. 同步架构

```text
React UI
  ├─ Supabase Auth（publishable key）
  ├─ Account Profile / Avatar（严格 RLS）
  └─ cloud_sync_* typed command
                     ↓
Rust CloudSyncService
  ├─ ProfileRepository / GroupRepository
  ├─ CloudCryptoService
  ├─ Outbox / Cursor / Conflict Repository
  └─ Fixed Supabase Transport
                     ↓
<project-ref>.supabase.co
  ├─ Auth
  ├─ RLS-constrained RPC/Data API
  └─ private Storage bucket
```

边界：

- React 可以管理 Auth 会话、展示账号资料、上传用户主动选择的头像和渲染同步状态。
- Server 清单采集、加解密、Schema 验证、Merge、Repository Apply 和 Cursor Commit 全部属于 Rust。
- React 不读取本地 Profile JSON 组织同步 Payload，不拥有同步 Passphrase/Vault Key，不实现 Conflict Merge。
- Rust Transport 只接受编译/配置白名单中的 Supabase Origin 和固定端点，不提供通用 HTTP/SQL IPC。
- Access Token 通过窄 `cloud_session_update` 进入 Rust 的 Zeroizing 内存，退出登录、锁定和过期时立即清除。
- 所有 Cloud Error 跨 IPC 返回 Stable Error Code；错误和日志不包含 Token、Server 字段、Ciphertext 或 Passphrase。

## 10. 同步状态与流程

正式状态：

```text
disabled
signed-out
locked
idle
syncing
review-required
offline
error
```

首次开启：

1. 登录并确保 Personal Workspace 存在。
2. 用户显式打开“同步 Server 信息”。
3. 明确展示同步/不同步字段，不默认勾选 Credential。
4. Cloud 为空、本地有数据：显示上传对象计数后确认。
5. Cloud 有数据、本地为空：先生成导入预览。
6. 两边都有数据：执行三方比较；删除和同字段冲突必须逐项确认。
7. Durable Apply 成功后才推进 Cursor/Revision。

日常行为：

- 当前 Phase 9 Snapshot Flow 继续作为兼容和回滚路径，并在 UI 中准确称为“加密备份/手动同步”，不声称实时同步。
- 自动逐对象同步在 `CLOUD_SYNC_SECURITY_ARCHITECTURE.md` Gate M2 完成后启用：本地 Repository 变更与 Outbox Intent 在同一写锁下原子提交；Pull 在本地持久化成功后才 ACK。
- Desktop 在启动/解锁、网络恢复、用户点击“立即同步”时触发；前台低频轮询作为兜底，不强依赖 Realtime。
- Mobile 在前台/恢复时触发，不假设后台常驻。
- 云端不可用时 Outbox 保留，本地操作不失败；恢复网络后有界重试并指数退避。
- 写入 Server 信息后不得把任何远端执行/SSH 会话自动迁移到另一设备。

并发权威是服务端 Revision/CAS、对象 Parent Digest 和本地 Base，不是设备墙钟 `updatedAt`。时间戳只用于展示和旧 Snapshot 兼容。

## 11. 冲突规则

- 不同对象：独立应用。
- 同对象不同字段：Gate M2 可由 Rust 三方合并。
- 同字段双方修改：用户选择本地或云端。
- Secret/KeySource：本阶段不存在云端值，不参与合并。
- Delete vs Update：默认保留本地并要求确认。
- 删除 Group：不得连带静默删除 Profile；保留 Profile 并移到 Ungrouped，或由用户明确处理。
- Jump Host 引用：Target 必须在同一应用批次存在；缺失时阻止连接并进入修复状态。
- 冲突决策绑定 Workspace、Object、Local Digest、Remote Digest 和 Base Revision；Apply 时在 Rust Repository 写锁内重新校验，防止 TOCTOU。

## 12. UI 信息架构

Settings 增加两个相邻但独立的 Section：

### Account

- 未登录：邮箱、密码、登录、创建账号、忘记密码。
- 已登录：头像、展示名、只读邮箱、邮箱验证状态、退出登录。
- 上传头像前显示大小/格式错误；不显示 Supabase 原始错误正文。

### Cloud Sync

- Local-only 状态始终可见。
- Workspace Selector：默认 Personal，团队 Workspace 次级展示。
- 主开关：“同步 Server 信息”。
- 状态行：最后成功时间、当前状态、本地待上传数量、需处理冲突数量。
- 操作：立即同步、查看冲突、关闭同步。
- 说明：同步 Server 名称/地址/用户/分组；不含密码、私钥和终端数据。
- 关闭同步只停止上传/拉取并清除内存密钥，不删除本地数据。删除云端副本是独立的破坏性操作，需要二次确认和最近登录。

窄屏使用单列布局；所有文案进入 `zh-CN/en-US` i18n。Avatar、按钮和状态不能只用颜色表达，必须有文本/ARIA Label。

## 13. 域名拓扑

| 域名 | 用途 |
|---|---|
| `runory.app` | 官网、下载、Auth 确认/重置、法律页面 |
| `<production-project-ref>.supabase.co` | Production Supabase Auth/Data/Storage/Functions |
| `staging.runory.app` | Staging Web Auth 页面 |
| `<staging-project-ref>.supabase.co` | 独立 Staging Supabase Project |

Free Plan 不包含 Supabase Custom Domain Add-on。`runory.app` 只承载前端页面，所有 Supabase 产品统一走 Project Domain 的 `{auth,rest,storage,functions}/v1/...`。客户端仍只接受构建时配置的单一 Project Origin，不提供运行时任意 Origin 切换。

上线步骤：

1. Production/Staging 使用独立 Supabase Project、密钥、SMTP 和 DNS。
2. 把 `https://runory.app/auth/confirm` 与 `https://runory.app/auth/reset` 加入 Auth Redirect URL Allow List。
3. 客户端 `VITE_SUPABASE_URL` 使用 Production Project Domain；Staging Build 使用独立 Staging Project Domain。
4. 验证 Auth 邮件、Data API、Storage Signed URL、Edge Function、CORS、证书和移动 Deep Link。

新建 Free Plan Project 从 2026-06-03 起不能使用 Supabase 默认 SMTP 自定义 Auth 邮件模板。Runory 若保留双语模板，必须配置外部 Custom SMTP；否则使用 Supabase 默认模板，不能把本地模板测试结果当作远端已生效。

## 14. Supabase Schema/RLS 变更计划

按现有 Migration 纪律执行，不在 Dashboard 手改 Schema：

1. 新增 `organizations.kind` 和 Personal Workspace 唯一部分索引。
2. 新增 `public.user_profiles`、约束、索引和 RLS。
3. 新增最小 `ensure_personal_workspace` / `ensure_my_profile` RPC；固定空 `search_path`，撤销 PUBLIC/anon EXECUTE。
4. 创建私有 `avatars` Bucket 与 Storage RLS。
5. 增加同组织 Avatar/Profile 可读 Helper；用 pgTAP 覆盖跨组织隔离。
6. 显式检查 Data API Exposed Tables 和 `authenticated` GRANT；RLS 与 GRANT 两层都满足才可访问。
7. 不修改现有 `sync_objects` 密文列含义；未来 Gate M2 用版本化新表承载 `devices/vaults/vault_objects/vault_changes`，避免破坏 Snapshot 兼容。

建议索引：

- `organizations(owner_id) WHERE kind='personal'` 唯一索引。
- `organization_members(user_id, organization_id)`，支持共享组织资料/头像判断。
- `user_profiles(id)` 已由主键覆盖，不为低选择性 `display_name` 提前建索引。
- 保留现有 `(organization_id, kind, logical_id)` 唯一键和按更新时间/序列的同步分页索引。

## 15. 安全与隐私门禁

- `anon` 无业务表和私有头像访问。
- App 只打包 Supabase Publishable Key，永不打包 Service Role/Secret Key、数据库密码或 SMTP 密码。
- 所有 exposed table 启用 RLS；UPDATE 同时验证 `USING` 和 `WITH CHECK`。
- 不使用 `auth.role()`；Policy 用 `TO authenticated` 加 ownership/membership 条件。
- 不用 `user_metadata` 做授权。
- 云端和客户端日志不记录 Authorization Header、Auth 密码、同步 Passphrase、头像二进制、Server 字段或密文正文。
- 下载到 Rust 的密文也视为 Untrusted：先限制对象数/大小/版本，再验签/AEAD，再反序列化。
- Server Sync 不得改变 Known-host Verification：Unknown 仍为 Trust Once / Trust & Remember / Cancel，Changed 永远 Block。
- Account 删除必须明确说明：删除云端账号/密文不等于删除其他设备已经拥有的本地副本。

## 16. 测试与验收

### Auth/Profile

- 注册、重复邮箱、未确认登录、确认邮件、错误密码、重置密码、退出登录。
- 双语邮件和 `runory.app` Redirect 实际投递。
- 用户只能更新自己资料；同组织可读，跨组织不可读。
- Auth Trigger/RPC 失败不留下半创建的业务权限。

### Avatar

- JPEG/PNG/WebP 成功；SVG、伪造 MIME、超 2 MiB、超尺寸解码失败被拒绝。
- 用户只能写/删自己的目录；同组织可读；anon/跨组织不可读。
- 新对象上传成功但 Profile CAS 失败时可清理；旧对象删除失败不破坏当前头像。
- Signed URL 过期后刷新，URL 不持久化。

### Server Sync

- 空云/空本地、首次上传、首次下载、两端同时有数据。
- Profile/Group 增删改、Tombstone、Group 删除、Jump Host 引用缺失。
- 同字段冲突、Delete vs Update、CAS 冲突、重复 Operation ID、分页中断。
- Wrong Passphrase、损坏 Ciphertext、未知版本、超大 Payload、网络断开、磁盘满、Crash Recovery。
- 同步结果永不出现 Password/Passphrase/Private Key/Key Path/Terminal Output。
- 无 Supabase 配置和完全断网时 SSH/SFTP/本地 CRUD 全部通过。

### Supabase 门禁

- Migration 从空 Postgres 17 重放。
- pgTAP 覆盖 RLS、ACL、跨组织、Storage 和 RPC。
- `supabase db lint`、Database/Security Advisors 无未处理 Error/Warn。
- Staging 通过后才能把同一 Migration/Function/Client Hash 晋升 Production。

## 17. 实施切片

### S1 — Account Foundation

- `runory.app` Auth 页面和 Supabase Free Project Domain 配置。
- 注册/确认/登录/重置/退出。
- `user_profiles` 与 Account UI。
- 保持 Session 内存策略。

### S2 — Avatar

- 私有 Bucket、RLS、图片处理、Signed URL 和清理。
- 账号/团队成员 UI 展示头像。

### S3 — Personal Server Sync UX

- Personal Workspace 幂等创建。
- 在现有 Snapshot Sync 上完成首次同步向导、字段说明、冲突中心和状态模型。
- 不上传 Credential，不宣称实时同步。

### S4 — Automatic Object Sync（Gate M2）

- 按既有 E2EE 文档实现 Device Identity、Vault Envelope、Outbox/Cursor 和逐对象 CAS。
- 通过密码学、迁移、跨平台 Secure Storage、Rollback/Replay 与外部安全评审门禁后，才从手动 Snapshot 切换为自动同步。

每个切片都必须可独立回滚到 Local-only；任何失败不删除本地 Profile、Group 或 Credential。

## 18. 参考

- Supabase Auth: https://supabase.com/docs/guides/auth
- Supabase User Management: https://supabase.com/docs/guides/auth/managing-user-data
- Supabase Redirect URLs: https://supabase.com/docs/guides/auth/redirect-urls
- Supabase Storage Access Control: https://supabase.com/docs/guides/storage/security/access-control
- Supabase Custom Domains: https://supabase.com/docs/guides/platform/custom-domains
- Runory E2EE proposal: `docs/CLOUD_SYNC_SECURITY_ARCHITECTURE.md`
- Runory cloud deployment gates: `docs/CLOUD_DEPLOYMENT.md`
