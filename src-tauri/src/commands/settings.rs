use serde::Deserialize;
use tauri::State;

use crate::domain::{AppError, AppResult, AppSettings, Language, Theme};
use crate::settings::{AppSettingsPatch, SettingsService};

#[tauri::command]
pub async fn settings_get(settings: State<'_, SettingsService>) -> AppResult<AppSettings> {
    settings.get().await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdateRequest {
    pub theme: Option<Theme>,
    pub language: Option<Language>,
}

#[tauri::command]
pub async fn settings_update(
    request: SettingsUpdateRequest,
    settings: State<'_, SettingsService>,
) -> AppResult<AppSettings> {
    if request.theme.is_none() && request.language.is_none() {
        return Err(AppError::InvalidOperation);
    }
    settings
        .update(AppSettingsPatch {
            theme: request.theme,
            language: request.language,
        })
        .await
}
