use crate::{
    domain::{AppResult, ServerProfile},
    storage::JsonRepository,
};

#[derive(Clone)]
pub struct ProfileRepository {
    repository: JsonRepository<Vec<ServerProfile>>,
}

impl ProfileRepository {
    pub fn new(repository: JsonRepository<Vec<ServerProfile>>) -> Self {
        Self { repository }
    }

    pub async fn list(&self) -> AppResult<Vec<ServerProfile>> {
        self.repository.load_or_default().await
    }

    pub async fn save(&self, profiles: &[ServerProfile]) -> AppResult<()> {
        self.repository.save_atomic(&profiles.to_vec()).await
    }
}
