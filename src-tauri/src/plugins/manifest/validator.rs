use std::collections::HashSet;
use std::path::{Component, Path};

use super::logo::validate_logo_path;
use super::model::PluginManifest;
use crate::plugins::error::{PluginError, PluginResult};

const PLUGIN_ID_PATTERN: &str = r"^[a-z0-9][a-z0-9.-]{2,127}$";
const WORKSPACE_ENTRY_ID_PATTERN: &str = r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$";
const SUPPORTED_API_VERSION: &str = "1";
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

pub fn validate_manifest(manifest: &PluginManifest) -> PluginResult<()> {
    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(PluginError::InvalidManifest(format!(
            "unsupported schemaVersion {}",
            manifest.schema_version
        )));
    }
    if !plugin_id_valid(&manifest.id) {
        return Err(PluginError::InvalidManifest(
            "plugin id must match reverse-domain-like pattern".into(),
        ));
    }
    if !super::version::version_valid(&manifest.version) {
        return Err(PluginError::InvalidManifest(
            "version must be valid semver".into(),
        ));
    }
    if manifest.publisher.id.trim().is_empty() {
        return Err(PluginError::InvalidManifest(
            "publisher.id must be non-empty".into(),
        ));
    }
    if manifest.publisher.name.trim().is_empty() {
        return Err(PluginError::InvalidManifest(
            "publisher.name must be non-empty".into(),
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err(PluginError::InvalidManifest(
            "name must be non-empty".into(),
        ));
    }
    if manifest.api_version != SUPPORTED_API_VERSION {
        return Err(PluginError::InvalidManifest(format!(
            "unsupported apiVersion {}",
            manifest.api_version
        )));
    }
    if let Some(icon) = &manifest.icon {
        validate_package_relative_path(icon, "icon")?;
    }
    if let Some(logo) = &manifest.logo {
        validate_logo_path(logo)?;
    }

    let mut command_ids = HashSet::new();
    for command in &manifest.contributes.commands {
        if command.id.trim().is_empty() {
            return Err(PluginError::InvalidManifest(
                "command id must be non-empty".into(),
            ));
        }
        if !command_ids.insert(command.id.clone()) {
            return Err(PluginError::InvalidManifest(format!(
                "duplicate command id {}",
                command.id
            )));
        }
    }

    let mut workflow_ids = HashSet::new();
    for workflow in &manifest.contributes.workflows {
        if workflow.id.trim().is_empty() {
            return Err(PluginError::InvalidManifest(
                "workflow id must be non-empty".into(),
            ));
        }
        if !workflow_ids.insert(workflow.id.clone()) {
            return Err(PluginError::InvalidManifest(format!(
                "duplicate workflow id {}",
                workflow.id
            )));
        }
        validate_package_relative_path(&workflow.file, "workflow")?;
        if let Some(schema) = &workflow.input_schema {
            validate_package_relative_path(schema, "inputSchema")?;
        }
    }

    for menu in &manifest.contributes.host_menus {
        if !command_ids.contains(&menu.command) {
            return Err(PluginError::InvalidManifest(format!(
                "host menu references unknown command {}",
                menu.command
            )));
        }
        if let Some(when) = &menu.when {
            validate_when_expression(when)?;
        }
    }

    let mut workspace_entry_ids = HashSet::new();
    for (index, entry) in manifest.contributes.workspace_entries.iter().enumerate() {
        if !workspace_entry_id_valid(&entry.id) {
            return Err(PluginError::InvalidManifest(format!(
                "workspaceEntries[{index}].id is invalid"
            )));
        }
        if !workspace_entry_ids.insert(entry.id.clone()) {
            return Err(PluginError::InvalidManifest(format!(
                "duplicate workspace entry id {}",
                entry.id
            )));
        }
        if entry.title.trim().is_empty() {
            return Err(PluginError::InvalidManifest(format!(
                "workspaceEntries[{index}].title must be non-empty"
            )));
        }
        if !command_ids.contains(&entry.command) {
            return Err(PluginError::InvalidManifest(format!(
                "workspaceEntries[{index}].command references unknown command \"{}\"",
                entry.command
            )));
        }
        for (action_index, action) in entry.actions.iter().enumerate() {
            if !command_ids.contains(action) {
                return Err(PluginError::InvalidManifest(format!(
                    "workspaceEntries[{index}].actions[{action_index}] references unknown command \"{action}\""
                )));
            }
        }
    }

    let mut permission_ids = HashSet::new();
    for permission in &manifest.permissions {
        if !permission_ids.insert(permission.permission_id()) {
            return Err(PluginError::InvalidManifest("duplicate permission".into()));
        }
        permission
            .capability()
            .map_err(PluginError::InvalidManifest)?;
    }

    Ok(())
}

pub fn validate_package_relative_path(raw: &str, field: &str) -> PluginResult<()> {
    let path = Path::new(raw);
    if raw.trim().is_empty() {
        return Err(PluginError::InvalidManifest(format!(
            "{field} path must be non-empty"
        )));
    }
    if path.is_absolute() {
        return Err(PluginError::InvalidManifest(format!(
            "{field} path must be package-relative"
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                if text.contains('\0') {
                    return Err(PluginError::InvalidManifest(format!(
                        "{field} path is invalid"
                    )));
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PluginError::InvalidManifest(format!(
                    "{field} path escapes plugin package"
                )));
            }
        }
    }
    Ok(())
}

pub fn plugin_id_valid(id: &str) -> bool {
    let bytes = id.as_bytes();
    if !(3..=128).contains(&bytes.len()) {
        return false;
    }
    let Some((first, rest)) = bytes.split_first() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    rest.iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
    })
}

