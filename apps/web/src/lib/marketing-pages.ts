import type { Metadata } from "next";
import type { Locale } from "./i18n";

export const siteUrl = "https://runory.app";
export const marketingSlugs = ["product", "ssh", "operations", "ai", "security", "pricing", "download"] as const;
export type MarketingSlug = (typeof marketingSlugs)[number];

const zh = {
  common: {
    home: "首页", explore: "继续探索", capabilities: "核心能力", details: "深入了解",
    related: "接下来可以了解", ctaPrimary: "下载 Runory", ctaSecondary: "查看产品总览",
    faq: "常见问题", breadcrumb: "面包屑导航",
  },
  pages: {
    product: {
      label: "产品总览", eyebrow: "RUNORY 产品总览", title: "一个工作区，连接并管理你的服务器。",
      lead: "Runory 是本地优先的跨平台 SSH 与基础设施管理客户端。它把终端、远程文件、服务器状态、常用服务、部署和受控 AI 放进同一个上下文，减少工具切换，也让每一步操作更容易理解和复查。",
      meta: { title: "Runory 产品总览｜SSH、SFTP、服务器运维与部署客户端", description: "全面了解 Runory：跨平台 SSH、SFTP 文件管理、服务器监控、Docker、PM2、Nginx、Git 部署和逐条审批的 AI 协作。", keywords: ["SSH 客户端", "服务器管理工具", "SFTP 客户端", "Linux 运维工具", "Runory"] },
      signal: [["连接", "SSH + 多会话"], ["文件", "独立 SFTP Channel"], ["运维", "服务、日志与部署"]],
      highlights: [
        ["以连接为中心", "ServerSession 是终端、文件和运维操作的共同上下文。切换视图时，无需重新理解目标服务器。"],
        ["桌面优先，跨平台设计", "面向 Windows、macOS 与 Linux 的桌面体验，同时保持 iOS、Android 和移动密钥导入所需的核心边界。"],
        ["安全操作，而非黑盒自动化", "凭据留在本地；主机密钥必须验证；AI 命令逐条展示、逐条审批，并根据真实结果继续分析。"],
      ],
      sections: [
        { title: "从 SSH 会话开始，把相关工作放在一起", body: ["日常远程工作往往分散在终端、文件传输、监控面板和部署脚本之间。Runory 围绕同一个服务器会话组织这些能力：在终端执行命令，在文件视图处理远程目录，在 Dashboard 查看资源，在运维和部署视图处理服务。", "主机分组、搜索和多标签会话帮助你区分生产、预发与开发环境。连接断开后可以明确地重新连接，终端输出只流向 xterm，不被保存为应用状态。"], bullets: ["密码、私钥与加密私钥认证", "多会话、终端搜索、复制粘贴与 Resize", "中文与英文界面、深色与浅色主题"] },
        { title: "让状态、动作和结果彼此相连", body: ["Runory 的 Dashboard 与运维能力通过已有 SSH 连接获取信息，无需在服务器额外安装 Runory Agent。Docker、PM2、Nginx、日志、Git 部署、SSL、备份和 Cron 都围绕明确的业务动作组织。", "部署过程保留阶段和历史，便于判断失败发生在哪里。结构化写入通过审批、验证和可用时的回滚计划约束，避免把一次点击包装成不可解释的远程执行。"] },
        { title: "云同步保持可选", body: ["不登录账号也可以使用本地主机管理和 SSH 工作区。选择云同步后，主机与分组清单会在 Rust Core 中加密，再上传密文副本。密码、Passphrase、私钥、Known Hosts 和终端输出不进入同步数据。", "同步合并先展示冲突，团队空间通过组织、角色与访问策略约束。服务器连接仍由本地客户端建立，网页端不解密你的服务器清单。"] },
      ],
      faq: [["Runory 与普通 SSH 客户端有什么不同？", "它保留完整终端体验，同时把 SFTP、状态查看、服务运维、部署和受控 AI 放在同一个 ServerSession 上下文中。"], ["是否必须使用云端？", "不需要。核心连接和工作区默认在本地运行，云同步是可选服务。"], ["服务器需要安装 Runory Agent 吗？", "SSH、SFTP 与基础状态查看不需要。相关运维功能依赖服务器已有的软件和用户权限。"]],
    },
    ssh: {
      label: "SSH 与 SFTP", eyebrow: "SSH 与远程文件", title: "可靠连接，顺手操作远程文件。",
      lead: "用主机分组和多会话管理你的服务器，通过交互式 PTY 完成日常工作，并在同一连接中打开独立的 SFTP Channel。Runory 对未知主机要求显式信任，对发生变化的主机密钥直接阻止连接。",
      meta: { title: "跨平台 SSH 与 SFTP 客户端｜Runory", description: "Runory 提供 Windows、macOS、Linux SSH 客户端体验：密码和私钥认证、主机密钥验证、多会话终端、SFTP 文件管理与传输队列。", keywords: ["SSH 客户端", "SFTP 客户端", "Windows SSH", "macOS SSH", "Linux SSH 管理"] },
      signal: [["认证", "密码 / 私钥"], ["终端", "交互式 PTY"], ["验证", "Host Key 必选"]],
      highlights: [["主机资料与凭据分离", "ServerProfile 只保存显示和连接元数据；密码、Passphrase 与私钥内容由本地 Credential Vault 管理。"], ["多标签会话", "同时处理多台服务器，清晰查看连接、认证、打开 Shell、已连接或错误等完整会话状态。"], ["文件传输不打断终端", "SFTP 使用同一认证 Transport 上的独立 Channel，支持浏览、上传、下载、创建目录、重命名与删除。"]],
      sections: [
        { title: "为经常连接的服务器建立秩序", body: ["用自定义分组整理主机，搜索名称、地址或标签，快速进入需要的环境。认证支持密码、文件私钥和加密私钥 Passphrase；移动端使用不透明的 key_id 表达导入密钥，不假设任意桌面路径都可访问。", "连接流程使用单一状态机描述 idle、connecting、verifying-host、authenticating、opening-shell、connected、disconnected 和 error，界面可以准确告诉你正在发生什么。"] },
        { title: "把主机身份验证当作连接的一部分", body: ["遇到未知主机时，你可以 Trust Once、Trust & Remember 或取消。主机密钥发生变化时，Runory 会阻止连接，而不是静默接受。这个边界用于降低中间人攻击和误连服务器的风险。", "Known Hosts 和凭据都由 Rust Core 管理，React 界面无法绕过主机验证，也无法读取已经记住的密码明文。"] },
        { title: "在终端和文件之间自然切换", body: ["交互式 PTY 支持 Resize、复制、粘贴、搜索、ANSI、UTF-8 与中文输出。高频终端流通过 Tauri Channel 直接写入 xterm，不进入 Zustand，也不会持久化。", "文件视图支持面包屑导航、远程目录刷新、文件元数据和传输队列。终端与 SFTP 共享 ServerSession，但保持各自的 Channel 生命周期。"] },
      ],
      faq: [["是否支持加密私钥？", "支持，可在当前连接操作中提交 Passphrase，且不会写入 ServerProfile。"], ["主机密钥变化时能继续连接吗？", "不能直接继续。Runory 会阻止连接，需要你先独立核对并处理 Known Host 记录。"], ["SFTP 会新建一套登录吗？", "SFTP Channel 依附现有 ServerSession，在相同 SSH Transport 和认证上下文中工作。"]],
    },
    operations: {
      label: "运维与部署", eyebrow: "服务器运维与部署", title: "从查看状态，到完成一次可追踪的部署。",
      lead: "在同一服务器上下文中查看资源、服务与日志，管理 Docker、PM2 和 Nginx，再围绕 Git、环境变量、SSL、备份与 Cron 组织部署。每个动作都对应明确目标和结果。",
      meta: { title: "Linux 服务器运维与部署工具｜Runory", description: "通过 SSH 管理服务器状态、Docker、PM2、Nginx、日志、Git 部署、环境变量、SSL、备份和 Cron，并保留部署历史。", keywords: ["Linux 服务器管理", "Docker 管理工具", "Nginx 管理", "Git 自动部署", "服务器监控客户端"] },
      signal: [["状态", "CPU / 内存 / 磁盘"], ["服务", "Docker / PM2 / Nginx"], ["发布", "Git + 部署历史"]],
      highlights: [["Agentless Dashboard", "通过 SSH 读取服务器状态，不要求安装常驻的 Runory 监控 Agent。"], ["类型化运维动作", "服务、容器、日志和 Nginx 操作使用业务含义明确的接口，避免向前端暴露通用 Shell。"], ["部署历史", "记录阶段、状态与结果，让失败位置和后续验证更容易追踪。"]],
      sections: [
        { title: "先理解服务器，再决定动作", body: ["Dashboard 汇总 CPU、内存、磁盘和服务状态。日志视图帮助你在目标服务器的上下文中查看线索；Docker、PM2 与 Nginx 面板把高频操作放到熟悉的位置。", "这些功能建立在现有 SSH 与 ServerSession 上。React 负责交互和展示，Rust 负责远程协议、命令边界与结果处理。"] },
        { title: "围绕应用组织部署", body: ["Git 部署可以串联仓库、分支、环境变量、构建和启动步骤。SSL、备份和 Cron 作为相关但独立的运维能力呈现，部署历史记录每次运行的阶段与结果。", "当执行涉及写入时，系统需要明确审批。结构化 ChangeSet 绑定目标与版本，验证使用真实只读检查；只有真正实现回滚的操作才会显示回滚能力。"] },
        { title: "多服务器操作保持边界", body: ["Fleet 写入默认按目标顺序执行，并在关键节点暂停复核。审批绑定精确的目标列表和各目标的 ChangeSet 版本，任何目标或版本变化都会使审批失效。", "不同服务器的执行、验证和回滚状态分别记录。单个目标失败不会自动扩大处理范围，也不会把生产环境的全并行写入当作快捷方式。"] },
      ],
      faq: [["需要在服务器安装监控 Agent 吗？", "基础 Dashboard 通过 SSH 工作，不需要安装 Runory Agent。"], ["Runory 会自动删除磁盘文件吗？", "不会。磁盘运维包不会生成或执行自动删除动作。"], ["部署失败后会自动回滚吗？", "只有已真实实现且经过审批的回滚才可执行；产品不会在不支持时声称可以回滚。"]],
    },
    ai: {
      label: "受控 AI", eyebrow: "AI TERMINAL 与 AGENT RUNTIME", title: "让 AI 协助排查，但不替你接管服务器。",
      lead: "Runory 的对话式 Agent 围绕真实终端结果迭代：理解问题、提出一条命令、等待你审批、在绑定会话中执行、读取脱敏结果，再决定下一步。每一条命令都由你 Run 或 Cancel。",
      meta: { title: "安全可控的 AI SSH Terminal｜Runory", description: "Runory AI Terminal 逐条展示 Linux 命令、风险与原因，经用户审批后在真实 SSH 会话执行，再根据脱敏结果继续分析和验证。", keywords: ["AI SSH Terminal", "AI Linux 运维", "服务器 AI 助手", "命令审批", "Runory Agent"] },
      signal: [["推理", "一次提出一条命令"], ["控制", "每条命令都审批"], ["证据", "基于真实执行结果"]],
      highlights: [["命令原样展示", "执行前看到完整命令、执行原因，以及由 Rust 判定的风险和可变更性。"], ["终端始终可见", "批准的命令由 Rust 写入绑定的交互式 Terminal；原始输出只流向 xterm。"], ["写入后必须验证", "写入或未知命令完成后，需要再次审批成功的只读验证命令，Agent 才能给出最终结论。"]],
      sections: [
        { title: "一次只处理一个明确动作", body: ["Agent Runtime 采用 Reason → Command Proposal → Approval → Terminal → Observation → Reason 的循环。模型每轮最多提出一条非交互式 Linux 命令，避免用一串预生成命令掩盖中间结果。", "命令失败、被策略阻止或被你取消都会成为新的 Observation。Agent 可以据此解释问题或寻找替代方案，不会把取消操作伪装成执行失败。"] },
        { title: "风险与权限由 Rust 决定", body: ["模型给出的风险声明没有权限意义。Rust Core 对命令进行保守分类，Critical 命令会被阻止；审批精确绑定 command hash、run、target、session 与 policy snapshot。任何一项变化都使旧审批失效。", "React 只渲染 AgentEvent，并发送 Run 或 Cancel 等用户动作。它不拥有调度逻辑、Shell 执行、SSH Handle、PTY 注入或恢复流程。"] },
        { title: "用证据回答，而不是展示隐藏推理", body: ["原始终端流只显示在主 Terminal。进入 Agent 上下文的是 Rust 内部有界、脱敏的 Observation，单条命令卡最多携带 8 KiB 的结果预览。", "界面展示简洁的进度、动作、证据、诊断和最终答案，不展示或持久化模型的 private chain-of-thought。凭据、私钥、Passphrase 与 Vault Secret 不进入模型上下文或审计。"] },
      ],
      faq: [["AI 会自动运行只读命令吗？", "不会。Runtime V2 中包括只读命令在内的每条命令都需要明确审批。"], ["能否一次批准整个命令计划？", "对话主链不使用固定未来命令队列。每轮基于上一条真实结果再提出下一条命令。"], ["终端输出会上传到网页吗？", "不会。网页不接收 SSH 终端流；客户端中的 Agent 仅使用 Rust 生成的有界脱敏 Observation。"]],
    },
    security: {
      label: "安全架构", eyebrow: "LOCAL-FIRST 安全架构", title: "明确谁能访问凭据，谁能执行操作。",
      lead: "Runory 把安全边界落实在架构里：React 是交互层，Rust 是基础设施引擎。SSH、TCP、凭据、私钥解析、主机验证、远程文件和进程执行都不交给前端。",
      meta: { title: "Runory 安全架构｜本地凭据、主机验证与操作审批", description: "了解 Runory 如何保护 SSH 凭据：本地 Credential Vault、强制 Host Key Verification、最小权限 IPC、命令审批、验证与无秘密云同步。", keywords: ["SSH 安全", "Host Key Verification", "本地密码保险库", "Local-first 安全", "服务器操作审批"] },
      signal: [["凭据", "只在本地 Vault"], ["连接", "强制主机验证"], ["权限", "业务专用 IPC"]],
      highlights: [["秘密不进入主机资料", "ServerProfile 永远不保存 password、passphrase、privateKeyContent 或 vaultMasterSecret。"], ["未知与变化是两种状态", "未知主机可以临时或永久信任；已知主机密钥变化会直接阻止连接。"], ["远程内容默认不可信", "远程文件、终端输出、HTTP Body 与 MCP Output 都按 Untrusted Data 处理。"]],
      sections: [
        { title: "凭据的生命周期停留在 Rust Core", body: ["CredentialService 通过 CredentialVault 保存凭据。界面没有读取已记住密码明文的 API；临时秘密只在用户当前操作时一次性提交。日志可以记录 profile_id、session_id、host、port 和稳定错误码，但不能记录密码、Passphrase、私钥或 Vault Key。", "私钥同时支持 File 与 Vault 概念。桌面端可以引用文件，移动端使用导入后的不透明 key_id，避免把桌面路径假设带到 iOS 或 Android。"] },
        { title: "连接身份必须核对", body: ["Runory 不提供 accept_all_hosts、skip_host_verification 或 StrictHostChecking=no 之类的绕过入口。未知主机要求 Trust Once、Trust & Remember 或 Cancel；密钥变化则阻止连接。", "这套验证位于 Rust 的 KnownHostService，而不是依赖前端是否正确显示某个警告。"] },
        { title: "审批、验证与审计互相约束", body: ["AI 命令审批绑定精确命令和会话。结构化高风险操作绑定 ChangeSet Version，并要求 Verification。ChangeSet、Incident 与 Audit 只持久化必要的无内容元数据，重启后旧审批失效，不会自动续跑。", "Tauri 使用最小权限和业务专用 Command。终端输出通过 Channel 流向 xterm，既不经过通用事件广播，也不进入 React State 或持久化存储。"] },
      ],
      faq: [["网页端能读取服务器资料吗？", "不能。可选云同步保存的是本地加密后的清单密文，网页不解密服务器资料。"], ["Runory 会记录终端输出吗？", "不会持久化终端输出；原始流只通过 Tauri Channel 写入 xterm。"], ["插件或 Skill 能提升权限吗？", "不能。Skill、MCP 与模型都必须经过统一 Tool Registry、Policy 与审批边界。"]],
    },
    pricing: {
      label: "价格方案", eyebrow: "RUNORY PRICING", title: "本地工作区永久免费，按需扩展云与 AI。",
      lead: "无需账号即可使用 SSH、SFTP、多会话和本地运维。需要跨设备同步、托管 AI 或团队协作时，再选择对应方案。以下价格来自 Runory 当前的 Billing 配置；付费通道尚未开放。",
      meta: { title: "Runory 价格方案｜免费 SSH 客户端、Pro 与团队版", description: "查看 Runory Free、Pro、Team 与 Business 方案价格和功能：免费本地 SSH 与 SFTP、加密同步、托管 AI、团队空间、角色和审计。", keywords: ["Runory 价格", "免费 SSH 客户端", "SSH 客户端价格", "AI 运维价格", "服务器管理工具价格"] },
      signal: [["Free", "$0 · 永久"], ["Pro", "$12 · 月付"], ["团队", "按用户计费"]],
      highlights: [["本地能力无需付费", "Free 包含本地 SSH、凭据保险库、SFTP、多会话和本地运维，不要求登录账号。"], ["年付价格更低", "Pro 年付折合每月 $10；Team 与 Business 年付每用户每月分别为 $20 和 $30。"], ["消费边界清晰", "托管 AI 按真实 Token 使用量消耗 Credits；账本只记录用量和金额元数据，不保存 Prompt 或终端内容。"]],
      plans: {
        monthly: "月付", annual: "年付折合", perMonth: "/ 月", perUserMonth: "/ 用户 / 月", freeForever: "永久免费", recommended: "推荐", unavailable: "付费订阅即将开放", download: "下载免费版", included: "包含能力", credits: "每月 {{count}} Credits", noCredits: "不含托管 AI Credits", note: "美元计价。年付金额按月折算；Team 与 Business 按席位计费。当前支付通道尚未配置。",
        items: [
          { code: "free", name: "Free", description: "适合在一台设备上管理个人服务器。", monthlyPrice: "$0", annualPrice: "$0", perSeat: false, credits: "0", featured: false, features: ["本地 SSH 与多会话", "本地加密凭据保险库", "SFTP 文件管理", "本地运维能力"] },
          { code: "pro", name: "Pro", description: "适合需要跨设备工作和托管 AI 的个人用户。", monthlyPrice: "$12", annualPrice: "$10", perSeat: false, credits: "10,000", featured: true, features: ["加密桌面与移动同步", "本地加密凭据保险库", "Runory 托管 AI", "Credits 与 AI 用量历史"] },
          { code: "team", name: "Team", description: "适合共同管理基础设施的小型团队。", monthlyPrice: "$24", annualPrice: "$20", perSeat: true, credits: "25,000", featured: false, features: ["包含 Pro 全部能力", "团队工作区与成员角色", "团队共享 Credits", "14 天试用基础"] },
          { code: "business", name: "Business", description: "适合需要更高 AI 配额和治理能力的组织。", monthlyPrice: "$36", annualPrice: "$30", perSeat: true, credits: "50,000", featured: false, features: ["包含 Team 全部能力", "基础设施访问策略", "组织审计记录", "更高托管 AI 配额"] },
        ],
      },
      sections: [
        { title: "Free 不是限时试用", body: ["Free 方案没有使用期限。你可以在本地创建主机资料、使用密码或私钥连接、打开多个终端会话，并在同一 ServerSession 中使用 SFTP 与本地运维能力。", "本地连接不要求 Runory 账号。密码、Passphrase 与私钥仍由设备上的 Credential Vault 管理，不会为了使用免费版而上传。"] },
        { title: "Pro 把跨设备与托管 AI 加进工作区", body: ["Pro 包含加密同步、Runory 托管 AI 和用量历史，每月包含 10,000 Credits。同步数据在 Rust Core 中加密后上传；凭据、Known Hosts 和终端输出不参与同步。", "托管 AI 根据实际 Token 使用量扣减 Credits。产品中的额度账本只保留计算和结算所需的元数据，不保留 Prompt 或终端正文。"] },
        { title: "Team 与 Business 围绕组织协作计费", body: ["Team 和 Business 按席位计费，并为团队工作区提供共享额度。Team 增加成员角色和统一额度；Business 进一步增加访问策略、组织审计与更高 AI 配额。", "仓库已经建立 14 天团队试用和 Billing 基础，但当前支付通道未配置。官网不会显示可用的结账按钮；正式开放前，套餐、额度或价格仍可能调整。"] },
      ],
      faq: [["Free 方案真的可以一直使用吗？", "可以。Free 的本地 SSH、SFTP、多会话、凭据保险库和本地运维没有到期时间，也不要求登录。"], ["年付价格如何计算？", "页面显示的是年付后的每月折合价：Pro $10，Team 每用户 $20，Business 每用户 $30。实际年付总额会按 12 个月计算。"], ["现在可以购买 Pro、Team 或 Business 吗？", "暂时不可以。Billing 方案与试用基础已经建立，但支付通道尚未配置；正式开放后会在客户端提供可用入口。"]],
    },
    download: {
      label: "下载与安装", eyebrow: "下载 RUNORY", title: "在你的桌面上开始第一段连接。",
      lead: "Runory 桌面客户端面向 Windows、macOS 与 Linux。前往 GitHub Releases 查看当前实际提供的安装包、芯片架构与版本；安装后添加服务器，核对主机指纹，即可建立 SSH 会话。",
      meta: { title: "下载 Runory｜Windows、macOS、Linux SSH 客户端", description: "下载 Runory 桌面 SSH 与服务器管理客户端。查看 Windows、macOS、Linux 最新发布包、安装格式与首次连接步骤。", keywords: ["下载 SSH 客户端", "Windows SSH 客户端下载", "macOS SSH 客户端", "Linux SFTP 客户端", "Runory 下载"] },
      signal: [["Windows", ".msi / .exe"], ["macOS", ".dmg"], ["Linux", ".AppImage / .deb"]],
      highlights: [["按发布页选择安装包", "不同版本可能提供不同平台与架构，GitHub Releases 是可用文件的准确来源。"], ["无需账号即可开始", "本地主机管理与 SSH 工作区不要求登录；可选云同步才需要 Runory 账号。"], ["移动端基础已经建立", "iOS 与 Android 已具备自适应界面和移动密钥能力，但应用商店下载尚未开放。"]],
      sections: [
        { title: "选择与你的系统匹配的格式", body: ["Windows 用户可在发布页查找 .msi 或 .exe；macOS 用户需要选择匹配 Apple Silicon 或 Intel 架构的 .dmg；Linux 用户可根据发行版选择 .AppImage 或 .deb。具体文件以每个 Release 的 Assets 列表为准。", "下载后按照操作系统的常规方式安装。若系统显示发布者或安全提示，请核对下载来源确实是 Runory 官方 GitHub 仓库，再决定是否继续。"] },
        { title: "完成第一次 SSH 连接", body: ["打开 Runory 后，添加服务器名称、地址、端口、用户名与认证方式。密码和 Passphrase 不写入 ServerProfile；选择记住凭据时，由本地 Credential Vault 管理。", "首次连接会显示主机指纹。请通过可信渠道与服务器管理员或云平台控制台提供的指纹核对，然后选择仅信任本次或信任并记住。"] },
        { title: "按需开启更多能力", body: ["连接建立后，可以从终端开始，再使用 Files、Dashboard、Operations 与 Deployment。AI 功能需要在客户端完成对应模型配置，并且每条命令仍需审批。", "如需跨设备同步主机与分组清单，可登录账号后主动启用可选云同步。凭据、Known Hosts 与终端输出不会进入同步数据。"] },
      ],
      faq: [["哪个页面提供最新安装包？", "GitHub Releases 页面列出当前实际发布的版本、平台、架构和文件。"], ["使用前必须创建账号吗？", "不需要。本地连接和主机管理可以直接使用。"], ["可以从应用商店下载移动版吗？", "目前不可以。移动端能力仍在产品演进中，尚未开放应用商店下载。"]],
    },
  },
};

