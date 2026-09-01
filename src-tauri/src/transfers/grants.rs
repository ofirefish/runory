use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::domain::{AppError, AppResult, LocalFileSelection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalFileGrantKind {
    UploadSource,
    DownloadTarget,
}

pub(crate) struct LocalFileGrant {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub kind: LocalFileGrantKind,
}

struct PendingUploadDrop {
    paths: Vec<PathBuf>,
}

#[derive(Default)]
pub struct LocalFileGrantService {
    grants: Mutex<HashMap<Uuid, LocalFileGrant>>,
    pending_upload_drop: StdMutex<Option<PendingUploadDrop>>,
    upload_drop_ready: Notify,
}

impl LocalFileGrantService {
    pub fn stage_upload_drop(&self, paths: Vec<PathBuf>) {
        let mut pending = self
            .pending_upload_drop
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *pending = Some(PendingUploadDrop { paths });
        drop(pending);
        self.upload_drop_ready.notify_one();
    }

    pub async fn accept_latest_upload_drop(&self) -> AppResult<Vec<LocalFileSelection>> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let pending = loop {
            let ready = self.upload_drop_ready.notified();
            let staged = {
                let mut pending = self
                    .pending_upload_drop
                    .lock()
                    .map_err(|_| AppError::LocalFileInvalid)?;
                pending.take()
            };
            if let Some(pending) = staged {
                break pending;
            }
            tokio::time::timeout_at(deadline, ready)
                .await
                .map_err(|_| AppError::LocalFileInvalid)?;
        };
        let mut selections = Vec::with_capacity(pending.paths.len());
        for path in pending.paths {
            if let Ok(selection) = self.grant_upload(path).await {
                selections.push(selection);
            }
        }
        if selections.is_empty() {
            return Err(AppError::LocalFileInvalid);
        }
        Ok(selections)
    }

    pub async fn grant_upload(&self, path: PathBuf) -> AppResult<LocalFileSelection> {
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|_| AppError::LocalFileInvalid)?;
        if !metadata.is_file() {
            return Err(AppError::LocalFileInvalid);
        }
        let name = file_name(&path)?;
        self.insert(path, name, metadata.len(), LocalFileGrantKind::UploadSource)
            .await
    }

    pub async fn grant_download(
        &self,
        path: PathBuf,
        suggested_name: &str,
    ) -> AppResult<LocalFileSelection> {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(suggested_name)
            .to_owned();
        if name.is_empty() || path.is_dir() {
            return Err(AppError::LocalFileInvalid);
        }
        self.insert(path, name, 0, LocalFileGrantKind::DownloadTarget)
            .await
    }

    pub(crate) async fn consume(
        &self,
        grant_id: Uuid,
        expected: LocalFileGrantKind,
    ) -> AppResult<LocalFileGrant> {
        let grant = self
            .grants
            .lock()
            .await
            .remove(&grant_id)
            .ok_or(AppError::LocalFileInvalid)?;
        if grant.kind != expected {
            return Err(AppError::LocalFileInvalid);
        }
        Ok(grant)
    }

    async fn insert(
        &self,
        path: PathBuf,
        name: String,
        size: u64,
        kind: LocalFileGrantKind,
    ) -> AppResult<LocalFileSelection> {
        let grant_id = Uuid::new_v4();
        self.grants.lock().await.insert(
            grant_id,
            LocalFileGrant {
                path,
                name: name.clone(),
                size,
                kind,
            },
        );
        Ok(LocalFileSelection {
            grant_id,
            name,
            size,
        })
    }
}

fn file_name(path: &std::path::Path) -> AppResult<String> {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(AppError::LocalFileInvalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grants_are_single_use_and_kind_scoped() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("source.txt");
        tokio::fs::write(&source, b"runory")
            .await
            .expect("write fixture");
        let service = LocalFileGrantService::default();
        let selected = service
            .grant_upload(source.clone())
            .await
            .expect("grant upload");
        let consumed = service
            .consume(selected.grant_id, LocalFileGrantKind::UploadSource)
            .await
            .expect("consume upload");
        assert_eq!(consumed.path, source);
        assert!(matches!(
            service
                .consume(selected.grant_id, LocalFileGrantKind::UploadSource)
                .await,
            Err(AppError::LocalFileInvalid)
        ));
    }

    #[tokio::test]
    async fn dropped_paths_are_consumed_as_scoped_grants_and_ignore_directories() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("source.txt");
        tokio::fs::write(&source, b"runory")
            .await
            .expect("write fixture");
        let service = LocalFileGrantService::default();
        service.stage_upload_drop(vec![directory.path().to_path_buf(), source.clone()]);
        let selected = service
            .accept_latest_upload_drop()
            .await
            .expect("accept dropped paths");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "source.txt");
    }

    #[tokio::test]
    async fn accept_waits_for_native_staging_when_drop_callbacks_race() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("source.txt");
        tokio::fs::write(&source, b"runory")
            .await
            .expect("write fixture");
        let service = std::sync::Arc::new(LocalFileGrantService::default());
        service.stage_upload_drop(vec![source.clone()]);
        service
            .accept_latest_upload_drop()
            .await
            .expect("consume initial drop");
        let accepting = {
            let service = std::sync::Arc::clone(&service);
            tokio::spawn(async move { service.accept_latest_upload_drop().await })
        };
        tokio::task::yield_now().await;
        service.stage_upload_drop(vec![source]);

        let selected = accepting
            .await
            .expect("join accepting task")
            .expect("accept staged drop");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "source.txt");
    }
}
