# Java 项目部署

本增量参考宝塔面板的 Java 项目配置（项目 JAR、JDK 版本、端口、运行用户、JVM 内存与参数、开机启动）和 Runory 现有的 Python / PHP 项目流程（四步向导、SSH 会话上的 Rust DeploymentService、systemd 生命周期、部署后状态核验）。不依赖宝塔面板或常驻 Runory Agent。

## 使用流程

1. 在「部署 → Java 项目」中填写项目名称和项目目录，可选通过 Git 克隆。
2. 选择 JDK（读取服务器上 `PATH` 中的 `java` 以及 `/usr/lib/jvm`、`/usr/java`、`/opt`、`/usr/local` 下的 `bin/java`，也可固定使用 `PATH` 中的 `java`），填写可执行 JAR 路径（相对项目目录或绝对路径），可选部署前用 Maven / Gradle 构建。
3. 配置运行：监听端口、初始/最大堆内存（16–262144 MB，初始不大于最大）、JVM 参数、程序参数、运行用户、是否开机启动。
4. 项目列表中的「JDK 管理」可查看服务器上的 JDK、安装 OpenJDK（8 / 11 / 17 / 21 / 25）或切换系统默认 `java`，每项操作都需确认。
5. 完成后确认远端部署。列表中可单独启动、停止、重启服务并读取最近 200 行 journal；所有操作需确认，完成后刷新真实状态。删除项目记录不会删除远端文件或服务。

## 远端产物

| 产物 | 位置 | 说明 |
| --- | --- | --- |
| systemd 服务 | `/etc/systemd/system/runory-java-<名称>.service` | `java -Xms -Xmx -Dserver.port=<端口> <JVM 参数> -jar <JAR> <程序参数>`，工作目录为项目目录，`SuccessExitStatus=143`，`Restart=on-failure` |
| unit 草稿 | `<项目>/.runory/runory-java-<名称>.service` | 无 root / 免密 sudo 时保留于此并返回失败 |

`java` 在部署时于远端解析为绝对路径（`readlink -f`）后再写入 unit，因为 systemd 的 `ExecStart` 不搜索 `PATH`。构建以 SSH 用户在项目目录执行：优先 `./mvnw` / `./gradlew`，否则使用系统 `mvn` / `gradle`，并跳过测试。构建后 JAR 不存在会明确失败，不会安装服务。

## JDK 安装与切换

- 安装：`deployment_java_install_jdk` 只接受 8/11/17/21/25 的枚举，由 Rust 选择包名并通过服务器自带的包管理器安装（apt：`openjdk-<v>-jdk-headless`；dnf/yum：`java-<v>-openjdk-devel`，8 为 `java-1.8.0-openjdk-devel`；zypper 同理）。需要 root 或免密 sudo，否则返回 `PRIVILEGE_REQUIRED`，不会执行安装；没有受支持的包管理器返回 `UNSUPPORTED_REMOTE`；软件源缺少该版本时包管理器报错并原样返回。
- 切换：`deployment_java_set_default_jdk` 通过 `update-alternatives` / `alternatives` 切换系统级 `java`（已注册 `javac` 时一并切换），只接受 JDK 扫描得到的绝对 `bin/java` 路径，且该路径必须已在 alternatives 中注册，切换后再核对 `java` 实际指向。
- 项目级「切换」即在向导中选择具体 JDK（或 `PATH` 中的 `java`）；固定 JDK 的项目不受系统默认影响，使用 `java` 的项目重启后才会采用新默认值（unit 中写入的是部署时解析出的绝对路径，需重新部署才会改变）。
- 不含：卸载 JDK、Oracle / 手动解压的 JDK 安装、Alpine（apk）、自动为已有项目改写 JDK。

## 安全与兼容性

- 目标服务器需要 Linux/systemd 和 JDK；选择构建时还需要对应构建工具；服务安装需要 root 或免密 sudo。缺失项返回 `UNSUPPORTED_REMOTE`，不会记为部署成功。
- 项目路径、JAR 路径、JDK 路径仅允许 `A-Za-z0-9 . _ - / + @`（因此不支持空格）；JVM 参数必须以 `-` 开头，参数字符额外允许 `= : ,`。这些值会进入 systemd unit，Rust 端渲染前校验并按 systemd 规则逐词加引号，前端校验仅用于提前提示。
- JVM 参数不能设置 `-Xms/-Xmx`（有专门字段）、`-Dserver.port`（由端口字段注入）以及 `-agent*`、`-Xrun*`、`-Xdebug`，避免通过参数开放调试端口或加载任意 agent。
- 同一服务器上服务名相同（名称转 slug 后相同）或端口相同的 Java 项目不可重复创建。
- 控制 IPC `deployment_java_control` 只接受项目 ID 与 start/stop/restart 枚举，Rust 根据 SSH 会话所属 profile 查找 Java 项目并生成服务名，不接受任意命令。状态查询与日志复用已有的按服务名的只读接口。
- 本次未包含：JDK 安装与切换、nginx 反向代理 / 域名绑定、TLS、防火墙放行、WAR / Tomcat 部署、JAR 上传、日志轮转配置、启动后的端口探活。Spring Boot 以 `Type=simple` 运行，部署只检查服务在 2 秒后仍为 `active`，启动较慢才崩溃的应用请查看状态与日志。
- 配置持久化在 `deployment.db` 新增的 `java_json` 列（旧库启动时自动 `ALTER TABLE`，幂等），云同步载荷同步携带可选 `java` 字段；旧数据缺少该字段时按无 Java 配置处理。

## 验证

Rust：unit 渲染、JDK / JAR / 参数 / 堆内存校验、仓库持久化、端口与服务冲突、其他项目类型拒绝 Java 配置、运行时与解释器路径解析等单元测试。前端：草稿校验、面板渲染、确认后才调用控制 IPC、草稿不显示控制按钮；i18n 完整性测试通过。

尚未在带 systemd + JDK 的真实 Linux 服务器上做端到端验收（仓库现有 Alpine OpenSSH fixture 不运行 systemd），上线前需要另行验证 Debian/Ubuntu 与 RHEL 系两种布局，以及 Maven / Gradle 构建路径。
