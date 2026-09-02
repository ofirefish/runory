//! Event repository wrapper that persists and live-broadcasts envelopes.

use std::sync::Arc;

use tokio::sync::broadcast;
use uuid::Uuid;

use super::event::AgentEventEnvelope;
use super::repository::{AgentEventRepository, AgentStoreError};

/// Persists events through `inner` and fan-outs each append to subscribers.
pub struct BroadcastingEventRepository {
    inner: Arc<dyn AgentEventRepository>,
    tx: broadcast::Sender<AgentEventEnvelope>,
}

impl BroadcastingEventRepository {
    pub fn new(inner: Arc<dyn AgentEventRepository>) -> Self {
        Self {
            inner,
            tx: broadcast::channel(256).0,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentEventEnvelope> {
        self.tx.subscribe()
    }
}

impl AgentEventRepository for BroadcastingEventRepository {
    fn append(&self, envelope: AgentEventEnvelope) -> Result<(), AgentStoreError> {
        self.inner.append(envelope.clone())?;
        let _ = self.tx.send(envelope);
        Ok(())
    }

    fn events_after(
        &self,
        run_id: Uuid,
        after_seq: u64,
    ) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
        self.inner.events_after(run_id, after_seq)
    }

    fn all_events(&self, run_id: Uuid) -> Result<Vec<AgentEventEnvelope>, AgentStoreError> {
        self.inner.all_events(run_id)
    }
}
