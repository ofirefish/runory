# Python 项目部署

本次增量参考 [宝塔 Python 项目功能说明](https://docs.bt.cn/ai-ops/mcp/tools-reference/) 的项目配置、进程控制和部署后状态核验。Runory 继续通过现有 SSH 会话和 Rust DeploymentService 执行，不依赖宝塔面板或常驻 Runory Agent。

## 使用流程

1. 在「部署 → Python 项目」中配置项目目录、可选 Git 来源、Python 解释器、虚拟环境和依赖文件。
2. 选择 Gunicorn、uWSGI 或 Python 脚本启动方式，填写运行用户。Gunicorn 支持 WSGI/ASGI，uWSGI 仅支持 WSGI。
3. Web 服务可设置 1–64 个工作进程和 1–64 个线程。ASGI 使用工作进程控制并发，线程设置仅对 WSGI 生效。uWSGI 的可调用对象可填 `app`、`application` 等名称；空值沿用 `application`。
4. 设置是否开机启动，点击完成并确认远端部署。关闭开机启动仍会启动本次部署的服务。
5. 项目列表可单独启动、停止、重启和读取最近 200 行 systemd journal。启停操作均须确认，执行后刷新真实服务状态。删除项目记录不会删除远端文件或停止服务。

## 执行与兼容性

- 目标服务器需要 Linux/systemd、Python venv/pip，以及可用的运行账户和服务安装权限。
- 服务监听地址沿用 `127.0.0.1`。本次未增加自动防火墙放行、域名映射、TLS 或反向代理管理。
- 部署安装依赖、重载 systemd、应用开机启动设置并重启服务。只有步骤成功且 systemd 报告 active，才记为部署成功；这不代表 HTTP/业务健康检查通过。
- 缺少 Python/systemd、运行账户无效、权限不足或启动失败均不会被记录为部署成功。权限不足时可能保留项目 `.runory` 下的 unit 草稿，供排错查看。
- 旧配置缺少新增字段时，进程数/线程数默认为 1、开机启动默认为开启，与旧部署行为兼容。配置继续使用现有 Repository。
- 新增控制 IPC 仅接受项目 ID 和 start/stop/restart 枚举。Rust 根据 SSH 会话所属 profile 查找 Python 项目并生成服务名，不接受任意命令。审计仅记录操作与结果元数据。
- systemd 参数使用其自身的转义规则，处理路径空格、引号、反斜杠及 `%`/`$` 展开。相同服务器上会映射到相同服务名的项目不可重复创建。

## 验证

前端测试覆盖确认/取消、断连及部署中禁用、状态刷新、失败提示、日志纯文本展示、参数范围和旧配置兼容；Rust 测试覆盖 unit 生成、默认值、参数校验、Repository 和部署结果合并。

真实 OpenSSH 的负向集成测试：先启动仓库 `openssh` fixture，再执行：

```powershell
cd src-tauri
cargo test --lib python_deployment_fails_closed_on_unsupported_openssh_host -- --ignored
```

现有 Alpine fixture 不运行真实 systemd；成功安装/启停 Gunicorn 和 uWSGI 的完整验收需要另行使用具有 systemd 的 Linux 测试服务器。

2026-10-03 本地验证：39 项前端测试、21 项 Rust 部署测试通过；6 项真实 OpenSSH 集成测试通过（Python 不支持环境拒绝、Deployment 共享传输、密码/PTY/Resize、私钥/加密密钥、错误凭据、网络中断/Host Key 变化）。TypeScript、此次变更文件 ESLint 和 Rust 格式检查通过，Clippy 完成并报告仓库既有告警。全仓库 `pnpm lint` 仍被 `.next` 生成文件及其他模块的既有错误阻塞。