type CopyShape<T> = T extends string ? string : T extends readonly (infer U)[] ? CopyShape<U>[] : { [K in keyof T]: CopyShape<T[K]> };
const en: CopyShape<typeof zh> = {
  common: { home: "Home", explore: "Keep exploring", capabilities: "Core capabilities", details: "In detail", related: "Explore next", ctaPrimary: "Download Runory", ctaSecondary: "View product overview", faq: "Frequently asked questions", breadcrumb: "Breadcrumb" },
  pages: {
    product: {
      label: "Product overview", eyebrow: "RUNORY PRODUCT OVERVIEW", title: "One workspace to connect to and manage your servers.",
      lead: "Runory is a local-first, cross-platform SSH and infrastructure management client. It keeps terminals, remote files, server health, common services, deployments and controlled AI in the same context, reducing tool switching while making each operation easier to understand and review.",
      meta: { title: "Runory Product Overview | SSH, SFTP and Server Operations", description: "Explore Runory: cross-platform SSH, SFTP file management, server monitoring, Docker, PM2, Nginx, Git deployments and AI with per-command approval.", keywords: ["SSH client", "server management tool", "SFTP client", "Linux operations tool", "Runory"] },
      signal: [["Connect", "SSH + multiple sessions"], ["Files", "Independent SFTP Channel"], ["Operate", "Services, logs and deploys"]],
      highlights: [["Built around the connection", "ServerSession provides the shared context for terminals, files and operations, so switching views does not mean rediscovering the target server."], ["Desktop-first and cross-platform", "A focused Windows, macOS and Linux experience with core boundaries that also support iOS, Android and imported mobile keys."], ["Controlled operations, not black-box automation", "Credentials stay local, host keys must be verified, and each AI command is shown and approved before real execution."]],
      sections: [
        { title: "Start with SSH and keep related work together", body: ["Remote work is often scattered across a terminal, file-transfer client, monitoring page and deployment scripts. Runory organizes those jobs around one server session: run commands in Terminal, work with directories in Files, review resources in Dashboard, then handle services and deployments without losing context.", "Host groups, search and multiple tabs separate production, staging and development. Connection state is explicit, while terminal output streams directly to xterm instead of becoming persisted application state."], bullets: ["Password, private-key and encrypted-key authentication", "Multiple sessions, terminal search, copy, paste and resize", "English and Chinese interfaces with light and dark themes"] },
        { title: "Connect status, action and evidence", body: ["Runory collects dashboard and operations data over the existing SSH connection, so it does not require a resident Runory monitoring agent. Docker, PM2, Nginx, logs, Git deployments, SSL, backups and Cron are organized as business-specific actions.", "Deployment stages and history make failures easier to locate. Structured writes use approval, verification and rollback plans where rollback is truly supported, rather than hiding remote execution behind a single button."] },
        { title: "Cloud sync remains optional", body: ["You can use local host management and SSH without an account. When you opt into sync, host and group inventories are encrypted inside the Rust Core before an encrypted copy is uploaded. Passwords, passphrases, private keys, known hosts and terminal output stay out of sync data.", "Conflicts are previewed before merging, and team spaces use organizations, roles and access policies. Server connections still originate in the local client; the web portal does not decrypt your server inventory."] },
      ],
      faq: [["How is Runory different from a regular SSH client?", "It preserves a full terminal while placing SFTP, health, service operations, deployment and controlled AI in the same ServerSession context."], ["Do I have to use the cloud?", "No. Core connections and the workspace run locally by default; cloud sync is optional."], ["Does my server need a Runory agent?", "No for SSH, SFTP and basic health. Operations features depend on the relevant software and permissions already present on the server."]],
    },
    ssh: {
      label: "SSH & SFTP", eyebrow: "SSH AND REMOTE FILES", title: "Reliable connections and remote files within reach.",
      lead: "Organize servers with host groups and multiple sessions, work in an interactive PTY, and open an independent SFTP Channel on the same connection. Runory requires explicit trust for unknown hosts and blocks connections when a known host key changes.",
      meta: { title: "Cross-Platform SSH and SFTP Client | Runory", description: "Runory brings password and key authentication, host-key verification, multiple terminal sessions, SFTP file management and transfer queues to Windows, macOS and Linux.", keywords: ["SSH client", "SFTP client", "Windows SSH client", "macOS SSH client", "Linux server manager"] },
      signal: [["Auth", "Password / private key"], ["Terminal", "Interactive PTY"], ["Verify", "Host key required"]],
      highlights: [["Profiles and credentials stay separate", "ServerProfile contains display and connection metadata only; the local Credential Vault owns passwords, passphrases and private-key content."], ["Multiple terminal tabs", "Work across several servers with explicit connecting, verifying, authenticating, shell-opening, connected and error states."], ["File transfers do not interrupt the terminal", "SFTP uses an independent Channel on the authenticated Transport for browsing, uploads, downloads, directories, renames and deletes."]],
      sections: [
        { title: "Bring order to the servers you use every day", body: ["Create custom groups, search names and addresses, and move quickly between environments. Authentication supports passwords, file-based keys and passphrases for encrypted keys. On mobile, imported keys use opaque key IDs instead of assuming arbitrary desktop paths are available.", "A single state model describes idle, connecting, verifying-host, authenticating, opening-shell, connected, disconnected and error, so the interface can tell you what is happening without conflicting indicators."] },
        { title: "Treat server identity as part of the connection", body: ["For an unknown host, choose Trust Once, Trust & Remember or Cancel. When an established host key changes, Runory blocks the connection instead of silently accepting it. This reduces the risk of man-in-the-middle attacks and accidental connections to the wrong machine.", "Known hosts and credentials live in the Rust Core. The React interface cannot bypass host verification or read a remembered password in plaintext."] },
        { title: "Move naturally between Terminal and Files", body: ["The interactive PTY supports resize, copy, paste, search, ANSI, UTF-8 and CJK output. High-volume output travels over a Tauri Channel straight to xterm, never through Zustand and never into persistent storage.", "Files provides breadcrumbs, remote directory refresh, metadata and a transfer queue. Terminal and SFTP share a ServerSession while retaining separate Channel lifecycles."] },
      ],
      faq: [["Does Runory support encrypted private keys?", "Yes. A passphrase can be submitted for the current connection without being stored in ServerProfile."], ["Can I continue when a host key changes?", "Not directly. Runory blocks the connection so you can independently verify and resolve the known-host record."], ["Does SFTP require another login?", "The SFTP Channel belongs to the existing ServerSession and uses the same SSH Transport and authentication context."]],
    },
    operations: {
      label: "Operations & deployment", eyebrow: "SERVER OPERATIONS AND DEPLOYMENT", title: "From server health to a traceable deployment.",
      lead: "Review resources, services and logs in the same server context. Operate Docker, PM2 and Nginx, then organize deployment around Git, environment variables, SSL, backups and Cron. Every action has a clear target and result.",
      meta: { title: "Linux Server Operations and Deployment Tool | Runory", description: "Manage server health, Docker, PM2, Nginx, logs, Git deployments, environment variables, SSL, backups and Cron over SSH with deployment history.", keywords: ["Linux server management", "Docker management tool", "Nginx manager", "Git deployment tool", "server monitoring client"] },
      signal: [["Health", "CPU / memory / disk"], ["Services", "Docker / PM2 / Nginx"], ["Release", "Git + deployment history"]],
      highlights: [["Agentless Dashboard", "Read server health over SSH without installing a resident Runory monitoring agent."], ["Typed operations", "Services, containers, logs and Nginx use business-specific interfaces rather than exposing a general shell to the frontend."], ["Deployment history", "Track stages, status and results so failures and subsequent verification are easier to follow."]],
      sections: [
        { title: "Understand the server before acting", body: ["Dashboard brings CPU, memory, disk and service status together. Logs keep evidence near the target server, while Docker, PM2 and Nginx panels place frequent operations in predictable locations.", "These features build on the established SSH connection and ServerSession. React presents interactions; Rust owns remote protocols, command boundaries and result handling."] },
        { title: "Organize deployment around the application", body: ["Git deployment can connect repository, branch, environment, build and startup stages. SSL, backups and Cron remain related but distinct operations, while deployment history records the stages and result of every run.", "Writes require explicit approval. Structured ChangeSets bind the target and version, verification uses a real read-only check, and rollback appears only where it has been implemented."] },
        { title: "Keep boundaries across multiple servers", body: ["Fleet writes run sequentially by default and pause for review at defined points. Approval binds the exact target list and each target's ChangeSet version; any change invalidates the approval.", "Execution, verification and rollback remain independently visible per server. A failure on one target does not expand scope, and production writes cannot use an unrestricted parallel-all shortcut."] },
      ],
      faq: [["Do I need a monitoring agent?", "No. The basic Dashboard works over SSH without installing a Runory agent."], ["Will Runory automatically delete files to free disk space?", "No. The Disk operations pack never generates or performs automatic deletion."], ["Does a failed deployment roll back automatically?", "Rollback is available only when it is truly implemented and explicitly approved; unsupported operations never claim rollback support."]],
    },
    ai: {
      label: "Controlled AI", eyebrow: "AI TERMINAL AND AGENT RUNTIME", title: "Let AI help investigate without taking over your server.",
      lead: "Runory's conversational Agent iterates on real terminal results: understand the problem, propose one command, wait for your approval, execute in the bound session, review a sanitized result, and then decide what comes next. You choose Run or Cancel for every command.",
      meta: { title: "Controlled AI SSH Terminal | Runory", description: "Runory AI Terminal shows each Linux command, reason and risk, executes only after approval in a real SSH session, then continues from sanitized evidence and verification.", keywords: ["AI SSH terminal", "AI Linux operations", "server AI assistant", "command approval", "Runory Agent"] },
      signal: [["Reason", "One command at a time"], ["Control", "Approval every time"], ["Evidence", "Real execution results"]],
      highlights: [["See the exact command", "Review the full command, why it is needed, and the risk and mutability assigned by Rust before execution."], ["Keep the terminal visible", "Rust writes an approved command to the bound interactive Terminal; raw output remains in xterm."], ["Verify after a write", "After a write or unknown command, a separately approved read-only verification must succeed before the Agent can finish."]],
      sections: [
        { title: "Handle one explicit action at a time", body: ["Agent Runtime follows a Reason → Command Proposal → Approval → Terminal → Observation → Reason loop. The model proposes at most one non-interactive Linux command per turn, so a pre-generated queue cannot hide what intermediate results mean.", "A failed command, policy block or user cancellation becomes another Observation. The Agent can explain it or seek an alternative without misrepresenting cancellation as infrastructure failure."] },
        { title: "Rust decides risk and permission", body: ["A model's risk claim grants no authority. Rust classifies commands conservatively and blocks Critical commands. Approval binds the exact command hash, run, target, session and policy snapshot; changing any one invalidates the old approval.", "React renders AgentEvent and sends user actions such as Run or Cancel. It does not own orchestration, shell execution, SSH handles, PTY injection or resume behavior."] },
        { title: "Answer from evidence without exposing hidden reasoning", body: ["Raw terminal output stays in the main Terminal. The Agent receives a bounded, sanitized Observation produced inside Rust, and a command card can carry no more than an 8 KiB preview.", "The interface shows concise progress, actions, evidence, diagnosis and a final answer. It does not expose or persist private chain-of-thought. Credentials, keys, passphrases and vault secrets never enter model context or audit data."] },
      ],
      faq: [["Does AI automatically run read-only commands?", "No. In Runtime V2, every command, including read-only commands, requires explicit approval."], ["Can I approve an entire command plan at once?", "The conversation loop does not use a fixed future queue. Each new command is proposed after reviewing the previous real result."], ["Is terminal output uploaded to the website?", "No. The web portal never receives SSH streams; the client Agent sees only a bounded, sanitized Observation from Rust."]],
    },
    security: {
      label: "Security architecture", eyebrow: "LOCAL-FIRST SECURITY ARCHITECTURE", title: "Make it clear who can access credentials and execute operations.",
      lead: "Runory encodes security boundaries in its architecture: React is the interaction layer and Rust is the infrastructure engine. SSH, TCP, credentials, key parsing, host verification, remote files and process execution never belong to the frontend.",
      meta: { title: "Runory Security | Local Credentials, Host Verification and Approval", description: "Learn how Runory protects SSH access with a local Credential Vault, mandatory host-key verification, least-privilege IPC, command approval, verification and secret-free sync.", keywords: ["SSH security", "host key verification", "local credential vault", "local-first security", "server command approval"] },
      signal: [["Credentials", "Local Vault only"], ["Connections", "Mandatory host checks"], ["Permissions", "Business-specific IPC"]],
      highlights: [["Secrets never enter host profiles", "ServerProfile never stores a password, passphrase, private-key content or vault master secret."], ["Unknown and changed are different", "You may temporarily or permanently trust an unknown host; a changed known-host key blocks the connection."], ["Remote content is untrusted", "Remote files, terminal output, HTTP bodies and MCP output are all treated as untrusted data."]],
      sections: [
        { title: "Credential lifecycles stay inside the Rust Core", body: ["CredentialService stores credentials through CredentialVault. The interface has no API for reading a remembered password in plaintext; transient secrets are submitted once for the current operation. Logs may include profile and session IDs, host, port and stable error codes, but never passwords, passphrases, keys or vault secrets.", "Private keys support both File and Vault concepts. Desktop can reference a file, while mobile uses an opaque ID for an imported key instead of inheriting assumptions about desktop filesystem paths."] },
        { title: "Server identity must be checked", body: ["Runory exposes no accept-all, skip-verification or StrictHostKeyChecking=no shortcut. Unknown hosts require Trust Once, Trust & Remember or Cancel; changed keys block the connection.", "Verification lives in KnownHostService inside Rust rather than relying on whether the frontend happens to show a warning correctly."] },
        { title: "Approval, verification and audit reinforce each other", body: ["AI approval binds an exact command and session. Structured high-risk operations bind a ChangeSet version and require Verification. ChangeSet, Incident and Audit persistence retains only necessary content-free metadata; restarts invalidate old approval and never resume writes automatically.", "Tauri uses least privilege and business-specific Commands. Terminal output travels over a Channel to xterm, avoiding generic event broadcasts, React state and persistence."] },
      ],
      faq: [["Can the web portal read my server inventory?", "No. Optional sync stores a locally encrypted inventory copy, and the web portal does not decrypt it."], ["Does Runory record terminal output?", "No. Terminal output is not persisted; the raw stream travels through a Tauri Channel to xterm."], ["Can a plugin or Skill raise its own permissions?", "No. Skills, MCP and models remain subject to the same Tool Registry, Policy and approval boundaries."]],
    },
    pricing: {
      label: "Pricing", eyebrow: "RUNORY PRICING", title: "Keep the local workspace free. Add cloud and AI when you need them.",
      lead: "Use SSH, SFTP, multiple sessions and local operations without an account. Choose a plan only when you need cross-device sync, managed AI or team collaboration. These prices come from Runory's current Billing configuration; paid checkout is not yet available.",
      meta: { title: "Runory Pricing | Free SSH Client, Pro and Team Plans", description: "Compare Runory Free, Pro, Team and Business pricing for local SSH and SFTP, encrypted sync, managed AI, team workspaces, roles and audit history.", keywords: ["Runory pricing", "free SSH client", "SSH client pricing", "AI operations pricing", "server management pricing"] },
      signal: [["Free", "$0 · forever"], ["Pro", "$12 · monthly"], ["Teams", "Per-seat pricing"]],
      highlights: [["Local work stays free", "Free includes local SSH, the credential vault, SFTP, multiple sessions and local operations, with no account required."], ["Annual plans cost less", "Pro is $10 per month when billed annually; Team and Business are $20 and $30 per user per month on annual billing."], ["Usage has clear boundaries", "Managed AI spends credits from actual token usage. The ledger stores usage and amount metadata, never prompts or terminal content."]],
      plans: {
        monthly: "Monthly", annual: "Annual equivalent", perMonth: "/ month", perUserMonth: "/ user / month", freeForever: "Free forever", recommended: "Recommended", unavailable: "Paid subscriptions coming soon", download: "Download Free", included: "What's included", credits: "{{count}} credits monthly", noCredits: "No managed AI credits", note: "Prices are in USD. Annual prices are shown as monthly equivalents. Team and Business are billed per seat. The payment channel is not yet configured.",
        items: [
          { code: "free", name: "Free", description: "For managing personal servers on one device.", monthlyPrice: "$0", annualPrice: "$0", perSeat: false, credits: "0", featured: false, features: ["Local SSH and multiple sessions", "Local encrypted credential vault", "SFTP file management", "Local operations"] },
          { code: "pro", name: "Pro", description: "For individuals who need cross-device work and managed AI.", monthlyPrice: "$12", annualPrice: "$10", perSeat: false, credits: "10,000", featured: true, features: ["Encrypted desktop and mobile sync", "Local encrypted credential vault", "Runory Managed AI", "Credit and AI usage history"] },
          { code: "team", name: "Team", description: "For small teams managing infrastructure together.", monthlyPrice: "$24", annualPrice: "$20", perSeat: true, credits: "25,000", featured: false, features: ["Everything in Pro", "Team workspaces and member roles", "Shared team credit pool", "14-day trial foundation"] },
          { code: "business", name: "Business", description: "For organizations that need more AI capacity and governance.", monthlyPrice: "$36", annualPrice: "$30", perSeat: true, credits: "50,000", featured: false, features: ["Everything in Team", "Infrastructure access policies", "Organization audit records", "Higher Managed AI allowance"] },
        ],
      },
      sections: [
        { title: "Free is not a time-limited trial", body: ["Free has no expiration. Create host profiles locally, connect with passwords or keys, open multiple terminal sessions, and use SFTP and local operations within the same ServerSession.", "Local connections do not require a Runory account. Passwords, passphrases and private keys remain in the device Credential Vault and are not uploaded to use the free plan."] },
        { title: "Pro adds cross-device work and managed AI", body: ["Pro includes encrypted sync, Runory Managed AI and usage history, with 10,000 credits each month. Sync payloads are encrypted inside the Rust Core before upload; credentials, known hosts and terminal output stay outside sync data.", "Managed AI deducts credits from actual token usage. The product ledger stores only the metadata needed to calculate and settle usage, never prompts or terminal content."] },
        { title: "Team and Business are built around organizations", body: ["Team and Business use per-seat pricing and shared workspace credits. Team adds member roles and consolidated credits; Business adds access policies, organization audit records and a higher AI allowance.", "The repository includes 14-day team trial and Billing foundations, but the payment channel is not configured. This page does not present a working checkout button, and plans, allowances or prices may still change before general availability."] },
      ],
      faq: [["Can I keep using Free indefinitely?", "Yes. Free local SSH, SFTP, multiple sessions, the credential vault and local operations do not expire and do not require sign-in."], ["How are annual prices calculated?", "The page shows monthly equivalents for annual billing: Pro $10, Team $20 per user and Business $30 per user. The annual total is calculated across 12 months."], ["Can I buy Pro, Team or Business now?", "Not yet. Billing plans and trial foundations exist, but the payment channel is not configured. A working entry point will appear in the client when purchasing opens."]],
    },
    download: {
      label: "Download & install", eyebrow: "DOWNLOAD RUNORY", title: "Start your first connection on your desktop.",
      lead: "Runory Desktop targets Windows, macOS and Linux. Visit GitHub Releases for the packages, architectures and versions that are actually available. Install the app, add a server, verify its fingerprint and begin an SSH session.",
      meta: { title: "Download Runory | SSH Client for Windows, macOS and Linux", description: "Download the Runory desktop SSH and server-management client. Find current Windows, macOS and Linux packages, formats and first-connection instructions.", keywords: ["download SSH client", "Windows SSH client download", "macOS SSH client", "Linux SFTP client", "Runory download"] },
      signal: [["Windows", ".msi / .exe"], ["macOS", ".dmg"], ["Linux", ".AppImage / .deb"]],
      highlights: [["Use the release page as the source", "Platforms and architectures may differ between versions; GitHub Releases lists the files that are actually available."], ["Start without an account", "Local host management and SSH do not require sign-in. A Runory account is needed only for optional services such as sync."], ["Mobile foundations are in place", "Adaptive iOS and Android interfaces and mobile key support exist, but app-store downloads are not yet available."]],
      sections: [
        { title: "Choose the format that matches your system", body: ["On the release page, Windows users can look for an .msi or .exe, macOS users should choose a .dmg matching Apple Silicon or Intel, and Linux users can select an .AppImage or .deb for their distribution. The Assets list for each release is the definitive source.", "Install using your operating system's normal flow. If the system displays a publisher or security warning, verify that the download came from Runory's official GitHub repository before continuing."] },
        { title: "Make your first SSH connection", body: ["Open Runory and add a server name, address, port, username and authentication method. Passwords and passphrases never enter ServerProfile; when you remember a credential, the local Credential Vault manages it.", "The first connection shows a host fingerprint. Compare it with a trusted value from your administrator or cloud console, then choose whether to trust it once or trust and remember it."] },
        { title: "Enable more when you need it", body: ["Once connected, start in Terminal and open Files, Dashboard, Operations or Deployment as needed. AI requires the appropriate model configuration in the client, and every proposed command still requires approval.", "To carry host and group inventories across devices, sign in and explicitly enable optional cloud sync. Credentials, known hosts and terminal output remain outside sync data."] },
      ],
      faq: [["Where can I find the latest installer?", "GitHub Releases lists the versions, platforms, architectures and files currently published."], ["Must I create an account first?", "No. Local connections and host management work without an account."], ["Can I download the mobile app from an app store?", "Not yet. Mobile capabilities continue to evolve, and app-store downloads are not currently offered."]],
    },
  },
};