fn workspace_entry_id_valid(id: &str) -> bool {
    let bytes = id.as_bytes();
    if !(1..=64).contains(&bytes.len()) {
        return false;
    }
    let Some((first, rest)) = bytes.split_first() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    rest.iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_when_expression(expr: &str) -> PluginResult<()> {
    let trimmed = expr.trim();
    let allowed = [
        "host.os == \"linux\"",
        "host.os == 'linux'",
        "host.os == \"windows\"",
        "host.os == 'windows'",
        "host.arch == \"x86_64\"",
        "host.arch == 'x86_64'",
        "host.tags contains \"production\"",
        "host.tags contains 'production'",
    ];
    if allowed.iter().any(|item| *item == trimmed) {
        return Ok(());
    }
    Err(PluginError::InvalidManifest(format!(
        "unsupported when expression: {trimmed}"
    )))
}

#[allow(dead_code)]
fn _pattern_doc() {
    let _ = PLUGIN_ID_PATTERN;
    let _ = WORKSPACE_ENTRY_ID_PATTERN;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::model::{
        CommandContribution, HostMenuContribution, PluginContributions, PluginPublisher,
        PluginWorkspaceCategory, WorkflowContribution, WorkspaceEntryContribution,
    };

    fn valid_manifest() -> PluginManifest {
        PluginManifest {
            schema_version: 1,
            id: "runory.hello".into(),
            name: "Runory Hello".into(),
            version: "1.0.0".into(),
            description: Some("Minimal".into()),
            publisher: PluginPublisher {
                id: "runory".into(),
                name: "Runory".into(),
            },
            api_version: "1".into(),
            min_runory_version: Some("0.1.0".into()),
            max_runory_version: None,
            icon: None,
            logo: None,
            category: None,
            permissions: vec![crate::plugins::manifest::model::PermissionRequest::Simple(
                "host.read".into(),
            )],
            contributes: PluginContributions {
                commands: vec![CommandContribution {
                    id: "hello.inspect".into(),
                    title: "Inspect Host".into(),
                    category: Some("Runory Hello".into()),
                    icon: None,
                }],
                host_menus: vec![HostMenuContribution {
                    command: "hello.inspect".into(),
                    group: Some("runory".into()),
                    when: Some("host.os == 'linux'".into()),
                }],
                workflows: vec![WorkflowContribution {
                    id: "hello.inspect".into(),
                    file: "workflows/hello.yaml".into(),
                    input_schema: None,
                }],
                agent_tools: Vec::new(),
                workspace_entries: Vec::new(),
            },
        }
    }

    #[test]
    fn accepts_valid_manifest() {
        validate_manifest(&valid_manifest()).expect("valid");
    }

    #[test]
    fn accepts_workspace_entry_with_category() {
        let mut manifest = valid_manifest();
        manifest.category = Some(PluginWorkspaceCategory::Deployment);
        manifest.logo = Some("assets/logo.png".into());
        manifest.contributes.workspace_entries = vec![WorkspaceEntryContribution {
            id: "hello.main".into(),
            title: "Hello".into(),
            description: Some("Demo".into()),
            command: "hello.inspect".into(),
            actions: vec!["hello.inspect".into()],
            order: Some(100),
        }];
        validate_manifest(&manifest).expect("valid workspace entry");
    }

    #[test]
    fn accepts_workspace_entries_without_category() {
        // Valid but entries will not be exposed at runtime.
        let mut manifest = valid_manifest();
        manifest.contributes.workspace_entries = vec![WorkspaceEntryContribution {
            id: "hello.main".into(),
            title: "Hello".into(),
            description: None,
            command: "hello.inspect".into(),
            actions: Vec::new(),
            order: None,
        }];
        validate_manifest(&manifest).expect("no category still valid");
    }

    #[test]
    fn rejects_unknown_workspace_command() {
        let mut manifest = valid_manifest();
        manifest.contributes.workspace_entries = vec![WorkspaceEntryContribution {
            id: "hello.main".into(),
            title: "Hello".into(),
            description: None,
            command: "django.foo".into(),
            actions: Vec::new(),
            order: None,
        }];
        let err = validate_manifest(&manifest).expect_err("unknown command");
        let message = err.to_string();
        assert!(message.contains("django.foo"), "{message}");
    }

    #[test]
    fn rejects_unknown_workspace_action() {
        let mut manifest = valid_manifest();
        manifest.contributes.workspace_entries = vec![WorkspaceEntryContribution {
            id: "hello.main".into(),
            title: "Hello".into(),
            description: None,
            command: "hello.inspect".into(),
            actions: vec!["missing.action".into()],
            order: None,
        }];
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_duplicate_workspace_entry_id() {
        let mut manifest = valid_manifest();
        let entry = WorkspaceEntryContribution {
            id: "hello.main".into(),
            title: "Hello".into(),
            description: None,
            command: "hello.inspect".into(),
            actions: Vec::new(),
            order: None,
        };
        manifest.contributes.workspace_entries = vec![entry.clone(), entry];
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_invalid_logo_extension() {
        let mut manifest = valid_manifest();
        manifest.logo = Some("assets/logo.svg".into());
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_invalid_schema_version() {
        let mut manifest = valid_manifest();
        manifest.schema_version = 99;
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_invalid_plugin_id() {
        let mut manifest = valid_manifest();
        manifest.id = "runory plugin".into();
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_invalid_semver() {
        let mut manifest = valid_manifest();
        manifest.version = "1".into();
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_duplicate_command() {
        let mut manifest = valid_manifest();
        manifest.contributes.commands.push(CommandContribution {
            id: "hello.inspect".into(),
            title: "Dup".into(),
            category: None,
            icon: None,
        });
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_duplicate_workflow() {
        let mut manifest = valid_manifest();
        manifest.contributes.workflows.push(WorkflowContribution {
            id: "hello.inspect".into(),
            file: "workflows/other.yaml".into(),
            input_schema: None,
        });
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_unknown_permission() {
        let mut manifest = valid_manifest();
        manifest
            .permissions
            .push(crate::plugins::manifest::model::PermissionRequest::Simple(
                "root.everything".into(),
            ));
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_path_traversal_workflow() {
        let mut manifest = valid_manifest();
        manifest.contributes.workflows[0].file = "../evil.yaml".into();
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_absolute_workflow_path() {
        let mut manifest = valid_manifest();
        manifest.contributes.workflows[0].file = "/tmp/evil.yaml".into();
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }

    #[test]
    fn rejects_unsupported_api_version() {
        let mut manifest = valid_manifest();
        manifest.api_version = "99".into();
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PluginError::InvalidManifest(_))
        ));
    }
}
