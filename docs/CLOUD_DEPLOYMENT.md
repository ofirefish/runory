# Runory Supabase Production Deployment

Runory 的云能力是可选模块。没有 Supabase 配置时，SSH、SFTP、Vault 与本地数据仍保持完整可用。

## 1. Environment isolation

至少使用独立的 Staging 与 Production Supabase Project。数据库变更只通过 `supabase/migrations` 部署，不在 Dashboard 手工维护另一套 Schema。

前端只配置：

```text
VITE_SUPABASE_URL
VITE_SUPABASE_PUBLISHABLE_KEY
```

`SUPABASE_ACCESS_TOKEN`、数据库密码与 SMTP 密码禁止使用 `VITE_` 前缀，也禁止进入应用包。仓库已忽略 `.env`、`.env.cloud` 和环境变体，只保留无密钥的示例文件。前端命令不读取 `.env.cloud`。

策略签名使用独立 Ed25519 密钥对。私钥只部署到 Supabase Edge Function Secret；32 字节公钥在 Rust 编译时固定，不经过 React/Vite，也不能由运行时 IPC 替换。

## 2. Auth email readiness

先验证仓库中的五套 Auth 模板：

```text
pnpm cloud:auth:check
```

复制 `.env.cloud.example` 为 `.env.cloud`，填写 Supabase Management API 与 SMTP 环境变量。该文件只由下面的部署命令读取：

```text
pnpm cloud:auth:apply
pnpm cloud:auth:verify
```

配置工具只调用固定的 Supabase Auth Configuration endpoint，不接受自定义 URL；不会打印 Access Token、SMTP 密码或远端配置正文。`--apply` 完成后会重新读取并逐字段验证非秘密配置与邮件模板。

上线前还必须：

- 为 Auth 邮件使用独立发信子域和 From 地址。
- 配置 SPF、DKIM 与 DMARC。
- 关闭 SMTP 服务商的链接跟踪，避免改写 Supabase 确认链接。
- 在 Staging 实际验证注册确认、账号邀请、重置密码、Magic Link 与邮箱变更。
- 根据预期流量调整 Auth 邮件速率限制并监控退信。

`invite.html` 是 Supabase Auth 的账号邀请模板。Runory Organization 成员邀请当前仍是应用内邀请，不会把 Service Role Key 放入客户端来发送管理端邀请邮件。

## 3. Migration verification

Runory 的本地 Supabase 栈使用隔离的 `54520`–`54529` 端口段，避免与同一工作站上的其他默认 Supabase 项目互相停止或抢占端口。本地 Auth Site URL 与 Tauri/Vite 开发地址统一为 `http://localhost:1420`。

本地栈启动后可运行统一数据库门禁：

```text
pnpm cloud:db:verify
```

该命令依次执行 63 项 pgTAP 测试、`plpgsql_check` Lint，以及会阻止 Warning/Error 的 Security/Performance Advisors。pgTAP 同时断言关键外键/保留索引存在，避免 Advisor 的全新数据库 `unused_index` 信息掩盖索引回归。

远端发布前先查看 CLI 当前参数：

```text
pnpm exec supabase --version
pnpm exec supabase db push --help
```

对已 Link 的 Staging Project 先执行 dry run，再部署并比较迁移历史：

```text
pnpm exec supabase db push --dry-run
pnpm exec supabase db push
pnpm exec supabase migration list
```

在 Staging 上运行 `supabase/tests/cloud_security.test.sql`，并执行 Database Advisors。生产环境只接受已在 Staging 通过 Migration、RLS、pgTAP 与 Advisor 检查的同一版本。当前本地门禁覆盖跨组织读取、直接 DML 绕过、Security Definer ACL/search path、策略 deny-overrides、审计写入、Cron schema 隔离、有界保留批次与关键索引。

## 4. Production checks

- Email Confirmation 保持启用，OTP/链接有效期不超过一小时。
- Database SSL Enforcement 与适当的 Network Restrictions 已启用。
- Supabase Organization 管理员启用 MFA，并至少有两名可恢复 Owner。
- `anon` 对 Runory 业务表没有权限；`authenticated` 只有 migration 中的显式 GRANT 与 RLS。
- `runory-audit-retention-daily` 每天 03:17 UTC 调用私有保留函数，每次最多清理 10,000 条超过 180 天的记录；Cron schema 与函数均不可被客户端访问。
- 部署完成后重新生成并核对 TypeScript Database Types。