export type MarketingCopy = CopyShape<typeof zh>;
export type MarketingPage = MarketingCopy["pages"][MarketingSlug];
export type PricingPage = MarketingCopy["pages"]["pricing"];

export function isMarketingSlug(value: string): value is MarketingSlug {
  return marketingSlugs.includes(value as MarketingSlug);
}

export function getMarketingCopy(locale: Locale): MarketingCopy {
  return locale === "zh-CN" ? zh : en;
}

export function localizedPath(locale: Locale, slug?: MarketingSlug): string {
  return `/${locale}${slug ? `/${slug}` : ""}`;
}

export function marketingMetadata(locale: Locale, slug: MarketingSlug): Metadata {
  const page = getMarketingCopy(locale).pages[slug];
  const canonical = localizedPath(locale, slug);
  return {
    title: page.meta.title,
    description: page.meta.description,
    keywords: [...page.meta.keywords],
    alternates: {
      canonical,
      languages: { "zh-CN": localizedPath("zh-CN", slug), "en-US": localizedPath("en-US", slug), "x-default": localizedPath("en-US", slug) },
    },
    openGraph: { type: "website", url: canonical, siteName: "Runory", locale, title: page.meta.title, description: page.meta.description },
    twitter: { card: "summary", title: page.meta.title, description: page.meta.description },
  };
}
