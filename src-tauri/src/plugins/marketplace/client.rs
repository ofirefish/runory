use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::installer::package::MAX_PACKAGE_BYTES;
use crate::plugins::manifest::{validator::plugin_id_valid, version::version_valid};
use crate::plugins::marketplace::model::{
    MarketplaceCatalog, MarketplacePlugin, MarketplacePluginVersion, RevocationEntry,
};
use crate::plugins::registry::repository::PluginRegistryRepository;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePublisherKey {
    pub publisher_id: String,
    pub key_id: String,
    pub algorithm: String,
    pub public_key: String,
    pub status: String,
}

#[derive(Clone)]
pub struct MarketplaceClient {
    base_url: Option<String>,
    repository: Arc<PluginRegistryRepository>,
    http: reqwest::Client,
}

impl MarketplaceClient {
    pub fn new(repository: Arc<PluginRegistryRepository>, base_url: Option<String>) -> Self {
        Self {
            base_url,
            repository,
            http: reqwest::Client::new(),
        }
    }

    pub fn resolve_url(&self, url: &str) -> PluginResult<String> {
        if url.starts_with("http://") || url.starts_with("https://") {
            return Ok(url.to_owned());
        }
        let base = self
            .base_url
            .as_ref()
            .ok_or(PluginError::MarketplaceUnavailable)?;
        let base = base.trim_end_matches('/');
        if url.starts_with('/') {
            Ok(format!("{base}{url}"))
        } else {
            Ok(format!("{base}/{url}"))
        }
    }

