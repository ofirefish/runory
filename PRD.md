# Runory PRD

> Document scope: 第 5–22 节是 v0.1 / Phase 1 的验收基线与历史范围，不是当前仓库全部功能清单。当前已实现状态以 `ROADMAP.md` 和代码为准；`AGENTIC.md` 描述的完整 Agentic Infrastructure Workspace 是 future architecture，不能从本 PRD 推导为已经交付。

## 1. 产品定义

Runory 是一款 Local-first、跨平台的 SSH 与基础设施运维客户端。

产品不是“另一个 Terminal Emulator”，而是从可靠 SSH Core 出发，逐步演化为统一的服务器工作空间：

```text
SSH
→ SFTP
→ Server Monitor
→ Docker / PM2 / Nginx
→ Deployment
→ AI DevOps
```

Tagline：

> Run your infrastructure from anywhere.

### 长期产品模型

Runory 将从可靠连接工具演化为 **AI-native Infrastructure Workspace**：

```text
Connect → Observe → Diagnose → Plan → Approve → Execute → Verify → Rollback
```

核心扩展体系：

```text
Native Tools   = 能做什么
Skills         = 如何解决一类问题
MCP            = 如何连接外部世界
ChangeSet      = 如何安全执行变化
Approval/Audit = 如何保持人在回路与可追溯
```

详细 Agentic 产品与技术规范见 `AGENTIC.md`。上述模型是长期方向，不代表当前已经交付。

## 2. 目标用户

- 管理 Linux VPS 的开发者
- Indie Hacker / SaaS Founder
- 没有专职 DevOps 的小型研发团队
- 同时维护大量客户服务器的 Agency
- 需要更现代 SSH 工作流的技术人员
- 未来需要手机端应急登录服务器的用户

## 3. 核心问题

Runory 要解决：

- SSH 主机信息散落在终端历史、表格、笔记
- 多窗口 Terminal 缺乏清晰上下文
- SSH 密码、私钥处理不够安全
- SSH 与文件传输被割裂在两个软件中
- 很多传统工具缺乏现代分组、搜索、多语言体验
- 桌面工具很难自然扩展到手机
- 未来 AI 运维不能绕过安全和人工确认

## 4. 产品原则

### Local First

第一阶段无账户、无云端中转、无 Runory Backend。

```text
Runory
  │
  └── Direct SSH
        │
        └── User Server
```

### Secure by Default

Host Verification、Credential Isolation、Least Privilege 都是默认能力。

### Human in the Loop

未来 AI 不得默认静默执行高风险命令。

### Desktop-first, Mobile-aware

桌面先交付，Core 从第一天保持五端可演进。

## 5. v0.1 第一阶段目标

用户必须能够：

1. 创建并管理主机。
2. 创建自定义分组管理主机。
3. 使用 Password 或 Private Key 建立 SSH。
4. 选择 Session-only 或安全记住凭据。
5. 首次连接核对 Host Fingerprint。
6. Host Key 改变时阻止连接。
7. 打开完整交互式 PTY Terminal。
8. 同时打开多个 SSH Session。
9. 网络断开后手动 Reconnect。
10. 在简体中文与英文之间即时切换。
11. 在 Windows/macOS/Linux 使用。
12. 在技术 Spike 中证明 Android/iOS Rust SSH Core 可连接。

## 6. 第一阶段非目标（历史边界）

禁止主动实现：

- SFTP / SCP / File Manager
- Port Forwarding / Tunnel
- Jump Host / Proxy
- CPU/RAM/Disk Monitor
- Docker / PM2 / Nginx
- Logs Dashboard
- Backup / Deployment
- GitHub Deploy
- AI Terminal / AI Agent
- Cloud Sync
- Account / Team / Billing
- 上传服务器元数据的 Telemetry

这些条目只约束当时的 Phase 1。仓库中已按后续 Phase 实现的 SFTP、Operations、Deployment、Mobile、有限 AI 与 Optional Cloud 应保留；本节不授权删除、回退或重写它们。

## 7. 核心领域对象

### HostGroup

```ts
type HostGroup = {
  id: string
  name: string
  sortOrder: number
  collapsed: boolean
  createdAt: string
  updatedAt: string
}
```

规则：

- 用户创建
- 可重命名、排序、删除
- 删除分组不得删除主机
- 被删除分组中的主机自动变成 `groupId = null`
- “未分组 / Ungrouped”是虚拟系统分类，不写入 Group Entity

### ServerProfile

```ts
type ServerProfile = {
  id: string
  name: string
  host: string
  port: number
  username: string
  groupId: string | null
  authMethod: "password" | "privateKey"
  keySource?: {
    type: "file" | "vault"
    path?: string
    keyId?: string
  }
  sortOrder: number
  createdAt: string
  updatedAt: string
  lastConnectedAt?: string
}
```

禁止字段：

```text
password
privateKeyPassphrase
privateKeyContent
vaultMasterPassword
```

### ServerSession

运行时的已认证 SSH Transport。

未来：

```text
ServerSession
├── TerminalChannel
├── SFTPChannel
├── ExecChannel
└── TunnelChannel
```

Phase 1 即便一个 Session 暂时只有一个 Terminal，也不得把两个概念永久绑定。

## 8. 主机管理

### Add Host

字段：

