//! Plugin API 1.0 compatibility suite.
//!
//! Validates frozen schemas exist and that official fixtures
//! (`runory.hello`, `runory.django-deployer`, optionally `runory.docker-manager`)
//! still parse and validate against Core contracts.

#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

use crate::plugins::manifest::{parse_manifest_json, validate_manifest, PluginManifest};
use crate::plugins::workflow::actions::ActionRegistry;
use crate::plugins::workflow::parser::parse_workflow_yaml;
use crate::plugins::workflow::validator::{is_allowed_action, validate_workflow};
use crate::tools::RiskLevel;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri parent")
        .to_path_buf()
}

fn schema_dir() -> PathBuf {
    repo_root().join("plugin-sdk/schemas/v1")
}

fn fixtures_index() -> PathBuf {
    repo_root().join("tests/plugin-api/v1/fixtures.json")
}

fn read_json(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("failed to read {}: {err}", path.display());
    });
    serde_json::from_str(&text).unwrap_or_else(|err| {
        panic!("invalid JSON {}: {err}", path.display());
    })
}

fn assert_schema_object(name: &str) {
    let path = schema_dir().join(name);
    let value = read_json(&path);
    assert_eq!(
        value.get("$schema").and_then(|v| v.as_str()),
        Some("https://json-schema.org/draft/2020-12/schema"),
        "{name} must declare draft 2020-12"
    );
    assert!(
        value.get("$id").and_then(|v| v.as_str()).is_some(),
        "{name} must declare $id"
    );
    assert_eq!(
        value.get("type").and_then(|v| v.as_str()),
        Some("object"),
        "{name} root must be object"
    );
}

fn load_manifest(plugin_dir: &Path) -> PluginManifest {
    let path = plugin_dir.join("manifest.json");
    let text = fs::read_to_string(&path).expect("manifest.json");
    let manifest = parse_manifest_json(text.as_bytes()).expect("parse manifest");
    validate_manifest(&manifest).expect("validate manifest");
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.api_version, "1");
    manifest
}

fn validate_input_schema_file(path: &Path) {
    let value = read_json(path);
    assert_eq!(
        value.get("type").and_then(|v| v.as_str()),
        Some("object"),
        "{} must be type object",
        path.display()
    );
    assert!(
        value
            .get("properties")
            .map(|p| p.is_object())
            .unwrap_or(true),
        "{} properties must be object when present",
        path.display()
    );
}

fn validate_plugin_fixture(plugin_rel: &str) -> PluginManifest {
    let plugin_dir = repo_root().join(plugin_rel);
    assert!(
        plugin_dir.is_dir(),
        "missing plugin fixture {}",
        plugin_dir.display()
    );
    let manifest = load_manifest(&plugin_dir);
    let registry = ActionRegistry::with_builtins();

    for workflow in &manifest.contributes.workflows {
        let yaml_path = plugin_dir.join(&workflow.file);
        let yaml = fs::read_to_string(&yaml_path).unwrap_or_else(|err| {
            panic!("missing workflow {}: {err}", yaml_path.display());
        });
        let definition = parse_workflow_yaml(&yaml).unwrap_or_else(|err| {
            panic!("parse {}: {err}", yaml_path.display());
        });
        validate_workflow(&definition).unwrap_or_else(|err| {
            panic!("validate {}: {err}", yaml_path.display());
        });
        assert_eq!(definition.api_version, "runory/v1");
        assert_eq!(definition.kind, "Workflow");
        assert_eq!(
            definition.metadata.id, workflow.id,
            "workflow metadata.id must match contribution id"
        );
        for step in &definition.steps {
            assert!(
                is_allowed_action(&step.uses),
                "unknown action {} in {}",
                step.uses,
                workflow.id
            );
            assert!(
                registry.contains(&step.uses),
                "action {} not registered in builtins",
                step.uses
            );
            // Ensure action input deserializes / risk is computable for static with blocks
            // that contain no unresolved expression tokens.
            if step_with_is_static(&step.with) {
                let action = registry.get(&step.uses).expect("action");
                let _caps = action
                    .required_capabilities(&step.with)
                    .unwrap_or_else(|err| {
                        panic!("capabilities for {} / {}: {err}", workflow.id, step.uses);
                    });
                let _risk = action.risk(&step.with).unwrap_or_else(|err| {
                    panic!("risk for {} / {}: {err}", workflow.id, step.uses);
                });
            }
        }
        if let Some(schema_rel) = &workflow.input_schema {
            validate_input_schema_file(&plugin_dir.join(schema_rel));
        }
    }

    for tool in &manifest.contributes.agent_tools {
        assert!(
            manifest
                .contributes
                .workflows
                .iter()
                .any(|w| w.id == tool.workflow),
            "agent tool {} references missing workflow {}",
            tool.name,
            tool.workflow
        );
        let _ = matches!(
            tool.risk,
            RiskLevel::R0 | RiskLevel::R1 | RiskLevel::R2 | RiskLevel::R3 | RiskLevel::R4
        );
    }

    for menu in &manifest.contributes.host_menus {
        assert!(
            manifest
                .contributes
                .commands
                .iter()
                .any(|c| c.id == menu.command),
            "host menu references missing command {}",
            menu.command
        );
    }

    manifest
}

