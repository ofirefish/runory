mod service;

use crate::domain::{AppResult, AppSettings};
use crate::storage::JsonRepository;

pub use service::{AppSettingsPatch, SettingsService};

#[derive(Clone)]
pub struct SettingsRepository {
    repository: JsonRepository<AppSettings>,
}

impl SettingsRepository {
    pub fn new(repository: JsonRepository<AppSettings>) -> Self {
        Self { repository }
    }

    pub async fn load(&self) -> AppResult<AppSettings> {
        self.repository.load_or_default().await
    }

    pub async fn save(&self, settings: &AppSettings) -> AppResult<()> {
        self.repository.save_atomic(settings).await
    }
}
