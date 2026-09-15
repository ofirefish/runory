export const locales = ["zh-CN", "en-US"] as const;
export type Locale = (typeof locales)[number];

export function isLocale(value: string): value is Locale {
  return locales.includes(value as Locale);
}

const dictionaries = {
  "zh-CN": {
    nav: { label: "主导航", product: "产品", security: "安全", sync: "同步服务", signIn: "登录" },
    hero: {
      eyebrow: "LOCAL-FIRST INFRASTRUCTURE WORKSPACE",
      title: "让每一次远程运维，都清楚、可控、可追溯。",
      description: "Runory 把 SSH、跳板、隧道、文件、服务、Incident、部署与受控 AI 放进一个跨平台工作区。服务器连接留在本地，云端只保存经过加密的同步数据。",
      primary: "查看同步服务",
      secondary: "了解安全设计",
      privacy: "本地优先 · 强制主机验证 · 敏感操作逐项确认",
      imageAlt: "Runory 桌面基础设施工作区界面",
    },
    proof: { platforms: "Windows · macOS · Linux · iOS · Android", status: "Desktop first, mobile ready" },
    features: {
      eyebrow: "ONE CALM WORKSPACE",
      title: "不是给终端加一个聊天框。",
      description: "从可靠的接入路径出发，把高频运维工作组织成明确的上下文、动作与证据。",
      items: [
        ["可靠接入", "SSH、跳板机、本地隧道、堡垒接入与强制 Host Key Verification。"],
        ["文件与运维", "SFTP、服务、Docker、Nginx、Incident 与部署共用同一个 ServerSession。"],
        ["受控 AI", "命令原样展示、用户审批、真实终端执行，再根据结果继续分析。"],
      ],
    },
    sync: {
      eyebrow: "OPTIONAL CLOUD SYNC",
      title: "云端同步，但不把服务器交给云端。",
      description: "Profile 与 Group 清单在 Rust Core 中加密后上传。密码、Passphrase、私钥和终端输出不进入网页或同步数据。",
      steps: ["本地仓库", "Rust 加密与冲突预览", "Supabase 密文存储"],
      cta: "登录同步服务",
    },
    security: {
      eyebrow: "SECURITY BY BOUNDARY",
      title: "权限边界比功能数量更重要。",
      items: ["未知主机必须显式信任，主机密钥变化立即阻止", "凭据由本地 Vault 管理，UI 无法读取已记住的密码", "写入与未知命令执行后必须经过只读验证"],
    },
    common: { home: "返回首页", account: "账号中心", admin: "运营后台", signOut: "退出登录", planned: "规划中", configuration: "服务尚未配置", configurationDescription: "请在本地或 Vercel 环境中配置 Supabase URL、Publishable Key 和可信站点地址。", unavailable: "服务暂时不可用", unavailableDescription: "无法读取所需数据，请确认数据库迁移已经应用并稍后重试。" },
    auth: {
      signInTitle: "登录同步服务", signInDescription: "查看账号资料、组织与云端密文副本状态。", signUpTitle: "创建 Runory 账号", signUpDescription: "注册后请先完成邮箱确认，再在 Runory 中启用可选同步。", forgotTitle: "重置密码", forgotDescription: "我们会向你的邮箱发送一次性恢复链接。", updateTitle: "设置新密码", updateDescription: "完成后将返回账号中心。",
      displayName: "显示名称", email: "邮箱", password: "密码", newPassword: "新密码", passwordHint: "至少 8 位，最多 128 位", signIn: "登录", signUp: "创建账号", sendReset: "发送恢复邮件", updatePassword: "更新密码", forgot: "忘记密码", noAccount: "还没有账号？", haveAccount: "已有账号？", backToSignIn: "返回登录",
      asideTitle: "默认本地优先。", asideDescription: "云同步保持可选、加密，并与 SSH 凭据和终端数据流严格分离。", invalid: "请检查输入内容。", authFailed: "操作未完成，请检查账号信息或稍后重试。", configuration: "同步服务尚未配置。", confirmationSent: "确认邮件已发送，请完成邮箱验证。", resetSent: "恢复邮件已发送，请检查收件箱。", updated: "已保存。", confirmFailed: "确认链接无效或已过期。",
      openAppTitle: "邮箱已确认", openAppDescription: "正在打开 Runory 桌面应用并完成登录。如果没有自动打开，请点击下方按钮。", openAppOpening: "正在打开 Runory…", openAppButton: "打开 Runory", openAppAccount: "继续使用网页账号中心", openAppMissing: "当前没有可用的登录会话，请重新登录。",
    },
    portal: {
      navOverview: "同步概览", navAdmin: "运营后台", eyebrow: "ACCOUNT & SYNC", title: "你的 Runory 云端空间", description: "这里只展示账号和加密同步元数据。服务器资料不会在网页中解密。", profileTitle: "账号资料", displayName: "显示名称", email: "登录邮箱", save: "保存资料", workspaces: "空间", personalWorkspace: "个人空间", teamWorkspace: "团队空间", lastSync: "云端副本更新", revision: "Revision", noSync: "尚无云端副本", encrypted: "仅密文", privacyTitle: "数据边界", privacyDescription: "密码、Passphrase、私钥、Known Hosts 与终端输出不会出现在这个门户。", noWorkspace: "尚未创建空间", openAdmin: "打开运营后台",
    },
    admin: {
      eyebrow: "OPERATIONS CONSOLE", title: "注册账号", description: "只读查看账号、组织和同步元数据。当前后台不提供封禁、删除或权限提升操作。", accounts: "账号", releases: "版本", skills: "Skills", store: "Store", email: "邮箱", displayName: "显示名称", status: "邮箱状态", created: "注册时间", lastSignIn: "最近登录", organizations: "空间数", lastSync: "最近同步", confirmed: "已确认", pending: "待确认", never: "从未", view: "查看", empty: "暂无注册账号", back: "返回账号列表", details: "账号详情", userId: "User ID", memberships: "空间成员关系", role: "角色", roleOwner: "所有者", roleAdmin: "管理员", roleOperator: "操作员", roleViewer: "查看者", syncObjects: "同步对象", previous: "上一页", next: "下一页", page: "第 {page} 页", forbiddenTitle: "无权访问", forbiddenDescription: "当前账号不是已启用的平台管理员。", migrationTitle: "后台尚未启用", migrationDescription: "请先应用 Web Platform Admins 迁移，并在 SQL Editor 中登记首个平台管理员。", skillsTitle: "Skills 入口已预留", skillsDescription: "本阶段不创建 Skill Runtime、签名包、安装流程或 Marketplace 数据。", storeTitle: "Store 入口已预留", storeDescription: "本阶段不创建商品、交易、Billing 或发布工作流。",
      releasesTitle: "版本发布", releasesDescription: "登记版本号、各平台安装包链接与更新说明。下载页优先展示已发布且标记为最新的版本；桌面自动更新仍走 GitHub latest.json。", releasesEmpty: "暂无版本记录", releasesNew: "新建版本", releasesEdit: "编辑版本", releasesBack: "返回版本列表", releasesVersion: "版本号", releasesStatus: "状态", releasesLatest: "最新", releasesPublishedAt: "发布时间", releasesNotesZh: "更新说明（中文）", releasesNotesEn: "更新说明（英文）", releasesPageUrl: "发行页链接（可选）", releasesPageUrlHint: "留空则使用 GitHub Releases 页面。", releasesAssets: "安装包链接", releasesFormat: "格式", releasesUrl: "下载 URL", releasesWindows: "Windows", releasesMacosApple: "macOS Apple Silicon", releasesMacosIntel: "macOS Intel", releasesLinux: "Linux", releasesSave: "保存", releasesCreate: "创建草稿", releasesPublish: "发布", releasesUnpublish: "退回草稿", releasesArchive: "归档", releasesSetLatest: "设为最新", releasesReadOnly: "当前角色为只读，无法修改版本。", releasesStatusDraft: "草稿", releasesStatusPublished: "已发布", releasesStatusArchived: "已归档", releasesYes: "是", releasesNo: "否", releasesUpdated: "已保存。", releasesConflict: "版本号已存在。", releasesInvalid: "请检查版本号与 https 下载链接。", releasesForbidden: "无权修改版本。", releasesUnavailable: "无法保存，请确认迁移已应用后重试。", releasesMissing: "版本不存在。",
    },
    footer: "Runory · Run your infrastructure from anywhere.",
  },
  "en-US": {
    nav: { label: "Primary navigation", product: "Product", security: "Security", sync: "Cloud sync", signIn: "Sign in" },
    hero: {
      eyebrow: "LOCAL-FIRST INFRASTRUCTURE WORKSPACE",
      title: "Make every remote operation clear, controlled, and reviewable.",
      description: "Runory brings SSH, jump hosts, tunnels, files, services, Incident workflows, deployment, and controlled AI into one cross-platform workspace. Connections stay local; the cloud stores encrypted sync data only.",
      primary: "Explore cloud sync",
      secondary: "See the security model",
      privacy: "Local-first · Mandatory host verification · Explicit approval",
      imageAlt: "Runory desktop infrastructure workspace",
    },
    proof: { platforms: "Windows · macOS · Linux · iOS · Android", status: "Desktop first, mobile ready" },
    features: {
      eyebrow: "ONE CALM WORKSPACE",
      title: "More than a chat box beside a terminal.",
      description: "Runory starts with reliable access paths and turns operations into clear context, actions, and evidence.",
      items: [
        ["Reliable access", "SSH, jump hosts, local tunnels, bastion access, and mandatory host-key verification."],
        ["Files and operations", "SFTP, services, Docker, Nginx, Incident, and deployment share one ServerSession."],
        ["Controlled AI", "Commands are shown exactly, approved by you, run in the real terminal, and reviewed from evidence."],
      ],
    },
    sync: {
      eyebrow: "OPTIONAL CLOUD SYNC",
      title: "Sync through the cloud without handing it your servers.",
      description: "Profile and Group inventories are encrypted in the Rust Core before upload. Passwords, passphrases, private keys, and terminal output never enter the website or sync payload.",
      steps: ["Local repository", "Rust encryption and conflict preview", "Supabase ciphertext storage"],
      cta: "Sign in to cloud sync",
    },
    security: {
      eyebrow: "SECURITY BY BOUNDARY",
      title: "Permission boundaries matter more than feature count.",
      items: ["Unknown hosts require explicit trust; changed host keys are blocked", "The local Vault owns credentials; the UI cannot read remembered passwords", "Write and unknown commands require a separate read-only verification"],
    },
    common: { home: "Back home", account: "Account", admin: "Operations", signOut: "Sign out", planned: "Planned", configuration: "Service not configured", configurationDescription: "Configure the Supabase URL, Publishable Key, and trusted application origin in local or Vercel environment settings.", unavailable: "Service unavailable", unavailableDescription: "The required data could not be loaded. Confirm that database migrations are applied and try again." },
    auth: {
      signInTitle: "Sign in to cloud sync", signInDescription: "Review your account, organizations, and encrypted cloud-copy status.", signUpTitle: "Create a Runory account", signUpDescription: "Confirm your email, then opt into sync from Runory.", forgotTitle: "Reset your password", forgotDescription: "We will send a one-time recovery link to your email.", updateTitle: "Set a new password", updateDescription: "You will return to your account when it is complete.",
      displayName: "Display name", email: "Email", password: "Password", newPassword: "New password", passwordHint: "8–128 characters", signIn: "Sign in", signUp: "Create account", sendReset: "Send recovery email", updatePassword: "Update password", forgot: "Forgot password", noAccount: "New to Runory?", haveAccount: "Already have an account?", backToSignIn: "Back to sign in",
      asideTitle: "Local-first by default.", asideDescription: "Cloud sync remains optional, encrypted, and separate from SSH credentials and terminal streams.", invalid: "Check the information you entered.", authFailed: "The operation could not be completed. Check your account details or try again.", configuration: "Cloud sync is not configured.", confirmationSent: "Confirmation sent. Verify your email to continue.", resetSent: "Recovery email sent. Check your inbox.", updated: "Saved.", confirmFailed: "The confirmation link is invalid or expired.",
      openAppTitle: "Email confirmed", openAppDescription: "Opening the Runory desktop app to finish signing in. If nothing happens, use the button below.", openAppOpening: "Opening Runory…", openAppButton: "Open Runory", openAppAccount: "Continue in the web account portal", openAppMissing: "No signed-in session is available. Sign in again.",
    },
    portal: {
      navOverview: "Sync overview", navAdmin: "Operations", eyebrow: "ACCOUNT & SYNC", title: "Your Runory cloud space", description: "This portal only shows account and encrypted-sync metadata. Server profiles are never decrypted here.", profileTitle: "Account profile", displayName: "Display name", email: "Sign-in email", save: "Save profile", workspaces: "Spaces", personalWorkspace: "Personal space", teamWorkspace: "Team space", lastSync: "Cloud copy updated", revision: "Revision", noSync: "No cloud copy yet", encrypted: "Ciphertext only", privacyTitle: "Data boundary", privacyDescription: "Passwords, passphrases, private keys, known hosts, and terminal output never appear in this portal.", noWorkspace: "No space created yet", openAdmin: "Open operations console",
    },
    admin: {
      eyebrow: "OPERATIONS CONSOLE", title: "Registered accounts", description: "Read-only account, organization, and sync metadata. Suspension, deletion, and privilege changes are not available.", accounts: "Accounts", releases: "Releases", skills: "Skills", store: "Store", email: "Email", displayName: "Display name", status: "Email status", created: "Created", lastSignIn: "Last sign-in", organizations: "Spaces", lastSync: "Last sync", confirmed: "Confirmed", pending: "Pending", never: "Never", view: "View", empty: "No registered accounts", back: "Back to accounts", details: "Account details", userId: "User ID", memberships: "Space memberships", role: "Role", roleOwner: "Owner", roleAdmin: "Administrator", roleOperator: "Operator", roleViewer: "Viewer", syncObjects: "Sync objects", previous: "Previous", next: "Next", page: "Page {page}", forbiddenTitle: "Access denied", forbiddenDescription: "This account is not an enabled platform administrator.", migrationTitle: "Operations console not enabled", migrationDescription: "Apply the Web Platform Admins migration and register the first platform administrator in the SQL Editor.", skillsTitle: "Skills entry reserved", skillsDescription: "This phase does not add a Skill Runtime, signed packages, installation flow, or Marketplace data.", storeTitle: "Store entry reserved", storeDescription: "This phase does not add products, transactions, billing, or publishing workflows.",
      releasesTitle: "Release catalog", releasesDescription: "Register version numbers, installer URLs, and release notes. Download pages prefer the published release marked latest. Desktop auto-update still uses GitHub latest.json.", releasesEmpty: "No releases yet", releasesNew: "New release", releasesEdit: "Edit release", releasesBack: "Back to releases", releasesVersion: "Version", releasesStatus: "Status", releasesLatest: "Latest", releasesPublishedAt: "Published", releasesNotesZh: "Release notes (Chinese)", releasesNotesEn: "Release notes (English)", releasesPageUrl: "Release page URL (optional)", releasesPageUrlHint: "Leave blank to use the GitHub Releases page.", releasesAssets: "Installer links", releasesFormat: "Format", releasesUrl: "Download URL", releasesWindows: "Windows", releasesMacosApple: "macOS Apple Silicon", releasesMacosIntel: "macOS Intel", releasesLinux: "Linux", releasesSave: "Save", releasesCreate: "Create draft", releasesPublish: "Publish", releasesUnpublish: "Move to draft", releasesArchive: "Archive", releasesSetLatest: "Set as latest", releasesReadOnly: "Your role is read-only and cannot change releases.", releasesStatusDraft: "Draft", releasesStatusPublished: "Published", releasesStatusArchived: "Archived", releasesYes: "Yes", releasesNo: "No", releasesUpdated: "Saved.", releasesConflict: "That version already exists.", releasesInvalid: "Check the version and https download URLs.", releasesForbidden: "You cannot modify releases.", releasesUnavailable: "Could not save. Confirm migrations are applied and try again.", releasesMissing: "Release not found.",
    },
    footer: "Runory · Run your infrastructure from anywhere.",
  },
} as const;

export function getDictionary(locale: Locale) {
  return dictionaries[locale];
}