## 5. Signed offline policy decisions

首次为一个环境生成密钥：

```text
pnpm cloud:policy:keygen
```

命令只创建被 Git 忽略的 `.env.policy-signing` 与 `.env.policy-build`，不会把密钥打印到终端；已存在文件时拒绝覆盖。把签名私钥部署为 Function Secret，然后部署需要用户 JWT 的 Function：

```text
pnpm cloud:functions:typecheck
pnpm exec supabase secrets set --env-file .env.policy-signing
pnpm exec supabase functions deploy evaluate-access-policy
```

在受控 CI 或本机打包 shell 中，从 `.env.policy-build` 注入 `RUNORY_POLICY_VERIFYING_KEYS_JSON` 后再执行 Tauri build。该变量包含最多四个 `keyId → 32 字节公钥` 的 JSON 信任集，是公开验证材料，但必须作为受审查的信任根固定到正式二进制；不能从 Vite、用户配置或远端响应动态读取。Windows PowerShell 可在当前进程加载而不回显值：

```powershell
Get-Content -LiteralPath .env.policy-build | ForEach-Object {
  $name, $value = $_.Split('=', 2)
  Set-Item -Path "Env:$name" -Value $value
}
pnpm tauri build
```

Function 保持 `verify_jwt = true`，并把调用者的 Authorization 原样用于 `evaluate_access_policy` RPC，因此数据库仍按用户身份、Organization membership 与 RLS 判定；Function 不使用 Secret/Service Role API Key 读写业务数据。签名私钥采用 PKCS#8 DER 的 Base64，永不进入客户端、日志或仓库。

已固定公钥的 Rust 构建接受带 `keyId` 的 version 2 决策包；version 1 仅用于兼容已经发布的单密钥构建。包把版本、keyId、Organization ID、Profile ID、Action、允许/拒绝、签发时间和过期时间全部纳入签名；最大有效期 300 秒，未来时钟容差 30 秒。有效包按精确动作原子缓存，在线不可达、短期 Token 丢失或应用重启后可在剩余有效期内继续使用。未知 keyId、过期、篡改、错误上下文、未知版本和超长 TTL 全部 fail-closed。未固定公钥的开发构建不启用离线缓存，继续使用原有在线 RPC。

密钥轮换采用重叠信任窗口：先把新旧私钥都加入 `RUNORY_POLICY_SIGNING_KEYS_JSON`，并发布同时固定新旧公钥的客户端；客户端覆盖率满足要求后，把 `RUNORY_POLICY_ACTIVE_SIGNING_KEY_ID` 切到新 keyId；至少等待 300 秒并确认旧版本退出支持范围后，才能从 Function Secret 和后续客户端构建中移除旧 keyId。尚未包含新公钥的旧客户端在切换后安全地 fail-closed，不会静默接受未知签名。

本地 Supabase 启动后运行真实链路门禁：

```text
pnpm cloud:policy:e2e
```

该脚本拒绝非 localhost URL，临时生成两套密钥、用户和组织，验证 Auth JWT、成员 RLS、默认 allow、deny-overrides、v2 签名、篡改拒绝和 active key 轮换，结束时删除测试用户与临时私钥。

Rust 新增直接依赖仅为已经存在于锁文件和依赖树中的 `ring 0.17` 与 `base64 0.22`。`ring` 用于 Ed25519 验签，`base64` 只负责固定长度密钥与签名编码；两者维护活跃、许可证兼容，均不依赖 Desktop Window、任意文件系统或 Node.js，保持 Android/iOS Core 可编译。

## 6. Read-only remote release gate

先验证仓库内的远端门禁、Auth 模板和 Edge Function 类型：

```text
pnpm cloud:remote:test
pnpm cloud:remote:check
```

`cloud:remote:check` 不访问远端；它会验证七个本地 Migration 版本唯一、Policy Function 保持 `verify_jwt = true`、`.env.cloud.example` 完整，以及 Auth/Edge 静态门禁通过。当前工作站若没有 `.env.cloud` 或没有执行项目 Link，只把它们报告为外部 blocker。

