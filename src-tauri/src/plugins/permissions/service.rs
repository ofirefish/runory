use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::capability::PluginCapability;
use super::grant::PermissionGrant;
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::registry::repository::PluginRegistryRepository;

pub struct PermissionService {
    repository: Arc<PluginRegistryRepository>,
    cache: RwLock<HashMap<String, Vec<PermissionGrant>>>,
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[tokio::test]
    async fn scoped_grants_require_exact_host_and_reflect_revocation() {
        let repo = Arc::new(PluginRegistryRepository::open_in_memory().expect("repository"));
        let service = PermissionService::new(repo.clone());
        let grant = PermissionGrant {
            plugin_id: "runory.scoped".into(),
            capability: PluginCapability::HostRead,
            granted: true,
            hosts: vec!["host-a".into()],
        };
        service
            .set_grants("runory.scoped", vec![grant.clone()])
            .await
            .expect("grant");
        assert!(service
            .require_for_host("runory.scoped", PluginCapability::HostRead, Some("host-a"))
            .is_ok());
        for host in [None, Some("host-b"), Some("*")] {
            assert!(service
                .require_for_host("runory.scoped", PluginCapability::HostRead, host)
                .is_err());
        }
        repo.replace_permissions(
            "runory.scoped",
            &[PermissionGrant {
                granted: false,
                ..grant
            }],
        )
        .expect("revoke");
        assert!(service
            .require_for_host("runory.scoped", PluginCapability::HostRead, Some("host-a"))
            .is_err());
    }
}

impl PermissionService {
    pub fn new(repository: Arc<PluginRegistryRepository>) -> Self {
        Self {
            repository,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub async fn load_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        let grants = self.repository.list_permissions(plugin_id)?;
        self.cache
            .write()
            .await
            .insert(plugin_id.to_owned(), grants);
        Ok(())
    }

    pub async fn set_grants(
        &self,
        plugin_id: &str,
        grants: Vec<PermissionGrant>,
    ) -> PluginResult<()> {
        self.repository.replace_permissions(plugin_id, &grants)?;
        self.cache
            .write()
            .await
            .insert(plugin_id.to_owned(), grants);
        Ok(())
    }

    pub async fn require(&self, plugin_id: &str, capability: PluginCapability) -> PluginResult<()> {
        let cache = self.cache.read().await;
        let grants = cache.get(plugin_id).ok_or_else(|| {
            PluginError::PermissionDenied(format!(
                "no grants loaded for {plugin_id} ({})",
                capability.as_str()
            ))
        })?;
        let allowed = grants
            .iter()
            .any(|grant| grant.capability == capability && grant.granted);
        if allowed {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied(capability.as_str().into()))
        }
    }

    pub async fn clear(&self, plugin_id: &str) {
        self.cache.write().await.remove(plugin_id);
    }

    /// Host restrictions are checked against the real session owner by the runtime.
    pub fn require_for_host(
        &self,
        plugin_id: &str,
        capability: PluginCapability,
        host_id: Option<&str>,
    ) -> PluginResult<()> {
        let grants = self.repository.list_permissions(plugin_id)?;
        if grants.iter().any(|grant| {
            grant.capability == capability
                && grant.granted
                && (grant.hosts.is_empty()
                    || host_id.is_some_and(|host| grant.hosts.iter().any(|id| id == host)))
        }) {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied(capability.as_str().into()))
        }
    }
}
