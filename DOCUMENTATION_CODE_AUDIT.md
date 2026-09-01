# Runory 文档与代码一致性审查

> 审查日期：2026-08-30  
> 范围：`AGENTS.md`、`SECURITY.md`、`AGENTIC.md`、`ARCHITECTURE.md`、`ROADMAP.md`、`DESIGN.md`、`PRD.md`、`README.md` 与当前 `src/`、`src-tauri/`、测试及配置。  
> 本次未修改 `src/`、`src-tauri/` 或其他业务代码。

## 1. 当前项目实际完成状态

| 能力 | 代码证据 | 审查结论 |
|---|---|---|
| Phase 1 SSH Core | `profiles`、`groups`、`known_hosts`、`credentials`、`ssh`、xterm、Session Store、Docker OpenSSH fixture | 已实现；是应优先保留的稳定底座 |
| Phase 2 SFTP / Files | `ssh/sftp.rs`、`commands/sftp.rs`、`transfers`、`features/files` | 已实现 |
| Phase 3 Dashboard | Rust `dashboard`、React `features/dashboard` | 已实现 |
| Phase 4 Operations | Rust `operations` 与 typed commands；React `features/operations` | 已实现；不是 raw shell IPC |
| Phase 5 Deployment | Rust `deployment`、React `features/deployment`、JSON history repository | 已实现 |
| Phase 6 Mobile | Android/iOS Tauri 配置、Android 生成工程、mobile UI、portable vault、biometric capability | 产品代码基线已实现；iOS 最终签名归档仍需 macOS/Xcode |
| Phase 7 AI Terminal | Rust 本地规则 Provider、32 KiB 易失 Terminal Context、React AI View | 已实现有限本地能力；无云模型、无自动执行 |
| Phase 8 AI Plan | `AiAgentService`、`AiAgentPlan`、固定 `AiToolInput`、逐项审批、一次性执行、`ai-audit.json` | 已实现有限类型化计划执行；不是完整 Agentic Runtime |
| Phase 9 Optional Cloud | Supabase Auth UI、加密同步、Policy、RLS migrations、Edge Function、release tooling | 本地代码与两阶段发布门禁已实现；Staging/Production 凭据化验证尚未完成 |
| Agentic Infrastructure Workspace | 代码中没有通用 Agent Runtime、Model Provider 调度、Tool Registry / Descriptor、Context Manager、Diagnosis / Evidence、版本化 ChangeSet、Verification / Rollback、Skills 或 MCP | Future / Not Started |

质量门禁结果：前端 11 个测试文件、23 个测试全部通过；TypeScript typecheck 与 ESLint 通过。当前环境找不到 `cargo`，因此本次无法重新执行 Rust 测试；仓库中可见 Rust 单元测试和 Docker OpenSSH 集成测试代码，但不把“代码存在”当作本次运行通过。

## 2. 文档与代码冲突列表

### Phase 与产品范围

1. `AGENTS.md` 原先仍写“Phase 1 只做 SSH Core”，并禁止 SFTP、Operations、Deployment、AI、Cloud；当前代码和 `ROADMAP.md` 已到 Phase 9。若照旧执行，会诱导删除稳定功能或把正常维护误判为越界。
2. `PRD.md` 是 v0.1 PRD，却没有声明历史范围；其中“第一阶段非目标”与当前 Phase 2–9 实现表面冲突。
3. `ROADMAP.md` 没有给 Phase 1/2 明确 Implemented 状态，同时把 Phase 8 命名为 “AI DevOps Agent”，容易被误解为 `AGENTIC.md` 全部完成。
4. `AGENTIC.md` 原研发顺序仍从 SFTP Foundation 开始，落后于现有代码。

### 目录结构与模块命名

5. `ARCHITECTURE.md` 和 `README.md` 的目录树只覆盖早期 SSH Core，遗漏当前 `ai`、`cloud`、`dashboard`、`deployment`、`operations`、`transfers` 以及对应 React feature。
6. `ARCHITECTURE.md` 将 SFTP 标成 Future，但实际已经有共享 Session 上的 SFTP Channel、传输队列和 Files UI。
7. 当前 `AiAgentService` / `AiAgentPlan` 是有限执行器与计划对象，不等价于 future `Agent Runtime` / `AgentRun` / `ChangeSet`。不应为了名称对齐而重命名或迁移现有模块。
8. `AGENTIC.md` 推荐的 `agent/`、`tools/`、`policy/`、`approval/`、`changes/`、`skills/`、`mcp/` 是目标边界，不是当前目录要求。

