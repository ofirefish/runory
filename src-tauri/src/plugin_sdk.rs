//! Public developer-facing surface for the `runory-plugin` CLI.
//!
//! Reuses Core Plugin API 1.0 validators and package rules so CLI validation
//! matches desktop install checks.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use ring::digest::{self, SHA256};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::installer::package::{
    inspect_package, validate_zip_entry_name, MAX_FILE_COUNT, MAX_PACKAGE_BYTES,
    MAX_SINGLE_FILE_BYTES,
};
use crate::plugins::manifest::{parse_manifest_json, validate_compatibility, validate_manifest};
use crate::plugins::workflow::parser::parse_workflow_yaml;
use crate::plugins::workflow::validator::validate_workflow;

pub use crate::plugins::error::{PluginError as CliError, PluginResult as CliResult};

const DEFAULT_RUNORY_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub plugin_id: String,
    pub version: String,
    pub workflow_count: usize,
    pub permission_count: usize,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PackResult {
    pub output_path: PathBuf,
    pub plugin_id: String,
    pub version: String,
    pub file_count: usize,
    pub sha256_hex: String,
}

#[derive(Debug, Clone)]
pub struct InspectReport {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub publisher_id: String,
    pub publisher_name: String,
    pub api_version: String,
    pub logo: Option<String>,
    pub category: Option<String>,
    pub workspace_entries: Vec<String>,
    pub file_count: usize,
    pub sha256_hex: String,
    pub signature_status: String,
    pub permissions: Vec<String>,
    pub workflows: Vec<String>,
}

pub fn default_runory_version() -> &'static str {
    DEFAULT_RUNORY_VERSION
}

pub fn validate_plugin_dir(root: &Path) -> PluginResult<ValidationReport> {
    let manifest_path = root.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|_| {
        PluginError::InvalidManifest(format!("missing {}", manifest_path.display()))
    })?;
    let manifest = parse_manifest_json(&bytes)?;
    validate_manifest(&manifest)?;
    validate_compatibility(&manifest, DEFAULT_RUNORY_VERSION)?;

    let mut messages = vec![
        "manifest valid".into(),
        format!("{} permissions valid", manifest.permissions.len()),
    ];

    if let Some(logo) = &manifest.logo {
        crate::plugins::manifest::validate_logo_file(root, logo)?;
        messages.push(format!("logo valid ({logo})"));
    }
    if let Some(category) = &manifest.category {
        messages.push(format!("workspace category {}", category.as_str()));
    }
    if !manifest.contributes.workspace_entries.is_empty() {
        messages.push(format!(
            "{} workspace entries valid",
            manifest.contributes.workspace_entries.len()
        ));
    }

    for workflow in &manifest.contributes.workflows {
        let path = root.join(&workflow.file);
        let text = fs::read_to_string(&path).map_err(|_| {
            PluginError::WorkflowValidation(format!("missing workflow file {}", workflow.file))
        })?;
        let definition = parse_workflow_yaml(&text)?;
        validate_workflow(&definition)?;
        if definition.metadata.id != workflow.id {
            return Err(PluginError::WorkflowValidation(format!(
                "{}: metadata.id `{}` must match contribution id `{}`",
                workflow.file, definition.metadata.id, workflow.id
            )));
        }
        if let Some(schema_rel) = &workflow.input_schema {
            let schema_path = root.join(schema_rel);
            let schema_text = fs::read_to_string(&schema_path).map_err(|_| {
                PluginError::InvalidManifest(format!("missing inputSchema {schema_rel}"))
            })?;
            let schema: serde_json::Value = serde_json::from_str(&schema_text).map_err(|err| {
                PluginError::InvalidManifest(format!("invalid inputSchema {schema_rel}: {err}"))
            })?;
            if schema.get("type").and_then(|v| v.as_str()) != Some("object") {
                return Err(PluginError::InvalidManifest(format!(
                    "inputSchema {schema_rel} must have type object"
                )));
            }
        }
    }
    messages.push(format!(
        "{} workflows valid",
        manifest.contributes.workflows.len()
    ));

    for tool in &manifest.contributes.agent_tools {
        if !manifest
            .contributes
            .workflows
            .iter()
            .any(|w| w.id == tool.workflow)
        {
            return Err(PluginError::InvalidManifest(format!(
                "agentTool `{}` references missing workflow `{}`",
                tool.name, tool.workflow
            )));
        }
    }
    messages.push("paths valid".into());
    messages.push("Plugin is valid.".into());

    Ok(ValidationReport {
        plugin_id: manifest.id,
        version: manifest.version,
        workflow_count: manifest.contributes.workflows.len(),
        permission_count: manifest.permissions.len(),
        messages,
    })
}

