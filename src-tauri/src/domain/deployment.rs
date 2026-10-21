use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::SessionId;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BuildPreset {
    None,
    Npm,
    Pnpm,
    Cargo,
    Pip,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProjectKind {
    Php,
    Java,
    Node,
    Go,
    Python,
    Net,
    ReverseProxy,
    Html,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PythonStartupMethod {
    Command,
    Uwsgi,
    Gunicorn,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PythonProtocol {
    Wsgi,
    Asgi,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentSetupStatus {
    #[default]
    Draft,
    Ready,
    Provisioning,
    Provisioned,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentSourceKind {
    #[default]
    ExistingPath,
    GitClone,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PythonProjectConfig {
    pub port: u16,
    pub python_bin: String,
    pub startup: PythonStartupMethod,
    pub entry_file: String,
    pub protocol: PythonProtocol,
    pub application: String,
    pub run_user: String,
    pub requirements_path: String,
    /// Relative argv for command-line startup (e.g. `app.py` or `manage.py runserver`).
    #[serde(default)]
    pub startup_command: String,
    #[serde(default = "default_python_processes")]
    pub processes: u16,
    #[serde(default = "default_python_processes")]
    pub threads: u16,
    #[serde(default = "default_python_auto_start")]
    pub auto_start: bool,
}

fn default_python_processes() -> u16 {
    1
}
fn default_python_auto_start() -> bool {
    true
}

/// Nginx `location /` behaviour. `FrontController` covers Laravel, ThinkPHP,
/// Symfony, Yii and WordPress, which all route unknown paths to `index.php`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PhpRewriteMode {
    #[default]
    None,
    FrontController,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PhpProcessManager {
    #[default]
    Dynamic,
    Static,
    Ondemand,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PhpProjectConfig {
    /// CLI binary name (`php`, `php8.2`, `php82`); the matching php-fpm is resolved remotely.
    pub php_bin: String,
    pub port: u16,
    /// Space-separated host names for nginx `server_name`; empty matches any host.
    #[serde(default)]
    pub server_name: String,
    /// Web root relative to the project path (e.g. `public`); empty means the project root.
    #[serde(default)]
    pub run_dir: String,
    #[serde(default)]
    pub rewrite: PhpRewriteMode,
    pub run_user: String,
    #[serde(default)]
    pub process_manager: PhpProcessManager,
    #[serde(default = "default_php_max_children")]
    pub max_children: u16,
    #[serde(default = "default_php_memory_limit_mb")]
    pub memory_limit_mb: u16,
    #[serde(default = "default_php_max_execution_seconds")]
    pub max_execution_seconds: u16,
    #[serde(default = "default_php_upload_max_mb")]
    pub upload_max_mb: u16,
    /// Restricts PHP file access to the project, `/tmp` and system PHP libraries.
    #[serde(default = "default_php_flag_on")]
    pub open_basedir: bool,
    #[serde(default = "default_php_flag_on")]
    pub composer_install: bool,
    #[serde(default = "default_php_flag_on")]
    pub auto_start: bool,
}

fn default_php_max_children() -> u16 {
    8
}
fn default_php_memory_limit_mb() -> u16 {
    256
}
fn default_php_max_execution_seconds() -> u16 {
    60
}
fn default_php_upload_max_mb() -> u16 {
    50
}
fn default_php_flag_on() -> bool {
    true
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpRuntime {
    pub binary: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub project_path: String,
    pub php: PhpProjectConfig,
}

/// Same shape and semantics as `PythonControlRequest`; the target app must be a PHP project.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpControlRequest {
    pub session_id: SessionId,
    pub app_id: Uuid,
    pub action: PythonServiceAction,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum JavaBuildTool {
    #[default]
    None,
    Maven,
    Gradle,
}

/// Spring Boot style executable jar run by a dedicated systemd unit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JavaProjectConfig {
    /// `java` (resolved from PATH on the remote host) or an absolute path to a JDK/JRE `bin/java`.
    pub java_bin: String,
    /// Executable jar, relative to the project path or absolute.
    pub jar_path: String,
    pub port: u16,
    #[serde(default = "default_java_xms_mb")]
    pub xms_mb: u32,
    #[serde(default = "default_java_xmx_mb")]
    pub xmx_mb: u32,
    /// Extra JVM options, space separated (e.g. `-XX:+UseG1GC -Dspring.profiles.active=prod`).
    #[serde(default)]
    pub jvm_args: String,
    /// Program arguments appended after `-jar`.
    #[serde(default)]
    pub app_args: String,
    pub run_user: String,
    #[serde(default)]
    pub build_tool: JavaBuildTool,
    #[serde(default = "default_python_auto_start")]
    pub auto_start: bool,
}

fn default_java_xms_mb() -> u32 {
    256
}
fn default_java_xmx_mb() -> u32 {
    512
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    /// Absolute path of the `java` executable.
    pub binary: String,
    pub version: String,
    /// True when this is what `java` on the remote PATH resolves to.
    pub is_default: bool,
}

/// LTS releases offered for installation through the system package manager.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum JdkVersion {
    #[serde(rename = "8")]
    V8,
    #[serde(rename = "11")]
    V11,
    #[serde(rename = "17")]
    V17,
    #[serde(rename = "21")]
    V21,
    #[serde(rename = "25")]
    V25,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInstallJdkRequest {
    pub session_id: SessionId,
    pub version: JdkVersion,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaSetDefaultJdkRequest {
    pub session_id: SessionId,
    /// Absolute `bin/java` path taken from the runtime scan.
    pub binary: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub project_path: String,
    pub java: JavaProjectConfig,
}

/// Same shape and semantics as `PythonControlRequest`; the target app must be a Java project.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaControlRequest {
    pub session_id: SessionId,
    pub app_id: Uuid,
    pub action: PythonServiceAction,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NodePackageManager {
    #[default]
    Npm,
    Yarn,
    Pnpm,
}

/// `Script` runs a `package.json` script through the package manager; `Entry` runs `node <file>`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NodeStartMode {
    #[default]
    Script,
    Entry,
}

/// Node.js service run by a dedicated systemd unit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeProjectConfig {
    /// `node` (resolved from PATH on the remote host) or an absolute path to a `bin/node`.
    pub node_bin: String,
    #[serde(default)]
    pub package_manager: NodePackageManager,
    #[serde(default)]
    pub start_mode: NodeStartMode,
    /// `package.json` script for `Script` mode (e.g. `start`).
    #[serde(default)]
    pub script: String,
    /// Entry file relative to the project path for `Entry` mode (e.g. `server.js`).
    #[serde(default)]
    pub entry_file: String,
    /// Extra program arguments, space separated.
    #[serde(default)]
    pub app_args: String,
    pub port: u16,
    #[serde(default = "default_node_env")]
    pub node_env: String,
    #[serde(default = "default_python_auto_start")]
    pub install_deps: bool,
    /// Optional `package.json` script run after installing dependencies (e.g. `build`).
    #[serde(default)]
    pub build_script: String,
    pub run_user: String,
    #[serde(default = "default_python_auto_start")]
    pub auto_start: bool,
}

fn default_node_env() -> String {
    "production".into()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRuntime {
    /// Absolute path of the `node` executable.
    pub binary: String,
    pub version: String,
    /// True when this is what `node` on the remote PATH resolves to.
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub project_path: String,
    pub node: NodeProjectConfig,
}

/// Same shape and semantics as `PythonControlRequest`; the target app must be a Node project.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeControlRequest {
    pub session_id: SessionId,
    pub app_id: Uuid,
    pub action: PythonServiceAction,
}

/// Compiled Go service run by a dedicated systemd unit; optionally built on the server first.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GoProjectConfig {
    /// Executable relative to the project path or absolute; the build output when `build_enabled`.
    pub executable: String,
    /// Run `go build -o <executable> <build_package>` before installing the service.
    #[serde(default)]
    pub build_enabled: bool,
    /// `go` (resolved from PATH on the remote host) or an absolute path to a `bin/go`.
    #[serde(default = "default_go_bin")]
    pub go_bin: String,
    /// Package to build, relative to the project path (e.g. `.` or `./cmd/server`).
    #[serde(default = "default_go_build_package")]
    pub build_package: String,
    pub port: u16,
    /// Program arguments, space separated.
    #[serde(default)]
    pub app_args: String,
    /// Space separated `KEY=value` pairs added to the service environment.
    #[serde(default)]
    pub env_vars: String,
    pub run_user: String,
    #[serde(default = "default_python_auto_start")]
    pub auto_start: bool,
}

fn default_go_bin() -> String {
    "go".into()
}
fn default_go_build_package() -> String {
    ".".into()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoRuntime {
    /// Absolute path of the `go` executable.
    pub binary: String,
    pub version: String,
    /// True when this is what `go` on the remote PATH resolves to.
    pub is_default: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub project_path: String,
    pub go: GoProjectConfig,
}

/// Same shape and semantics as `PythonControlRequest`; the target app must be a Go project.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoControlRequest {
    pub session_id: SessionId,
    pub app_id: Uuid,
    pub action: PythonServiceAction,
}

/// Static website served by a dedicated nginx site; there is no process to supervise.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HtmlProjectConfig {
    pub port: u16,
    /// Space-separated host names for nginx `server_name`; empty matches any host.
    #[serde(default)]
    pub server_name: String,
    /// Web root relative to the project path (e.g. `dist`); empty means the project root.
    #[serde(default)]
    pub root_dir: String,
    /// Space-separated default documents, tried in order.
    #[serde(default = "default_html_index_files")]
    pub index_files: String,
    /// Route unknown paths to the first default document (single-page applications).
    #[serde(default)]
    pub spa_fallback: bool,
    #[serde(default = "default_html_flag_on")]
    pub gzip: bool,
    /// Long-lived browser caching for static assets (css, js, images, fonts).
    #[serde(default = "default_html_flag_on")]
    pub cache_static: bool,
}

fn default_html_index_files() -> String {
    "index.html index.htm".into()
}
fn default_html_flag_on() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub project_path: String,
    pub html: HtmlProjectConfig,
}

/// Which `Host` header the upstream receives.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProxyHostHeader {
    /// The host the visitor requested (`$host`).
    #[default]
    Original,
    /// The host of the target URL (`$proxy_host`).
    Target,
}

/// nginx reverse proxy site that forwards a listener to an upstream URL.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReverseProxyConfig {
    pub port: u16,
    /// Space-separated host names for nginx `server_name`; empty matches any host.
    #[serde(default)]
    pub server_name: String,
    /// Upstream, e.g. `http://127.0.0.1:3000` or `https://api.internal/v1`.
    pub target_url: String,
    #[serde(default)]
    pub host_header: ProxyHostHeader,
    /// Forward WebSocket upgrades.
    #[serde(default = "default_proxy_flag_on")]
    pub websocket: bool,
    #[serde(default = "default_proxy_max_body_mb")]
    pub max_body_mb: u16,
    #[serde(default = "default_proxy_timeout_seconds")]
    pub timeout_seconds: u16,
}

fn default_proxy_flag_on() -> bool {
    true
}
fn default_proxy_max_body_mb() -> u16 {
    50
}
fn default_proxy_timeout_seconds() -> u16 {
    60
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub proxy: ReverseProxyConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PythonServiceAction {
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonControlRequest {
    pub session_id: SessionId,
    pub app_id: Uuid,
    pub action: PythonServiceAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonRuntime {
    pub binary: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonPrepareEnvRequest {
    pub session_id: SessionId,
    pub project_path: String,
    pub python_bin: String,
    #[serde(default)]
    pub venv_path: String,
    #[serde(default)]
    pub requirements_path: String,
    #[serde(default)]
    pub install_packages: bool,
    #[serde(default)]
    pub startup: Option<PythonStartupMethod>,
    #[serde(default)]
    pub protocol: Option<PythonProtocol>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RestartTarget {
    None,
    Systemd { service: String },
    Pm2 { process: String },
    DockerCompose { service: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSetupRequest {
    pub session_id: SessionId,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeployRequest {
    pub session_id: SessionId,
    pub repository_path: String,
    pub branch: String,
    pub build: BuildPreset,
    pub restart: RestartTarget,
    #[serde(default)]
    pub requirements_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonProvisionRequest {
    pub session_id: SessionId,
    pub name: String,
    pub project_path: String,
    pub python: PythonProjectConfig,
    /// Absolute remote venv path; empty means `{project_path}/.venv`.
    #[serde(default)]
    pub venv_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentConfigRequest {
    pub session_id: SessionId,
    pub path: String,
    pub entries: Vec<EnvironmentEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslInspectRequest {
    pub session_id: SessionId,
    pub domain: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslIssueRequest {
    pub session_id: SessionId,
    pub domain: String,
    pub email: String,
    pub webroot: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    pub session_id: SessionId,
    pub source_path: String,
    pub destination_directory: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CronSchedule {
    Hourly,
    Daily,
    Weekly,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CronTask {
    Backup {
        source_path: String,
        destination_directory: String,
    },
    ServiceRestart {
        service: String,
    },
    GitPull {
        repository_path: String,
        branch: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronAddRequest {
    pub session_id: SessionId,
    pub schedule: CronSchedule,
    pub task: CronTask,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronRemoveRequest {
    pub session_id: SessionId,
    pub cron_id: Uuid,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CronEntry {
    pub id: Uuid,
    pub schedule: CronSchedule,
    pub task_kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentRecord {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub operation: String,
    pub target: String,
    pub started_at_epoch_seconds: u64,
    pub success: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentHistoryRequest {
    pub profile_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentApp {
    pub id: Uuid,
    pub profile_id: Uuid,
    pub name: String,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
    pub build: BuildPreset,
    pub restart: RestartTarget,
    #[serde(default)]
    pub project_kind: ProjectKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<PythonProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub php: Option<PhpProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java: Option<JavaProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go: Option<GoProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<HtmlProjectConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ReverseProxyConfig>,
    /// Wizard step currently in progress (1..=3). Defaults to 3 for legacy apps.
    #[serde(default = "default_setup_step")]
    pub setup_step: u8,
    #[serde(default)]
    pub setup_status: DeploymentSetupStatus,
    #[serde(default)]
    pub source_kind: DeploymentSourceKind,
    /// Absolute remote venv path; empty means `{repository_path}/.venv`.
    #[serde(default)]
    pub venv_path: String,
    pub updated_at_epoch_seconds: u64,
}

fn default_setup_step() -> u8 {
    4
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAppsListRequest {
    pub profile_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAppUpsertRequest {
    pub id: Option<Uuid>,
    pub profile_id: Uuid,
    pub name: String,
    pub repository_path: String,
    pub remote_url: String,
    pub branch: String,
    pub build: BuildPreset,
    pub restart: RestartTarget,
    #[serde(default)]
    pub project_kind: ProjectKind,
    #[serde(default)]
    pub python: Option<PythonProjectConfig>,
    #[serde(default)]
    pub php: Option<PhpProjectConfig>,
    #[serde(default)]
    pub java: Option<JavaProjectConfig>,
    #[serde(default)]
    pub node: Option<NodeProjectConfig>,
    #[serde(default)]
    pub go: Option<GoProjectConfig>,
    #[serde(default)]
    pub html: Option<HtmlProjectConfig>,
    #[serde(default)]
    pub proxy: Option<ReverseProxyConfig>,
    #[serde(default = "default_setup_step")]
    pub setup_step: u8,
    #[serde(default)]
    pub setup_status: DeploymentSetupStatus,
    #[serde(default)]
    pub source_kind: DeploymentSourceKind,
    #[serde(default)]
    pub venv_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAppDeleteRequest {
    pub id: Uuid,
    pub profile_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonStatusRequest {
    pub session_id: SessionId,
    pub service: String,
}
