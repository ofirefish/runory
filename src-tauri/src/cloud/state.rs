use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AppResult, CloudObjectKind};
use crate::storage::JsonRepository;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudTombstone {
    pub kind: CloudObjectKind,
    pub id: Uuid,
    pub deleted_at: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrganizationSyncState {
    #[serde(default)]
    pub known_groups: HashSet<Uuid>,
    #[serde(default)]
    pub known_profiles: HashSet<Uuid>,
    #[serde(default)]
    pub tombstones: Vec<CloudTombstone>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudSyncState {
    #[serde(default)]
    pub organizations: HashMap<Uuid, OrganizationSyncState>,
}

#[derive(Clone)]
pub struct CloudSyncStateRepository {
    repository: JsonRepository<CloudSyncState>,
}

impl CloudSyncStateRepository {
    pub fn at_path(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            repository: JsonRepository::new(path),
        }
    }

    pub(crate) async fn load(&self) -> AppResult<CloudSyncState> {
        self.repository.load_or_default().await
    }

    pub(crate) async fn save(&self, state: &CloudSyncState) -> AppResult<()> {
        self.repository.save_atomic(state).await
    }
}

impl OrganizationSyncState {
    pub(crate) fn tombstone(&self, kind: CloudObjectKind, id: Uuid) -> Option<&CloudTombstone> {
        self.tombstones
            .iter()
            .find(|item| item.kind == kind && item.id == id)
    }

    pub(crate) fn remove_tombstone(&mut self, kind: CloudObjectKind, id: Uuid) {
        self.tombstones
            .retain(|item| item.kind != kind || item.id != id);
    }

    pub(crate) fn upsert_tombstone(&mut self, tombstone: CloudTombstone) {
        self.remove_tombstone(tombstone.kind, tombstone.id);
        self.tombstones.push(tombstone);
    }
}
