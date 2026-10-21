//! PP1.1 workspace integration security / lifecycle tests.

#![cfg(test)]

use std::fs;
use std::io::Write;
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::plugins::contributions::list_plugin_workspace_entries;
use crate::plugins::manifest::model::PluginWorkspaceCategory;
use crate::plugins::registry::service::PluginInstallRequest;
use crate::plugins::test_support::PluginTestHarness;

fn write_minimal_png(path: &Path) {
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, PNG).unwrap();
}

fn pack_workspace_plugin(dir: &Path, category: &str, logo: &str) -> std::path::PathBuf {
    fs::create_dir_all(dir.join("workflows")).unwrap();
    write_minimal_png(&dir.join(logo));
    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "id": "runory.workspace-demo",
        "name": "Workspace Demo",
        "version": "1.0.0",
        "publisher": { "id": "runory", "name": "Runory" },
        "apiVersion": "1",
        "logo": logo,
        "category": category,
        "permissions": ["host.read", "ssh.exec"],
        "contributes": {
            "commands": [
                { "id": "demo.run", "title": "Run" },
                { "id": "demo.logs", "title": "Logs" }
            ],
            "workflows": [
                { "id": "demo.run", "file": "workflows/run.yaml" },
                { "id": "demo.logs", "file": "workflows/logs.yaml" }
            ],
            "workspaceEntries": [{
                "id": "demo.main",
                "title": "Demo",
                "description": "Workspace card",
                "command": "demo.run",
                "actions": ["demo.run", "demo.logs"],
                "order": 50
            }]
        }
    });
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    for name in ["run", "logs"] {
        fs::write(
            dir.join(format!("workflows/{name}.yaml")),
            format!(
                r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: demo.{name}
  name: {name}
steps:
  - id: detect
    uses: system.detect
"#
            ),
        )
        .unwrap();
    }

    let zip_path = dir.join("plugin.runory-plugin");
    let file = fs::File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for rel in [
        "manifest.json",
        logo,
        "workflows/run.yaml",
        "workflows/logs.yaml",
    ] {
        zip.start_file(rel, options).unwrap();
        zip.write_all(&fs::read(dir.join(rel)).unwrap()).unwrap();
    }
    zip.finish().unwrap();
    zip_path
}

#[tokio::test]
async fn workspace_entries_respect_category_and_disable() {
    let harness = PluginTestHarness::new();
    let root = tempfile::tempdir().unwrap();
    let zip = pack_workspace_plugin(root.path(), "deployment", "assets/logo.png");
    let plugin_id = harness.install_zip(&zip).await;

    let entries = list_plugin_workspace_entries(
        harness.registry.as_ref(),
        "0.1.1",
        Some(PluginWorkspaceCategory::Deployment),
    )
    .await
    .expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].entry_id, "demo.main");
    assert_eq!(entries[0].command, "demo.run");
    assert!(entries[0].logo_url.is_some());

    let ops = list_plugin_workspace_entries(
        harness.registry.as_ref(),
        "0.1.1",
        Some(PluginWorkspaceCategory::Operations),
    )
    .await
    .expect("ops");
    assert!(ops.is_empty());

    harness
        .registry
        .set_enabled(&plugin_id, false)
        .await
        .expect("disable");
    let after_disable = list_plugin_workspace_entries(
        harness.registry.as_ref(),
        "0.1.1",
        Some(PluginWorkspaceCategory::Deployment),
    )
    .await
    .expect("list disabled");
    assert!(after_disable.is_empty());
}

#[tokio::test]
async fn revoked_plugin_hidden_from_workspace() {
    let harness = PluginTestHarness::new();
    let root = tempfile::tempdir().unwrap();
    let zip = pack_workspace_plugin(root.path(), "operations", "assets/logo.png");
    let plugin_id = harness.install_zip(&zip).await;

    harness
        .registry
        .repository()
        .upsert_revocation("plugin", &plugin_id, None, Some("test"))
        .expect("revoke");

    let entries = list_plugin_workspace_entries(
        harness.registry.as_ref(),
        "0.1.1",
        Some(PluginWorkspaceCategory::Operations),
    )
    .await
    .expect("list");
    assert!(entries.is_empty());
}

#[tokio::test]
async fn missing_category_hides_workspace_entries() {
    let harness = PluginTestHarness::new();
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("workflows")).unwrap();
    let manifest = serde_json::json!({
        "schemaVersion": 1,
        "id": "runory.no-category",
        "name": "No Category",
        "version": "1.0.0",
        "publisher": { "id": "runory", "name": "Runory" },
        "apiVersion": "1",
        "permissions": ["host.read"],
        "contributes": {
            "commands": [{ "id": "demo.run", "title": "Run" }],
            "workflows": [{ "id": "demo.run", "file": "workflows/run.yaml" }],
            "workspaceEntries": [{
                "id": "demo.main",
                "title": "Demo",
                "command": "demo.run",
                "actions": [],
                "order": 1
            }]
        }
    });
    fs::write(
        root.path().join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(
        root.path().join("workflows/run.yaml"),
        r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: demo.run
  name: run
steps:
  - id: detect
    uses: system.detect
"#,
    )
    .unwrap();
    let zip_path = root.path().join("plugin.runory-plugin");
    let file = fs::File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for rel in ["manifest.json", "workflows/run.yaml"] {
        zip.start_file(rel, options).unwrap();
        zip.write_all(&fs::read(root.path().join(rel)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();

    let _ = harness
        .registry
        .install(PluginInstallRequest {
            package_path: zip_path.display().to_string(),
            granted_permissions: None,
            allow_unsigned: true,
        })
        .await
        .expect("install");

    let entries = list_plugin_workspace_entries(harness.registry.as_ref(), "0.1.1", None)
        .await
        .expect("list");
    assert!(entries.is_empty());
}
