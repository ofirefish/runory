use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::registry::repository::PluginRegistryRepository;

use super::model::RevocationEntry;

pub fn apply_revocation_feed(
    repository: &PluginRegistryRepository,
    entries: &[RevocationEntry],
) -> PluginResult<usize> {
    let mut applied = 0usize;
    for entry in entries {
        repository.upsert_revocation(
            &entry.kind,
            &entry.target_id,
            entry.version.as_deref(),
            entry.reason.as_deref(),
        )?;
        if entry.kind == "plugin_version" {
            if let Some(version) = &entry.version {
                repository.set_plugin_security_status(
                    &entry.target_id,
                    version,
                    "revoked",
                    entry.reason.as_deref(),
                )?;
            }
        } else if entry.kind == "plugin" {
            // Mark all known installed versions when present.
            for version in repository.list_installed_versions(&entry.target_id)? {
                repository.set_plugin_security_status(
                    &entry.target_id,
                    &version,
                    "revoked",
                    entry.reason.as_deref(),
                )?;
            }
        } else if entry.kind == "publisher_key" {
            // target_id encodes publisher_id/key_id as publisher_id#key_id
            if let Some((publisher_id, key_id)) = entry.target_id.split_once('#') {
                repository.upsert_publisher_key(key_id, publisher_id, "", "revoked")?;
            }
        }
        applied += 1;
    }
    let _ = applied;
    Ok(entries.len())
}

pub fn assert_not_revoked(
    repository: &PluginRegistryRepository,
    plugin_id: &str,
    version: &str,
) -> PluginResult<()> {
    if repository.is_revoked("plugin", plugin_id, None)? {
        return Err(PluginError::PluginRevoked);
    }
    if repository.is_revoked("plugin_version", plugin_id, Some(version))? {
        return Err(PluginError::PluginVersionRevoked);
    }
    if let Some(status) = repository.get_plugin_security_status(plugin_id, version)? {
        if status == "revoked" {
            return Err(PluginError::PluginVersionRevoked);
        }
    }
    Ok(())
}
