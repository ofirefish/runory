use crate::{
    domain::{AppResult, HostGroup},
    storage::JsonRepository,
};

#[derive(Clone)]
pub struct GroupRepository {
    repository: JsonRepository<Vec<HostGroup>>,
}

impl GroupRepository {
    pub fn new(repository: JsonRepository<Vec<HostGroup>>) -> Self {
        Self { repository }
    }

    pub async fn list(&self) -> AppResult<Vec<HostGroup>> {
        self.repository.load_or_default().await
    }

    pub async fn save(&self, groups: &[HostGroup]) -> AppResult<()> {
        self.repository.save_atomic(&groups.to_vec()).await
    }
}
