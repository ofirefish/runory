use serde::Deserialize;
use tauri::{ipc::Channel, AppHandle, State};

use crate::app_update::{AppUpdateEvent, AppUpdateService, AppUpdateSnapshot};
use crate::domain::{AppError, AppResult};
use crate::ssh::ServerSessionManager;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInstallRequest {
    force: bool,
}

#[tauri::command]
pub async fn app_update_status(
    app: AppHandle,
    updates: State<'_, AppUpdateService>,
) -> AppResult<AppUpdateSnapshot> {
    Ok(updates.snapshot(&app).await)
}

#[tauri::command]
pub async fn app_update_check(
    app: AppHandle,
    updates: State<'_, AppUpdateService>,
) -> AppResult<AppUpdateSnapshot> {
    updates.check(&app).await
}

#[tauri::command]
pub async fn app_update_download(
    app: AppHandle,
    updates: State<'_, AppUpdateService>,
    on_event: Channel<AppUpdateEvent>,
) -> AppResult<AppUpdateSnapshot> {
    updates.download(&app, on_event).await
}

#[tauri::command]
pub async fn app_update_install(
    request: AppUpdateInstallRequest,
    app: AppHandle,
    updates: State<'_, AppUpdateService>,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<()> {
    if !request.force && sessions.has_active_sessions().await {
        return Err(AppError::UpdateBusy);
    }
    if request.force {
        sessions.disconnect_all().await?;
    }
    updates.install().await?;
    #[cfg(not(windows))]
    app.restart();
    #[cfg(windows)]
    let _ = app;
    Ok(())
}
