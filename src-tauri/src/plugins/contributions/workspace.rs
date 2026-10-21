//! Normalized plugin workspace entry registry (PP1.1).
//!
//! Terminal / Files / Dashboard / Operations / Deployment must not parse
//! plugin manifests independently — they query this single source.

use std::path::Path;

use serde::Serialize;

use crate::plugins::assets::PluginAssetResolver;
use crate::plugins::error::PluginResult;
use crate::plugins::manifest::compatibility::validate_compatibility;
use crate::plugins::manifest::model::{
    PluginManifest, PluginWorkspaceCategory, WorkspaceEntryContribution,
};
use crate::plugins::registry::PluginRegistryService;

const DEFAULT_ORDER: i32 = 1000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginWorkspaceEntryDto {
    pub plugin_id: String,
    pub plugin_version: String,
    pub entry_id: String,
    pub title: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub category: PluginWorkspaceCategory,
    pub command: String,
    pub actions: Vec<String>,
    pub order: i32,
    pub publisher_id: String,
    /// Workflow id for the primary command when a matching workflow exists.
    pub workflow_id: Option<String>,
    /// Display titles for overflow actions (same order as `actions`).
    pub action_titles: Vec<String>,
}

pub async fn list_plugin_workspace_entries(
    registry: &PluginRegistryService,
    runory_version: &str,
    category: Option<PluginWorkspaceCategory>,
) -> PluginResult<Vec<PluginWorkspaceEntryDto>> {
    let installed = registry.list_installed().await?;
    let mut out = Vec::new();

    for plugin in installed.into_iter().filter(|p| p.enabled) {
        if is_revoked(registry, &plugin.id, &plugin.version) {
            continue;
        }
        let Ok((record, manifest)) = registry.require_enabled(&plugin.id) else {
            continue;
        };
        if validate_compatibility(&manifest, runory_version).is_err() {
            continue;
        }
        let Some(plugin_category) = manifest.category else {
            // Entries without a category are kept valid but not exposed.
            continue;
        };
        if let Some(filter) = category {
            if filter != plugin_category {
                continue;
            }
        }
        let logo_url = resolve_logo_url(Path::new(&record.install_path), &manifest);
        for entry in &manifest.contributes.workspace_entries {
            out.push(to_dto(&manifest, plugin_category, entry, logo_url.clone()));
        }
    }

    out.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.plugin_id.cmp(&b.plugin_id))
            .then_with(|| a.entry_id.cmp(&b.entry_id))
    });
    Ok(out)
}

fn is_revoked(registry: &PluginRegistryService, plugin_id: &str, version: &str) -> bool {
    registry
        .repository()
        .is_revoked("plugin", plugin_id, None)
        .unwrap_or(false)
        || registry
            .repository()
            .is_revoked("plugin_version", plugin_id, Some(version))
            .unwrap_or(false)
        || registry
            .repository()
            .get_plugin_security_status(plugin_id, version)
            .ok()
            .flatten()
            .as_deref()
            == Some("revoked")
}

fn resolve_logo_url(install_path: &Path, manifest: &PluginManifest) -> Option<String> {
    let logo = manifest.logo.as_deref()?;
    PluginAssetResolver::resolve_logo_from_install(install_path, logo)
        .ok()
        .map(|asset| asset.data_url)
}

fn to_dto(
    manifest: &PluginManifest,
    category: PluginWorkspaceCategory,
    entry: &WorkspaceEntryContribution,
    logo_url: Option<String>,
) -> PluginWorkspaceEntryDto {
    let workflow_id = manifest
        .contributes
        .workflows
        .iter()
        .find(|workflow| workflow.id == entry.command)
        .map(|workflow| workflow.id.clone());
    let action_titles = entry
        .actions
        .iter()
        .map(|action| {
            manifest
                .contributes
                .commands
                .iter()
                .find(|command| &command.id == action)
                .map(|command| command.title.clone())
                .unwrap_or_else(|| action.clone())
        })
        .collect();
    PluginWorkspaceEntryDto {
        plugin_id: manifest.id.clone(),
        plugin_version: manifest.version.clone(),
        entry_id: entry.id.clone(),
        title: entry.title.clone(),
        description: entry.description.clone(),
        logo_url,
        category,
        command: entry.command.clone(),
        actions: entry.actions.clone(),
        order: entry.order.unwrap_or(DEFAULT_ORDER),
        publisher_id: manifest.publisher.id.clone(),
        workflow_id,
        action_titles,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::model::{
        CommandContribution, PluginContributions, PluginPublisher, WorkspaceEntryContribution,
    };
    use std::cmp::Ordering;

    fn sample_manifest(
        category: Option<PluginWorkspaceCategory>,
        order: Option<i32>,
    ) -> PluginManifest {
        PluginManifest {
            schema_version: 1,
            id: "runory.demo".into(),
            name: "Demo".into(),
            version: "1.0.0".into(),
            description: None,
            publisher: PluginPublisher {
                id: "runory".into(),
                name: "Runory".into(),
            },
            api_version: "1".into(),
            min_runory_version: None,
            max_runory_version: None,
            icon: None,
            logo: None,
            category,
            permissions: Vec::new(),
            contributes: PluginContributions {
                commands: vec![CommandContribution {
                    id: "demo.run".into(),
                    title: "Run".into(),
                    category: None,
                    icon: None,
                }],
                host_menus: Vec::new(),
                workflows: Vec::new(),
                agent_tools: Vec::new(),
                workspace_entries: vec![WorkspaceEntryContribution {
                    id: "demo.main".into(),
                    title: "Demo Tool".into(),
                    description: Some("desc".into()),
                    command: "demo.run".into(),
                    actions: vec!["demo.run".into()],
                    order,
                }],
            },
        }
    }

    #[test]
    fn sorts_by_order_then_title() {
        let a = to_dto(
            &sample_manifest(Some(PluginWorkspaceCategory::Operations), Some(200)),
            PluginWorkspaceCategory::Operations,
            &sample_manifest(Some(PluginWorkspaceCategory::Operations), Some(200))
                .contributes
                .workspace_entries[0],
            None,
        );
        let mut higher = sample_manifest(Some(PluginWorkspaceCategory::Operations), Some(50));
        higher.contributes.workspace_entries[0].title = "Alpha".into();
        let b = to_dto(
            &higher,
            PluginWorkspaceCategory::Operations,
            &higher.contributes.workspace_entries[0],
            None,
        );
        assert_eq!(b.order.cmp(&a.order), Ordering::Less);
        assert_eq!(b.order, 50);
        assert_eq!(a.order, 200);
    }

    #[test]
    fn default_order_is_1000() {
        let manifest = sample_manifest(Some(PluginWorkspaceCategory::Deployment), None);
        let dto = to_dto(
            &manifest,
            PluginWorkspaceCategory::Deployment,
            &manifest.contributes.workspace_entries[0],
            None,
        );
        assert_eq!(dto.order, DEFAULT_ORDER);
    }
}
