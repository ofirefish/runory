# Runory UI / UX Design Specification

> Version: Desktop UI v3  
> Visual baseline: Runory VI v2  
> Primary reference: `docs/assets/brand/runory-desktop-ui-reference-v3.png`

> Scope note: 当前代码已经包含 Phase 1–9 的相应入口。本文的 Phase Mapping 用于约束功能何时可见，不要求隐藏或删除已经真实实现的模块；`AGENTIC.md` 中的 Agent Timeline、Diagnosis、Evidence、ChangeSet、Skills 与 MCP UI 仍属 future，不得用现有有限 AI Plan 假装完成。

---

## 1. 设计定位

Runory 桌面端不再按“传统 SSH 客户端 + 一个终端窗口”的思路设计，而采用：

> **Infrastructure Operations Workspace / 基础设施运维工作台**

核心体验由五个区域组成：

```text
Global Top Bar
+
Primary Navigation Rail
+
Resource Explorer
+
Main Workspace
+
Context Inspector
```

产品应该同时具备：

- Modern
- Technical
- Dense but Clear
- Fast
- Secure
- Professional
- Infrastructure-oriented
- Cross-platform

科技感应来自清晰的信息架构、暗色层次、精准线框、实时状态和基础设施语境，而不是无意义发光、粒子动画和复杂渐变。

参考原则：Obsidian / VS Code 的多栏工作区、Linear 的层级与紧凑度、Raycast 的暗色精致度、Vercel 的排版、Warp 的现代 Terminal 体验。只参考原则，不复制具体产品。

---

## 2. Runory VI v2

### 2.1 品牌标识

Runory 使用第二版 VI 的抽象 **R** 图形，表达：

```text
R + Connection + Node + Forward Motion
```

Logo 主要用于 App Title Bar、Primary Navigation Rail、Splash、App Icon、About 和 Mobile Header。不得用 `>_` 作为主品牌标识，以免把 Runory 限定为单纯 Terminal 产品。

### 2.2 品牌色

```text
Runory Indigo    #4F46E5
Runory Blue      #6366F1
Runory Cyan      #06B6D4
Runory Teal      #14B8A6
```

推荐渐变：

```css
linear-gradient(
  135deg,
  #4F46E5 0%,
  #6366F1 40%,
  #06B6D4 75%,
  #14B8A6 100%
)
```

Gradient 只用于 Logo、Primary CTA、极少量 Active Indicator 和 Marketing/Onboarding。核心工作区不得铺大面积彩色渐变。

---

## 3. Dark Theme

Dark Mode 是 Runory 的主视觉展示模式。

```text
--app-bg:               #07111F
--topbar-bg:            #091321
--rail-bg:              #081321
--explorer-bg:          #0A1524
--workspace-bg:         #08111D
--surface-1:            #0D1828
--surface-2:            #111D2E
--surface-3:            #162337

--border-default:       #1D2A3D
--border-subtle:        #142134
--border-strong:        #2B3B52

--text-primary:         #F4F7FB
--text-secondary:       #AAB5C5
--text-muted:           #6F7F94
--text-disabled:        #4C5B6E

--accent:               #6366F1
--accent-hover:         #7175F5
--accent-soft:          rgba(99,102,241,.14)
--focus-ring:           rgba(99,102,241,.42)
```

---

## 4. Light Theme

Light Mode 必须完整设计，不是附属模式。

```text
--app-bg:               #F4F7FB
--topbar-bg:            #FFFFFF
--rail-bg:              #F8FAFC
--explorer-bg:          #FFFFFF
--workspace-bg:         #F8FAFC
--surface-1:            #FFFFFF
--surface-2:            #F1F5F9
--surface-3:            #E9EEF5

--border-default:       #DCE3EC
--border-subtle:        #E9EEF5

--text-primary:         #0F172A
--text-secondary:       #475569
--text-muted:           #94A3B8
```

Dark / Light 必须保持同样的信息层级和布局能力。

