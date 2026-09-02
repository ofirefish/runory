use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{AppError, AppResult, UploadDirectoryHistoryEntry};
use crate::storage::JsonRepository;

const MAX_HISTORY_ENTRIES: usize = 256;
const MAX_REMOTE_PATH_LENGTH: usize = 4096;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadDirectoryEntry {
    profile_id: Uuid,
    remote_directory: String,
    local_directory: PathBuf,
    #[serde(default)]
    last_uploaded_at_ms: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct UploadDirectoryHistory {
    entries: Vec<UploadDirectoryEntry>,
}

pub struct UploadDirectoryHistoryService {
    repository: JsonRepository<UploadDirectoryHistory>,
    write_lock: Mutex<()>,
}

impl UploadDirectoryHistoryService {
    pub fn at_path(path: impl Into<PathBuf>) -> Self {
        Self {
            repository: JsonRepository::new(path),
            write_lock: Mutex::new(()),
        }
    }

    pub async fn find(
        &self,
        profile_id: Uuid,
        remote_directory: &str,
    ) -> AppResult<Option<PathBuf>> {
        validate_remote_directory(remote_directory)?;
        let history = self.repository.load_or_default().await?;
        Ok(history
            .entries
            .iter()
            .rev()
            .find(|entry| {
                entry.profile_id == profile_id && entry.remote_directory == remote_directory
            })
            .map(|entry| entry.local_directory.clone()))
    }

    pub async fn list(&self, profile_id: Uuid) -> AppResult<Vec<UploadDirectoryHistoryEntry>> {
        let history = self.repository.load_or_default().await?;
        let mut entries = history
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.profile_id == profile_id)
            .collect::<Vec<_>>();
        entries.sort_by(|(left_index, left), (right_index, right)| {
            right
                .last_uploaded_at_ms
                .cmp(&left.last_uploaded_at_ms)
                .then_with(|| right_index.cmp(left_index))
        });
        let mut seen_remote_directories = HashSet::new();
        Ok(entries
            .into_iter()
            .map(|(_, entry)| entry)
            .filter(|entry| seen_remote_directories.insert(entry.remote_directory.clone()))
            .map(|entry| UploadDirectoryHistoryEntry {
                remote_directory: entry.remote_directory.clone(),
                last_uploaded_at_ms: entry.last_uploaded_at_ms,
            })
            .collect())
    }

    pub async fn remember(
        &self,
        profile_id: Uuid,
        remote_directory: String,
        local_directory: PathBuf,
    ) -> AppResult<()> {
        validate_remote_directory(&remote_directory)?;
        if !local_directory.is_absolute() {
            return Err(AppError::LocalFileInvalid);
        }
        let _guard = self.write_lock.lock().await;
        let mut history = self.repository.load_or_default().await?;
        history.entries.retain(|entry| {
            entry.profile_id != profile_id || entry.remote_directory != remote_directory
        });
        history.entries.push(UploadDirectoryEntry {
            profile_id,
            remote_directory,
            local_directory,
            last_uploaded_at_ms: now_epoch_ms(),
        });
        if history.entries.len() > MAX_HISTORY_ENTRIES {
            let remove_count = history.entries.len() - MAX_HISTORY_ENTRIES;
            history.entries.drain(..remove_count);
        }
        self.repository.save_atomic(&history).await
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn validate_remote_directory(path: &str) -> AppResult<()> {
    if path.starts_with('/') && !path.contains('\0') && path.len() <= MAX_REMOTE_PATH_LENGTH {
        Ok(())
    } else {
        Err(AppError::SftpPathInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn remembers_local_directories_per_profile_and_remote_directory() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let repository_path = directory.path().join("upload-directory-history.json");
        let profile_a = Uuid::new_v4();
        let profile_b = Uuid::new_v4();
        let local_a = directory.path().join("local-a");
        let latest_local_a = directory.path().join("latest-local-a");
        let local_b = directory.path().join("local-b");
        let service = UploadDirectoryHistoryService::at_path(&repository_path);

        service
            .remember(profile_a, "/srv/app".into(), local_a.clone())
            .await
            .expect("remember first directory");
        service
            .remember(profile_b, "/srv/app".into(), local_b.clone())
            .await
            .expect("remember second directory");
        service
            .remember(profile_a, "/srv/app".into(), latest_local_a.clone())
            .await
            .expect("replace first directory");

        let reloaded = UploadDirectoryHistoryService::at_path(repository_path);
        assert_eq!(
            reloaded
                .find(profile_a, "/srv/app")
                .await
                .expect("find first directory"),
            Some(latest_local_a)
        );
        assert_eq!(
            reloaded
                .find(profile_b, "/srv/app")
                .await
                .expect("find second directory"),
            Some(local_b)
        );
        assert_eq!(
            reloaded
                .find(profile_a, "/srv/other")
                .await
                .expect("missing association"),
            None
        );
    }

    #[tokio::test]
    async fn lists_remote_directories_in_most_recent_upload_order() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let repository_path = directory.path().join("upload-directory-history.json");
        let profile = Uuid::new_v4();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        let service = UploadDirectoryHistoryService::at_path(repository_path);

        service
            .remember(profile, "/srv/one".into(), first.clone())
            .await
            .expect("remember first directory");
        service
            .remember(profile, "/srv/two".into(), second.clone())
            .await
            .expect("remember second directory");
        service
            .remember(profile, "/srv/three".into(), first.clone())
            .await
            .expect("remember first directory again");

        let entries = service.list(profile).await.expect("list history");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].remote_directory, "/srv/three");
        assert_eq!(entries[1].remote_directory, "/srv/two");
        assert_eq!(entries[2].remote_directory, "/srv/one");
        assert!(entries[0].last_uploaded_at_ms > 0);
    }
}
