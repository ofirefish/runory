use crate::domain::{AppResult, DeploymentRecord};
use crate::storage::JsonRepository;

#[derive(Clone)]
pub struct DeploymentHistoryRepository {
    repository: JsonRepository<Vec<DeploymentRecord>>,
}

impl DeploymentHistoryRepository {
    pub fn new(repository: JsonRepository<Vec<DeploymentRecord>>) -> Self {
        Self { repository }
    }
    pub async fn list(&self) -> AppResult<Vec<DeploymentRecord>> {
        self.repository.load_or_default().await
    }
    pub async fn save(&self, records: &[DeploymentRecord]) -> AppResult<()> {
        self.repository.save_atomic(&records.to_vec()).await
    }
}
