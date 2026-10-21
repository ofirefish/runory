# Node 项目部署

本增量参考宝塔面板的 Node 项目配置（项目目录、启动脚本、Node 版本、包管理器、端口、运行用户、开机启动）和 Runory 现有的 Python / PHP / Java 项目流程（四步向导、SSH 会话上的 Rust DeploymentService、systemd 生命周期、部署后状态核验）。不依赖宝塔面板、PM2 或常驻 Runory Agent。

## 使用流程

1. 在「部署 → Node 项目」中填写项目名称和项目目录，可选通过 Git 克隆。
2. 选择 Node.js（读取 `PATH` 中的 `node` 以及 `~/.nvm`、`/usr/local/nvm`、`/usr/local/n`、`/opt/node*`、`/usr/local/node*` 下的 `bin/node`，也可使用 `PATH` 中的 `node`）、包管理器（npm / yarn / pnpm）、是否部署前安装依赖、可选构建脚本，以及启动方式：`package.json` 脚本（如 `npm run start`）或入口文件（`node server.js`）。
3. 配置运行：监听端口（以 `PORT` 环境变量传入）、`NODE_ENV`（默认 `production`）、程序参数、运行用户、是否开机启动。
4. 完成后确认远端部署。列表中可单独启动、停止、重启服务并读取最近 200 行 journal；所有操作需确认，完成后刷新真实状态。删除项目记录不会删除远端文件或服务。

## 远端产物

| 产物 | 位置 | 说明 |
| --- | --- | --- |
| systemd 服务 | `/etc/systemd/system/runory-node-<名称>.service` | 工作目录为项目目录；`Environment` 写入 `PATH`（所选 Node 目录在前）、`NODE_ENV`、`PORT`；脚本模式 `ExecStart=<pm> run <script> [-- <参数>]`，入口模式 `ExecStart=<node> <项目>/<入口> <参数>`；`SuccessExitStatus=143`，`Restart=on-failure` |
| unit 草稿 | `<项目>/.runory/runory-node-<名称>.service` | 无 root / 免密 sudo 时保留于此并返回失败 |

部署时在远端把 `node` 解析为绝对路径（`readlink -f`），包管理器优先取 Node 同目录下的可执行文件，否则取 `PATH`，再写入 unit，因为 systemd 的 `ExecStart` 不搜索 `PATH`。依赖安装与构建以 SSH 用户在项目目录执行：npm 有 `package-lock.json` 时用 `npm ci`，pnpm 有 `pnpm-lock.yaml` 时用 `--frozen-lockfile`，其余用 `install`；随后运行构建脚本。脚本模式会校验 `package.json` 中存在该脚本，入口模式校验文件存在，缺失时明确失败，不会安装服务。npm 的程序参数通过 `--` 转发。

## 安全与兼容性

- 目标服务器需要 Linux/systemd 和 Node.js 与所选包管理器；服务安装需要 root 或免密 sudo。缺失项返回 `UNSUPPORTED_REMOTE`，不会记为部署成功。
- 项目路径、Node 路径、入口文件仅允许 `A-Za-z0-9 . _ - / + @`（不支持空格）；脚本名仅允许字母、数字和 `. _ - :`；入口只接受相对的 `.js/.mjs/.cjs`；`NODE_ENV` 为小写标识；程序参数字符额外允许 `= : ,`。这些值会进入 systemd unit，Rust 端渲染前校验并按 systemd 规则逐词加引号，前端校验仅用于提前提示。
- 同一服务器上服务名相同（名称转 slug 后相同）或端口相同的 Node 项目不可重复创建。
- 控制 IPC `deployment_node_control` 只接受项目 ID 与 start/stop/restart 枚举，Rust 根据 SSH 会话所属 profile 查找 Node 项目并生成服务名，不接受任意命令。状态查询与日志复用已有的按服务名的只读接口。
- 依赖安装以 SSH 用户执行而服务以运行用户启动：运行用户必须能读取项目目录、`node_modules` 与所选 Node 安装目录（例如位于其他用户 `~/.nvm` 下的 Node 可能无法访问）。
- 本次未包含：Node.js 安装与版本切换（可使用 nvm / n 等预先安装）、PM2 / 集群模式、nginx 反向代理 / 域名绑定、TLS、防火墙放行、`.env` 编辑（可用已有的环境变量写入功能）、启动后的端口探活。部署只检查服务在 2 秒后仍为 `active`。
- 配置持久化在 `deployment.db` 新增的 `node_json` 列（旧库启动时自动 `ALTER TABLE`，幂等），云同步载荷同步携带可选 `node` 字段；旧数据缺少该字段时按无 Node 配置处理。
- Java 与 Node 共用 `systemd_install.rs` 中的 unit 安装脚本。

## 验证

Rust：unit 渲染（脚本/入口模式、npm 参数转发）、Node 路径 / 脚本 / 入口 / 环境 / 参数校验、仓库持久化、端口与服务冲突、其他项目类型拒绝 Node 配置、运行时与路径解析等单元测试。前端：草稿校验、面板渲染、确认后才调用控制 IPC、草稿不显示控制按钮；i18n 完整性测试通过。远端准备脚本在本机用真实 node/npm 验证了依赖安装、构建、脚本缺失与入口缺失的分支（跳过 systemd 与用户检查）。

尚未在带 systemd 的真实 Linux 服务器上做端到端验收（仓库现有 Alpine OpenSSH fixture 不运行 systemd），上线前需要另行验证 Debian/Ubuntu 与 RHEL 系布局，以及 yarn / pnpm 路径。