pub fn pack_plugin_dir(root: &Path, output: Option<&Path>) -> PluginResult<PackResult> {
    let report = validate_plugin_dir(root)?;
    let dist_dir = root.join("dist");
    fs::create_dir_all(&dist_dir).map_err(|_| PluginError::Storage)?;
    let default_name = format!("{}-{}.runory-plugin", report.plugin_id, report.version);
    let out_path = output
        .map(PathBuf::from)
        .unwrap_or_else(|| dist_dir.join(default_name));
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
    }

    let mut files = collect_pack_files(root)?;
    files.sort();
    if files.len() > MAX_FILE_COUNT {
        return Err(PluginError::InvalidPackage);
    }

    let file = fs::File::create(&out_path).map_err(|_| PluginError::Storage)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(
            zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0)
                .unwrap_or_else(|_| zip::DateTime::default_for_write()),
        );
    for rel in &files {
        validate_zip_entry_name(rel)?;
        let abs = root.join(rel);
        let meta = fs::metadata(&abs).map_err(|_| PluginError::InvalidPackage)?;
        if meta.len() > MAX_SINGLE_FILE_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        zip.start_file(rel.as_str(), options)
            .map_err(|_| PluginError::Storage)?;
        let bytes = fs::read(&abs).map_err(|_| PluginError::Storage)?;
        zip.write_all(&bytes).map_err(|_| PluginError::Storage)?;
    }
    zip.finish().map_err(|_| PluginError::Storage)?;

    let meta = fs::metadata(&out_path).map_err(|_| PluginError::Storage)?;
    if meta.len() > MAX_PACKAGE_BYTES {
        let _ = fs::remove_file(&out_path);
        return Err(PluginError::InvalidPackage);
    }

    let sha256_hex = sha256_file(&out_path)?;
    Ok(PackResult {
        output_path: out_path,
        plugin_id: report.plugin_id,
        version: report.version,
        file_count: files.len(),
        sha256_hex,
    })
}

pub fn inspect_plugin_package(path: &Path) -> PluginResult<InspectReport> {
    let inspection = inspect_package(path, DEFAULT_RUNORY_VERSION)?;
    let sha256_hex = sha256_file(path)?;
    let permissions = inspection
        .manifest
        .permissions
        .iter()
        .map(|p| p.permission_id().to_owned())
        .collect();
    let workflows = inspection
        .manifest
        .contributes
        .workflows
        .iter()
        .map(|w| w.id.clone())
        .collect();
    let workspace_entries = inspection
        .manifest
        .contributes
        .workspace_entries
        .iter()
        .map(|entry| format!("{} → {}", entry.id, entry.command))
        .collect();
    let signature_status = crate::plugins::signing::verify::inspect_signature_status(
        path,
        &crate::plugins::signing::verify::VerifyOptions::default(),
    );
    Ok(InspectReport {
        plugin_id: inspection.manifest.id,
        name: inspection.manifest.name,
        version: inspection.manifest.version,
        publisher_id: inspection.manifest.publisher.id,
        publisher_name: inspection.manifest.publisher.name,
        api_version: inspection.manifest.api_version,
        logo: inspection.manifest.logo,
        category: inspection
            .manifest
            .category
            .map(|category| category.as_str().to_owned()),
        workspace_entries,
        file_count: inspection.file_count,
        sha256_hex,
        signature_status,
        permissions,
        workflows,
    })
}

pub fn keygen(
    publisher: &str,
    key_id: &str,
    private_path: &Path,
    public_path: Option<&Path>,
    password: Option<&str>,
) -> PluginResult<()> {
    let pair = crate::plugins::signing::generate_keypair(publisher, key_id)?;
    crate::plugins::signing::save_keypair(&pair, private_path, public_path, password)
}

pub fn sign_plugin_package(
    package: &Path,
    key_path: &Path,
    password: Option<&str>,
) -> PluginResult<String> {
    let key = crate::plugins::signing::load_private_key(key_path, password)?;
    let meta = crate::plugins::signing::sign_package(package, &key)?;
    Ok(meta.package_sha256)
}