---

## 5. Semantic Colors

```text
Connected / Success     #22C55E
Connecting / Info       #3B82F6
Warning                 #F59E0B
Error                   #EF4444
Disconnected            #64748B
Favorite                #FACC15
```

状态不能只用颜色，例如 Connected 应同时显示状态点和文字。

---

## 6. Typography

UI：`Inter`  
Terminal / Code：`JetBrains Mono`

```text
App Title           18 / 26 / 650
Workspace Title     16 / 24 / 600
Section Title       12 / 18 / 600
Body                13 / 20 / 400
Body Strong         13 / 20 / 550
Caption             12 / 18 / 400
Tiny                11 / 16 / 450
Terminal            13–14 / 1.50
```

Runory 是高信息密度工具，核心工作区不使用营销页级别的大字号。

---

## 7. Spacing / Radius / Icons

### Spacing

4px Grid：

```text
4 8 12 16 20 24 32
```

核心工作区以 4 / 8 / 12 / 16 为主。

### Radius

```text
Control Small       5px
Control Default     6px
Panel               8px
Dialog              10px
App Card            10px
```

不要全部做大圆角或 Pill。

### Icons

使用 Lucide：

```text
Primary Rail       20px
Toolbar            16px
Tree / Menu        15–16px
Status             12–14px
Stroke             1.75px
```

所有纯图标按钮必须有 Tooltip 与 `aria-label`。

---

## 8. Desktop 总体布局

最新版 Runory Desktop 使用五区结构：

```text
┌─────────────────────────────────────────────────────────────────────┐
│ Global Top Bar                                                      │
├──────┬────────────────┬────────────────────────────────┬────────────┤
│      │                │ Workspace Tabs                 │            │
│ Nav  │ Resource       ├────────────────────────────────┤ Inspector  │
│ Rail │ Explorer       │                                │            │
│      │                │ Main Workspace                 │            │
│      │                │                                │            │
│      │                ├────────────────────────────────┤            │
│      │                │ Context Dock                   │            │
└──────┴────────────────┴────────────────────────────────┴────────────┘
```

这是后续 SSH、SFTP、Monitor、Deploy、AI 等功能统一使用的 Desktop Shell。

---

## 9. Global Top Bar

高度建议：`48–52px`。

结构：

```text
[ Runory ]
[ Environment: Production ▼ ]
                [ Search servers, IP, commands...   Ctrl K ]
                                        [ + New Connection ]
[ Notifications ] [ Terminal ] [ Help ] [ Profile ] [ Window Controls ]
```

### Global Search

快捷键：`Ctrl/Cmd + K`。

Phase 1 只搜索：

- Server
- Group
- Host/IP

未来再扩展 Command、Files、Actions。Placeholder 必须与当前真实功能一致，不能暗示未实现能力。

### New Connection

Primary CTA 使用 Runory Indigo 或品牌渐变。

---

## 10. Primary Navigation Rail

建议宽度：`56–64px`。

概念结构：

```text
Runory Logo

Servers
Sessions
Files
Monitor
Deploy
AI Assistant

Settings
```

### 功能可见规则

设计稿会展示完整产品方向，但实际 App 必须按开发阶段逐步开放：

```text
Phase 1     Servers / Sessions / Settings
Phase 2     + Files
Phase 3     + Monitor
Phase 5     + Deploy
Phase 7     + AI Assistant
```

**Codex 不得因为 DESIGN.md 或参考图出现未来入口而提前实现未来模块。未实现的模块不应以 Disabled 占位长期展示。**

---

## 11. Resource Explorer

默认宽度：`260px`，范围 `220–420px`，支持拖动 Resize。

结构：

```text
Runory / Servers

Search servers or tags...

Favorites
    API Server
    DB Server

Production
    API Server
    DB Server
    Nginx Server
    Redis Cluster
    MongoDB

Staging
Development

Recent Connections
    Jump Server
    Staging API
    Dev Box

Tag Management
```

