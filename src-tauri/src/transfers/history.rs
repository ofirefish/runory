use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{AppError, AppResult};
use crate::storage::JsonRepository;

const MAX_HISTORY_ENTRIES: usize = 256;
const MAX_REMOTE_PATH_LENGTH: usize = 4096;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadDirectoryEntry {
    profile_id: Uuid,
    remote_directory: String,
    local_directory: PathBuf,
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
        });
        if history.entries.len() > MAX_HISTORY_ENTRIES {
            let remove_count = history.entries.len() - MAX_HISTORY_ENTRIES;
            history.entries.drain(..remove_count);
        }
        self.repository.save_atomic(&history).await
    }
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
}