由发布人员核对目标确实是 Staging 后，显式执行一次：

```text
supabase link --project-ref <staging-project-ref>
```

Link 会改变本地 Supabase Project 指向，因此不由验证脚本自动执行。将 `.env.cloud.example` 复制为被忽略的 `.env.cloud` 并填写 Management Token、Project Ref 和期望的 SMTP 配置后，运行只读远端门禁：

```text
pnpm cloud:remote:verify
```

验证器要求 Link 的 Project Ref 与环境完全一致，并通过 Supabase CLI/Management API 检查：

- 远端 Migration 历史与本地七个版本逐项一致；
- 六张业务表全部启用 RLS；
- Policy RPC ACL、私有 Retention Function 和 Retention Index 正确；
- `evaluate-access-policy` 已部署且明确报告 JWT Verification 已启用；
- Active Signing Key 与 Signing Key Set 两个 Secret 名称都存在，Secret 值不读取、不输出；
- 远端 Auth/Custom SMTP 的非秘密字段与五套模板一致；
- Audit Cron 唯一、启用、时间和命令正确，并且至少运行成功一次且最近一次成功。

数据库探针使用 `supabase db query --linked --file` 通过 Management API 执行仓库内固定的单条只读 SELECT，不接受动态 SQL，也不需要把数据库密码放入命令参数。门禁不会执行 Link、Migration Push、Function Deploy、Secret Set、Auth Apply 或任何修复操作；这些变更必须由发布人员分别审查和执行。新环境需等 03:17 UTC 的首次定时任务成功后，远端门禁才会完全通过。

### 6.1 Machine-readable release evidence

CI 或发布人员可以请求单行 JSON 证据：

```text
pnpm --silent cloud:remote:check:json
pnpm --silent cloud:remote:verify:json
```

报告使用稳定的 `schemaVersion: 2`，包含目标 Project Ref、Migration 版本、门禁布尔结果、最近一次 Audit Cron 状态，以及完整 React/Rust 应用源码、Migration、Policy Function、Auth 模板、固定 SQL 探针和 pgTAP 测试的 SHA-256。`tooling_ready` 只表示本地发布候选输入有效；只有连接目标环境并完成全部远端核对后才会输出 `release_ready`。

JSON 不包含 CLI 原始输出、Access Token、JWT、数据库连接串、SMTP 密码、策略签名私钥或 Edge Secret 值。发布系统可以把标准输出保存为受控 Artifact；失败时进程保持非零退出码，不得把绕过门禁的手工截图当作等价证据。

### 6.2 Staging to Production promotion gate

晋级分为部署前批准和部署后复核，不能用后置 Production 检查替代前置批准。

先对已经通过真实远端门禁的 Staging 生成 `staging-readiness.json`，并从准备部署的同一工作区生成本地候选：

```text
pnpm --silent cloud:remote:check:json > artifacts/candidate-readiness.json
pnpm --silent cloud:promotion:prepare -- --staging artifacts/staging-readiness.json --candidate artifacts/candidate-readiness.json
```

只有输出 `deployment_candidate_ready` 后才能执行 Production 的显式 Migration、Function、Secret 与 Auth 部署动作。部署完成并等待 Production Audit Cron 成功后，生成 `production-readiness.json`，再执行后置复核：

```text
pnpm --silent cloud:promotion:finalize -- --staging artifacts/staging-readiness.json --production artifacts/production-readiness.json
```

门禁默认只接受最近 60 分钟生成的证据，并要求：

- 所有证据严格符合 `schemaVersion: 2`，不存在未批准的额外字段；
- 部署前 Staging 已远端通过，本地候选只携带允许的本地 blocker，不伪造远端状态；
- 部署后 Staging 与 Production 使用不同的 20 字符 Project Ref，且两边所有远端检查均通过；
- Migration 版本顺序完全一致；
- React/Rust 应用源码、Migration、Policy Function、Auth 模板、SQL 探针和 pgTAP 测试摘要完全一致。

部署前成功输出单行 `deployment_candidate_ready` JSON，部署后成功输出 `production_verified`；任一条件不满足都会以非零状态退出。证据有效期可通过 `--max-age-minutes` 调整为 5–1440 分钟，但发布流水线不应为了绕过过期失败而放宽窗口。