### Phase 约束

- Group：Phase 1
- Recent Connections：Phase 1 可实现
- Favorites：Phase 1.5 可选
- Tags：后续可选，不得与 Group 混为同一模型

---

## 12. Host Group

Group Header：

```text
▼ Production                       5
```

支持：Collapse、Expand、Context Menu、Drag Reorder。

Context Menu：

```text
New Connection
Rename
Expand
Collapse
────────────
Delete Group
```

删除 Group 不得删除 Host。

---

## 13. Host Item

高度建议：`48–56px`。

```text
● API Server                  ⋯
  root@10.0.0.11
```

状态：

```text
Green       Connected
Blue        Connecting
Amber       Warning
Red         Error
Gray        Offline
```

Selected：

```text
background: accent-soft
border: 1px solid subtle accent
```

不要使用强 Glow。

---

## 14. Favorites / Recent Connections

Favorites 是系统虚拟 Section，Starred Server 仍保留原 `groupId`，未来可通过 `favorite: boolean` 实现。

Recent Connections 只是一种动态索引，不复制 Profile。

---

## 15. Workspace Tabs

顶部支持多对象 Tab：

```text
[ API Server × ]
[ Database × ]
[ Nginx Config × ]
[ Deploy History × ]
[ + ]
```

Phase 1 只有 Server Session Tab；未来可承载 Config、Logs、Deployment、AI Session。

Tab 必须支持：

- Activate
- Close
- Reorder
- Overflow

---

## 16. Server Workspace Header

进入 Host 后增加 Context Header：

```text
API Server ▼     root@10.0.0.11 ▼
                                  SSH 🔒  Layout  Settings
```

用于显示当前 Server、User、Transport 和 Layout Controls。

---

## 17. Main Terminal Area

Phase 1 的核心区域：

```text
┌────────────────────────────────────────────┐
│ root@api-server:~                    +  ⋯  │
├────────────────────────────────────────────┤
│                                            │
│ Terminal Output                            │
│                                            │
│ root@api-server:~#                         │
│                                            │
└────────────────────────────────────────────┘
```

Toolbar 只保留高频动作：

```text
Copy
Paste
Find
More
```

Terminal Background 推荐 `#07111B` / `#08111D`。不得使用复杂背景图。允许极淡的 dot/grid decoration，但透明度不超过 3–5%。

Prompt Accent 可使用 Runory Teal/Green，ANSI Output 必须忠实显示。

---

## 18. Terminal Status Bar

高度 `28–32px`。

```text
● Connected   SSH   20ms   xterm-256color                  120 × 34
```

未来可以增加 Encoding、SFTP、Tunnel，但不得过载。

---

## 19. Context Dock

Main Workspace 下方支持可调高度 Dock。

未来可以容纳：

```text
Files
Terminal
Command History
Port Forwarding
Environment Variables
```

### Phase 1

默认 Hidden，保持 Terminal 为主。

### Phase 2

开启 Files。

### Future

逐步加入 Command History、Port Forwarding、Environment Variables。

Terminal 与 Context Dock 之间使用 Horizontal Splitter：

- Dock 最小 180px
- Terminal 最小 220px
- Double Click 恢复默认高度
- Pane 高度可持久化到 Layout Preference

---

## 20. SFTP Files View — Phase 2

采用 `Tree + File List`：

```text
┌──────────────────────────────────────────────────────┐
│ /home/www/api ▼             Refresh   Upload  New ⋯ │
├────────────────┬─────────────────────────────────────┤
│ /              │ Name        Size  Modified  Perm   │
│ ├ home         │ app         —     ...       ...    │
│ ├ var          │ config      —     ...       ...    │
│ └ etc          │ .env        2KB   ...       ...    │
│                │ README.md   4KB   ...       ...    │
└────────────────┴─────────────────────────────────────┘
```

Tree：`160–240px`。

File List 列：

```text
Name
Size
Type
Modified
Permissions
Actions
```

