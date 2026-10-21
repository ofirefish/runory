//! Bundled official publisher public keys (never private keys).

use crate::plugins::error::PluginResult;
use crate::plugins::registry::repository::PluginRegistryRepository;

/// Matches `plugin-sdk/keys/runory-2026-main.pub.json`.
pub const OFFICIAL_RUNORY_PUBLISHER: &str = "runory";
pub const OFFICIAL_RUNORY_KEY_ID: &str = "runory-2026-main";
pub const OFFICIAL_RUNORY_PUBLIC_KEY_B64: &str = "YfFuwISbG9zyHwlChH6PPyQ6rueVJp7ofqFC1VGndiI=";

/// Ensure the official Runory publisher + public key exist in `plugins.db`.
pub fn seed_official_publisher_keys(repository: &PluginRegistryRepository) -> PluginResult<()> {
    repository.upsert_publisher(OFFICIAL_RUNORY_PUBLISHER, "Runory", "official")?;
    repository.upsert_publisher_key(
        OFFICIAL_RUNORY_KEY_ID,
        OFFICIAL_RUNORY_PUBLISHER,
        OFFICIAL_RUNORY_PUBLIC_KEY_B64,
        "active",
    )?;
    Ok(())
}
