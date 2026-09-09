use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::domain::{AppError, AppResult};

const AVATAR_CACHE_LIMIT: usize = 2 * 1024 * 1024;
const AVATAR_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAvatarCacheRequest {
    pub user_id: Uuid,
    #[serde(default)]
    pub avatar_version: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAvatarCacheStoreRequest {
    pub user_id: Uuid,
    pub avatar_version: u64,
    pub signed_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedCloudAvatar {
    pub mime_type: String,
    pub data_base64: String,
}

#[tauri::command]
pub async fn cloud_avatar_cache_get(
    request: CloudAvatarCacheRequest,
    app: AppHandle,
) -> AppResult<Option<CachedCloudAvatar>> {
    let directory = avatar_cache_directory(&app)?;
    match request.avatar_version {
        Some(version) => read_cached_avatar(&directory, request.user_id, version).await,
        None => read_latest_cached_avatar(&directory, request.user_id).await,
    }
}

#[tauri::command]
pub async fn cloud_avatar_cache_store(
    request: CloudAvatarCacheStoreRequest,
    app: AppHandle,
) -> AppResult<CachedCloudAvatar> {
    let directory = avatar_cache_directory(&app)?;
    if let Some(cached) =
        read_cached_avatar(&directory, request.user_id, request.avatar_version).await?
    {
        return Ok(cached);
    }
    validate_signed_avatar_url(&request.signed_url)?;
    let response = reqwest::Client::builder()
        .timeout(AVATAR_DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|_| AppError::CloudPolicyUnavailable)?
        .get(&request.signed_url)
        .send()
        .await
        .map_err(|_| AppError::CloudPolicyUnavailable)?
        .error_for_status()
        .map_err(|_| AppError::CloudPolicyUnavailable)?;
    if response
        .content_length()
        .is_some_and(|length| length > AVATAR_CACHE_LIMIT as u64)
    {
        return Err(AppError::CloudInvalid);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| AppError::CloudPolicyUnavailable)?;
    if bytes.is_empty() || bytes.len() > AVATAR_CACHE_LIMIT {
        return Err(AppError::CloudInvalid);
    }
    let (extension, mime_type) = detect_avatar_format(&bytes).ok_or(AppError::CloudInvalid)?;
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|_| AppError::Storage)?;
    remove_user_avatar_files(&directory, request.user_id).await?;
    let target = avatar_cache_path(
        &directory,
        request.user_id,
        request.avatar_version,
        extension,
    );
    let temporary = target.with_extension(format!("{extension}.{}.tmp", Uuid::new_v4()));
    tokio::fs::write(&temporary, &bytes)
        .await
        .map_err(|_| AppError::Storage)?;
    if tokio::fs::rename(&temporary, &target).await.is_err() {
        let target_exists = tokio::fs::try_exists(&target)
            .await
            .map_err(|_| AppError::Storage)?;
        let _ = tokio::fs::remove_file(&temporary).await;
        if !target_exists {
            return Err(AppError::Storage);
        }
    }
    Ok(CachedCloudAvatar {
        mime_type: mime_type.into(),
        data_base64: BASE64.encode(bytes),
    })
}

#[tauri::command]
pub async fn cloud_avatar_cache_remove(user_id: Uuid, app: AppHandle) -> AppResult<()> {
    remove_user_avatar_files(&avatar_cache_directory(&app)?, user_id).await
}

fn avatar_cache_directory(app: &AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_cache_dir()
        .map(|path| path.join("avatars"))
        .map_err(|_| AppError::Storage)
}

fn avatar_cache_path(directory: &Path, user_id: Uuid, version: u64, extension: &str) -> PathBuf {
    directory.join(format!("{user_id}-{version}.{extension}"))
}

async fn read_cached_avatar(
    directory: &Path,
    user_id: Uuid,
    version: u64,
) -> AppResult<Option<CachedCloudAvatar>> {
    for (extension, mime_type) in [
        ("webp", "image/webp"),
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
    ] {
        let path = avatar_cache_path(directory, user_id, version, extension);
        match tokio::fs::read(path).await {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= AVATAR_CACHE_LIMIT => {
                return Ok(Some(CachedCloudAvatar {
                    mime_type: mime_type.into(),
                    data_base64: BASE64.encode(bytes),
                }));
            }
            Ok(_) => return Err(AppError::CloudInvalid),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(AppError::Storage),
        }
    }
    Ok(None)
}

async fn read_latest_cached_avatar(
    directory: &Path,
    user_id: Uuid,
) -> AppResult<Option<CachedCloudAvatar>> {
    let mut entries = match tokio::fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AppError::Storage),
    };
    let prefix = format!("{user_id}-");
    let mut latest_version = None;
    while let Some(entry) = entries.next_entry().await.map_err(|_| AppError::Storage)? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(version) = name
            .strip_prefix(&prefix)
            .and_then(|suffix| suffix.split('.').next())
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        latest_version = Some(latest_version.map_or(version, |current: u64| current.max(version)));
    }
    match latest_version {
        Some(version) => read_cached_avatar(directory, user_id, version).await,
        None => Ok(None),
    }
}