Toolbar：

```text
Breadcrumb / Path
Refresh
Upload
New ▼
More
```

New：

```text
New File
New Folder
```

单击 Select；双击 Folder Open；Text File 在未来 Editor 阶段打开。

---

## 21. Right Context Inspector

默认宽度：`300px`，范围 `260–420px`，可 Collapse。

Inspector 始终展示当前 Active Context，而不是永久 Dashboard。

概念区块：

```text
Connection Status
Information
Tags
System Overview
Service Status
Quick Actions
```

### Phase 1 Inspector

只显示：

```text
API Server
● Connected
Host
Port
Username
Auth
Last Connected

Quick Actions
- New Terminal
- Disconnect
- Edit Connection
```

### Phase 3 Inspector

Monitor 完成后才加入：

```text
CPU
Memory
Disk
Load
```

使用 `Value + Subtitle + Sparkline`，不要巨大 Dashboard Chart。

### Service Status

未来真实数据示例：

```text
nginx        ● Running
php-fpm      ● Running
mysql        ● Running
redis        ● Running
supervisor   ● Error
```

不得使用假数据长期装饰界面。

---

## 22. Quick Actions

Phase 1：

```text
New Terminal
Disconnect
Edit Connection
```

Phase 2 增加：

```text
File Manager
```

Future：

```text
Port Forward
Deploy
```

Primary Quick Actions 最多 4 个，其余进入 More Actions。

---

## 23. Information Density

Runory 允许高信息密度，但视觉优先级必须是：

```text
1. Main Workspace
2. Resource Explorer
3. Context Inspector
4. Primary Rail
```

四栏不得同时使用高对比和强颜色。

---

## 24. Pane Resize / Toggle / Focus Mode

支持：

- Explorer Resize
- Inspector Resize
- Terminal/Dock Vertical Resize
- Hide Explorer
- Hide Inspector
- Hide Context Dock

建议：

```text
Ctrl/Cmd + B          Explorer
Ctrl/Cmd + Shift+B    Inspector
```

快捷键最终必须集中注册。

可提供 Terminal Focus Mode：隐藏 Explorer、Inspector、Dock，只保留 Tabs、Terminal 和 Status。

Pane Size 保存到 Layout Preferences，不得写入 `ServerProfile`。

---

## 25. Session UX

建议：

```text
Single Click Host     Select
Double Click Host     Connect/Open
Enter                 Connect/Open
```

已连接主机 Double Click → Activate Session。

避免单击列表时误连生产服务器。

Host、Tab、Inspector 的状态必须来自同一 `ServerSessionManager`，不能出现多个区域状态不一致。

---

## 26. Connection Dialog

继续采用 Vertical Form，建议 `560–640px`。

```text
Connection Name
Host
Port
Username
Group
Authentication
Credential
Remember Securely

[Test Connection]
Cancel
Save
Save & Connect
```

多语言场景不要采用固定 Label Width。

---

## 27. Host Key Dialog

安全关键界面。

Unknown Host：

```text
Host
Key Type
SHA256 Fingerprint

Trust Once
Trust & Remember
Cancel
```

Changed Host Key：

```text
Block Connection
```

严禁 Ignore and Connect。

Fingerprint 使用 Monospace。

---

## 28. Notifications / Toast

Top Bar Notification 用于：

- Connection Lost
- Transfer Completed / Failed
- Security Warning
- Future Deployment Events

普通操作成功使用 Toast。

Toast 保持一个固定位置，建议 Bottom Right。

---

## 29. Settings

可作为 Workspace Tab 或独立 Settings View。

Sections：

```text
General
Appearance
Terminal
Connections
Security
Shortcuts
About
```

Phase 1：

- Language
- System / Light / Dark
- Terminal Font / Size / Cursor / Scrollback
- Vault
- Trusted Hosts
- Version

---

## 30. i18n

第一阶段：

```text
简体中文
English
```

所有用户可见 UI Text 必须走 i18n。

