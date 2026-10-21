use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::manifest::model::PluginManifest;

use super::version::compare_versions as cmp_version;

pub fn validate_compatibility(manifest: &PluginManifest, runory_version: &str) -> PluginResult<()> {
    if let Some(min) = &manifest.min_runory_version {
        match cmp_version(runory_version, min) {
            Some(std::cmp::Ordering::Less) => return Err(PluginError::IncompatibleVersion),
            None => {
                return Err(PluginError::InvalidManifest(
                    "minRunoryVersion is not a comparable version".into(),
                ))
            }
            _ => {}
        }
    }
    if let Some(max) = &manifest.max_runory_version {
        match cmp_version(runory_version, max) {
            Some(std::cmp::Ordering::Greater) => return Err(PluginError::IncompatibleVersion),
            None => {
                return Err(PluginError::InvalidManifest(
                    "maxRunoryVersion is not a comparable version".into(),
                ))
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest::model::{PluginContributions, PluginPublisher};

    fn manifest_with_bounds(min: Option<&str>, max: Option<&str>) -> PluginManifest {
        PluginManifest {
            schema_version: 1,
            id: "runory.hello".into(),
            name: "Hello".into(),
            version: "1.0.0".into(),
            description: None,
            publisher: PluginPublisher {
                id: "runory".into(),
                name: "Runory".into(),
            },
            api_version: "1".into(),
            min_runory_version: min.map(str::to_owned),
            max_runory_version: max.map(str::to_owned),
            icon: None,
            logo: None,
            category: None,
            permissions: Vec::new(),
            contributes: PluginContributions::default(),
        }
    }

    #[test]
    fn rejects_incompatible_min_version() {
        let manifest = manifest_with_bounds(Some("9.0.0"), None);
        assert!(matches!(
            validate_compatibility(&manifest, "0.1.1"),
            Err(PluginError::IncompatibleVersion)
        ));
    }

    #[test]
    fn accepts_compatible_version() {
        let manifest = manifest_with_bounds(Some("0.1.0"), Some("1.0.0"));
        validate_compatibility(&manifest, "0.1.1").expect("compatible");
    }
}