async fn remove_user_avatar_files(directory: &Path, user_id: Uuid) -> AppResult<()> {
    let mut entries = match tokio::fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AppError::Storage),
    };
    let prefix = format!("{user_id}-");
    while let Some(entry) = entries.next_entry().await.map_err(|_| AppError::Storage)? {
        let name = entry.file_name();
        let supported_extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| matches!(value, "webp" | "png" | "jpg"));
        if name.to_string_lossy().starts_with(&prefix) && supported_extension {
            tokio::fs::remove_file(entry.path())
                .await
                .map_err(|_| AppError::Storage)?;
        }
    }
    Ok(())
}

fn validate_signed_avatar_url(value: &str) -> AppResult<()> {
    let url = reqwest::Url::parse(value).map_err(|_| AppError::CloudInvalid)?;
    let trusted_host = url
        .host_str()
        .is_some_and(|host| host.ends_with(".supabase.co"));
    if url.scheme() != "https"
        || !trusted_host
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AppError::CloudInvalid);
    }
    Ok(())
}

fn detect_avatar_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(("jpg", "image/jpeg"));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(("png", "image/png"));
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(("webp", "image/webp"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_supabase_https_signed_urls_are_accepted() {
        assert!(validate_signed_avatar_url(
            "https://project.supabase.co/storage/v1/object/sign/avatars/a.webp?token=x"
        )
        .is_ok());
        assert!(validate_signed_avatar_url("http://project.supabase.co/avatar.webp").is_err());
        assert!(validate_signed_avatar_url("https://example.com/avatar.webp").is_err());
    }

    #[test]
    fn detects_supported_avatar_bytes() {
        assert_eq!(
            detect_avatar_format(b"RIFF0000WEBPdata"),
            Some(("webp", "image/webp"))
        );
        assert_eq!(detect_avatar_format(b"not-an-image"), None);
    }

    #[tokio::test]
    async fn latest_cached_avatar_is_available_offline() {
        let directory = tempfile::tempdir().expect("cache directory");
        let user_id = Uuid::new_v4();
        tokio::fs::write(
            avatar_cache_path(directory.path(), user_id, 1, "webp"),
            b"old",
        )
        .await
        .expect("old avatar");
        tokio::fs::write(
            avatar_cache_path(directory.path(), user_id, 2, "webp"),
            b"new",
        )
        .await
        .expect("new avatar");

        let cached = read_latest_cached_avatar(directory.path(), user_id)
            .await
            .expect("read cache")
            .expect("cached avatar");
        assert_eq!(cached.mime_type, "image/webp");
        assert_eq!(cached.data_base64, BASE64.encode(b"new"));
    }

    #[tokio::test]
    async fn removing_avatar_cache_is_scoped_to_one_user() {
        let directory = tempfile::tempdir().expect("cache directory");
        let first_user = Uuid::new_v4();
        let second_user = Uuid::new_v4();
        let first_path = avatar_cache_path(directory.path(), first_user, 1, "webp");
        let second_path = avatar_cache_path(directory.path(), second_user, 1, "webp");
        tokio::fs::write(&first_path, b"first")
            .await
            .expect("first avatar");
        tokio::fs::write(&second_path, b"second")
            .await
            .expect("second avatar");

        remove_user_avatar_files(directory.path(), first_user)
            .await
            .expect("remove first user cache");
        assert!(!tokio::fs::try_exists(first_path).await.expect("first path"));
        assert!(tokio::fs::try_exists(second_path)
            .await
            .expect("second path"));
    }
}
