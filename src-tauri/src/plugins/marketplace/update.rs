use crate::plugins::manifest::PluginManifest;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceListing {
    #[serde(flatten)]
    pub plugin: super::model::MarketplacePlugin,
    pub installed_version: Option<String>,
    pub update_available: bool,
}

pub fn marketplace_listing(
    plugin: super::model::MarketplacePlugin,
    installed_version: Option<String>,
) -> MarketplaceListing {
    let update_available = installed_version.as_ref().is_some_and(|current| {
        crate::plugins::manifest::version::compare_versions(&plugin.latest_version, current)
            == Some(std::cmp::Ordering::Greater)
    });
    MarketplaceListing {
        plugin,
        installed_version,
        update_available,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub plugin_id: String,
    pub installed_version: String,
    pub available_version: String,
    pub permission_additions: Vec<String>,
    pub requires_permission_review: bool,
}

pub fn permission_diff_ids(current: &PluginManifest, candidate: &PluginManifest) -> Vec<String> {
    let current_set: std::collections::HashSet<_> = current
        .permissions
        .iter()
        .map(|p| p.permission_id().to_owned())
        .collect();
    candidate
        .permissions
        .iter()
        .map(|p| p.permission_id().to_owned())
        .filter(|p| !current_set.contains(p))
        .collect()
}

pub fn build_update_check(
    plugin_id: &str,
    installed_version: &str,
    available_version: &str,
    current: &PluginManifest,
    candidate: &PluginManifest,
) -> UpdateCheckResult {
    let permission_additions = permission_diff_ids(current, candidate);
    UpdateCheckResult {
        plugin_id: plugin_id.to_owned(),
        installed_version: installed_version.to_owned(),
        available_version: available_version.to_owned(),
        requires_permission_review: !permission_additions.is_empty(),
        permission_additions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::model::{
        PermissionRequest, PluginContributions, PluginPublisher,
    };

    fn manifest(perms: &[&str]) -> PluginManifest {
        PluginManifest {
            schema_version: 1,
            id: "demo.plugin".into(),
            name: "Demo".into(),
            version: "1.0.0".into(),
            description: None,
            publisher: PluginPublisher {
                id: "demo".into(),
                name: "Demo".into(),
            },
            api_version: "1".into(),
            min_runory_version: None,
            max_runory_version: None,
            icon: None,
            logo: None,
            category: None,
            permissions: perms
                .iter()
                .map(|p| PermissionRequest::Simple((*p).into()))
                .collect(),
            contributes: PluginContributions::default(),
        }
    }

    #[test]
    fn detects_permission_expansion() {
        let current = manifest(&["host.read", "ssh.exec"]);
        let candidate = manifest(&["host.read", "ssh.exec", "ssh.exec.sudo"]);
        let diff = permission_diff_ids(&current, &candidate);
        assert_eq!(diff, vec!["ssh.exec.sudo".to_string()]);
        let check = build_update_check("demo.plugin", "1.0.0", "1.1.0", &current, &candidate);
        assert!(check.requires_permission_review);
    }

    #[test]
    fn listing_only_offers_newer_valid_versions() {
        for (current, available, expected) in [
            ("1.9.0", "1.10.0", true),
            ("2.0.0", "1.10.0", false),
            ("1.0.0-rc.1", "1.0.0", true),
            ("1.0.0", "1.0.0+build", false),
            ("1.0.0", "invalid", false),
        ] {
            let plugin = super::super::model::MarketplacePlugin {
                id: "runory.test".into(),
                publisher_id: "runory".into(),
                name: "Test".into(),
                description: None,
                category: None,
                latest_version: available.into(),
                api_version: "1".into(),
                verified: false,
            };
            assert_eq!(
                marketplace_listing(plugin, Some(current.into())).update_available,
                expected
            );
        }
    }
}