pub fn init_plugin(
    target_dir: &Path,
    plugin_id: &str,
    publisher_id: &str,
    publisher_name: &str,
    display_name: &str,
    description: &str,
    category: Option<&str>,
    logo: Option<&str>,
) -> PluginResult<PathBuf> {
    if target_dir.exists()
        && fs::read_dir(target_dir)
            .map(|mut d| d.next().is_some())
            .unwrap_or(true)
    {
        return Err(PluginError::InvalidManifest(format!(
            "target directory is not empty: {}",
            target_dir.display()
        )));
    }
    fs::create_dir_all(target_dir.join("workflows")).map_err(|_| PluginError::Storage)?;
    if logo.is_some() || category.is_some() {
        fs::create_dir_all(target_dir.join("assets")).map_err(|_| PluginError::Storage)?;
    }

    let command_id = "hello".to_string();
    let logo_path = logo.unwrap_or("assets/logo.png");

    let mut manifest = serde_json::json!({
        "schemaVersion": 1,
        "id": plugin_id,
        "name": display_name,
        "version": "1.0.0",
        "description": description,
        "publisher": {
            "id": publisher_id,
            "name": publisher_name
        },
        "apiVersion": "1",
        "minRunoryVersion": "0.1.0",
        "permissions": ["host.read", "ssh.exec"],
        "contributes": {
            "commands": [{
                "id": command_id,
                "title": "Hello",
                "category": display_name
            }],
            "hostMenus": [{
                "command": command_id,
                "group": "runory",
                "when": "host.os == 'linux'"
            }],
            "workflows": [{
                "id": command_id,
                "file": "workflows/hello.yaml"
            }]
        }
    });

    if let Some(category) = category {
        manifest["category"] = serde_json::json!(category);
        manifest["logo"] = serde_json::json!(logo_path);
        manifest["contributes"]["workspaceEntries"] = serde_json::json!([{
            "id": format!(
                "{}.main",
                plugin_id.rsplit('.').next().unwrap_or("plugin")
            ),
            "title": display_name,
            "description": description,
            "command": command_id,
            "actions": [command_id],
            "order": 100
        }]);
        // Minimal valid 1×1 PNG so validate/pack succeed out of the box.
        write_placeholder_png(&target_dir.join(logo_path))?;
    }

    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| PluginError::InvalidManifest(err.to_string()))?;
    fs::write(
        target_dir.join("manifest.json"),
        format!("{manifest_text}\n"),
    )
    .map_err(|_| PluginError::Storage)?;

    let workflow = format!(
        r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: {command_id}
  name: Hello
  description: Generated by runory-plugin init
steps:
  - id: detect
    uses: system.detect
  - id: uname
    uses: ssh.command
    with:
      executable: uname
      args:
        - -a
"#
    );
    fs::write(target_dir.join("workflows/hello.yaml"), workflow)
        .map_err(|_| PluginError::Storage)?;

    let readme = format!(
        r#"# {display_name}

Generated Runory Plugin API 1.0 scaffold.

```bash
runory-plugin validate
runory-plugin pack
```

Install the resulting `.runory-plugin` package from Runory → Plugins.
"#
    );
    fs::write(target_dir.join("README.md"), readme).map_err(|_| PluginError::Storage)?;

    // Fail closed: generated plugin must validate.
    validate_plugin_dir(target_dir)?;
    Ok(target_dir.to_path_buf())
}

fn write_placeholder_png(path: &Path) -> PluginResult<()> {
    // 1×1 transparent PNG
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
    }
    fs::write(path, PNG).map_err(|_| PluginError::Storage)
}

fn collect_pack_files(root: &Path) -> PluginResult<Vec<String>> {
    let mut out = Vec::new();
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) -> PluginResult<()> {
        let entries = fs::read_dir(dir).map_err(|_| PluginError::Storage)?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "dist" || name_str.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out)?;
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .map_err(|_| PluginError::InvalidPackage)?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(rel);
            }
        }
        Ok(())
    }
    walk(root, root, &mut out)?;
    if !out.iter().any(|p| p == "manifest.json") {
        return Err(PluginError::InvalidPackage);
    }
    Ok(out)
}

fn sha256_file(path: &Path) -> PluginResult<String> {
    let mut file = fs::File::open(path).map_err(|_| PluginError::InvalidPackage)?;
    let mut context = digest::Context::new(&SHA256);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|_| PluginError::InvalidPackage)?;
        if n == 0 {
            break;
        }
        context.update(&buf[..n]);
    }
    let digest = context.finish();
    Ok(digest.as_ref().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn init_validate_pack_inspect_roundtrip() {
        let dir = tempdir().expect("temp");
        let plugin_dir = dir.path().join("demo.hello");
        init_plugin(
            &plugin_dir,
            "demo.hello",
            "demo",
            "Demo",
            "Demo Hello",
            "CLI scaffold",
            None,
            None,
        )
        .expect("init");
        let report = validate_plugin_dir(&plugin_dir).expect("validate");
        assert_eq!(report.plugin_id, "demo.hello");
        let packed = pack_plugin_dir(&plugin_dir, None).expect("pack");
        assert!(packed.output_path.exists());
        assert_eq!(packed.sha256_hex.len(), 64);
        let inspected = inspect_plugin_package(&packed.output_path).expect("inspect");
        assert_eq!(inspected.plugin_id, "demo.hello");
        assert_eq!(inspected.signature_status, "Unsigned");
    }

    #[test]
    fn init_with_workspace_category_includes_logo() {
        let dir = tempdir().expect("temp");
        let plugin_dir = dir.path().join("demo.ops");
        init_plugin(
            &plugin_dir,
            "demo.ops",
            "demo",
            "Demo",
            "Demo Ops",
            "Ops tool",
            Some("operations"),
            Some("assets/logo.png"),
        )
        .expect("init");
        let report = validate_plugin_dir(&plugin_dir).expect("validate");
        assert!(report.messages.iter().any(|m| m.contains("logo valid")));
        let inspected = {
            let packed = pack_plugin_dir(&plugin_dir, None).expect("pack");
            inspect_plugin_package(&packed.output_path).expect("inspect")
        };
        assert_eq!(inspected.category.as_deref(), Some("operations"));
        assert_eq!(inspected.logo.as_deref(), Some("assets/logo.png"));
        assert!(!inspected.workspace_entries.is_empty());
    }
}
