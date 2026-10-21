//! Safe packaged plugin asset resolution (PP1.1).
//!
//! Never exposes arbitrary local filesystem paths to React.
//! Only returns mime + bytes for validated package-relative assets.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use serde::Serialize;

use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::installer::package::safe_join;
use crate::plugins::manifest::logo::{logo_mime_type, validate_logo_file, MAX_LOGO_BYTES};
use crate::plugins::registry::PluginRegistryService;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPluginAsset {
    pub mime_type: String,
    pub bytes: Vec<u8>,
    /// Safe data URL for frontend display. Never a filesystem path.
    pub data_url: String,
}

pub struct PluginAssetResolver {
    registry: Arc<PluginRegistryService>,
}

impl PluginAssetResolver {
    pub fn new(registry: Arc<PluginRegistryService>) -> Self {
        Self { registry }
    }

    pub fn resolve_logo(
        &self,
        plugin_id: &str,
        logo_path: &str,
    ) -> PluginResult<ResolvedPluginAsset> {
        let (record, _manifest) = self.registry.require_enabled(plugin_id)?;
        Self::resolve_logo_from_install(Path::new(&record.install_path), logo_path)
    }

    /// Resolve a logo from an already-known install path.
    /// Callers must ensure the plugin is enabled / eligible before use.
    pub fn resolve_logo_from_install(
        install_path: &Path,
        logo_path: &str,
    ) -> PluginResult<ResolvedPluginAsset> {
        validate_logo_file(install_path, logo_path)?;
        let path = safe_join(install_path, logo_path)?;
        let meta = fs::metadata(&path).map_err(|_| PluginError::NotFound)?;
        if !meta.is_file() || meta.len() > MAX_LOGO_BYTES {
            return Err(PluginError::InvalidManifest("logo asset is invalid".into()));
        }
        let bytes = fs::read(&path).map_err(|_| PluginError::Storage)?;
        let mime_type = logo_mime_type(logo_path).unwrap_or("image/png").to_owned();
        let data_url = format!(
            "data:{mime_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        Ok(ResolvedPluginAsset {
            mime_type,
            bytes,
            data_url,
        })
    }
}