### 安全规则与实现语义

9. `SECURITY.md` 把已实现的 Mobile 与 SFTP 仍称为 Future，且没有明确区分现有 Phase 8 与 future Agentic 安全链。
10. 现有 Phase 8 所有步骤都逐项审批；future Agentic 允许部分只读 R0/R1 Tool 按 Policy 自动执行。两者可以共存，但必须标注适用阶段，不能用 future 默认值弱化现有安全行为。
11. `AGENTIC.md` 的 ChangeSet、Verification、Rollback、Prompt Injection、Skills 与 MCP 规则是未来实现的强制约束，不是当前代码已经满足的功能声明。
12. 当前可见代码仍符合关键高优先级不变量：Profile schema 无密码/口令字段；SSH secret 使用 transient input / vault 与 zeroizing wrapper；Host Verification 有独立 prepare/trust/cancel 流程；Tauri capability 未开放 broad shell/process/FS/HTTP；Terminal Output 直接写入 xterm，不进入 Zustand。

### UI 状态

13. `DESIGN.md` 的“当前实现优先级”仍把 UI v3 Shell、Files、Monitor 当作下一步，但对应 layout 与 feature 已存在。
14. 完整 Agentic Timeline、Diagnosis、Evidence、ChangeSet、Skills、MCP UI 尚无后端事实来源，当前不应展示占位或 Mock。

## 3. 本次修正的文档

- `AGENTS.md`：把早期 Phase 1 限制改为“历史基线 + 当前 Phase 2–9 可维护 + future Agentic 需明确授权”，并禁止为目录对齐重构稳定 Core。
- `SECURITY.md`：区分当前安全基线与 Future Agentic，修正 Mobile/SFTP 状态和章节编号。
- `AGENTIC.md`：明确整体为 Future / Not Implemented，说明现有有限 AI Plan 只是可复用底座，更新研发顺序。
- `ARCHITECTURE.md`：补齐当前前后端模块，修正 SFTP/Operations/AI Plan 状态，新增 Future Agentic 增量边界。
- `ROADMAP.md`：补齐 Phase 1/2 状态，收窄 Phase 8 名称与含义，新增 Future Phase 10 最小边界。
- `DESIGN.md`：将 UI v3 优先级标为历史实施顺序，禁止提前展示无真实后端的 Agentic UI。
- `PRD.md`：明确第 5–22 节是 v0.1 历史验收，不与当前 Phase 2–9 冲突。
- `README.md`：更新当前能力、目录和文档索引，避免把有限 AI Plan 宣传成完整 Agentic Runtime。
- `DOCUMENTATION_CODE_AUDIT.md`：记录本次审查结论与开发边界。

## 4. 下一阶段建议开发边界

下一阶段如获得明确开发授权，只做 Agentic Foundation 的最小只读切片：

1. 在 Rust 新增 Native Tool Adapter / Registry 的领域模型：`ToolDescriptor`、`ToolResult`、Risk、Mutability。
2. Adapter 只复用现有 `ServerSessionManager`、Dashboard、Operations、SFTP 和 Deployment Service，不改写 SSH Transport。
3. 第一批仅提供只读 `system.info`、`system.disk`、`service.status`、`service.logs`、`network.port_check`、`http.request`、`nginx.test`。
4. 不接 LLM Provider；不做 Agent UI；先完成 schema、risk metadata、timeout、output bound 和 Rust tests。
5. 保持现有 Phase 7/8 行为与 IPC 兼容；新 Tool Invocation 由 Rust 内部路由，React 不获得 arbitrary tool / shell 能力。

## 5. 暂时不应实现的内容

- 完整 Autonomous Agent、Trusted Automation 或自动 Root 修复
- Write Tool：`file.patch/write/delete`、`service.restart`、`nginx.reload` 等新的 Agentic 写路径
- ChangeSet 执行、Snapshot、Verification、Rollback（可以先设计 Draft，但不能伪装可执行）
- Skills Runtime、User Skills、Marketplace
- MCP Client、MCP Credential、外部写 Tool
- 多服务器自主执行与后台静默重连
- 新云模型 Provider、BYOK、Runory Managed AI
- RDP、Port Forwarding、Jump Host、Proxy、Billing、Remote Telemetry
- 为匹配 future 推荐目录而批量移动/重命名现有 SSH、SFTP、Operations 或 AI Plan 代码
