//! Single-use installation reviews bind user consent to Core-owned package bytes.
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::model::PluginDto;
use super::service::{PluginInstallRequest, PluginRegistryService};
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::installer::package::MAX_PACKAGE_BYTES;
use crate::plugins::manifest::PluginManifest;
use crate::plugins::signing::{verify::verify_package, TrustStatus};

const REVIEW_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_REVIEWS: usize = 8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallReview {
    pub review_id: String,
    pub manifest: PluginManifest,
    pub trust: TrustStatus,
    pub installed_version: Option<String>,
    pub permission_additions: Vec<String>,
    pub file_count: usize,
    pub compressed_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmPluginInstall {
    pub review_id: String,
    pub granted_permissions: Vec<String>,
}

pub(super) struct PendingReview {
    package: tempfile::NamedTempFile,
    digest: Vec<u8>,
    review: PluginInstallReview,
    baseline: serde_json::Value,
    created: Instant,
}

#[derive(Default)]
pub(super) struct InstallReviews(pub Mutex<HashMap<String, PendingReview>>);

impl PluginRegistryService {
    pub async fn prepare_install(
        &self,
        path: &Path,
        developer_mode: bool,
        expected: Option<(&str, &str)>,
    ) -> PluginResult<PluginInstallReview> {
        let _guard = self.mutation_lock.lock().await;
        let mut reviews = self.reviews.0.lock().map_err(|_| PluginError::Storage)?;
        reviews.retain(|_, review| review.created.elapsed() < REVIEW_TTL);
        if reviews.len() >= MAX_REVIEWS {
            return Err(PluginError::PolicyDenied);
        }
        let source = File::open(path).map_err(|_| PluginError::InvalidPackage)?;
        if !source
            .metadata()
            .map_err(|_| PluginError::InvalidPackage)?
            .is_file()
        {
            return Err(PluginError::InvalidPackage);
        }
        let mut bytes = Vec::new();
        source
            .take(MAX_PACKAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| PluginError::InvalidPackage)?;
        if bytes.len() as u64 > MAX_PACKAGE_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        let mut package = tempfile::NamedTempFile::new().map_err(|_| PluginError::Storage)?;
        package
            .write_all(&bytes)
            .map_err(|_| PluginError::Storage)?;
        package.flush().map_err(|_| PluginError::Storage)?;
        let inspection = self.inspect_package(&package.path().to_string_lossy())?;
        let manifest = inspection.manifest;
        if expected.is_some_and(|(id, version)| manifest.id != id || manifest.version != version) {
            return Err(PluginError::InvalidPackage);
        }
        let verification = verify_package(package.path(), &self.verify_options(developer_mode)?)?;
        if verification
            .metadata
            .as_ref()
            .is_some_and(|metadata| metadata.publisher != manifest.publisher.id)
        {
            return Err(PluginError::PublisherMismatch);
        }
        Self::require_install_trust(verification.trust, developer_mode)?;
        self.require_not_revoked(&manifest.id, &manifest.version)?;
        if self
            .repository
            .get_plugin_version(&manifest.id, &manifest.version)?
            .is_some()
        {
            return Err(PluginError::AlreadyInstalled);
        }
        let current = self.repository.get_active(&manifest.id)?;
        if current
            .as_ref()
            .is_some_and(|record| record.publisher_id != manifest.publisher.id)
        {
            return Err(PluginError::PublisherMismatch);
        }
        let grants = self.repository.list_permissions(&manifest.id)?;
        // A newly granted capability or wider host scope must be visible during review.
        let permission_additions = manifest
            .permissions
            .iter()
            .filter(|requested| {
                !grants.iter().any(|grant| {
                    grant.granted
                        && grant.capability.as_str() == requested.permission_id()
                        && (grant.hosts.is_empty()
                            || (!requested.hosts().is_empty()
                                && requested
                                    .hosts()
                                    .iter()
                                    .all(|host| grant.hosts.contains(host))))
                })
            })
            .map(|permission| permission.permission_id().to_owned())
            .collect();
        let review = PluginInstallReview {
            review_id: uuid::Uuid::new_v4().to_string(),
            installed_version: current.map(|record| record.version),
            permission_additions,
            manifest,
            trust: verification.trust,
            file_count: inspection.file_count,
            compressed_size: inspection.compressed_size,
        };
        let baseline = self.install_baseline(&review.manifest.id)?;
        reviews.insert(
            review.review_id.clone(),
            PendingReview {
                package,
                digest: ring::digest::digest(&ring::digest::SHA256, &bytes)
                    .as_ref()
                    .to_vec(),
                review: review.clone(),
                baseline,
                created: Instant::now(),
            },
        );
        Ok(review)
    }

    pub fn discard_install_review(&self, review_id: &str) -> PluginResult<()> {
        self.reviews
            .0
            .lock()
            .map_err(|_| PluginError::Storage)?
            .remove(review_id);
        Ok(())
    }

    pub(super) fn invalidate_install_reviews(&self, plugin_id: &str) -> PluginResult<()> {
        self.reviews
            .0
            .lock()
            .map_err(|_| PluginError::Storage)?
            .retain(|_, pending| pending.review.manifest.id != plugin_id);
        Ok(())
    }

    pub async fn confirm_install(
        &self,
        request: ConfirmPluginInstall,
        developer_mode: bool,
    ) -> PluginResult<PluginDto> {
        let _guard = self.mutation_lock.lock().await;
        // Consume before doing work: a failed or concurrent confirmation cannot replay consent.
        let pending = self
            .reviews
            .0
            .lock()
            .map_err(|_| PluginError::Storage)?
            .remove(&request.review_id)
            .ok_or(PluginError::PlanStale)?;
        if pending.created.elapsed() >= REVIEW_TTL
            || pending.baseline != self.install_baseline(&pending.review.manifest.id)?
        {
            return Err(PluginError::PlanStale);
        }
        let mut bytes = Vec::new();
        File::open(pending.package.path())
            .map_err(|_| PluginError::InvalidPackage)?
            .take(MAX_PACKAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| PluginError::InvalidPackage)?;
        if ring::digest::digest(&ring::digest::SHA256, &bytes).as_ref() != pending.digest {
            return Err(PluginError::HashMismatch);
        }
        if request.granted_permissions.iter().any(|grant| {
            !pending
                .review
                .manifest
                .permissions
                .iter()
                .any(|permission| permission.permission_id() == grant)
        }) {
            return Err(PluginError::PermissionDenied(
                "undeclared capability".into(),
            ));
        }
        self.install_inner(PluginInstallRequest {
            package_path: pending.package.path().to_string_lossy().into_owned(),
            granted_permissions: Some(request.granted_permissions),
            allow_unsigned: developer_mode,
        })
        .await
    }

    fn install_baseline(&self, plugin_id: &str) -> PluginResult<serde_json::Value> {
        let current = self.repository.get_active(plugin_id)?;
        let grants = self.repository.list_permissions(plugin_id)?;
        Ok(serde_json::json!({
            "version": current.as_ref().map(|record| &record.version),
            "enabled": current.as_ref().map(|record| record.enabled),
            "updatedAt": current.as_ref().map(|record| record.updated_at),
            "grants": grants,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::test_support::{detect_only_yaml, write_plugin_zip, PluginTestHarness};

    #[tokio::test]
    async fn expired_or_tampered_review_never_installs() {
        for expired in [true, false] {
            let harness = PluginTestHarness::new();
            let path = harness._root.path().join("review.zip");
            write_plugin_zip(
                &path,
                "runory.review",
                "1.0.0",
                &[],
                detect_only_yaml(),
                "detect.only",
            );
            let review = harness
                .registry
                .prepare_install(&path, true, None)
                .await
                .expect("review");
            {
                let mut reviews = harness.registry.reviews.0.lock().expect("lock");
                let pending = reviews.get_mut(&review.review_id).expect("pending");
                if expired {
                    pending.created = Instant::now() - REVIEW_TTL;
                } else {
                    std::fs::write(pending.package.path(), b"replaced").expect("tamper");
                }
            }
            let error = harness
                .registry
                .confirm_install(
                    ConfirmPluginInstall {
                        review_id: review.review_id,
                        granted_permissions: vec![],
                    },
                    true,
                )
                .await
                .expect_err("blocked");
            assert!(matches!(
                error,
                PluginError::PlanStale | PluginError::HashMismatch
            ));
            assert!(harness
                .registry
                .list_installed()
                .await
                .expect("list")
                .is_empty());
        }
    }
}
