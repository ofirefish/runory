use crate::domain::{AppError, AppResult};
use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct JsonRepository<T> {
    path: PathBuf,
    marker: std::marker::PhantomData<T>,
}
impl<T> JsonRepository<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            marker: std::marker::PhantomData,
        }
    }
    pub async fn load_or_default(&self) -> AppResult<T>
    where
        T: Default,
    {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| AppError::Storage),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(_) => Err(AppError::Storage),
        }
    }
    pub async fn save_atomic(&self, value: &T) -> AppResult<()> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| AppError::Storage)?;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || persist_atomic(&path, &bytes))
            .await
            .map_err(|_| AppError::Storage)?
    }
}

fn persist_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path.parent().ok_or(AppError::Storage)?;
    std::fs::create_dir_all(parent).map_err(|_| AppError::Storage)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| AppError::Storage)?;
    temporary.write_all(bytes).map_err(|_| AppError::Storage)?;
    temporary.flush().map_err(|_| AppError::Storage)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| AppError::Storage)?;
    temporary.persist(path).map_err(|_| AppError::Storage)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn saves_and_loads_json_atomically() {
        let directory = tempfile::tempdir().expect("temp directory");
        let repository = JsonRepository::<Vec<String>>::new(directory.path().join("items.json"));
        repository
            .save_atomic(&vec!["one".into()])
            .await
            .expect("save");
        assert_eq!(
            repository.load_or_default().await.expect("load"),
            vec!["one"]
        );
    }

    #[tokio::test]
    async fn missing_repository_returns_default() {
        let directory = tempfile::tempdir().expect("temp directory");
        let repository = JsonRepository::<Vec<String>>::new(directory.path().join("missing.json"));
        assert!(repository
            .load_or_default()
            .await
            .expect("default")
            .is_empty());
    }
}
