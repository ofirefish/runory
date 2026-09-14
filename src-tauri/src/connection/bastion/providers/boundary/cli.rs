//! Boundary CLI wrapper.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::connection::bastion::asset::{AssetProtocol, BastionAsset, BastionProtocol};
use crate::helper::HelperError;

use super::auth::BoundarySessionState;

pub struct BoundaryCli {
    binary: PathBuf,
}

impl BoundaryCli {
    pub fn new(binary: PathBuf) -> Self {
        Self { binary }
    }

    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    pub fn version_string(&self) -> Result<String, HelperError> {
        let output = Command::new(&self.binary)
            .arg("version")
            .output()
            .map_err(|_| HelperError::Missing)?;
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().next().unwrap_or("boundary").trim().to_string();
        if line.is_empty() {
            Err(HelperError::VersionMismatch)
        } else {
            Ok(line)
        }
    }

    pub fn connect_args(target_id: &str, listen_port: u16) -> Vec<String> {
        vec![
            "connect".into(),
            "-target-id".into(),
            target_id.into(),
            "-token".into(),
            "env://BOUNDARY_TOKEN".into(),
            "-listen-addr".into(),
            "127.0.0.1".into(),
            "-listen-port".into(),
            listen_port.to_string(),
            "-inactive-timeout".into(),
            "-1".into(),
            "-format".into(),
            "json".into(),
        ]
    }

    pub fn connect_env(state: &BoundarySessionState) -> Vec<(String, String)> {
        vec![
            ("BOUNDARY_ADDR".into(), state.addr.clone()),
            ("BOUNDARY_TOKEN".into(), state.token.clone()),
        ]
    }

    /// Extract username/password from Boundary `connect -format=json` credentials.
    pub fn parse_brokered_ssh_credentials(
        payload: &str,
    ) -> Option<(String, zeroize::Zeroizing<String>)> {
        let value: serde_json::Value = serde_json::from_str(payload.trim()).ok().or_else(|| {
            let start = payload.find('{')?;
            let end = payload.rfind('}')?;
            serde_json::from_str(&payload[start..=end]).ok()
        })?;
        let credentials = value.get("credentials")?.as_array()?;
        for item in credentials {
            let credential = item
                .get("credential")
                .or_else(|| item.pointer("/secret/decoded"))?;
            let username = credential
                .get("username")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())?;
            let password = credential
                .get("password")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())?;
            return Some((
                username.to_string(),
                zeroize::Zeroizing::new(password.to_string()),
            ));
        }
        None
    }

    /// List targets the token can see (`boundary targets list -recursive -format=json`).
    pub fn list_targets_as_assets(
        &self,
        state: &BoundarySessionState,
        search: Option<&str>,
    ) -> Result<Vec<BastionAsset>, HelperError> {
        let output = Command::new(&self.binary)
            .args([
                "targets",
                "list",
                "-recursive",
                "-scope-id",
                "global",
                "-token",
                "env://BOUNDARY_TOKEN",
                "-format",
                "json",
            ])
            .env("BOUNDARY_ADDR", &state.addr)
            .env("BOUNDARY_TOKEN", &state.token)
            .output()
            .map_err(|_| HelperError::SpawnFailed)?;
        if !output.status.success() {
            tracing::warn!(
                status = ?output.status.code(),
                stderr_len = output.stderr.len(),
                "boundary targets list failed"
            );
            return Err(HelperError::EndpointDiscoveryFailed);
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        let items = parse_targets_json(&raw);
        let search = search
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        Ok(items
            .into_iter()
            .filter(|asset| {
                search.as_ref().is_none_or(|needle| {
                    asset.name.to_ascii_lowercase().contains(needle)
                        || asset.remote_id.to_ascii_lowercase().contains(needle)
                        || asset
                            .address
                            .as_ref()
                            .is_some_and(|address| address.to_ascii_lowercase().contains(needle))
                })
            })
            .collect())
    }
}

fn parse_targets_json(raw: &str) -> Vec<BastionAsset> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let list = if let Some(items) = value.get("items").and_then(|v| v.as_array()) {
        items.clone()
    } else if let Some(array) = value.as_array() {
        array.clone()
    } else {
        return Vec::new();
    };
    list.into_iter().filter_map(map_target_item).collect()
}

fn map_target_item(item: serde_json::Value) -> Option<BastionAsset> {
    let id = item.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(id)
        .to_string();
    let port = item
        .pointer("/attributes/default_port")
        .and_then(|v| v.as_u64())
        .and_then(|v| u16::try_from(v).ok())
        .or(Some(22));
    let scope_path = item
        .get("scope")
        .and_then(|scope| scope.get("name").and_then(|v| v.as_str()))
        .map(str::to_string);
    Some(BastionAsset {
        provider: "boundary".into(),
        remote_id: id.to_string(),
        name,
        address: None,
        platform: Some("linux".into()),
        protocols: vec![AssetProtocol {
            protocol: BastionProtocol::Ssh,
            port,
            enabled: true,
        }],
        node_path: scope_path,
        labels: HashMap::new(),
        metadata: item,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wrapped_and_raw_target_arrays() {
        let wrapped =
            r#"{"items":[{"id":"ttcp_1","name":"web","attributes":{"default_port":22}}]}"#;
        let assets = parse_targets_json(wrapped);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].remote_id, "ttcp_1");
        assert_eq!(assets[0].name, "web");

        let raw = r#"[{"id":"ttcp_2","name":"db"}]"#;
        let assets = parse_targets_json(raw);
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].remote_id, "ttcp_2");
    }

    #[test]
    fn parses_brokered_username_password_credentials() {
        let json = r#"{"address":"127.0.0.1","port":18022,"credentials":[{"credential":{"username":"zzly","password":"secret"}}]}"#;
        let parsed = BoundaryCli::parse_brokered_ssh_credentials(json).expect("creds");
        assert_eq!(parsed.0, "zzly");
        assert_eq!(parsed.1.as_str(), "secret");
    }
}
