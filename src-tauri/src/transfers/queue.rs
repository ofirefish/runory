use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use russh_sftp::client::SftpSession;
use tauri::ipc::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock, Semaphore};
use uuid::Uuid;

use crate::domain::{
    AppError, AppResult, SessionId, TransferDirection, TransferEvent, TransferJob, TransferJobId,
    TransferState,
};
use crate::ssh::SftpChannel;
use crate::transfers::grants::LocalFileGrant;

const TRANSFER_CONCURRENCY: usize = 2;
const CHUNK_SIZE: usize = 256 * 1024;

#[derive(Clone)]
enum TransferSpec {
    Upload {
        local_path: PathBuf,
        remote_path: String,
        overwrite: bool,
    },
    Download {
        remote_path: String,
        local_path: PathBuf,
    },
}

struct TransferRecord {
    job: TransferJob,
    spec: TransferSpec,
    sftp: Arc<SftpChannel>,
    cancelled: Arc<AtomicBool>,
}

struct TransferQueueInner {
    records: RwLock<HashMap<TransferJobId, TransferRecord>>,
    subscribers: Mutex<Vec<(SessionId, Channel<TransferEvent>)>>,
    semaphore: Arc<Semaphore>,
}

#[derive(Clone)]
pub struct TransferQueue {
    inner: Arc<TransferQueueInner>,
}

impl Default for TransferQueue {
    fn default() -> Self {
        Self {
            inner: Arc::new(TransferQueueInner {
                records: RwLock::new(HashMap::new()),
                subscribers: Mutex::new(Vec::new()),
                semaphore: Arc::new(Semaphore::new(TRANSFER_CONCURRENCY)),
            }),
        }
    }
}

impl TransferQueue {
    pub async fn enqueue_upload(
        &self,
        session_id: SessionId,
        sftp: Arc<SftpChannel>,
        grant: LocalFileGrant,
        remote_directory: String,
        overwrite: bool,
    ) -> AppResult<TransferJob> {
        let remote_path = sftp
            .resolve_upload_destination(&remote_directory, &grant.name)
            .await?;
        let job = TransferJob {
            id: Uuid::new_v4(),
            session_id,
            direction: TransferDirection::Upload,
            name: grant.name,
            remote_path: remote_path.clone(),
            total_bytes: grant.size,
            transferred_bytes: 0,
            state: TransferState::Queued,
            error_code: None,
        };
        self.insert_and_spawn(
            job.clone(),
            TransferSpec::Upload {
                local_path: grant.path,
                remote_path,
                overwrite,
            },
            sftp,
        )
        .await;
        Ok(job)
    }

    pub async fn enqueue_download(
        &self,
        session_id: SessionId,
        sftp: Arc<SftpChannel>,
        grant: LocalFileGrant,
        requested_remote_path: String,
    ) -> AppResult<TransferJob> {
        let (remote_path, total_bytes) =
            sftp.resolve_download_source(&requested_remote_path).await?;
        let name = remote_path
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .ok_or(AppError::SftpPathInvalid)?
            .to_owned();
        let job = TransferJob {
            id: Uuid::new_v4(),
            session_id,
            direction: TransferDirection::Download,
            name,
            remote_path: remote_path.clone(),
            total_bytes,
            transferred_bytes: 0,
            state: TransferState::Queued,
            error_code: None,
        };
        self.insert_and_spawn(
            job.clone(),
            TransferSpec::Download {
                remote_path,
                local_path: grant.path,
            },
            sftp,
        )
        .await;
        Ok(job)
    }

    pub async fn list(&self) -> Vec<TransferJob> {
        let mut jobs = self
            .inner
            .records
            .read()
            .await
            .values()
            .map(|record| record.job.clone())
            .collect::<Vec<_>>();
        jobs.sort_by_key(|job| job.id);
        jobs
    }

    pub async fn list_for_session(&self, session_id: SessionId) -> Vec<TransferJob> {
        self.list()
            .await
            .into_iter()
            .filter(|job| job.session_id == session_id)
            .collect()
    }

    pub async fn subscribe(&self, session_id: SessionId, channel: Channel<TransferEvent>) {
        for job in self.list_for_session(session_id).await {
            let _ = channel.send(TransferEvent::Updated { job });
        }
        self.inner
            .subscribers
            .lock()
            .await
            .push((session_id, channel));
    }

    pub async fn cancel(&self, job_id: TransferJobId) -> AppResult<()> {
        let mut records = self.inner.records.write().await;
        let record = records.get_mut(&job_id).ok_or(AppError::TransferNotFound)?;
        if !matches!(
            record.job.state,
            TransferState::Queued | TransferState::Running
        ) {
            return Err(AppError::TransferStateInvalid);
        }
        record.cancelled.store(true, Ordering::Release);
        if record.job.state == TransferState::Queued {
            record.job.state = TransferState::Cancelled;
            let job = record.job.clone();
            drop(records);
            self.broadcast(job).await;
        }
        Ok(())
    }

