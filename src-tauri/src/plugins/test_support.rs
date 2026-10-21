//! Shared test harness for plugin security / lifecycle tests.

#![cfg(test)]

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::plugins::error::PluginResult;
use crate::plugins::permissions::capability::PluginCapability;
use crate::plugins::permissions::grant::PermissionGrant;
use crate::plugins::permissions::service::PermissionService;
use crate::plugins::registry::repository::PluginRegistryRepository;
use crate::plugins::registry::service::{PluginInstallRequest, PluginRegistryService};
use crate::plugins::workflow::actions::{
    ActionContext, ActionPlan, ActionRegistry, ActionResult, WorkflowAction,
};
use crate::plugins::workflow::WorkflowExecutor;
use crate::ssh::ServerSessionManager;
use crate::tools::RiskLevel;

pub struct PluginTestHarness {
    pub _root: tempfile::TempDir,
    pub registry: Arc<PluginRegistryService>,
    pub executor: Arc<WorkflowExecutor>,
    pub permissions: Arc<PermissionService>,
}

impl PluginTestHarness {
    pub fn new() -> Self {
        Self::with_actions(ActionRegistry::with_builtins())
    }

    pub fn with_actions(mut actions: ActionRegistry) -> Self {
        // Always include a slow test action for cancellation coverage.
        actions.register(Arc::new(TestSleepAction));
        let root = tempfile::tempdir().expect("tempdir");
        let repository = Arc::new(PluginRegistryRepository::open_in_memory().expect("db"));
        let permissions = Arc::new(PermissionService::new(Arc::clone(&repository)));
        let registry = Arc::new(PluginRegistryService::new(
            root.path().join("plugins"),
            Arc::clone(&repository),
            Arc::clone(&permissions),
            "0.1.1",
        ));
        registry.startup_recovery().expect("startup");
        let executor = Arc::new(WorkflowExecutor::with_actions(
            Arc::clone(&registry),
            Arc::new(ServerSessionManager::default()),
            Arc::new(actions),
            None,
            None,
        ));
        Self {
            _root: root,
            registry,
            executor,
            permissions,
        }
    }

    pub async fn install_zip(&self, zip_path: &Path) -> String {
        let dto = self
            .registry
            .install(PluginInstallRequest {
                package_path: zip_path.display().to_string(),
                granted_permissions: None,
                allow_unsigned: true,
            })
            .await
            .expect("install");
        self.permissions
            .load_plugin(&dto.id)
            .await
            .expect("load grants");
        dto.id
    }

    pub async fn revoke(&self, plugin_id: &str, capability: PluginCapability) {
        let mut grants = self
            .registry
            .repository()
            .list_permissions(plugin_id)
            .expect("list");
        for grant in &mut grants {
            if grant.capability == capability {
                grant.granted = false;
            }
        }
        self.permissions
            .set_grants(plugin_id, grants)
            .await
            .expect("revoke");
    }

    pub async fn grant_only(&self, plugin_id: &str, caps: &[PluginCapability]) {
        let grants = caps
            .iter()
            .map(|capability| PermissionGrant {
                plugin_id: plugin_id.to_owned(),
                capability: *capability,
                granted: true,
                hosts: Vec::new(),
            })
            .collect();
        self.permissions
            .set_grants(plugin_id, grants)
            .await
            .expect("grant");
    }
}

pub struct TestSleepAction;

#[async_trait]
impl WorkflowAction for TestSleepAction {
    fn id(&self) -> &'static str {
        "test.sleep"
    }

    fn required_capabilities(
        &self,
        _input: &serde_json::Value,
    ) -> PluginResult<Vec<PluginCapability>> {
        Ok(vec![PluginCapability::HostRead])
    }

    fn risk(&self, _input: &serde_json::Value) -> PluginResult<RiskLevel> {
        Ok(RiskLevel::R0)
    }

    async fn plan(
        &self,
        ctx: &ActionContext,
        _input: &serde_json::Value,
    ) -> PluginResult<ActionPlan> {
        ctx.permission_service
            .require(&ctx.plugin_id, PluginCapability::HostRead)
            .await?;
        Ok(ActionPlan {
            action_id: self.id().into(),
            risk: RiskLevel::R0,
            summary: "Sleep for cancellation tests".into(),
            changes: Vec::new(),
            requires_approval: false,
        })
    }

    async fn execute(
        &self,
        ctx: &ActionContext,
        _input: &serde_json::Value,
    ) -> PluginResult<ActionResult> {
        for _ in 0..40 {
            ctx.check_cancelled()?;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(ActionResult::outputs_only(
            serde_json::json!({ "slept": true }),
        ))
    }
}

pub fn write_plugin_zip(
    path: &Path,
    plugin_id: &str,
    version: &str,
    permissions: &[&str],
    workflow_yaml: &str,
    workflow_id: &str,
) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    let file = fs::File::create(path).expect("create zip");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let perms = permissions
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = format!(
        r#"{{
  "schemaVersion": 1,
  "id": "{plugin_id}",
  "name": "{plugin_id}",
  "version": "{version}",
  "publisher": {{ "id": "runory", "name": "Runory" }},
  "apiVersion": "1",
  "minRunoryVersion": "0.1.0",
  "permissions": [{perms}],
  "contributes": {{
    "commands": [{{ "id": "{workflow_id}", "title": "Run" }}],
    "hostMenus": [{{ "command": "{workflow_id}", "group": "runory", "when": "host.os == 'linux'" }}],
    "workflows": [{{ "id": "{workflow_id}", "file": "workflows/main.yaml" }}]
  }}
}}"#
    );
    zip.start_file("manifest.json", options).expect("manifest");
    zip.write_all(manifest.as_bytes()).expect("write manifest");
    zip.start_file("workflows/main.yaml", options)
        .expect("workflow");
    zip.write_all(workflow_yaml.as_bytes()).expect("write wf");
    zip.finish().expect("finish");
}

pub fn hello_workflow_yaml() -> &'static str {
    r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: hello.inspect
  name: Inspect Linux Host
steps:
  - id: detect
    uses: system.detect
  - id: uname
    uses: ssh.command
    with:
      executable: uname
      args: ["-a"]
  - id: whoami
    uses: ssh.command
    with:
      executable: whoami
"#
}

pub fn detect_only_yaml() -> &'static str {
    r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: detect.only
  name: Detect Only
steps:
  - id: detect
    uses: system.detect
"#
}

pub fn sleep_workflow_yaml() -> &'static str {
    r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: cancel.sleep
  name: Cancel Sleep
steps:
  - id: sleep
    uses: test.sleep
"#
}

pub fn pack_fixture_hello(out: &Path) {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.hello");
    if fixture.join("manifest.json").is_file() {
        let file = fs::File::create(out).expect("create");
        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for entry in walkdir_simple(&fixture) {
            let rel = entry
                .strip_prefix(&fixture)
                .expect("rel")
                .to_string_lossy()
                .replace('\\', "/");
            if entry.is_file() {
                zip.start_file(&rel, options).expect("start");
                zip.write_all(&fs::read(&entry).expect("read"))
                    .expect("bytes");
            }
        }
        zip.finish().expect("finish");
    } else {
        write_plugin_zip(
            out,
            "runory.hello",
            "1.0.0",
            &["host.read", "ssh.exec"],
            hello_workflow_yaml(),
            "hello.inspect",
        );
    }
}

fn walkdir_simple(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
    }
    walk(root, &mut out);
    out
}