不翻译：

- Host Name
- Group Name
- Tag
- Username
- Path
- File Name
- Terminal Content

系统虚拟名称要翻译：

- Ungrouped
- Recent Connections
- Favorites
- Settings

Inspector 等布局必须使用 Flex/Grid，不得按中文长度硬编码 Label Width。

---

## 31. Accessibility

必须支持：

- Keyboard Navigation
- Visible Focus Ring
- aria-label
- Tooltip
- Sufficient Contrast
- Reduced Motion
- No color-only state

---

## 32. Motion

科技感来自精度，不来自动画堆叠。

推荐 Transition：`120–180ms`。

只用于：

- Pane Collapse
- Dialog
- Hover
- Tab Transition

禁止：

- Terminal Glow Animation
- Everywhere Pulsing Border
- Large Spring
- Background Particle Animation

---

## 33. Table Style

Files、Processes、Deployment History 使用统一 Data Table Language。

```text
Header      12px / muted / semibold
Row         32–38px
Selected    accent-soft
Hover       surface-2
```

避免每一行都画强边框。

---

## 34. Loading / Empty / Error

### Empty

```text
No servers yet
Create your first SSH connection.
[New Connection]
```

核心工作区不要使用巨大插画。

### Loading

SSH Connecting 状态同时体现在 Host、Tab、Inspector，不使用 Full-screen Spinner。

Files 使用 Skeleton Rows。

### Error

```text
Authentication failed
Check the username, password, or private key.
Technical Details ▸
```

Technical Details 必须 Sanitized。

---

## 35. Scrollbars

使用细 Scrollbar：`6–8px`。

默认弱化，Hover 提高对比。Terminal Scrollbar 与 App Scrollbar 可以独立 Theme。

---

## 36. Desktop Size / DPI

最低建议窗口：`1024 × 680`。推荐：`1440 × 900+`。

1024 宽时：

- Inspector 默认 Collapse
- Explorer 可缩小
- Context Dock 可隐藏

必须检查：

```text
Windows 100%
Windows 125%
Windows 150%
macOS Retina
Linux fractional scaling
```

图标使用 Vector。

---

## 37. Cross-platform Window Chrome

### Windows

正确处理 Minimize / Maximize / Close；Drag Region 不得覆盖 Interactive Controls。

### macOS

正确处理 Traffic Lights，并避免与 Logo / Toolbar 冲突。

### Linux

避免依赖单一 Window Decoration Behavior。

---

## 38. Mobile Relationship

Desktop v3 不能直接缩小到 Mobile。

Mobile 使用：

```text
Hosts
→ Server
→ Terminal / Files
```

共享：

- VI
- Color Tokens
- Typography
- Domain
- Rust Core

不共享 Desktop Multi-pane Layout。

---

## 39. Feature Phase Mapping

| UI 模块 | Phase |
|---|---:|
| Servers | 1 |
| Groups | 1 |
| Sessions | 1 |
| Terminal | 1 |
| Inspector Basic Info | 1 |
| Favorites | 1.5 / optional |
| Files / SFTP | 2 |
| File Tree / Transfer | 2 |
| Monitor | 3 |
| Resource Sparklines | 3 |
| Service Status | 3/4 |
| Port Forwarding | Future / 未排期 |
| Deploy | 5 |
| AI Terminal / 有限 AI Plan | 7/8（current historical phases） |
| Read-only Server Doctor / Timeline | 10B（future） |
| Diagnosis / Evidence | 10B（future） |
| ChangeSet / Diff / Approval | 10C（future） |
| MCP Connections | 10E（future） |
| Incident Operations Packs | 10H（implemented） |
| Tags | Optional later |

如果对应 Phase 尚未完成，不显示不可用的主导航入口，也不要通过 Mock Data 假装功能已存在。

---

## 40. UI v3 历史实施顺序与当前边界