fn step_with_is_static(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => !s.contains("{{"),
        serde_json::Value::Array(items) => items.iter().all(step_with_is_static),
        serde_json::Value::Object(map) => map.values().all(step_with_is_static),
        _ => true,
    }
}

#[test]
fn api_v1_schemas_exist_and_are_objects() {
    assert_schema_object("manifest.schema.json");
    assert_schema_object("workflow.schema.json");
    assert_schema_object("input.schema.json");

    let manifest_schema = read_json(&schema_dir().join("manifest.schema.json"));
    assert_eq!(manifest_schema["properties"]["apiVersion"]["const"], "1");
    assert_eq!(manifest_schema["properties"]["schemaVersion"]["const"], 1);
    assert!(manifest_schema["properties"].get("logo").is_some());
    assert!(manifest_schema["properties"].get("category").is_some());
    assert_eq!(
        manifest_schema["properties"]["category"]["enum"]
            .as_array()
            .map(|items| items.len()),
        Some(5)
    );
    assert!(manifest_schema["$defs"]["contributions"]["properties"]
        .get("workspaceEntries")
        .is_some());
    assert!(manifest_schema["$defs"]
        .get("workspaceEntryContribution")
        .is_some());

    let workflow_schema = read_json(&schema_dir().join("workflow.schema.json"));
    assert_eq!(
        workflow_schema["properties"]["apiVersion"]["const"],
        "runory/v1"
    );
    assert_eq!(workflow_schema["properties"]["kind"]["const"], "Workflow");
}

#[test]
fn api_v1_fixtures_index_lists_official_plugins() {
    let index = read_json(&fixtures_index());
    assert_eq!(index["apiVersion"], "1");
    assert_eq!(index["workflowDsl"], "runory/v1");
    let plugins = index["plugins"].as_array().expect("plugins array");
    let ids: Vec<&str> = plugins
        .iter()
        .filter_map(|p| p.get("id").and_then(|v| v.as_str()))
        .collect();
    assert!(ids.contains(&"runory.hello"));
    assert!(ids.contains(&"runory.django-deployer"));
}

#[test]
fn api_v1_hello_fixture_validates() {
    let manifest = validate_plugin_fixture("tests/fixtures/plugins/runory.hello");
    assert_eq!(manifest.id, "runory.hello");
    assert!(!manifest.contributes.workflows.is_empty());
}

#[test]
fn api_v1_django_deployer_fixture_validates() {
    let manifest = validate_plugin_fixture("tests/fixtures/plugins/runory.django-deployer");
    assert_eq!(manifest.id, "runory.django-deployer");
    assert!(manifest.contributes.workflows.len() >= 8);
    assert!(manifest
        .permissions
        .iter()
        .any(|p| p.permission_id() == "secret.reference"));
}

#[test]
fn api_v1_docker_manager_fixture_validates() {
    let manifest = validate_plugin_fixture("tests/fixtures/plugins/runory.docker-manager");
    assert_eq!(manifest.id, "runory.docker-manager");
    let ids: Vec<_> = manifest
        .contributes
        .workflows
        .iter()
        .map(|w| w.id.as_str())
        .collect();
    for required in [
        "docker.inspect-host",
        "docker.list",
        "docker.inspect",
        "docker.logs",
        "docker.start",
        "docker.stop",
        "docker.restart",
        "docker.pull",
    ] {
        assert!(ids.contains(&required), "missing workflow {required}");
    }
    assert!(
        !manifest.contributes.agent_tools.is_empty(),
        "docker plugin should declare agent tools"
    );
}

#[test]
fn api_v1_manifest_schema_permission_catalog_matches_core() {
    let schema = read_json(&schema_dir().join("manifest.schema.json"));
    let enum_values = schema["properties"]["permissions"]["items"]["oneOf"][0]["enum"]
        .as_array()
        .expect("permission enum");
    let expected = [
        "host.read",
        "host.metadata.read",
        "ssh.exec",
        "ssh.exec.sudo",
        "ssh.shell",
        "remote.fs.read",
        "remote.fs.write",
        "remote.fs.upload",
        "remote.fs.delete",
        "secret.reference",
        "network.https",
    ];
    let got: Vec<&str> = enum_values.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(got, expected);
}

#[test]
fn api_v1_workflow_schema_action_allowlist_matches_core() {
    let schema = read_json(&schema_dir().join("workflow.schema.json"));
    let enum_values = schema["$defs"]["step"]["properties"]["uses"]["enum"]
        .as_array()
        .expect("uses enum");
    for action in enum_values {
        let id = action.as_str().expect("string");
        assert!(
            is_allowed_action(id),
            "schema lists action {id} not allowed by Core"
        );
    }
}
