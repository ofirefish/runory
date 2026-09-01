use crate::domain::{AiAuditRecord, AppResult};
use crate::storage::JsonRepository;

#[derive(Clone)]
pub struct AiAuditRepository {
    repository: JsonRepository<Vec<AiAuditRecord>>,
}

impl AiAuditRepository {
    pub fn new(repository: JsonRepository<Vec<AiAuditRecord>>) -> Self {
        Self { repository }
    }

    pub async fn list(&self) -> AppResult<Vec<AiAuditRecord>> {
        self.repository.load_or_default().await
    }

    pub async fn save(&self, records: &[AiAuditRecord]) -> AppResult<()> {
        self.repository.save_atomic(&records.to_vec()).await
    }
}
