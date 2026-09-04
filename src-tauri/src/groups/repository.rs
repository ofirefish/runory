use crate::{
    domain::{AppResult, HostGroup},
    storage::CatalogDatabase,
};

#[derive(Clone)]
pub struct GroupRepository {
    database: CatalogDatabase,
}

impl GroupRepository {
    pub fn new(database: CatalogDatabase) -> Self {
        Self { database }
    }

    pub async fn list(&self) -> AppResult<Vec<HostGroup>> {
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.list_groups())
            .await
            .map_err(|_| crate::domain::AppError::Storage)?
    }

    pub async fn save(&self, groups: &[HostGroup]) -> AppResult<()> {
        let database = self.database.clone();
        let groups = groups.to_vec();
        tokio::task::spawn_blocking(move || database.replace_groups(&groups))
            .await
            .map_err(|_| crate::domain::AppError::Storage)?
    }
}
