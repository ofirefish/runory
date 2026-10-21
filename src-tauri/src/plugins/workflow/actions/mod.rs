pub mod checkpoint;
pub mod database;
pub mod fs_extra;
pub mod git;
pub mod git_mutate;
pub mod http;
pub mod network;
pub mod nginx_action;
pub mod preflight;
pub mod probe;
pub mod python;
pub mod release;
pub mod remote_fs;
pub mod secret_env;
pub mod ssh;
pub mod system;
pub mod systemd;
pub mod systemd_service;
pub mod template;
pub mod util;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::ast::PrivilegeMode;
use crate::domain::SessionId;
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::{PermissionService, PluginCapability};
use crate::plugins::secrets::SharedSecretResolver;
use crate::ssh::ServerSessionManager;
use crate::tools::RiskLevel;

#[async_trait]
pub trait WorkflowAction: Send + Sync {
    fn id(&self) -> &'static str;

    fn required_capabilities(
        &self,
        input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>>;

    fn risk(&self, input: &serde_json::Value) -> PluginResult<RiskLevel>;

    async fn plan(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionPlan>;

    async fn execute(
        &self,
        ctx: &ActionContext,
        input: &serde_json::Value,
    ) -> PluginResult<ActionResult>;
}

/// Core-owned action context. Plugins never receive this directly.
/// Do not place secret plaintext here.
pub struct ActionContext {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub workflow_id: String,
    pub host_id: Option<String>,
    pub host_os: Option<String>,
    pub host_arch: Option<String>,
    pub host_distribution: Option<String>,
    pub host_version: Option<String>,
    pub session_id: Option<SessionId>,
    pub sessions: Option<Arc<ServerSessionManager>>,
    pub permission_service: Arc<PermissionService>,
    /// Package root for reading plugin-bundled template files.
    pub plugin_root: Option<PathBuf>,
    /// Core-owned secret resolver. Never put resolved plaintext into WorkflowContext.
    pub secret_resolver: Option<SharedSecretResolver>,
    pub app_handle: Option<tauri::AppHandle>,
    pub cancellation: CancellationToken,
}

impl ActionContext {
    pub fn check_cancelled(&self) -> PluginResult<()> {
        if self.cancellation.is_cancelled() {
            Err(PluginError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionPlan {
    pub action_id: String,
    pub risk: RiskLevel,
    pub summary: String,
    pub changes: Vec<PlannedChange>,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlannedChange {
    RemoteCommand {
        executable: String,
        args: Vec<String>,
        privilege: PrivilegeMode,
    },
    RemoteFileWrite {
        path: String,
    },
    RemoteFileDelete {
        path: String,
    },
    PackageInstall {
        packages: Vec<String>,
    },
    ServiceRestart {
        service: String,
    },
    DatabaseMigration {
        description: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub outputs: serde_json::Value,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

impl ActionResult {
    pub fn outputs_only(outputs: serde_json::Value) -> Self {
        Self {
            outputs,
            stdout: None,
            stderr: None,
            exit_code: None,
        }
    }

    /// Truncate and redact stdout/stderr before persistence or UI emission.
    pub fn sanitize(mut self) -> Self {
        const MAX: usize = 1024 * 1024;
        if let Some(ref mut out) = self.stdout {
            *out = redact_secrets(out);
            if out.len() > MAX {
                out.truncate(MAX);
                out.push_str("\n…[truncated]");
            }
        }
        if let Some(ref mut err) = self.stderr {
            *err = redact_secrets(err);
            if err.len() > MAX {
                err.truncate(MAX);
                err.push_str("\n…[truncated]");
            }
        }
        self.outputs = redact_json_secrets(self.outputs);
        self
    }
}

/// Redact common secret-bearing patterns from untrusted remote output.
pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_owned();
    for (needle, replacement) in [
        ("password=", "password=********"),
        ("PASSWORD=", "PASSWORD=********"),
        ("passwd=", "passwd=********"),
        ("secret=", "secret=********"),
        ("SECRET=", "SECRET=********"),
        ("token=", "token=********"),
        ("TOKEN=", "TOKEN=********"),
        ("api_key=", "api_key=********"),
        ("API_KEY=", "API_KEY=********"),
        ("Authorization: Bearer ", "Authorization: Bearer ********"),
        (
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "[REDACTED PRIVATE KEY]",
        ),
        ("-----BEGIN RSA PRIVATE KEY-----", "[REDACTED PRIVATE KEY]"),
        ("-----BEGIN PRIVATE KEY-----", "[REDACTED PRIVATE KEY]"),
    ] {
        if let Some(idx) = out.find(needle) {
            if needle.ends_with('=') || needle.ends_with(' ') {
                let start = idx + needle.len();
                let end = out[start..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                    .map(|n| start + n)
                    .unwrap_or(out.len());
                out.replace_range(idx..end, replacement);
            } else {
                // PEM header: drop until matching END or end of string.
                let start = idx;
                let end_marker = needle.replacen("BEGIN", "END", 1);
                let end = out[start..]
                    .find(&end_marker)
                    .map(|n| start + n + end_marker.len())
                    .unwrap_or(out.len());
                out.replace_range(start..end, replacement);
            }
        }
    }
    out
}

fn redact_json_secrets(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(redact_secrets(&s)),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(redact_json_secrets).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                let key_l = key.to_ascii_lowercase();
                if matches!(
                    key_l.as_str(),
                    "password" | "passphrase" | "secret" | "token" | "api_key" | "private_key"
                ) {
                    out.insert(key, serde_json::Value::String("********".into()));
                } else {
                    out.insert(key, redact_json_secrets(val));
                }
            }
            serde_json::Value::Object(out)
        }
        other => other,
    }
}

pub struct ActionRegistry {
    actions: HashMap<String, Arc<dyn WorkflowAction>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(system::SystemDetectAction));
        registry.register(Arc::new(probe::SystemCommandExistsAction));
        registry.register(Arc::new(probe::SystemDiskAction));
        registry.register(Arc::new(probe::SystemMemoryAction));
        registry.register(Arc::new(network::NetworkTcpCheckAction));
        registry.register(Arc::new(ssh::SshCommandAction));
        registry.register(Arc::new(remote_fs::RemoteFsReadAction));
        registry.register(Arc::new(remote_fs::RemoteFsWriteAction));
        registry.register(Arc::new(fs_extra::RemoteFsMkdirAction));
        registry.register(Arc::new(fs_extra::RemoteFsSymlinkAction));
        registry.register(Arc::new(template::TemplateRenderAction));
        registry.register(Arc::new(git::GitCloneAction));
        registry.register(Arc::new(git::GitRevParseAction));
        registry.register(Arc::new(git_mutate::GitFetchAction));
        registry.register(Arc::new(git_mutate::GitCheckoutAction));
        registry.register(Arc::new(python::PythonDetectAction));
        registry.register(Arc::new(python::PythonVenvAction));
        registry.register(Arc::new(python::PythonPipInstallAction));
        registry.register(Arc::new(systemd::SystemdInstallAction));
        registry.register(Arc::new(systemd::SystemdDaemonReloadAction));
        registry.register(Arc::new(systemd_service::SystemdEnableAction));
        registry.register(Arc::new(systemd_service::SystemdRestartAction));
        registry.register(Arc::new(systemd_service::SystemdReloadAction));
        registry.register(Arc::new(systemd::SystemdStatusAction));
        registry.register(Arc::new(nginx_action::NginxTestAction));
        registry.register(Arc::new(nginx_action::NginxReloadAction));
        registry.register(Arc::new(http::HttpCheckAction));
        registry.register(Arc::new(database::DatabaseMigrationAction));
        registry.register(Arc::new(checkpoint::ChangesetCheckpointAction));
        registry.register(Arc::new(preflight::SystemPreflightReportAction));
        registry.register(Arc::new(release::ReleaseActivateAction));
        registry.register(Arc::new(secret_env::RemoteFsWriteEnvAction));
        registry
    }

    pub fn register(&mut self, action: Arc<dyn WorkflowAction>) {
        self.actions.insert(action.id().to_owned(), action);
    }

    pub fn get(&self, id: &str) -> PluginResult<Arc<dyn WorkflowAction>> {
        self.actions
            .get(id)
            .cloned()
            .ok_or_else(|| PluginError::ActionNotFound(id.to_owned()))
    }

    pub fn contains(&self, id: &str) -> bool {
        self.actions.contains_key(id)
    }
}

impl Default for ActionRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    #[test]
    fn redacts_password_and_token_patterns() {
        let raw = "password=secret123 token=abc";
        let out = redact_secrets(raw);
        assert!(!out.contains("secret123"));
        assert!(!out.contains("abc"));
    }

    #[test]
    fn sanitize_redacts_json_secret_keys() {
        let result = ActionResult {
            outputs: serde_json::json!({ "api_key": "leak", "ok": true }),
            stdout: Some("password=leak".into()),
            stderr: None,
            exit_code: Some(0),
        }
        .sanitize();
        assert_eq!(result.outputs["api_key"], "********");
        assert!(!result.stdout.unwrap().contains("leak"));
    }
}

pub fn risk_requires_approval(risk: RiskLevel) -> bool {
    !matches!(risk, RiskLevel::R0)
}
