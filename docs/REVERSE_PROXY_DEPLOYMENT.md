# 反向代理项目部署

本增量参考宝塔面板的反向代理配置（目标 URL、域名、发送域名、WebSocket、请求体与超时）和 Runory 现有的 HTML / PHP 项目流程（向导、SSH 会话上的 Rust DeploymentService、nginx 站点校验与回滚）。不依赖宝塔面板或常驻 Runory Agent。

## 使用流程

1. 在「部署 → 反向代理」中填写项目名称和目标 URL（如 `http://127.0.0.1:3000`、`https://api.internal/v1/`）。
2. 配置站点：监听端口、域名（可多个，可带 `*.` 前缀，空表示任意主机）。
3. 配置选项：发送给目标的 Host（原始域名 `$host` / 目标域名 `$proxy_host`）、请求体上限（1–2048 MB）、超时（1–3600 秒）、是否转发 WebSocket。
4. 完成后确认远端部署。列表显示目标地址和最近一次部署结果（配置中 / 未部署 / 部署中 / 已部署 / 失败），可重新部署、读取 nginx 错误日志最近 200 行、编辑或删除项目记录。删除项目记录不会删除远端 nginx 站点。

## 远端产物

| 产物 | 位置 | 说明 |
| --- | --- | --- |
| nginx 站点 | `/etc/nginx/conf.d/runory-proxy-<名称>.conf` | 首行带 `# runory-proxy:` 标记；`location /` 反代到目标，始终设置 `X-Real-IP`、`X-Forwarded-For`、`X-Forwarded-Proto`；目标为 https 时开启 `proxy_ssl_server_name`（SNI） |

反向代理没有项目目录：站点先写入远端私有临时文件，再以 `install -m 644` 安装到 `conf.d`，临时文件在脚本退出时清理。应用记录中的路径固定为上述站点文件路径（由名称推导，不接受输入），借助既有「同一主机路径唯一」约束防止两个代理占用同一站点文件。

## 安全与兼容性

- 目标服务器需要 nginx；写入 `/etc/nginx/conf.d` 需要 root 或免密 sudo，否则返回 `PRIVILEGE_REQUIRED` 且不做任何修改。缺失 nginx 返回 `UNSUPPORTED_REMOTE`。
- 站点写入后先执行 `nginx -t`；失败时恢复旧站点（或删除新站点）并不重载 nginx，避免影响同一 nginx 上的其他站点。重载优先使用 systemd，没有 systemd 时使用 `nginx -s reload`。
- 目标 URL 会写入 `proxy_pass`：只接受 `http(s)://主机[:端口][/路径]`；主机仅字母、数字、`.`、`-`（含 `localhost`、IPv4、域名和容器 / 服务别名）；路径仅 `A-Za-z0-9 _ - . / + @` 且不含 `..`；不接受账号密码、查询、片段、空白、`;`、`$`、IPv6 字面量。域名按主机名校验（允许 `*.` 通配前缀与 `localhost`）。校验在 Rust 端渲染前执行，前端校验仅用于提前提示。
- WebSocket 通过对每个请求设置 `Connection "upgrade"` 实现，避免定义 http 级 `map` 变量（多个站点重复定义同名 `map` 会使 nginx 拒绝配置）。代价是该站点的所有请求都使用 upgrade 连接头。
- 同一服务器上站点文件名相同（名称转 slug 后相同）或端口加域名相同的反向代理不可重复创建。
- 本次未包含：TLS / 证书、HTTP 跳转 HTTPS、缓存、多条路径规则（仅 `location /`）、负载均衡 / 多上游、内容替换、IP 黑白名单、站点启用 / 停用、删除远端站点。
- 配置持久化在 `deployment.db` 新增的 `proxy_json` 列（旧库启动时自动 `ALTER TABLE`，幂等），云同步载荷同步携带可选 `proxy` 字段；旧数据缺少该字段时按无代理配置处理。反向代理项目的 `restart` 固定为 `none`。

## 部署页变更

随本次变更，「部署」页原来的通用面板（Git / 部署 / 环境变量 / SSL / 备份 / 定时任务 / 历史 七个分区）及其表单、应用列表和界面测试已删除，因为所有项目类型现在都有专用面板，通用面板不再有入口。后端对应的 IPC（`deployment_git_setup`、`deployment_run`、`deployment_environment_write`、`deployment_ssl_*`、`deployment_backup`、`deployment_cron_*`、`deployment_history`）与前端 `lib/tauri/infrastructure.ts` 中的封装仍保留，只是界面上没有调用方。标签栏为 PHP、Java、Node、Go、Python、反向代理、HTML；之前保存的 `net` / `other` 类型记录仍在数据库中，但不再显示。

## 验证

Rust：nginx 站点渲染（转发头、WebSocket、https SNI、Host 模式）、目标 URL / 域名 / 限制校验与注入拒绝、仓库持久化与站点路径推导、草稿首屏可无目标但之后必须有、其他项目类型拒绝代理配置、站点与监听冲突等单元测试。前端：草稿校验、面板渲染、确认后才调用部署 IPC 且状态依次写为 provisioning / provisioned、草稿不显示日志按钮；「部署」页标签测试（七个标签、无 Net / 其他、按类型过滤、各类型空状态）。安装脚本通过 `bash -n` 语法检查。

尚未在带 nginx 的真实 Linux 服务器上做端到端验收，上线前需要另行验证 Debian/Ubuntu 与 RHEL 系两种布局。