以下是 Desktop UI v3 的历史实施顺序；当前仓库已经存在对应 Shell、Rail、Explorer、Workspace、Inspector、Files 与 Monitor 视图，不能再把它当作要求重写现有 UI 的待办：

```text
P0
- Desktop Shell v3
- Primary Navigation Rail
- Resource Explorer
- Workspace Tabs
- Server Terminal Workspace
- Right Inspector Basic
- Pane Resize / Collapse

P1
- SFTP Context Dock
- Files Tree / List

P2
- Monitor Inspector
```

下一轮 UI 工作只应配合明确批准的功能做增量改进。完整 Agentic Workspace UI 必须等 Phase 10 的只读 Runtime 和结构化事件真实存在后再开放，不得先展示伪 Timeline、Diagnosis、ChangeSet 或 MCP 入口。

Future Agentic UI 的实施顺序为：

```text
P2 / Phase 10B
- Agent Read-only Workspace / Activity Timeline
- Diagnosis / Evidence UI

P3 / Phase 10C
- ChangeSet Review
- Diff / Risk / Verification / Rollback Plan
- Version-bound Approval

Later / Phase 10E
- MCP Connection and Permission Review

Phase 10H
- Incident Operations Pack selector
- Investigation Timeline / Evidence / Root Cause
- Repair Plan / exact ChangeSet binding / Verification

Phase 10I
- Local Incident History and metadata-only recovery notice
- Cross-target scalar comparison with explicit drift state
- Operator Handoff / guarded Closure controls

Phase 10K
- Agent activity header shows bounded token use, cache hits, and run duration
- Budget-exceeded / cancelled / timed-out remain explicit terminal run states
- Context compaction and cache internals remain inspectable metrics, not raw remote content dumps
- Sanitized Audit Export presentation

Phase 10J
- No new production UI; qualification and fault controls remain test-only
- Existing Incident UI continues to show only validated repository state
```

这些入口必须由真实结构化状态驱动；不得用 Mock Deploy、AI 或 MCP 数据误导用户。

---

## 41. 当前参考图

正式 Desktop UI v3 参考图：

```text
docs/assets/brand/runory-desktop-ui-reference-v3.png
```

它定义：

- Overall Information Architecture
- Multi-pane Layout
- Navigation Hierarchy
- Terminal / Files Relationship
- Inspector Position
- Runory VI v2 Application
- Dark-mode Density

它不是逐像素实现稿。实际开发优先保证 Usability、Security、Responsive Layout 和真实 Feature Scope。

---

## 42. Design Review Checklist

### Brand

- [ ] 使用 Runory VI v2
- [ ] Logo 正确
- [ ] Brand Color 克制

### Layout

- [ ] Rail / Explorer / Workspace / Inspector 层级明确
- [ ] Pane Resize 正常
- [ ] 小窗口不溢出
- [ ] Focus Mode 可用

### Terminal

- [ ] Terminal 是核心视觉
- [ ] Decorative UI 不影响 Readability
- [ ] Resize 正常
- [ ] Focus 正确

### i18n

- [ ] zh-CN
- [ ] en-US
- [ ] 无硬编码 UI String
- [ ] 英文无截断

### Accessibility

- [ ] Keyboard
- [ ] Focus Ring
- [ ] Tooltip
- [ ] Contrast

### Scope

- [ ] 不提前展示未实现 Phase 功能
- [ ] 不使用 Mock Metrics 假装 Monitor 已实现
- [ ] 不使用 Mock Deploy / AI 入口误导用户

---

## 43. 最终设计原则

Runory Desktop UI v3 的核心是：

> **左侧组织基础设施，中间完成工作，右侧理解上下文。**

```text
Primary Navigation Rail
        ↓
产品模块

Resource Explorer
        ↓
服务器与资源

Main Workspace
        ↓
Terminal / Files / Operations

Context Inspector
        ↓
当前对象状态与快捷操作
```

Runory 应看起来像一款真正可以每天使用 8 小时的专业基础设施工具，而不是一张只适合展示的概念图。
