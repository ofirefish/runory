# Go 项目部署

本增量参考宝塔面板的 Go 项目配置（项目可执行文件、端口、运行用户、启动参数、环境变量、开机启动）和 Runory 现有的 Python / PHP / Java / Node 项目流程（四步向导、SSH 会话上的 Rust DeploymentService、systemd 生命周期、部署后状态核验）。不依赖宝塔面板或常驻 Runory Agent。

## 使用流程

1. 在「部署 → Go 项目」中填写项目名称和项目目录，可选通过 Git 克隆。
2. 构建：填写可执行文件路径（相对项目目录或绝对路径）。可选「部署前在服务器上构建」：选择 Go 工具链（读取 `PATH` 中的 `go` 以及 `/usr/local/go`、`/usr/lib/go*`、`~/sdk/go*`、`/opt/go*`、`/usr/local/go*` 下的 `bin/go`）和构建包（`.` 或 `./cmd/server`）。关闭构建则直接部署已上传的二进制。
3. 运行：监听端口（以 `PORT` 环境变量传入）、程序参数、环境变量（`KEY=value`）、运行用户、是否开机启动。
4. 完成后确认远端部署。列表中可单独启动、停止、重启服务并读取最近 200 行 journal；所有操作需确认，完成后刷新真实状态。删除项目记录不会删除远端文件或服务。

## 远端产物

| 产物 | 位置 | 说明 |
| --- | --- | --- |
| systemd 服务 | `/etc/systemd/system/runory-go-<名称>.service` | 工作目录为项目目录；`Environment` 写入 `PORT` 与自定义变量；`ExecStart=<可执行文件> <参数>`；`SuccessExitStatus=143`，`Restart=on-failure` |
| unit 草稿 | `<项目>/.runory/runory-go-<名称>.service` | 无 root / 免密 sudo 时保留于此并返回失败 |

启用构建时，以 SSH 用户在项目目录执行 `go build -o <可执行文件> <包>`（Go 所在目录置于 `PATH` 最前，输出目录自动创建）。之后校验文件存在且可执行：缺失会明确失败，不会安装服务；存在但无执行位时尝试 `chmod u+x`。

## 安全与兼容性

- 目标服务器需要 Linux/systemd；启用构建时还需要 Go 工具链；服务安装需要 root 或免密 sudo。缺失项返回 `UNSUPPORTED_REMOTE`，不会记为部署成功。
- 项目路径、可执行文件、Go 路径仅允许 `A-Za-z0-9 . _ - / + @`（不支持空格）；构建包不能以 `-` 开头（避免被 `go build` 当作选项）、不能是绝对路径或含 `..`；程序参数字符额外允许 `= : ,`。这些值会进入 systemd unit，Rust 端渲染前校验并按 systemd 规则逐词加引号，前端校验仅用于提前提示。
- 环境变量要求 `KEY=value`，键为字母/下划线开头的标识符；`PORT`（有专门字段）、`PATH` 与 `LD_*`（可改变加载的程序或库）不允许设置。
- 同一服务器上服务名相同（名称转 slug 后相同）或端口相同的 Go 项目不可重复创建。
- 控制 IPC `deployment_go_control` 只接受项目 ID 与 start/stop/restart 枚举，Rust 根据 SSH 会话所属 profile 查找 Go 项目并生成服务名，不接受任意命令。状态查询与日志复用已有的按服务名的只读接口。
- 构建以 SSH 用户执行而服务以运行用户启动：运行用户必须能执行二进制并读取项目目录；`go build` 需要服务器能访问模块代理。
- 本次未包含：Go 工具链安装与版本切换、交叉编译参数（`GOOS`/`GOARCH`、`-ldflags`、`-tags`）、nginx 反向代理 / 域名绑定、TLS、防火墙放行、启动后的端口探活。部署只检查服务在 2 秒后仍为 `active`。
- 配置持久化在 `deployment.db` 新增的 `go_json` 列（旧库启动时自动 `ALTER TABLE`，幂等），云同步载荷同步携带可选 `go` 字段；旧数据缺少该字段时按无 Go 配置处理。
- 与 Java、Node 共用 `systemd_install.rs` 中的 unit 安装脚本。

## 验证

Rust：unit 渲染（环境变量、参数、绝对路径）、可执行文件 / 构建包 / 环境变量 / 参数校验、仓库持久化、端口与服务冲突、其他项目类型拒绝 Go 配置、运行时解析等单元测试。前端：草稿校验、面板渲染、确认后才调用控制 IPC、草稿不显示控制按钮；i18n 完整性测试通过。远端准备脚本做了 `bash -n` 语法检查，并在本机验证了「二进制缺失」「已有二进制」「缺执行位」分支；本机没有 Go，`go build` 分支未实际执行。

尚未在带 systemd 与 Go 的真实 Linux 服务器上做端到端验收（仓库现有 Alpine OpenSSH fixture 不运行 systemd），上线前需要另行验证构建路径与 Debian/Ubuntu、RHEL 系布局。
