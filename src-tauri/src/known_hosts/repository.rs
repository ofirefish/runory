use crate::domain::{AppResult, KnownHost};
use crate::storage::JsonRepository;

#[derive(Clone)]
pub struct KnownHostRepository {
    storage: JsonRepository<Vec<KnownHost>>,
}

impl KnownHostRepository {
    pub fn new(storage: JsonRepository<Vec<KnownHost>>) -> Self {
        Self { storage }
    }

    pub async fn list(&self) -> AppResult<Vec<KnownHost>> {
        self.storage.load_or_default().await
    }

    pub async fn save(&self, hosts: &[KnownHost]) -> AppResult<()> {
        self.storage.save_atomic(&hosts.to_vec()).await
    }
}