- Connection Name
- Host / IP
- Port，默认 22
- Username
- Group
- Authentication
- Password
- Remember securely
- Private Key
- Passphrase
- Remember securely

操作：

```text
Test Connection
Cancel
Save
Save & Connect
```

### Edit Host

支持修改：
- name
- host
- port
- username
- group
- auth method
- key source

修改 Group 不得影响已经建立的 SSH Session。

### Delete Host

删除时：
- 删除 Profile Metadata
- 删除关联 Remembered Credential
- 活跃 Session 必须明确询问用户，不得静默处理

### Search

搜索：
- Host Name
- Host/IP
- Username
- Group Name

Group 命中时显示组内主机。

## 9. 分组

用户可：

- Create
- Rename
- Delete
- Expand / Collapse
- Reorder
- Move Host
- Move to Ungrouped
- Reorder Hosts

名称规则：
- trim
- 1–50 Unicode 字符
- 可以同名，但 UI 可提示

## 10. SSH Authentication

### Password

标准 SSH Password Authentication。

### Private Key

Desktop Phase 1 使用文件型 Key Source。

架构必须支持：

```text
File
Vault
```

未来手机优先使用 Vault Import。

### Passphrase

加密私钥支持 Passphrase：

```text
Session only
Remember securely
```

## 11. Host Key Verification

必须启用。

### First Connection

展示：
- Host
- Port
- Key Type
- SHA256 Fingerprint

操作：

```text
Trust Once
Trust & Remember
Cancel
```

### Known Host

Fingerprint 一致，继续。

### Changed Host Key

立即阻止。

第一阶段不提供：

```text
Ignore & Connect
```

用户必须先查看/删除 Trusted Host 再重新确认。

## 12. Session State

```text
Idle
Connecting
VerifyingHost
Authenticating
OpeningShell
Connected
Disconnected
Error
```

UI 必须基于统一枚举，不得用多个 Boolean 拼接状态。

## 13. Terminal

必须支持：

- Interactive PTY
- xterm-256color
- UTF-8
- ANSI
- Chinese
- Ctrl+C
- Ctrl+D
- Tab
- Arrow Keys
- Resize
- Copy / Paste
- Search
- Scrollback
- Reconnect
- Multiple Tabs

验收：

```text
ls
cd
cat
clear
sudo
vim
nano
top
htop
less
```

Shell：
- bash
- zsh

## 14. Terminal Data Path

输出：

```text
russh
 ↓
Rust async task
 ↓
Tauri IPC Channel
 ↓
xterm.write()
```

不得进入：
- React State
- Zustand
- Persistent Storage

输入可以使用很短的 5–10ms Buffer 兼顾延迟与 IPC 开销。

## 15. Credential

模式：

```text
Session Only
Remember Securely
```

默认：

```text
Session Only
```

Remembered credentials 必须通过 `CredentialVault`。

## 16. i18n

第一阶段：

```text
zh-CN
en-US
```

要求：
- 切换即时生效
- 禁止硬编码 UI 文案
- 用户创建的主机名/分组名不翻译
- Terminal 内容不翻译
- “Ungrouped”属于系统虚拟文案，要翻译
- 时间/数字用 Intl

## 17. Theme

```text
System
Light
Dark
```

默认 System。

## 18. Desktop Layout

```text
┌────────────────┬────────────────────────────────────┐
│ Runory         │ Terminal Tabs                      │
│ Search         ├────────────────────────────────────┤
│                │                                    │
│ Groups / Hosts │             Terminal               │
│                │                                    │
│                ├────────────────────────────────────┤
│ Settings       │ Connected · SSH · 20ms             │
└────────────────┴────────────────────────────────────┘
```

## 19. Mobile 要求

第一阶段只做技术验证，不交付完整 Mobile Product。

未来 Terminal 必须提供：

```text
ESC CTRL ALT TAB
↑ ↓ ← →
HOME END
PGUP PGDN
```

手机端采用 Screen Navigation，而不是缩小桌面 Sidebar。

## 20. Persistence

第一阶段：

```text
profiles.json
groups.json
known-hosts.json
settings.json
```

全部 Atomic Write。

Credential 独立保存在 Secure Vault。

## 21. Error Codes

至少：

```text
HOST_NOT_FOUND
CONNECTION_REFUSED
CONNECTION_TIMEOUT
AUTH_FAILED
PRIVATE_KEY_INVALID
PASSPHRASE_REQUIRED
HOST_KEY_UNKNOWN
HOST_KEY_CHANGED
CONNECTION_LOST
VAULT_LOCKED
INVALID_PROFILE
UNKNOWN
```

Rust 返回 Code，React i18n 渲染语言。

## 22. Definition of Done

### SSH
- Password Login
- Private Key Login
- Encrypted Key Login
- Host Verification
- Changed Key Block
- Disconnect / Reconnect

### Terminal
- PTY
- UTF-8
- ANSI
- vim/top/htop/nano
- resize
- search
- copy/paste

### Organization
- Host CRUD
- Group CRUD
- Reorder
- Move
- Search

### Security
- No plaintext credential
- No secret logs
- Strict CSP
- Least-privilege capabilities
- No arbitrary renderer FS

### Stability
- 10 concurrent sessions
- 2-hour session
- network loss no crash
- server restart no crash
- high-output no renderer freeze

### Platforms
- Windows build
- macOS build
- Linux validation
- Android SSH Spike
- iOS SSH Spike
