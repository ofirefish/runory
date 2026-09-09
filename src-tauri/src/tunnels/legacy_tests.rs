use super::{
    model::*,
    runtime::LiveTunnel,
    service::{ensure_stopped, TunnelService},
};
use crate::{
    domain::{AppError, AppResult},
    ssh::ServerSessionManager,
};
use tokio::net::TcpListener;
use uuid::Uuid;

impl TunnelService {
    pub async fn start(
        &self,
        id: Uuid,
        session_id: Uuid,
        authorized_profile: Uuid,
        sessions: &ServerSessionManager,
    ) -> AppResult<()> {
        let mut guard = self.data.lock().await;
        self.load(&mut guard).await?;
        let data = guard.as_mut().ok_or(AppError::Storage)?;
        let rule = data
            .rules
            .iter()
            .find(|rule| rule.id == id)
            .cloned()
            .ok_or(AppError::TunnelNotFound)?;
        // Recheck after IPC policy awaits: an edit cannot redirect an authorized start.
        if rule.profile_id != authorized_profile {
            return Err(AppError::TunnelSessionMismatch);
        }
        ensure_stopped(data, id).await?;
        let mut running = 0;
        for live in data.live.values() {
            if live.snapshot().await.state == TunnelState::Running {
                running += 1;
            }
        }
        if running >= 16 {
            return Err(AppError::TunnelLimit);
        }
        let transport = sessions
            .forward_transport(session_id, rule.profile_id)
            .await?;
        if let Some(mut previous) = data.live.remove(&id) {
            previous.shutdown().await;
        }
        let listener =
            match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, rule.local_port)).await {
                Ok(listener) => listener,
                Err(error) => {
                    let error = if error.kind() == std::io::ErrorKind::AddrInUse {
                        AppError::TunnelPortInUse
                    } else {
                        AppError::TunnelBindFailed
                    };
                    let mut status = TunnelStatus {
                        state: TunnelState::Error,
                        error_code: Some(error.code().into()),
                        ..Default::default()
                    };
                    status.event(error.code());
                    data.failures.insert(id, status);
                    return Err(error);
                }
            };
        if transport.is_closed() {
            return Err(AppError::ConnectionLost);
        }
        data.failures.remove(&id);
        data.live
            .insert(id, LiveTunnel::start(rule, session_id, transport, listener));
        tracing::info!(session_id = %session_id, profile_id = %authorized_profile, "local SSH tunnel started");
        Ok(())
    }
}
