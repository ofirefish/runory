use tauri::State;

use crate::cloud::{CloudAuthSession, CloudAuthSessionStore};
use crate::domain::AppResult;

#[tauri::command]
pub async fn cloud_auth_session_load(
    sessions: State<'_, CloudAuthSessionStore>,
) -> AppResult<Option<CloudAuthSession>> {
    sessions.load().await
}

#[tauri::command]
pub async fn cloud_auth_session_save(
    session: CloudAuthSession,
    sessions: State<'_, CloudAuthSessionStore>,
) -> AppResult<()> {
    sessions.save(session).await
}

#[tauri::command]
pub async fn cloud_auth_session_clear(sessions: State<'_, CloudAuthSessionStore>) -> AppResult<()> {
    sessions.clear().await
}
