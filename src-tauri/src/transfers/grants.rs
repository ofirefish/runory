use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::Mutex;
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

#[derive(Default)]
pub struct LocalFileGrantService {
    grants: Mutex<HashMap<Uuid, LocalFileGrant>>,
}

impl LocalFileGrantService {
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
}