    pub async fn retry(&self, job_id: TransferJobId, overwrite: bool) -> AppResult<TransferJob> {
        let mut records = self.inner.records.write().await;
        let record = records.get_mut(&job_id).ok_or(AppError::TransferNotFound)?;
        if !matches!(
            record.job.state,
            TransferState::Failed | TransferState::Cancelled
        ) {
            return Err(AppError::TransferStateInvalid);
        }
        if let TransferSpec::Upload {
            overwrite: spec_overwrite,
            ..
        } = &mut record.spec
        {
            *spec_overwrite = overwrite;
        }
        record.cancelled = Arc::new(AtomicBool::new(false));
        record.job.state = TransferState::Queued;
        record.job.transferred_bytes = 0;
        record.job.error_code = None;
        let job = record.job.clone();
        drop(records);
        self.broadcast(job.clone()).await;
        self.spawn(job_id);
        Ok(job)
    }

    pub async fn cancel_session(&self, session_id: SessionId) {
        let mut changed = Vec::new();
        let mut records = self.inner.records.write().await;
        for record in records.values_mut().filter(|record| {
            record.job.session_id == session_id
                && matches!(
                    record.job.state,
                    TransferState::Queued | TransferState::Running
                )
        }) {
            record.cancelled.store(true, Ordering::Release);
            if record.job.state == TransferState::Queued {
                record.job.state = TransferState::Cancelled;
                changed.push(record.job.clone());
            }
        }
        drop(records);
        for job in changed {
            self.broadcast(job).await;
        }
    }

    async fn insert_and_spawn(&self, job: TransferJob, spec: TransferSpec, sftp: Arc<SftpChannel>) {
        let job_id = job.id;
        self.inner.records.write().await.insert(
            job_id,
            TransferRecord {
                job: job.clone(),
                spec,
                sftp,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        );
        self.broadcast(job).await;
        self.spawn(job_id);
    }

    fn spawn(&self, job_id: TransferJobId) {
        let queue = self.clone();
        tokio::spawn(async move {
            queue.run(job_id).await;
        });
    }

    async fn run(&self, job_id: TransferJobId) {
        let permit = match Arc::clone(&self.inner.semaphore).acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => return,
        };
        let prepared = {
            let mut records = self.inner.records.write().await;
            let Some(record) = records.get_mut(&job_id) else {
                return;
            };
            if record.cancelled.load(Ordering::Acquire) || record.job.state != TransferState::Queued
            {
                return;
            }
            record.job.state = TransferState::Running;
            (
                record.job.clone(),
                record.spec.clone(),
                Arc::clone(&record.sftp),
                Arc::clone(&record.cancelled),
            )
        };
        self.broadcast(prepared.0.clone()).await;
        let session = prepared.2.session();
        let result = match &prepared.1 {
            TransferSpec::Upload {
                local_path,
                remote_path,
                overwrite,
            } => {
                self.upload(
                    job_id,
                    &session,
                    local_path,
                    remote_path,
                    *overwrite,
                    &prepared.3,
                )
                .await
            }
            TransferSpec::Download {
                remote_path,
                local_path,
            } => {
                self.download(job_id, &session, remote_path, local_path, &prepared.3)
                    .await
            }
        };
        drop(permit);
        let (job, success) = {
            let mut records = self.inner.records.write().await;
            let Some(record) = records.get_mut(&job_id) else {
                return;
            };
            match result {
                Ok(()) => {
                    record.job.transferred_bytes = record.job.total_bytes;
                    record.job.state = TransferState::Completed;
                    record.job.error_code = None;
                    (record.job.clone(), true)
                }
                Err(error) => {
                    record.job.state = if matches!(error, AppError::TransferCancelled)
                        || record.cancelled.load(Ordering::Acquire)
                    {
                        TransferState::Cancelled
                    } else {
                        TransferState::Failed
                    };
                    record.job.error_code = Some(error.code().to_owned());
                    (record.job.clone(), false)
                }
            }
        };
        tracing::info!(job_id = %job.id, session_id = %job.session_id, success, transferred_bytes = job.transferred_bytes, "SFTP transfer finished");
        self.broadcast(job).await;
    }

