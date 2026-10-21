# HTML 项目部署

本增量参考宝塔面板的静态站点 / HTML 项目配置（网站目录、域名、端口、默认文档、伪静态、gzip、静态资源缓存）和 Runory 现有的 PHP / Python 项目流程（四步向导、SSH 会话上的 Rust DeploymentService、nginx 站点校验与回滚）。不依赖宝塔面板或常驻 Runory Agent。

## 使用流程

1. 在「部署 → HTML 项目」中填写项目名称和项目目录，可选通过 Git 克隆。
2. 设置网站根目录（相对项目目录，如前端构建产物 `dist`，留空为项目根）、默认文档（默认 `index.html index.htm`）、是否启用单页应用回退（未知路径返回第一个默认文档，适配 Vue / React 路由）。
3. 配置站点：监听端口、域名（可多个，可带 `*.` 前缀，空表示任意主机）、gzip、静态资源 7 天浏览器缓存。
4. 完成后确认远端部署。列表显示最近一次部署结果（配置中 / 未部署 / 部署中 / 已部署 / 失败），可重新部署、读取 nginx 错误日志最近 200 行、编辑或删除项目记录。删除项目记录不会删除远端文件或 nginx 站点。

## 远端产物

| 产物 | 位置 | 说明 |
| --- | --- | --- |
| nginx 站点 | `/etc/nginx/conf.d/runory-html-<名称>.conf` | 首行带 `# runory-html:` 标记；禁止访问以 `.` 开头的路径（`.well-known` 除外），因此项目内的 `.runory` 草稿不会被对外提供 |
| 站点草稿 | `<项目>/.runory/runory-html-<名称>.conf` | 无 root / 免密 sudo 时保留于此并返回失败 |

静态站点没有进程，因此不创建 systemd 服务，也没有启动 / 停止 / 重启操作。

## 安全与兼容性

- 目标服务器需要 nginx；写入 `/etc/nginx/conf.d` 需要 root 或免密 sudo。缺失 nginx 返回 `UNSUPPORTED_REMOTE`，项目目录或网站根目录不存在返回 `INVALID_OPERATION`，均不会记为部署成功。
- 站点写入后先执行 `nginx -t`；失败时恢复旧站点（或删除新站点）并不重载 nginx，避免影响同一 nginx 上的其他站点。重载优先使用 systemd，没有 systemd 时使用 `nginx -s reload`。
- 项目路径与网站根目录仅允许 `A-Za-z0-9 . _ - / + @`（不支持空格）；域名按主机名校验（允许 `*.` 通配前缀与 `localhost`）；默认文档只能是不含目录、不以 `.` 开头的文件名。这些值会进入 nginx 配置，Rust 端渲染前校验，前端校验仅用于提前提示。
- 同一服务器上站点文件名相同（名称转 slug 后相同）或端口加域名相同的 HTML 项目不可重复创建。
- nginx 工作进程用户必须对网站根目录及其上级目录有读取 / 进入权限，否则站点会返回 403；Runory 不修改文件权限。
- 本次未包含：TLS / 证书、HTTP 跳转 HTTPS、防盗链、自定义 404 页面、目录浏览、前端构建、站点启用 / 停用、删除远端站点。
- 配置持久化在 `deployment.db` 新增的 `html_json` 列（旧库启动时自动 `ALTER TABLE`，幂等），云同步载荷同步携带可选 `html` 字段；旧数据缺少该字段时按无 HTML 配置处理。HTML 项目的 `restart` 固定为 `none`。

## 标签栏变更

「部署」页的项目类型标签栏移除了「Net 项目」和「其他项目」，默认选中 PHP。只保留已有专用面板的类型（PHP、Java、Node、Go、Python、HTML）以及仍使用通用面板的「反向代理」。后端 `ProjectKind` 枚举未改动，之前保存的 `net` / `other` 类型记录不会丢失，但在界面中不再显示。

## 验证

Rust：nginx 站点渲染（SPA 回退、gzip、缓存、无域名、首个默认文档）、路径 / 根目录 / 域名 / 默认文档校验与注入拒绝、仓库持久化、站点与监听冲突、其他项目类型拒绝 HTML 配置等单元测试。前端：草稿校验、面板渲染、确认后才调用部署 IPC 且状态依次写为 provisioning / provisioned、草稿不显示日志按钮；i18n 完整性与「部署」页标签测试通过。安装脚本通过 `bash -n` 语法检查，并验证了缺少 nginx 时返回 90。

尚未在带 nginx 的真实 Linux 服务器上做端到端验收（仓库现有 Alpine OpenSSH fixture 不含 nginx），上线前需要另行验证 Debian/Ubuntu 与 RHEL 系两种布局。
