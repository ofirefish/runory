use super::{
    model::*,
    runtime::LiveTunnel,
    service::{ensure_stopped, TunnelService},
};
use crate::{
    domain::{AppError, AppResult},
    ssh::{BackgroundConnection, ConnectionIdentity},
};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Weak},
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{watch, Mutex},
};
use uuid::Uuid;

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct BackgroundPool {
    connections: Mutex<HashMap<Uuid, (ConnectionIdentity, Weak<BackgroundConnection>)>>,
}

impl BackgroundPool {
    async fn acquire<F, Fut>(
        &self,
        identity: ConnectionIdentity,
        connect: F,
    ) -> AppResult<(Arc<BackgroundConnection>, bool)>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<(BackgroundConnection, bool)>>,
    {
        // Serialize authentication/reuse; callers bound the whole acquisition to 30 seconds.
        let mut pool = self.connections.lock().await;
        pool.retain(|_, (_, weak)| weak.strong_count() > 0);
        if let Some((existing, weak)) = pool.get(&identity.profile_id) {
            if existing == &identity {
                if let Some(connection) = weak.upgrade().filter(|c| !c.transport().is_closed()) {
                    // No new authentication occurred, so do not claim a newly supplied secret was saved.
                    return Ok((connection, false));
                }
            }
        }
        let (connection, saved) = connect().await?;
        let connection = Arc::new(connection);
        pool.insert(identity.profile_id, (identity, Arc::downgrade(&connection)));
        Ok((connection, saved))
    }
}

impl TunnelService {
    pub(crate) async fn start_background<F, Fut>(
        &self,
        id: Uuid,
        identity: ConnectionIdentity,
        connect: F,
    ) -> AppResult<bool>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<(BackgroundConnection, bool)>>,
    {
        let attempt = Uuid::new_v4();
        let (rule, mut cancel) = {
            let mut guard = self.data.lock().await;
            self.load(&mut guard).await?;
            let data = guard.as_mut().ok_or(AppError::Storage)?;
            let rule = data
                .rules
                .iter()
                .find(|rule| rule.id == id)
                .cloned()
                .ok_or(AppError::TunnelNotFound)?;
            if rule.profile_id != identity.profile_id {
                return Err(AppError::TunnelSessionMismatch);
            }
            ensure_stopped(data, id).await?;
            let mut count = data.pending.len();
            for live in data.live.values() {
                if live.snapshot().await.state == TunnelState::Running {
                    count += 1;
                }
            }
            if count >= 16 {
                return Err(AppError::TunnelLimit);
            }
            let (sender, receiver) = watch::channel(false);
            data.pending.insert(id, (attempt, sender));
            (rule, receiver)
        };
        // Stop is allowed while authenticating. Dropping the future releases transient credentials.
        let opened = tokio::select! {
            biased;
            _ = cancel.changed() => Err(AppError::TunnelStopped),
            result = tokio::time::timeout(Duration::from_secs(30), self.background.acquire(identity, connect)) =>
                result.unwrap_or(Err(AppError::ConnectionTimeout)),
        };
        let mut guard = self.data.lock().await;
        let data = guard.as_mut().ok_or(AppError::Storage)?;
        if data
            .pending
            .get(&id)
            .is_none_or(|(token, _)| *token != attempt)
        {
            return Err(AppError::TunnelStopped);
        }
        data.pending.remove(&id);
        let result = async {
            let (connection, saved) = opened?;
            let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, rule.local_port))
                .await
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AddrInUse {
                        AppError::TunnelPortInUse
                    } else {
                        AppError::TunnelBindFailed
                    }
                })?;
            if connection.transport().is_closed() {
                return Err(AppError::ConnectionLost);
            }
            if let Some(mut previous) = data.live.remove(&id) {
                previous.shutdown().await;
            }
            data.failures.remove(&id);
            data.live
                .insert(id, LiveTunnel::start_background(rule, connection, listener));
            Ok(saved)
        }
        .await;
        if let Err(error) = &result {
            if !matches!(
                error,
                AppError::TunnelConnectionRequired | AppError::TunnelStopped
            ) {
                if let Some(mut previous) = data.live.remove(&id) {
                    previous.shutdown().await;
                }
                let mut status = TunnelStatus {
                    state: TunnelState::Error,
                    error_code: Some(error.code().into()),
                    ..Default::default()
                };
                status.event(error.code());
                data.failures.insert(id, status);
            }
        }
        result
    }
}
