use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::credentials::CredentialService;
use crate::domain::{
    AppError, AppResult, CredentialStatus, CredentialStatusRequest, ForgetCredentialRequest,
    PrivateKeyImport, PrivateKeyRequest, VaultUnlockRequest,
};

#[tauri::command]
pub async fn vault_unlock(
    request: VaultUnlockRequest,
    credentials: State<'_, CredentialService>,
) -> AppResult<()> {
    credentials.unlock(request.master_password).await
}

#[tauri::command]
pub async fn vault_initialize(credentials: State<'_, CredentialService>) -> AppResult<()> {
    credentials.initialize_with_platform_key().await
}

#[tauri::command]
pub async fn vault_unlock_with_platform(
    credentials: State<'_, CredentialService>,
) -> AppResult<()> {
    credentials.unlock_with_platform_key().await
}

#[tauri::command]
pub async fn vault_lock(credentials: State<'_, CredentialService>) -> AppResult<()> {
    credentials.lock().await
}

#[tauri::command]
pub async fn credential_status(
    request: CredentialStatusRequest,
    credentials: State<'_, CredentialService>,
) -> AppResult<CredentialStatus> {
    credentials.status(request.profile_id, request.kind).await
}

#[tauri::command]
pub async fn credential_forget(
    request: ForgetCredentialRequest,
    credentials: State<'_, CredentialService>,
) -> AppResult<()> {
    credentials.forget(request.profile_id, request.kind).await
}

#[tauri::command]
pub async fn private_key_import(
    app: AppHandle,
    credentials: State<'_, CredentialService>,
) -> AppResult<Option<PrivateKeyImport>> {
    let Some(selected) = app.dialog().file().blocking_pick_file() else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| AppError::LocalFileInvalid)?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::LocalFileInvalid)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 {
        return Err(AppError::PrivateKeyInvalid);
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("private-key")
        .to_owned();
    let content = tokio::fs::read(path)
        .await
        .map_err(|_| AppError::LocalFileInvalid)?;
    let key_id = credentials.import_private_key(content).await?;
    Ok(Some(PrivateKeyImport { key_id, name }))
}

#[tauri::command]
pub async fn private_key_forget(
    request: PrivateKeyRequest,
    credentials: State<'_, CredentialService>,
) -> AppResult<()> {
    credentials.forget_private_key(request.key_id).await
}
