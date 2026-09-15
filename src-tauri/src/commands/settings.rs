use serde::Deserialize;
use tauri::State;

use crate::domain::{AppError, AppResult, AppSettings, Language, TerminalTheme, Theme};
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
    pub terminal_theme: Option<TerminalTheme>,
    pub boundary_cli_path: Option<String>,
    pub teleport_cli_path: Option<String>,
}

#[tauri::command]
pub async fn settings_update(
    request: SettingsUpdateRequest,
    settings: State<'_, SettingsService>,
) -> AppResult<AppSettings> {
    if request.theme.is_none()
        && request.language.is_none()
        && request.terminal_theme.is_none()
        && request.boundary_cli_path.is_none()
        && request.teleport_cli_path.is_none()
    {
        return Err(AppError::InvalidOperation);
    }
    settings
        .update(AppSettingsPatch {
            theme: request.theme,
            language: request.language,
            terminal_theme: request.terminal_theme,
            boundary_cli_path: request.boundary_cli_path,
            teleport_cli_path: request.teleport_cli_path,
        })
        .await
}
