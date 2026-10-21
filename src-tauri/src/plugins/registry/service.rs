use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::model::{
    InstalledPluginRecord, PluginCommandDto, PluginDto, PluginHostMenuDto, PluginPermissionDto,
    PluginWorkflowDto,
};
use super::repository::{now_secs, PluginRegistryRepository};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::installer::package::{
    cleanup_stale_temp_dirs, inspect_package, unpack_package, PluginPackageInspection,
};
use crate::plugins::manifest::{parse_manifest_json, PluginManifest};
use crate::plugins::permissions::grant::PermissionGrant;
use crate::plugins::permissions::service::PermissionService;
use crate::plugins::signing::verify::{verify_package, TrustedPublisherKey, VerifyOptions};
use crate::plugins::signing::TrustStatus;

#[derive(Clone)]
pub struct PluginRegistryService {
    root: PathBuf,
    pub(super) repository: Arc<PluginRegistryRepository>,
    permissions: Arc<PermissionService>,
    runory_version: String,
    pub(super) reviews: Arc<super::review::InstallReviews>,
    pub(super) mutation_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallRequest {
    pub package_path: String,
    #[serde(default)]
    pub granted_permissions: Option<Vec<String>>,
    /// When true, unsigned packages may install (Plugin Developer Mode).
    #[serde(default)]
    pub allow_unsigned: bool,
}

impl PluginRegistryService {
    pub fn new(
        root: PathBuf,
        repository: Arc<PluginRegistryRepository>,
        permissions: Arc<PermissionService>,
        runory_version: impl Into<String>,
    ) -> Self {
        Self {
            root,
            repository,
            permissions,
            runory_version: runory_version.into(),
            reviews: Arc::new(super::review::InstallReviews::default()),
            mutation_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn runory_version(&self) -> &str {
        &self.runory_version
    }

    pub fn repository(&self) -> Arc<PluginRegistryRepository> {
        Arc::clone(&self.repository)
    }

    pub fn permissions(&self) -> Arc<PermissionService> {
        Arc::clone(&self.permissions)
    }

    pub fn startup_recovery(&self) -> PluginResult<()> {
        fs::create_dir_all(&self.root).map_err(|_| PluginError::Storage)?;
        cleanup_stale_temp_dirs(&self.root)?;
        let _ = self.repository.interrupt_running_runs()?;
        crate::plugins::signing::seed_official_publisher_keys(self.repository.as_ref())?;
        Ok(())
    }

    pub async fn list_installed(&self) -> PluginResult<Vec<PluginDto>> {
        let records = self.repository.list_active()?;
        let mut out = Vec::new();
        for record in records {
            out.push(self.to_dto(&record).await?);
        }
        Ok(out)
    }

    pub async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginDto>> {
        let Some(record) = self.repository.get_active(plugin_id)? else {
            return Ok(None);
        };
        Ok(Some(self.to_dto(&record).await?))
    }

    pub fn inspect_package(&self, path: &str) -> PluginResult<PluginPackageInspection> {
        inspect_package(Path::new(path), &self.runory_version)
    }

    pub async fn install(&self, request: PluginInstallRequest) -> PluginResult<PluginDto> {
        let _guard = self.mutation_lock.lock().await;
        self.install_inner(request).await
    }

    pub(super) async fn install_inner(
        &self,
        request: PluginInstallRequest,
    ) -> PluginResult<PluginDto> {
        let package_path = Path::new(&request.package_path);
        let verify_options = self.verify_options(request.allow_unsigned)?;
        let verification = verify_package(package_path, &verify_options)?;
        Self::require_install_trust(verification.trust, request.allow_unsigned)?;

        let inspection = self.inspect_package(&request.package_path)?;
        if verification
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.publisher != inspection.manifest.publisher.id)
        {
            return Err(PluginError::PublisherMismatch);
        }
        let plugin_id = inspection.manifest.id.clone();
        let version = inspection.manifest.version.clone();
        let previous = self.repository.get_active(&plugin_id)?;
        if previous
            .as_ref()
            .is_some_and(|record| record.publisher_id != inspection.manifest.publisher.id)
        {
            return Err(PluginError::PublisherMismatch);
        }

        if self.repository.is_revoked("plugin", &plugin_id, None)? {
            return Err(PluginError::PluginRevoked);
        }
        if self
            .repository
            .is_revoked("plugin_version", &plugin_id, Some(&version))?
        {
            return Err(PluginError::PluginVersionRevoked);
        }
        if let Some(status) = self
            .repository
            .get_plugin_security_status(&plugin_id, &version)?
        {
            if status == "revoked" {
                return Err(PluginError::PluginVersionRevoked);
            }
        }

        if self.repository.get_active(&plugin_id)?.is_some() {
            // Same version already active — reject blind overwrite.
            if let Some(existing) = self.repository.get_active(&plugin_id)? {
                if existing.version == version {
                    return Err(PluginError::AlreadyInstalled);
                }
            }
        }

        let dest = self.root.join(&plugin_id).join(&version);
        if dest.exists() {
            return Err(PluginError::AlreadyInstalled);
        }
        unpack_package(package_path, &dest)?;

        let granted_set: Option<std::collections::HashSet<String>> = request
            .granted_permissions
            .map(|items| items.into_iter().collect());

        let mut grants = Vec::new();
        for permission in &inspection.manifest.permissions {
            let capability = permission
                .capability()
                .map_err(PluginError::InvalidManifest)?;
            let granted = match &granted_set {
                Some(set) => set.contains(capability.as_str()),
                None => true,
            };
            grants.push(PermissionGrant {
                plugin_id: plugin_id.clone(),
                capability,
                granted,
                hosts: permission.hosts().to_vec(),
            });
        }

        let now = now_secs();
        let record = InstalledPluginRecord {
            id: plugin_id.clone(),
            version: version.clone(),
            name: inspection.manifest.name.clone(),
            publisher_id: inspection.manifest.publisher.id.clone(),
            install_path: dest.display().to_string(),
            enabled: previous.as_ref().is_none_or(|record| record.enabled),
            installed_at: now,
            updated_at: now,
        };

        if let Err(error) = self.repository.install_version(&record, &grants) {
            let _ = fs::remove_dir_all(&dest);
            return Err(error);
        }

        self.invalidate_install_reviews(&plugin_id)?;
        self.permissions.load_plugin(&plugin_id).await?;
        self.write_version_hint(&plugin_id, &version);
        self.to_dto(&record).await
    }

    pub(super) fn verify_options(&self, developer_mode: bool) -> PluginResult<VerifyOptions> {
        let keys = self.repository.list_publisher_keys()?;
        Ok(VerifyOptions {
            developer_mode,
            known_keys: keys
                .into_iter()
                .map(
                    |(publisher_id, key_id, public_key_b64, revoked)| TrustedPublisherKey {
                        publisher_id,
                        key_id,
                        public_key_b64,
                        revoked,
                    },
                )
                .collect(),
            official_publishers: vec!["runory".into()],
            verified_publishers: Vec::new(),
        })
    }

    /// Activate an already-installed side-by-side version (PP7).
    pub async fn activate_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> PluginResult<PluginDto> {
        let _guard = self.mutation_lock.lock().await;
        let record = self
            .repository
            .get_plugin_version(plugin_id, version)?
            .ok_or(PluginError::NotFound)?;
        self.require_not_revoked(plugin_id, version)?;
        let manifest = self.load_manifest(&record)?;
        crate::plugins::manifest::validate_compatibility(&manifest, &self.runory_version)?;
        let grants = self.repository.list_permissions(plugin_id)?;
        // Version activation must never reuse grants with a broader host scope.
        if manifest.permissions.iter().any(|permission| {
            !grants.iter().any(|grant| {
                grant.capability.as_str() == permission.permission_id()
                    && grant.hosts == permission.hosts()
            })
        }) {
            return Err(PluginError::UpdatePermissionReviewRequired);
        }
        self.invalidate_install_reviews(plugin_id)?;
        self.repository.set_active_version(plugin_id, version)?;
        self.write_version_hint(plugin_id, version);
        self.to_dto(&record).await
    }

    /// Compute permission expansion between installed and candidate manifests.
    pub fn permission_diff(
        &self,
        current: &PluginManifest,
        candidate: &PluginManifest,
    ) -> Vec<String> {
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

    pub async fn set_enabled(&self, plugin_id: &str, enabled: bool) -> PluginResult<()> {
        let _guard = self.mutation_lock.lock().await;
        let record = self
            .repository
            .get_active(plugin_id)?
            .ok_or(PluginError::NotFound)?;
        if enabled {
            self.require_not_revoked(plugin_id, &record.version)?;
        }
        self.invalidate_install_reviews(plugin_id)?;
        self.repository.set_enabled(plugin_id, enabled)?;
        Ok(())
    }

    pub async fn uninstall(&self, plugin_id: &str) -> PluginResult<()> {
        let _guard = self.mutation_lock.lock().await;
        self.repository
            .get_active(plugin_id)?
            .ok_or(PluginError::NotFound)?;
        self.invalidate_install_reviews(plugin_id)?;
        let paths = self.repository.uninstall(plugin_id)?;
        self.permissions.clear(plugin_id).await;
        for path in paths {
            let _ = fs::remove_dir_all(path);
        }
        let plugin_dir = self.root.join(plugin_id);
        let _ = fs::remove_dir_all(plugin_dir);
        Ok(())
    }

    pub fn load_manifest(&self, record: &InstalledPluginRecord) -> PluginResult<PluginManifest> {
        let bytes = fs::read(Path::new(&record.install_path).join("manifest.json"))
            .map_err(|_| PluginError::InvalidPackage)?;
        parse_manifest_json(&bytes)
    }

    pub(super) fn require_install_trust(
        trust: TrustStatus,
        developer_mode: bool,
    ) -> PluginResult<()> {
        if trust.allows_install(developer_mode) {
            return Ok(());
        }
        Err(match trust {
            TrustStatus::Unsigned => PluginError::SignatureMissing,
            TrustStatus::RevokedKey => PluginError::SigningKeyRevoked,
            TrustStatus::UnknownPublisher => PluginError::SigningKeyUnknown,
            _ => PluginError::SignatureInvalid,
        })
    }

    pub(super) fn require_not_revoked(&self, id: &str, version: &str) -> PluginResult<()> {
        if self.repository.is_revoked("plugin", id, None)? {
            return Err(PluginError::PluginRevoked);
        }
        if self
            .repository
            .is_revoked("plugin_version", id, Some(version))?
            || self
                .repository
                .get_plugin_security_status(id, version)?
                .as_deref()
                == Some("revoked")
        {
            return Err(PluginError::PluginVersionRevoked);
        }
        if let Some(record) = self.repository.get_plugin_version(id, version)? {
            let signature = Path::new(&record.install_path)
                .join(crate::plugins::signing::model::SIGNATURE_ENTRY);
            if signature.try_exists().map_err(|_| PluginError::Storage)? {
                use std::io::Read;
                let mut bytes = Vec::new();
                fs::File::open(signature)
                    .map_err(|_| PluginError::Storage)?
                    .take(8193)
                    .read_to_end(&mut bytes)
                    .map_err(|_| PluginError::Storage)?;
                if bytes.len() > 8192 {
                    return Err(PluginError::SignatureInvalid);
                }
                let metadata: crate::plugins::signing::model::SignatureMetadata =
                    serde_json::from_slice(&bytes).map_err(|_| PluginError::SignatureInvalid)?;
                if metadata.publisher != record.publisher_id {
                    return Err(PluginError::PublisherMismatch);
                }
                if self.repository.list_publisher_keys()?.iter().any(
                    |(publisher, key, _, revoked)| {
                        publisher == &metadata.publisher && key == &metadata.key_id && *revoked
                    },
                ) {
                    return Err(PluginError::SigningKeyRevoked);
                }
            }
        }
        Ok(())
    }

    fn write_version_hint(&self, id: &str, version: &str) {
        // The SQLite transaction is authoritative. This legacy hint is never read by Core.
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            let parent = self.root.join(id);
            let mut file = tempfile::NamedTempFile::new_in(&parent)?;
            file.write_all(
                serde_json::json!({ "version": version })
                    .to_string()
                    .as_bytes(),
            )?;
            file.as_file().sync_all()?;
            file.persist(parent.join("current.json"))
                .map_err(|error| error.error)?;
            Ok(())
        };
        if write().is_err() {
            tracing::warn!(plugin_id = id, "plugin version hint unavailable");
        }
    }

    pub fn require_enabled(
        &self,
        plugin_id: &str,
    ) -> PluginResult<(InstalledPluginRecord, PluginManifest)> {
        let record = self
            .repository
            .get_active(plugin_id)?
            .ok_or(PluginError::NotFound)?;
        if !record.enabled {
            return Err(PluginError::Disabled);
        }
        self.require_not_revoked(plugin_id, &record.version)?;
        let manifest = self.load_manifest(&record)?;
        Ok((record, manifest))
    }

    async fn to_dto(&self, record: &InstalledPluginRecord) -> PluginResult<PluginDto> {
        let manifest = self.load_manifest(record)?;
        let grants = self.repository.list_permissions(&record.id)?;
        let _ = self.permissions.load_plugin(&record.id).await;
        let permissions = grants
            .into_iter()
            .map(|grant| PluginPermissionDto {
                permission: grant.capability.as_str().to_owned(),
                granted: grant.granted,
                hosts: grant.hosts,
            })
            .collect();
        let commands = manifest
            .contributes
            .commands
            .iter()
            .map(|command| PluginCommandDto {
                id: command.id.clone(),
                title: command.title.clone(),
                category: command.category.clone(),
            })
            .collect::<Vec<_>>();
        let workflows = manifest
            .contributes
            .workflows
            .iter()
            .map(|workflow| PluginWorkflowDto {
                id: workflow.id.clone(),
                file: workflow.file.clone(),
            })
            .collect::<Vec<_>>();
        let host_menus = manifest
            .contributes
            .host_menus
            .iter()
            .map(|menu| {
                let title = commands
                    .iter()
                    .find(|command| command.id == menu.command)
                    .map(|command| command.title.clone())
                    .unwrap_or_else(|| menu.command.clone());
                let workflow_id = workflows
                    .iter()
                    .find(|workflow| workflow.id == menu.command)
                    .map(|workflow| workflow.id.clone());
                PluginHostMenuDto {
                    plugin_id: record.id.clone(),
                    command: menu.command.clone(),
                    group: menu.group.clone(),
                    when: menu.when.clone(),
                    title,
                    workflow_id,
                }
            })
            .collect();
        Ok(PluginDto {
            id: record.id.clone(),
            name: record.name.clone(),
            version: record.version.clone(),
            publisher_id: record.publisher_id.clone(),
            enabled: record.enabled,
            description: manifest.description.clone(),
            category: manifest.category,
            logo: manifest.logo.clone(),
            logo_url: manifest.logo.as_ref().and_then(|logo| {
                crate::plugins::assets::PluginAssetResolver::resolve_logo_from_install(
                    Path::new(&record.install_path),
                    logo,
                )
                .ok()
                .map(|asset| asset.data_url)
            }),
            permissions,
            commands,
            host_menus,
            workflows,
        })
    }
}
