use crate::domain::{AppResult, KnownHost};
use crate::storage::CatalogDatabase;

#[derive(Clone)]
pub struct KnownHostRepository {
    database: CatalogDatabase,
}

impl KnownHostRepository {
    pub fn new(database: CatalogDatabase) -> Self {
        Self { database }
    }

    pub async fn list(&self) -> AppResult<Vec<KnownHost>> {
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.list_known_hosts())
            .await
            .map_err(|_| crate::domain::AppError::Storage)?
    }

    pub async fn save(&self, hosts: &[KnownHost]) -> AppResult<()> {
        let database = self.database.clone();
        let hosts = hosts.to_vec();
        tokio::task::spawn_blocking(move || database.replace_known_hosts(&hosts))
            .await
            .map_err(|_| crate::domain::AppError::Storage)?
    }
}
