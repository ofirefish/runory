use crate::{
    domain::{AppResult, ServerProfile},
    storage::CatalogDatabase,
};

#[derive(Clone)]
pub struct ProfileRepository {
    database: CatalogDatabase,
}

impl ProfileRepository {
    pub fn new(database: CatalogDatabase) -> Self {
        Self { database }
    }

    pub async fn list(&self) -> AppResult<Vec<ServerProfile>> {
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || database.list_profiles())
            .await
            .map_err(|_| crate::domain::AppError::Storage)?
    }

    pub async fn save(&self, profiles: &[ServerProfile]) -> AppResult<()> {
        let database = self.database.clone();
        let profiles = profiles.to_vec();
        tokio::task::spawn_blocking(move || database.replace_profiles(&profiles))
            .await
            .map_err(|_| crate::domain::AppError::Storage)?
    }
}
