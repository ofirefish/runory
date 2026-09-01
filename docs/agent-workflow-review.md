# AI Agent 运维工作流评估与实现

## 目标

Agent 的成功标准不是“回答了一段诊断文本”，而是以最少人工切换帮助用户安全达到运维目标：收集真实证据、给出可执行建议、将变更纳入审批、执行后验证，并在真实支持时回滚。

## 已落地工作流

```text
用户目标
  → 目标驱动的只读调查
  → 引用 Tool Evidence 的修复建议
  → Rust 校验并创建 ChangeSet Draft
  → 对话内展示风险 / 差异 / 前置条件 / Policy
  → 绑定 ChangeSet Version 的审批
  → 写 Tool 执行
  → 强制 Verification
  → 真实支持时 Rollback
```

对话内卡片和独立 ChangeSet Workspace 使用同一个 Rust 领域对象。前者优化当前任务的操作效率，后者保留完整审阅与审计入口。

## 能力评估

| 维度 | 原问题 | 当前实现 |
| --- | --- | --- |
| 目标理解 | 任意提示词可能落入固定诊断模板 | 有限 Observe–Plan–Act，按目标选择只读 Typed Tool；信息不足时在对话中提问 |
| 证据可信度 | 诊断结论和修复动作可能缺少绑定 | Proposal 必须引用本次成功 Tool Evidence，Rust 对目标和参数做二次校验 |
| 变更安全 | 建议和执行割裂，或容易直接执行 | 模型只能提议；所有写入创建真实 ChangeSet，Policy 与 Risk 决定审批方式 |
| 审批正确性 | 审批可能对不上后来变化的计划 | 审批绑定精确版本、目标、步骤和 Policy Snapshot；变化即失效 |
| 执行可靠性 | “执行成功”不等于目标恢复 | 每个写 Tool 内建读回或状态 Verification；前置条件变化时停止 |
| 回滚真实性 | UI 容易泛化宣称可回滚 | 仅 `file.patch` 等具有真实逆向能力的步骤显示回滚；其余明确不支持 |
| 操作效率 | Agent、ChangeSet 标签页之间频繁跳转 | 对话内完成审批、执行、验证、回滚；仍可打开完整审阅页 |
| 运行反馈 | 长调查只有笼统等待提示 | Tauri Channel 推送阶段和 Typed Tool 名称；不向 React 暴露远程输出或隐藏推理 |
| 用户控制 | 运行中无法及时中止 | Composer 在运行时切换为停止按钮，调用 Rust cancellation |
| 多目标安全 | 单服务器建议可能被误扩展到 Fleet | 多目标写建议 fail-closed，要求显式进入 Fleet ChangeSet |

## 支持的受约束变更

- `file.patch`：R3；要求用户提供精确路径、原文本和替换文本，并先读取同一路径证明匹配；支持真实 reverse-patch rollback。
- `service.restart`：R3；要求同名服务状态/日志证据；执行后验证 active；不宣称回滚。
- `service.reload`：R2；要求同名服务证据；执行后验证 active；不宣称回滚。
- `nginx.reload`：R2；要求成功的 `nginx -t` Typed Tool Evidence；执行后再次验证；不宣称回滚。
- `docker.restart`：R3；要求同名容器 inspect Evidence；执行后验证 running；不宣称回滚。

模型无法新增工具名、调用写工具、授权自身提议或绕过 ChangeSet。远程文件、日志、HTTP 与 MCP 输出均作为不可信数据处理。

## UX 决策

- 对话开始后隐藏空状态快捷操作，减少视觉噪音。
- 用户消息 Bubble 缩窄，活动、证据与 ChangeSet 使用独立结构化块，不伪装成聊天文本。
- 已回答的澄清问题折叠为完成态，防止重复提交。
- ChangeSet 卡片只展示当前状态允许的动作；服务端仍是最终权限与状态机权威。
- 错误使用稳定错误码映射为中英文可操作提示，不直接显示笼统的英文异常。
- 恢复后的 metadata-only ChangeSet 只可审阅，不可审批、执行或回滚。

## 效率机制

- 已验证的 service / URL / Nginx routing hints 用于首轮并行调查，但不是权限输入。
- Observation Cache 仅用于 Read Tool，并绑定 Target、Source、Observed-at 和 TTL。
- 写入后失效目标缓存；Verification 与 ChangeSet Precondition 永远绕过缓存和去重。
- 上下文压缩保留 Evidence 引用；文件内容不进入模型证据摘要。
- 运行具有轮次、工具调用、耗时和上下文预算，并支持取消。

## 验收边界

当前功能是“建议并经审批执行”，不是自主修复。未新增通用 Shell、任意文件系统 IPC、模型直连 SSH、自动删除、Fleet 并行写入或 Trusted Automation。后续演进应优先增加更多有验证语义的 Typed Tool，而不是放宽通用执行权限。