    pub async fn list_plugins(&self) -> PluginResult<Vec<MarketplacePlugin>> {
        if let Some(base) = &self.base_url {
            let url = format!("{}/api/v1/plugins", base.trim_end_matches('/'));
            let response = self
                .http
                .get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable)?;
            if !response.status().is_success() {
                return Err(PluginError::MarketplaceUnavailable);
            }
            let plugins = response
                .json::<Vec<MarketplacePlugin>>()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable)?;
            for plugin in &plugins {
                let _ = self.repository.cache_marketplace_plugin(
                    &plugin.id,
                    &serde_json::to_string(plugin).unwrap_or_default(),
                );
            }
            return Ok(plugins);
        }
        self.list_from_cache()
    }

    pub async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<MarketplacePlugin>> {
        Ok(self
            .list_plugins()
            .await?
            .into_iter()
            .find(|p| p.id == plugin_id))
    }

    pub async fn list_versions(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Vec<MarketplacePluginVersion>> {
        if !plugin_id_valid(plugin_id) {
            return Err(PluginError::InvalidPackage);
        }
        if let Some(base) = &self.base_url {
            let url = format!(
                "{}/api/v1/plugins/{}/versions",
                base.trim_end_matches('/'),
                plugin_id
            );
            let response = self
                .http
                .get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable)?;
            if !response.status().is_success() {
                return Err(PluginError::MarketplaceUnavailable);
            }
            return response
                .json()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable);
        }
        Ok(Vec::new())
    }

    pub async fn fetch_publisher_keys(
        &self,
        publisher_id: &str,
    ) -> PluginResult<Vec<MarketplacePublisherKey>> {
        if let Some(base) = &self.base_url {
            let url = format!(
                "{}/api/v1/publishers/{}/keys",
                base.trim_end_matches('/'),
                publisher_id
            );
            let response = self
                .http
                .get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable)?;
            if !response.status().is_success() {
                return Err(PluginError::MarketplaceUnavailable);
            }
            return response
                .json()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable);
        }
        Ok(Vec::new())
    }

    /// Import active publisher keys from Marketplace into local trust store.
    pub async fn sync_publisher_keys(&self, publisher_id: &str) -> PluginResult<usize> {
        let keys = self.fetch_publisher_keys(publisher_id).await?;
        let mut imported = 0usize;
        for key in keys {
            if key.publisher_id != publisher_id
                || key.algorithm != "ed25519"
                || !matches!(key.status.as_str(), "active" | "revoked")
            {
                continue;
            }
            let status = if key.status == "revoked" {
                "revoked"
            } else {
                "active"
            };
            self.repository.upsert_publisher_key(
                &key.key_id,
                &key.publisher_id,
                &key.public_key,
                status,
            )?;
            imported += 1;
        }
        Ok(imported)
    }

    pub async fn fetch_revocations(&self) -> PluginResult<Vec<RevocationEntry>> {
        if let Some(base) = &self.base_url {
            let url = format!("{}/api/v1/security/revocations", base.trim_end_matches('/'));
            let response = self
                .http
                .get(&url)
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable)?;
            if !response.status().is_success() {
                return Err(PluginError::MarketplaceUnavailable);
            }
            return response
                .json()
                .await
                .map_err(|_| PluginError::MarketplaceUnavailable);
        }
        Ok(Vec::new())
    }

    pub async fn download_package(
        &self,
        download_url: &str,
        expected_sha256: &str,
        dest: &Path,
    ) -> PluginResult<()> {
        let url = self.resolve_url(download_url)?;
        if expected_sha256.len() != 64 || !expected_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(PluginError::HashMismatch);
        }
        let mut response = self
            .http
            .get(&url)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await
            .map_err(|_| PluginError::MarketplaceUnavailable)?;
        if !response.status().is_success() {
            return Err(PluginError::MarketplaceUnavailable);
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_PACKAGE_BYTES)
        {
            return Err(PluginError::InvalidPackage);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| PluginError::MarketplaceUnavailable)?
        {
            if (bytes.len() + chunk.len()) as u64 > MAX_PACKAGE_BYTES {
                return Err(PluginError::InvalidPackage);
            }
            bytes.extend_from_slice(&chunk);
        }
        let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
        let actual = digest
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(PluginError::HashMismatch);
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
        }
        std::fs::write(dest, bytes).map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    /// Download a Marketplace version into `dest_dir` and return the staged path.
    pub async fn download_and_stage(
        &self,
        plugin_id: &str,
        version: Option<&str>,
        dest_dir: &Path,
    ) -> PluginResult<(PathBuf, MarketplacePluginVersion)> {
        if !plugin_id_valid(plugin_id) || version.is_some_and(|v| !version_valid(v)) {
            return Err(PluginError::InvalidPackage);
        }
        let versions = self.list_versions(plugin_id).await?;
        if versions.is_empty() {
            return Err(PluginError::NotFound);
        }
        let selected = if let Some(want) = version {
            versions
                .into_iter()
                .find(|v| v.version == want)
                .ok_or(PluginError::NotFound)?
        } else {
            let latest = self
                .get_plugin(plugin_id)
                .await?
                .map(|p| p.latest_version)
                .unwrap_or_default();
            let mut versions = versions;
            if let Some(idx) = versions.iter().position(|v| v.version == latest) {
                versions.swap_remove(idx)
            } else {
                versions.pop().ok_or(PluginError::NotFound)?
            }
        };

        if selected.plugin_id != plugin_id
            || !version_valid(&selected.version)
            || selected.status != "published"
        {
            return Err(PluginError::InvalidPackage);
        }
        // Catalog fields never form filesystem paths; concurrent downloads are isolated.
        let file_name = format!("{}.runory-plugin", uuid::Uuid::new_v4());
        let dest = dest_dir.join(file_name);
        self.download_package(&selected.download_url, &selected.package_sha256, &dest)
            .await?;
        Ok((dest, selected))
    }

    pub fn import_local_catalog(&self, catalog: &MarketplaceCatalog) -> PluginResult<()> {
        for plugin in &catalog.plugins {
            self.repository.cache_marketplace_plugin(
                &plugin.id,
                &serde_json::to_string(plugin).map_err(|_| PluginError::Storage)?,
            )?;
        }
        crate::plugins::marketplace::apply_revocation_feed(&self.repository, &catalog.revocations)?;
        Ok(())
    }

    fn list_from_cache(&self) -> PluginResult<Vec<MarketplacePlugin>> {
        let mut out = Vec::new();
        for (_id, json) in self.repository.list_marketplace_cache()? {
            if let Ok(plugin) = serde_json::from_str(&json) {
                out.push(plugin);
            }
        }
        Ok(out)
    }
}
