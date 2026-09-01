# Runory

**Run your infrastructure from anywhere.**

Runory 是一款 Local-first、跨平台的 SSH 与服务器运维客户端。当前已在稳定 SSH Core 与共享 `ServerSession` 底座上实现 Files、服务器运维、部署、移动端产品外壳、本地 AI Terminal、有限类型化 AI Plan、可选 Cloud Foundation，以及 Phase 10A–10G Agentic Infrastructure Workspace Foundation / Production Hardening。

长期路线不是“给 Terminal 加聊天框”，而是把系统、文件、服务、Nginx、Docker 等能力抽象成安全的 Typed Tools，通过 Skills、MCP、ChangeSet、Approval、Verification、Rollback 和 Audit 实现可审查的服务器诊断与修复。当前实现状态与长期目标必须严格区分。

## 目标平台

- Windows
- macOS
- Linux
- iOS
- Android

第一阶段是 **Desktop-first，而不是 Desktop-only**。桌面端先发布，但 Rust SSH Core 必须在 Android/iOS 上完成技术验证。

## 当前功能

- 主机新增、编辑、删除、搜索
- 用户自定义主机分组
- 分组创建、重命名、删除、折叠、排序
- 主机移动分组、组内排序
- Password SSH Authentication
- Private Key Authentication
- 加密私钥 Passphrase
- Test Connection
- SSH Host Key Verification
- Trust Once / Trust & Remember
- Host Key Changed 强制阻止
- Interactive PTY
- 多 SSH Session / Terminal Tab
- Resize / Copy / Paste / Search
- ANSI / UTF-8 / 中文终端
- zh-CN / en-US
- System / Light / Dark
- Secure Credential Vault
- Local-first metadata storage
- 同一 SSH Transport/Auth 上的独立 SFTP Channel
- Terminal / Files / Details 会话视图
- 远端目录浏览、面包屑导航、刷新与文件元数据
- SFTP 上传、下载、创建目录、重命名、删除与传输队列
- Agentless Dashboard、Docker / PM2 / Nginx 与日志运维
- Git 部署、环境变量、SSL、备份、Cron 与部署历史
- Android / iOS 自适应 UI、移动私钥导入、隐私遮罩与生物识别解锁
- AI Terminal：命令解释、受限生成、输出诊断、修复建议、风险标记与用户确认
- 有限类型化 AI Plan：用户选择工具与活动会话，逐项审批、一次性执行并写入本地审计
- 可选 Supabase 邮箱登录、Organization/RLS、Rust 端到端加密同步、团队治理与审计（默认保持 Local-only）

AI Terminal 与 Phase 8 有限 AI Plan 继续独立保留。Phase 10 Agentic Foundation 通过 Rust Tool Registry 实现只读 Server Doctor、Diagnosis / Evidence、版本化 ChangeSet、精确审批、验证与真实可用时的回滚、受限 Skills、只读 MCP 外部上下文和多服务器 Drift；Production Hardening 进一步加入无内容 ChangeSet 崩溃恢复、审批失效/中断语义、现代与初始化型 MCP 双协议适配，以及 Agentic UI 按需加载。不接受任意 Shell，也不向 React 暴露通用 Tool/MCP 执行 API。Cloud Sync 当前同步不含凭据的 Profile/Group 清单，支持加密 tombstone、预览式安全合并、逐项冲突决策、成员角色、Access Policy 和 Audit 分页。Managed AI、Marketplace、Signed Skill Package、Trusted Automation、Editor、Drag & Drop 与 Port Forwarding 仍不在当前实现范围。

Supabase 生产部署、SMTP 配置、远端发布门禁、schema v2 无秘密 JSON 证据与 Production 部署前/后两阶段门禁见 [`docs/CLOUD_DEPLOYMENT.md`](docs/CLOUD_DEPLOYMENT.md)。

## 技术栈

### UI

- Tauri 2
- React
- TypeScript
- Vite
- Tailwind CSS
- shadcn/ui
- Lucide
- Zustand
- Zod
- React Hook Form
- i18next / react-i18next
- xterm.js

### Core

