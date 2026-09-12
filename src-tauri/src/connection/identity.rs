//! Logical target identity vs physical transport endpoint.
//!
//! Boundary / Teleport local proxies bind HostKey to a stable logical target,
//! never to `127.0.0.1:ephemeral_port`.

use serde::{Deserialize, Serialize};

/// Stable identity of the remote peer Runory is connecting to, independent of
/// the physical socket used by the transport layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicalTarget {
    /// Stable id used for Known Hosts / audit (e.g. profile id, asset id, Boundary target id).
    pub stable_id: String,
    pub display_name: String,
    /// Optional alias used when the transport endpoint differs from the identity host
    /// (local proxy, stdio proxy, session broker).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_identity_alias: Option<String>,
}

impl LogicalTarget {
    pub fn new(
        stable_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            stable_id: stable_id.into(),
            display_name: display_name.into(),
            host_identity_alias: None,
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.host_identity_alias = Some(alias.into());
        self
    }

    /// Scope string for KnownHost lookups. Prefer alias when present.
    pub fn known_host_scope(&self) -> &str {
        self.host_identity_alias
            .as_deref()
            .unwrap_or(self.stable_id.as_str())
    }
}

/// How Runory verifies the SSH peer identity for a prepared connection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostIdentityPolicy {
    /// Classic known_hosts fingerprint check keyed by hostname / alias.
    KnownHost {
        hostname: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// Teleport-style host certificate validated against a configured CA.
    HostCertificateAuthority {
        /// SSH public key bytes of the CA (OpenSSH format text).
        ca_openssh: String,
        principals: Vec<String>,
    },
    /// Provider owns verification (still must not accept-all).
    ProviderManaged {
        provider_id: String,
        identity: ProviderHostIdentity,
    },
}

/// Provider-supplied host identity metadata (no secrets).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHostIdentity {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_hint: Option<String>,
}

impl HostIdentityPolicy {
    pub fn known_host(hostname: impl Into<String>) -> Self {
        Self::KnownHost {
            hostname: hostname.into(),
            scope: None,
        }
    }

    pub fn known_host_scoped(hostname: impl Into<String>, scope: impl Into<String>) -> Self {
        Self::KnownHost {
            hostname: hostname.into(),
            scope: Some(scope.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_target_prefers_alias_for_known_host_scope() {
        let target = LogicalTarget::new("ttcp_abc", "prod-db-01").with_alias("boundary:ttcp_abc");
        assert_eq!(target.known_host_scope(), "boundary:ttcp_abc");
    }
}
