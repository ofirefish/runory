use std::sync::Arc;

use tokio::sync::Mutex;

use crate::domain::{AppResult, AppSettings, Language, Theme};

use super::SettingsRepository;

/// Persisted application settings (theme / language / helper CLI paths).
///
/// Follows the same Repository + write-lock pattern as `GroupService` so a
/// settings write can never race with another settings write or with the
/// JSON write lock held by the catalog services.
#[derive(Clone)]
pub struct SettingsService {
    repository: SettingsRepository,
    write_lock: Arc<Mutex<()>>,
}

impl SettingsService {
    pub fn new(repository: SettingsRepository, write_lock: Arc<Mutex<()>>) -> Self {
        Self {
            repository,
            write_lock,
        }
    }

    pub async fn get(&self) -> AppResult<AppSettings> {
        let _guard = self.write_lock.lock().await;
        self.repository.load().await
    }

    pub async fn update(&self, patch: AppSettingsPatch) -> AppResult<AppSettings> {
        let _guard = self.write_lock.lock().await;
        let mut settings = self.repository.load().await?;
        if let Some(theme) = patch.theme {
            settings.theme = theme;
        }
        if let Some(language) = patch.language {
            settings.language = language;
        }
        if let Some(path) = patch.boundary_cli_path {
            settings.boundary_cli_path = normalize_optional_path(path);
        }
        if let Some(path) = patch.teleport_cli_path {
            settings.teleport_cli_path = normalize_optional_path(path);
        }
        self.repository.save(&settings).await?;
        Ok(settings)
    }
}

fn normalize_optional_path(path: String) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Partial-update payload for `settings_update` so the frontend never has to
/// round-trip the whole file to change a single preference.
#[derive(Default)]
pub struct AppSettingsPatch {
    pub theme: Option<Theme>,
    pub language: Option<Language>,
    /// `Some("")` clears the stored path; `None` leaves it unchanged.
    pub boundary_cli_path: Option<String>,
    pub teleport_cli_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::JsonRepository;

    #[tokio::test]
    async fn persists_settings_across_reloads() {
        let directory = tempfile::tempdir().expect("temp directory");
        let service = SettingsService::new(
            SettingsRepository::new(JsonRepository::new(directory.path().join("settings.json"))),
            Arc::new(Mutex::new(())),
        );

        let initial = service.get().await.expect("default settings");
        assert_eq!(initial.theme, Theme::System);
        assert_eq!(initial.language, Language::EnUs);
        assert_eq!(initial.boundary_cli_path, None);
        assert_eq!(initial.teleport_cli_path, None);

        let updated = service
            .update(AppSettingsPatch {
                theme: Some(Theme::Dark),
                language: Some(Language::ZhCn),
                boundary_cli_path: Some(r"C:\Tools\boundary.exe".into()),
                teleport_cli_path: Some(r"C:\Tools\tsh.exe".into()),
            })
            .await
            .expect("update");

        assert_eq!(updated.theme, Theme::Dark);
        assert_eq!(updated.language, Language::ZhCn);
        assert_eq!(
            updated.boundary_cli_path.as_deref(),
            Some(r"C:\Tools\boundary.exe")
        );
        assert_eq!(updated.teleport_cli_path.as_deref(), Some(r"C:\Tools\tsh.exe"));

        let cleared = service
            .update(AppSettingsPatch {
                boundary_cli_path: Some(String::new()),
                ..Default::default()
            })
            .await
            .expect("clear boundary path");
        assert_eq!(cleared.boundary_cli_path, None);
        assert_eq!(cleared.teleport_cli_path.as_deref(), Some(r"C:\Tools\tsh.exe"));

        let reloaded = SettingsService::new(
            SettingsRepository::new(JsonRepository::new(directory.path().join("settings.json"))),
            Arc::new(Mutex::new(())),
        )
        .get()
        .await
        .expect("reload persisted settings");
        assert_eq!(reloaded.theme, Theme::Dark);
        assert_eq!(reloaded.language, Language::ZhCn);
        assert_eq!(reloaded.boundary_cli_path, None);
        assert_eq!(reloaded.teleport_cli_path.as_deref(), Some(r"C:\Tools\tsh.exe"));
    }
}
