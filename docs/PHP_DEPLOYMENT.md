# PHP 项目部署

本增量参考宝塔面板的 PHP 项目配置（PHP 版本、运行目录、伪静态、进程管理、防跨站、上传/内存限制）和 Runory 现有的 Python 项目流程（四步向导、SSH 会话上的 Rust DeploymentService、systemd 生命周期、部署后状态核验）。不依赖宝塔面板或常驻 Runory Agent。

## 使用流程

1. 在「部署 → PHP 项目」中填写项目名称和项目目录，可选通过 Git 克隆。
2. 选择 PHP 版本（读取服务器已安装的 `php`/`php8.x`），设置运行目录（如 Laravel/ThinkPHP 的 `public`），选择是否在存在 `composer.json` 时执行 `composer install --no-dev`。
3. 配置站点：监听端口、域名（可多个，空表示任意主机）、伪静态（无 / 单入口 `index.php`，覆盖 Laravel、ThinkPHP、Symfony、Yii、WordPress）。
4. 配置 PHP-FPM：进程管理（动态/静态/按需）、最大进程数（1–512）、内存限制（16–8192 MB）、最长执行时间（1–3600 秒）、上传上限（1–2048 MB）、`open_basedir` 防跨站、运行用户、是否开机启动。
5. 完成后确认远端部署。列表中可单独启动、停止、重启 php-fpm 服务并读取最近 200 行 journal；所有操作需确认，完成后刷新真实状态。删除项目记录不会删除远端文件、服务或 nginx 站点。

## 远端产物

| 产物 | 位置 | 说明 |
| --- | --- | --- |
| php-fpm 配置 | `<项目>/.runory/fpm.conf` | 独立 pool，Unix socket `/run/runory-php-<名称>.sock` |
| systemd 服务 | `/etc/systemd/system/runory-php-<名称>.service` | 独立 php-fpm master（`--nodaemonize --fpm-config`），不改动系统自带的 php-fpm 服务 |
| nginx 站点 | `/etc/nginx/conf.d/runory-php-<名称>.conf` | 带 `# runory-php:` 标记 |

php-fpm 二进制通过版本匹配解析（`php-fpm8.2`、`php-fpm82`、`php-fpm`，必须与所选 CLI 的主次版本一致），socket 属主取自 `nginx.conf` 的 `user`（无法识别时回退 `www-data`/`nginx`）。

## 安全与兼容性

- 目标服务器需要 Linux/systemd、PHP CLI 与对应版本 php-fpm、nginx；启用 Composer 时需要 `composer`；服务和站点安装需要 root 或免密 sudo。缺失项返回 `UNSUPPORTED_REMOTE`，不会记为部署成功。无权限时 unit 与配置草稿保留在 `.runory` 下。
- 站点写入后先执行 `nginx -t`；失败时恢复旧站点（或删除新站点）并不重载 nginx，避免影响同一 nginx 上的其他站点。
- 项目路径、运行目录仅允许 `A-Za-z0-9 . _ - / + @`，因此 PHP 项目路径不支持空格；域名按主机名校验。这些值会进入 nginx/php-fpm 配置，Rust 端在渲染前校验，前端校验仅用于提前提示。
- 同一服务器上服务名相同（名称转 slug 后相同）或端口加域名相同的 PHP 项目不可重复创建。
- 控制 IPC `deployment_php_control` 只接受项目 ID 与 start/stop/restart 枚举，Rust 根据 SSH 会话所属 profile 查找 PHP 项目并生成服务名，不接受任意命令。状态查询与日志复用已有的按服务名的只读接口。
- 本次未包含：TLS/证书、防火墙放行、PHP 扩展安装、多版本 PHP 的安装与切换、`disable_functions` 编辑、站点级 PHP.ini 自由编辑。停止服务只停止 php-fpm，nginx 站点仍在线并对 PHP 请求返回 502。
- 配置持久化在 `deployment.db` 新增的 `php_json` 列（旧库启动时自动 `ALTER TABLE`，幂等），云同步载荷同步携带可选 `php` 字段；旧数据缺少该字段时按无 PHP 配置处理。

## 验证

Rust：渲染（pool/unit/nginx）、校验、仓库持久化与冲突、旧库迁移、php-fpm 路径解析等单元测试；两段远端脚本通过 `bash -n` 语法检查，安装脚本使用的文件拆分逻辑已在本地 shell 验证。前端：草稿校验、面板渲染、确认后才调用控制 IPC、草稿不显示控制按钮；i18n 完整性测试通过。

尚未在带 systemd + nginx + php-fpm 的真实 Linux 服务器上做端到端验收（仓库现有 Alpine OpenSSH fixture 不运行 systemd），上线前需要另行验证 Debian/Ubuntu 与 RHEL 系两种布局。