- Rust
- Tokio
- russh
- serde / serde_json
- thiserror
- uuid
- tracing
- zeroize / secret wrapper
- Tauri Stronghold（通过 `CredentialVault` 抽象）

## 核心架构

```text
React UI
   │
   ├── Servers
   ├── Groups
   ├── Terminal
   └── Settings
   │
Tauri Commands / Channels
   │
Rust Application Core
   │
   ├── ProfileService
   ├── GroupService
   ├── CredentialService
   ├── KnownHostService
   ├── ServerSessionManager
   └── SSHService
   │
 russh
   │
SSH Server
```

高频 Terminal Output 必须使用 Tauri IPC **Channel**，不得经过 React State 或 Zustand。

## 当前高层目录

```text
runory/
├── src/
│   ├── app/
│   ├── components/
│   ├── features/
│   │   ├── profiles/
│   │   ├── groups/
│   │   ├── sessions/
│   │   ├── terminal/
│   │   ├── files/
│   │   ├── dashboard/
│   │   ├── operations/
│   │   ├── deployment/
│   │   ├── mobile/
│   │   ├── ai/
│   │   ├── ai-agent/
│   │   └── settings/
│   ├── hooks/
│   ├── i18n/
│   ├── lib/
│   ├── stores/
│   └── types/
│
├── src-tauri/
│   ├── capabilities/
│   └── src/
│       ├── commands/
│       ├── ai/
│       ├── cloud/
│       ├── credentials/
│       ├── dashboard/
│       ├── deployment/
│       ├── domain/
│       ├── groups/
│       ├── known_hosts/
│       ├── operations/
│       ├── profiles/
│       ├── settings/
│       ├── ssh/
│       ├── storage/
│       └── transfers/
│
├── tests/
│   └── ssh-server/
├── docs/
│   └── assets/brand/
├── supabase/
├── AGENTIC.md
├── AGENTS.md
├── ARCHITECTURE.md
├── BRAND.md
├── DESIGN.md
├── PRD.md
├── ROADMAP.md
├── SECURITY.md
└── README.md
```

## 开发原则

1. React 是交互层，Rust 是基础设施引擎。
2. Password、Passphrase、Private Key Content 不得进入 Profile JSON。
3. Host Key Verification 不得关闭。
4. Terminal Output 不得进入 Zustand。
5. Tauri Capability 遵循 Least Privilege。
6. Production 不加载 Remote JavaScript。
7. 所有可见 UI 文案走 i18n。
8. 用户自建主机名、分组名绝不自动翻译。
9. 按当前 Roadmap 严格控制范围；future 文档不等于开发授权。
10. Core 不依赖 Node.js。

## 文档

- [PRD.md](./PRD.md)
- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [DESIGN.md](./DESIGN.md)
- [SECURITY.md](./SECURITY.md)
- [AGENTS.md](./AGENTS.md)
- [ROADMAP.md](./ROADMAP.md)
- [AGENTIC.md](./AGENTIC.md)（future architecture）
- [CODEX_AGENTIC_MERGE.md](./CODEX_AGENTIC_MERGE.md)
- [BRAND.md](./BRAND.md)

## 品牌与 UI 基线

**Runory**

> Run your infrastructure from anywhere.

当前正式采用 Runory VI v2。核心标识为抽象字母 **R**，结合连接节点、网络基础设施与向前运行的控制感；不得使用传统 `>_` Terminal Prompt 作为主品牌标识。

核心品牌色：

```text
Runory Indigo    #4F46E5
Runory Blue      #6366F1
Runory Cyan      #06B6D4
Runory Teal      #14B8A6
```

产品 UI 以深色基础设施工作区为主，品牌色主要用于 Logo、Active Tab、Selected Host、Primary Action、Focus State 与 Brand Accent。Success、Warning、Error、Info 等语义状态色保持独立。

界面语言为 Modern、Calm、Technical、Precise、Secure、Cross-platform。移动端采用独立导航模型，不是简单缩小 Desktop Sidebar。正式 UI 参考为 `docs/assets/brand/runory-desktop-ui-reference-v3.png`；它只定义视觉方向与布局层级，不是逐像素实现稿。

## License

项目自身 License 在正式公开前决定。所有第三方依赖在发布前必须完成许可证检查。
