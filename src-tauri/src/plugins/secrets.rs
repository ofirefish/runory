//! Core-owned secret resolution for plugin workflows.
//!
//! Plugins may only pass opaque `secret://…` references. Plaintext is resolved
//! inside Core immediately before the consuming action and must never enter
//! WorkflowContext, ActionResult outputs, or audit payloads.
//!
//! Bindings are process-memory only (not written to plugins.db). Restart clears
//! them; unbound refs fail closed.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use zeroize::Zeroizing;

use crate::plugins::error::{PluginError, PluginResult};

/// Resolve opaque secret references without exposing values to plugins.
#[async_trait]
pub trait PluginSecretResolver: Send + Sync {
    async fn resolve(&self, secret_ref: &str) -> PluginResult<Zeroizing<String>>;
}

/// In-memory binding map used by Core IPC and tests.
#[derive(Default)]
pub struct MapSecretResolver {
    bindings: RwLock<HashMap<String, Zeroizing<String>>>,
}

impl MapSecretResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn bind(
        &self,
        secret_ref: impl Into<String>,
        value: Zeroizing<String>,
    ) -> PluginResult<()> {
        let secret_ref = secret_ref.into();
        validate_secret_ref_shape(&secret_ref)?;
        if value.is_empty() || value.len() > 256 * 1024 || value.contains('\0') {
            return Err(PluginError::WorkflowValidation(
                "secret value empty, too large, or contains NUL".into(),
            ));
        }
        self.bindings.write().await.insert(secret_ref, value);
        Ok(())
    }

    pub async fn unbind(&self, secret_ref: &str) -> PluginResult<()> {
        validate_secret_ref_shape(secret_ref)?;
        self.bindings.write().await.remove(secret_ref);
        Ok(())
    }

    /// Metadata only — never returns secret plaintext.
    pub async fn list_refs(&self) -> Vec<SecretBindingMeta> {
        let guard = self.bindings.read().await;
        let mut out: Vec<_> = guard
            .keys()
            .map(|r| SecretBindingMeta {
                secret_ref: r.clone(),
                redacted: redact_secret_ref(r),
            })
            .collect();
        out.sort_by(|a, b| a.secret_ref.cmp(&b.secret_ref));
        out
    }

    pub async fn contains(&self, secret_ref: &str) -> bool {
        self.bindings.read().await.contains_key(secret_ref)
    }
}

#[async_trait]
impl PluginSecretResolver for MapSecretResolver {
    async fn resolve(&self, secret_ref: &str) -> PluginResult<Zeroizing<String>> {
        validate_secret_ref_shape(secret_ref)?;
        self.bindings
            .read()
            .await
            .get(secret_ref)
            .cloned()
            .ok_or_else(|| PluginError::WorkflowValidation("secret reference not bound".into()))
    }
}

/// Always fail-closed when no resolver is configured.
pub struct FailClosedSecretResolver;

#[async_trait]
impl PluginSecretResolver for FailClosedSecretResolver {
    async fn resolve(&self, secret_ref: &str) -> PluginResult<Zeroizing<String>> {
        validate_secret_ref_shape(secret_ref)?;
        Err(PluginError::WorkflowValidation(
            "secretRef requires Core secret resolver (Vault binding)".into(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBindingMeta {
    pub secret_ref: String,
    pub redacted: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSecretBindRequest {
    pub secret_ref: String,
    /// Taken and zeroized; never logged.
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSecretUnbindRequest {
    pub secret_ref: String,
}

pub fn validate_secret_ref_shape(value: &str) -> PluginResult<()> {
    if value.starts_with("secret://")
        && value.len() > "secret://".len()
        && value.len() <= 256
        && !value.contains('\0')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'/' | b'.' | b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(PluginError::WorkflowValidation(
            "invalid secretRef shape".into(),
        ))
    }
}

pub fn redact_secret_ref(value: &str) -> String {
    if let Some(rest) = value.strip_prefix("secret://") {
        let keep = rest.chars().take(4).collect::<String>();
        format!("secret://{keep}***")
    } else {
        "secret://***".into()
    }
}

pub type SharedSecretResolver = Arc<dyn PluginSecretResolver>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_resolve_unbind_roundtrip() {
        let resolver = MapSecretResolver::new();
        resolver
            .bind(
                "secret://django/shop",
                Zeroizing::new("SECRET_KEY=x\n".into()),
            )
            .await
            .unwrap();
        let value = resolver.resolve("secret://django/shop").await.unwrap();
        assert_eq!(value.as_str(), "SECRET_KEY=x\n");
        let listed = resolver.list_refs().await;
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].redacted.contains("shop"));
        resolver.unbind("secret://django/shop").await.unwrap();
        assert!(resolver.resolve("secret://django/shop").await.is_err());
    }

    #[tokio::test]
    async fn rejects_bad_ref_and_empty_value() {
        let resolver = MapSecretResolver::new();
        assert!(resolver
            .bind("vault://x", Zeroizing::new("a".into()))
            .await
            .is_err());
        assert!(resolver
            .bind("secret://ok", Zeroizing::new(String::new()))
            .await
            .is_err());
    }
}
