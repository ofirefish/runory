use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;

use super::{BackgroundConnection, ConnectionIdentity};
use crate::domain::{AppError, AppResult, ServerProfile};

const PREPARATION_LIFETIME: Duration = Duration::from_secs(120);
const MAX_PENDING_PREPARATIONS: usize = 16;

struct PendingJump {
    target: ConnectionIdentity,
    jump: ConnectionIdentity,
    connection: Arc<BackgroundConnection>,
    expires_at: Instant,
}

#[derive(Default)]
pub struct JumpConnectionManager {
    pending: Mutex<HashMap<Uuid, PendingJump>>,
}

impl JumpConnectionManager {
    pub(crate) async fn store(
        &self,
        target: &ServerProfile,
        jump: &ServerProfile,
        connection: Arc<BackgroundConnection>,
    ) -> Uuid {
        let mut pending = self.pending.lock().await;
        pending.retain(|_, item| item.expires_at > Instant::now());
        if pending.len() >= MAX_PENDING_PREPARATIONS {
            if let Some(oldest_id) = pending
                .iter()
                .min_by_key(|(_, item)| item.expires_at)
                .map(|(id, _)| *id)
            {
                pending.remove(&oldest_id);
            }
        }
        let id = Uuid::new_v4();
        pending.insert(
            id,
            PendingJump {
                target: ConnectionIdentity::from(target),
                jump: ConnectionIdentity::from(jump),
                connection,
                expires_at: Instant::now() + PREPARATION_LIFETIME,
            },
        );
        id
    }

    pub(crate) async fn consume(
        &self,
        id: Uuid,
        target: &ServerProfile,
        jump: &ServerProfile,
    ) -> AppResult<Arc<BackgroundConnection>> {
        let item = self
            .pending
            .lock()
            .await
            .remove(&id)
            .filter(|item| item.expires_at > Instant::now())
            .filter(|item| item.target == ConnectionIdentity::from(target))
            .filter(|item| item.jump == ConnectionIdentity::from(jump))
            .ok_or(AppError::JumpPreparationExpired)?;
        Ok(item.connection)
    }

    pub async fn cancel(&self, id: Uuid) {
        self.pending.lock().await.remove(&id);
    }
}

pub(crate) fn jump_route_scope(jump: &ServerProfile) -> String {
    format!(
        "jump:{}:{}:{}",
        jump.id,
        jump.host.trim().to_lowercase(),
        jump.port
    )
}