    async fn upload(
        &self,
        job_id: TransferJobId,
        session: &SftpSession,
        local_path: &Path,
        remote_path: &str,
        overwrite: bool,
        cancelled: &AtomicBool,
    ) -> AppResult<()> {
        if session
            .try_exists(remote_path)
            .await
            .map_err(crate::ssh::map_sftp_error)?
            && !overwrite
        {
            return Err(AppError::SftpAlreadyExists);
        }
        let temp_path = remote_temp_path(remote_path, job_id)?;
        if session.try_exists(&temp_path).await.unwrap_or(false) {
            let _ = session.remove_file(&temp_path).await;
        }
        let mut source = tokio::fs::File::open(local_path)
            .await
            .map_err(|_| AppError::LocalFileInvalid)?;
        let mut destination = session
            .create(&temp_path)
            .await
            .map_err(crate::ssh::map_sftp_error)?;
        let result = async {
            let mut buffer = vec![0u8; CHUNK_SIZE];
            loop {
                ensure_not_cancelled(cancelled)?;
                let read = source
                    .read(&mut buffer)
                    .await
                    .map_err(|_| AppError::TransferFailed)?;
                if read == 0 {
                    break;
                }
                destination
                    .write_all(&buffer[..read])
                    .await
                    .map_err(|_| AppError::TransferFailed)?;
                self.add_progress(job_id, read as u64).await;
            }
            destination
                .flush()
                .await
                .map_err(|_| AppError::TransferFailed)?;
            destination
                .sync_all()
                .await
                .map_err(|_| AppError::TransferFailed)?;
            destination
                .close()
                .await
                .map_err(|_| AppError::TransferFailed)?;
            ensure_not_cancelled(cancelled)?;
            if session.try_exists(remote_path).await.unwrap_or(false) {
                session
                    .remove_file(remote_path)
                    .await
                    .map_err(crate::ssh::map_sftp_error)?;
            }
            session
                .rename(&temp_path, remote_path)
                .await
                .map_err(crate::ssh::map_sftp_error)
        }
        .await;
        if result.is_err() {
            let _ = session.remove_file(&temp_path).await;
        }
        result
    }

    async fn download(
        &self,
        job_id: TransferJobId,
        session: &SftpSession,
        remote_path: &str,
        local_path: &Path,
        cancelled: &AtomicBool,
    ) -> AppResult<()> {
        let temp_path = local_temp_path(local_path, job_id)?;
        let mut source = session
            .open(remote_path)
            .await
            .map_err(crate::ssh::map_sftp_error)?;
        let mut destination = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|_| AppError::LocalFileInvalid)?;
        let result = async {
            let mut buffer = vec![0u8; CHUNK_SIZE];
            loop {
                ensure_not_cancelled(cancelled)?;
                let read = source
                    .read(&mut buffer)
                    .await
                    .map_err(|_| AppError::TransferFailed)?;
                if read == 0 {
                    break;
                }
                destination
                    .write_all(&buffer[..read])
                    .await
                    .map_err(|_| AppError::TransferFailed)?;
                self.add_progress(job_id, read as u64).await;
            }
            destination
                .flush()
                .await
                .map_err(|_| AppError::TransferFailed)?;
            destination
                .sync_all()
                .await
                .map_err(|_| AppError::TransferFailed)?;
            ensure_not_cancelled(cancelled)?;
            if tokio::fs::try_exists(local_path).await.unwrap_or(false) {
                tokio::fs::remove_file(local_path)
                    .await
                    .map_err(|_| AppError::LocalFileInvalid)?;
            }
            tokio::fs::rename(&temp_path, local_path)
                .await
                .map_err(|_| AppError::TransferFailed)
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp_path).await;
        }
        result
    }

    async fn add_progress(&self, job_id: TransferJobId, bytes: u64) {
        let job = {
            let mut records = self.inner.records.write().await;
            let Some(record) = records.get_mut(&job_id) else {
                return;
            };
            record.job.transferred_bytes = record
                .job
                .transferred_bytes
                .saturating_add(bytes)
                .min(record.job.total_bytes);
            record.job.clone()
        };
        self.broadcast(job).await;
    }

    async fn broadcast(&self, job: TransferJob) {
        self.inner
            .subscribers
            .lock()
            .await
            .retain(|(session_id, channel)| {
                *session_id != job.session_id
                    || channel
                        .send(TransferEvent::Updated { job: job.clone() })
                        .is_ok()
            });
    }
}

fn ensure_not_cancelled(cancelled: &AtomicBool) -> AppResult<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(AppError::TransferCancelled)
    } else {
        Ok(())
    }
}

fn remote_temp_path(remote_path: &str, job_id: Uuid) -> AppResult<String> {
    let (parent, _) = remote_path
        .rsplit_once('/')
        .ok_or(AppError::SftpPathInvalid)?;
    let parent = if parent.is_empty() { "/" } else { parent };
    let name = format!(".runory-upload-{job_id}.part");
    Ok(if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    })
}

fn local_temp_path(local_path: &Path, job_id: Uuid) -> AppResult<PathBuf> {
    let parent = local_path.parent().ok_or(AppError::LocalFileInvalid)?;
    let name = local_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(AppError::LocalFileInvalid)?;
    Ok(parent.join(format!(".{name}.runory-{job_id}.part")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_paths_stay_next_to_destination() {
        let id = Uuid::nil();
        assert_eq!(
            remote_temp_path("/srv/archive.zip", id).expect("remote temp"),
            "/srv/.runory-upload-00000000-0000-0000-0000-000000000000.part"
        );
        assert_eq!(
            local_temp_path(Path::new("C:\\downloads\\archive.zip"), id)
                .expect("local temp")
                .file_name()
                .and_then(|value| value.to_str()),
            Some(".archive.zip.runory-00000000-0000-0000-0000-000000000000.part")
        );
    }
}
