use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AppError, AppResult};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TunnelRule {
    pub id: Uuid,
    pub name: String,
    pub profile_id: Uuid,
    pub target_host: String,
    pub target_port: u16,
    pub local_port: u16,
}

impl TunnelRule {
    pub fn validate(&self) -> AppResult<()> {
        let host = &self.target_host;
        let valid_host = host.parse::<std::net::IpAddr>().is_ok()
            || (host.len() <= 253
                && host
                    .strip_suffix('.')
                    .unwrap_or(host)
                    .split('.')
                    .all(|part| {
                        !part.is_empty()
                            && part.len() <= 63
                            && !part.starts_with('-')
                            && !part.ends_with('-')
                            && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    }));
        if self.id.is_nil()
            || self.profile_id.is_nil()
            || self.name.trim().is_empty()
            || self.name.encode_utf16().count() > 120
            || self.name.chars().any(char::is_control)
            || !valid_host
            || self.local_port == 0
            || self.target_port == 0
        {
            return Err(AppError::TunnelInvalid);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveTunnelRequest {
    pub id: Option<Uuid>,
    pub name: String,
    pub profile_id: Uuid,
    pub target_host: String,
    pub target_port: u16,
    pub local_port: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TunnelState {
    #[default]
    Stopped,
    Running,
    Interrupted,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TunnelHealth {
    #[default]
    Unchecked,
    Reachable,
    Unreachable,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelEvent {
    pub at: u64,
    pub code: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub state: TunnelState,
    pub session_id: Option<Uuid>,
    pub started_at: Option<u64>,
    pub checked_at: Option<u64>,
    pub health: TunnelHealth,
    pub error_code: Option<String>,
    pub active_connections: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub events: Vec<TunnelEvent>,
}

impl TunnelStatus {
    pub fn event(&mut self, code: &str) {
        if self.events.len() >= 16 {
            self.events.remove(0);
        }
        self.events.push(TunnelEvent {
            at: now(),
            code: code.into(),
        });
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub rule: TunnelRule,
    pub status: TunnelStatus,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
