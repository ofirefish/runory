use std::time::Duration;

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

use crate::domain::{AppError, AppResult};

const UPDATE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RELEASE_NOTES_CHARS: usize = 4_000;

struct PendingUpdate {
    update: Update,
    bytes: Option<Vec<u8>>,
}

#[derive(Default)]
pub struct AppUpdateService {
    pending: Mutex<Option<PendingUpdate>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateMetadata {
    pub current_version: String,
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateSnapshot {
    pub configured: bool,
    pub current_version: String,
    pub update: Option<AppUpdateMetadata>,
    pub downloaded: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum AppUpdateEvent {
    Started {
        total_bytes: Option<u64>,
    },
    Progress {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Finished {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    },
}

impl AppUpdateService {
    pub fn configured() -> bool {
        updater_configuration().is_some()
    }

    pub async fn snapshot(&self, app: &AppHandle) -> AppUpdateSnapshot {
        let pending = self.pending.lock().await;
        AppUpdateSnapshot {
            configured: Self::configured(),
            current_version: app.package_info().version.to_string(),
            update: pending.as_ref().map(|pending| metadata(&pending.update)),
            downloaded: pending
                .as_ref()
                .is_some_and(|pending| pending.bytes.is_some()),
        }
    }

    pub async fn check(&self, app: &AppHandle) -> AppResult<AppUpdateSnapshot> {
        let (endpoint, public_key) =
            updater_configuration().ok_or(AppError::UpdateNotConfigured)?;
        let endpoint = endpoint
            .parse()
            .map_err(|_| AppError::UpdateNotConfigured)?;
        let updater = app
            .updater_builder()
            .timeout(UPDATE_TIMEOUT)
            .endpoints(vec![endpoint])
            .map_err(|_| AppError::UpdateNotConfigured)?
            .pubkey(public_key)
            .build()
            .map_err(|error| {
                tracing::warn!(error = %error, "desktop updater could not be configured");
                AppError::UpdateCheckFailed
            })?;
        let update = updater.check().await.map_err(|error| {
            tracing::warn!(error = %error, "desktop update check failed");
            AppError::UpdateCheckFailed
        })?;
        let mut pending = self.pending.lock().await;
        *pending = update.map(|update| PendingUpdate {
            update,
            bytes: None,
        });
        drop(pending);
        Ok(self.snapshot(app).await)
    }

    pub async fn download(
        &self,
        app: &AppHandle,
        on_event: Channel<AppUpdateEvent>,
    ) -> AppResult<AppUpdateSnapshot> {
        let update = self
            .pending
            .lock()
            .await
            .as_ref()
            .map(|pending| pending.update.clone())
            .ok_or(AppError::UpdateNotReady)?;
        let expected_version = update.version.clone();
        let mut downloaded_bytes = 0_u64;
        let mut total_bytes = None;
        let _ = on_event.send(AppUpdateEvent::Started { total_bytes: None });
        let bytes = update
            .download(
                |chunk_length, content_length| {
                    total_bytes = content_length;
                    downloaded_bytes = downloaded_bytes
                        .saturating_add(u64::try_from(chunk_length).unwrap_or(u64::MAX));
                    let _ = on_event.send(AppUpdateEvent::Progress {
                        downloaded_bytes,
                        total_bytes,
                    });
                },
                || {},
            )
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, version = %expected_version, "desktop update download failed");
                AppError::UpdateDownloadFailed
            })?;
        let _ = on_event.send(AppUpdateEvent::Finished {
            downloaded_bytes,
            total_bytes,
        });
        let mut pending = self.pending.lock().await;
        let current = pending.as_mut().ok_or(AppError::UpdateNotReady)?;
        if current.update.version != expected_version
            || current.update.signature != update.signature
            || current.update.download_url != update.download_url
        {
            return Err(AppError::UpdateNotReady);
        }
        current.bytes = Some(bytes);
        drop(pending);
        Ok(self.snapshot(app).await)
    }

    pub async fn install(&self) -> AppResult<()> {
        let pending = self.pending.lock().await;
        let pending = pending.as_ref().ok_or(AppError::UpdateNotReady)?;
        let bytes = pending.bytes.as_ref().ok_or(AppError::UpdateNotReady)?;
        pending.update.install(bytes).map_err(|error| {
            tracing::warn!(error = %error, version = %pending.update.version, "desktop update installation failed");
            AppError::UpdateInstallFailed
        })
    }
}

fn updater_configuration() -> Option<(&'static str, &'static str)> {
    let endpoint = option_env!("RUNORY_UPDATER_ENDPOINT")?.trim();
    let public_key = option_env!("RUNORY_UPDATER_PUBKEY")?.trim();
    (!endpoint.is_empty() && !public_key.is_empty()).then_some((endpoint, public_key))
}

fn metadata(update: &Update) -> AppUpdateMetadata {
    AppUpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        notes: update.body.as_deref().map(truncate_release_notes),
        date: update.date.map(|date| date.to_string()),
    }
}

fn truncate_release_notes(value: &str) -> String {
    value.chars().take(MAX_RELEASE_NOTES_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_notes_are_bounded_on_character_boundaries() {
        let notes = "更".repeat(MAX_RELEASE_NOTES_CHARS + 10);
        let truncated = truncate_release_notes(&notes);
        assert_eq!(truncated.chars().count(), MAX_RELEASE_NOTES_CHARS);
    }
}
