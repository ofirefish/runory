use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::connection::BastionSessionBridge;
use crate::domain::{
    AppError, AppResult, LocalFileSelection, RemoteImagePreview, RemoteTextPreview,
    RetryTransferRequest, SelectDownloadTargetRequest, SelectUploadFilesRequest, SessionRequest,
    SftpCreateDirectoryRequest, SftpDeleteRequest, SftpDirectory, SftpMetadata, SftpPathRequest,
    SftpRenameRequest, StartDownloadRequest, StartUploadRequest, TransferDirection, TransferEvent,
    TransferJob, TransferJobRequest, UploadDirectoryHistoryEntry,
};
use crate::ssh::ServerSessionManager;
use crate::transfers::{LocalFileGrantKind, LocalFileGrantService, UploadDirectoryHistoryService};

async fn profile_id_for_session(
    sessions: &ServerSessionManager,
    bastion: &BastionSessionBridge,
    session_id: uuid::Uuid,
) -> AppResult<uuid::Uuid> {
    if bastion.has(session_id).await {
        return bastion.profile_id(session_id).await;
    }
    sessions.profile_id(session_id).await
}

async fn authorize(
    policies: &CloudPolicyService,
    sessions: &ServerSessionManager,
    bastion: &BastionSessionBridge,
    session_id: uuid::Uuid,
    action: CloudPolicyAction,
) -> AppResult<()> {
    policies
        .authorize(
            profile_id_for_session(sessions, bastion, session_id).await?,
            action,
        )
        .await
}

#[tauri::command]
pub async fn sftp_open(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion.sftp_open(request.session_id).await;
    }
    sessions.sftp_open(request.session_id).await
}

#[tauri::command]
pub async fn list_directory(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_list_directory(request.session_id, request.path)
            .await;
    }
    sessions
        .sftp_list_directory(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn stat(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpMetadata> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion.sftp_stat(request.session_id, request.path).await;
    }
    sessions.sftp_stat(request.session_id, request.path).await
}

#[tauri::command]
pub async fn sftp_preview_image(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<RemoteImagePreview> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_read_image_preview(request.session_id, request.path)
            .await;
    }
    sessions
        .sftp_read_image_preview(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn sftp_preview_text(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<RemoteTextPreview> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_read_text_preview(request.session_id, request.path)
            .await;
    }
    sessions
        .sftp_read_text_preview(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn change_directory(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_change_directory(request.session_id, request.path)
            .await;
    }
    sessions
        .sftp_change_directory(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn refresh(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion.sftp_refresh(request.session_id).await;
    }
    sessions.sftp_refresh(request.session_id).await
}

#[tauri::command]
pub async fn create_directory(
    request: SftpCreateDirectoryRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_create_directory(request.session_id, request.parent, request.name)
            .await;
    }
    sessions
        .sftp_create_directory(request.session_id, request.parent, request.name)
        .await
}

#[tauri::command]
pub async fn rename(
    request: SftpRenameRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_rename(request.session_id, request.path, request.new_name)
            .await;
    }
    sessions
        .sftp_rename(request.session_id, request.path, request.new_name)
        .await
}

#[tauri::command]
pub async fn delete(
    request: SftpDeleteRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    if bastion.has(request.session_id).await {
        return bastion
            .sftp_delete(request.session_id, request.path, request.recursive)
            .await;
    }
    sessions
        .sftp_delete(request.session_id, request.path, request.recursive)
        .await
}

#[tauri::command]
pub async fn sftp_select_upload_files(
    request: SelectUploadFilesRequest,
    app: AppHandle,
    grants: State<'_, LocalFileGrantService>,
    history: State<'_, UploadDirectoryHistoryService>,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
) -> AppResult<Vec<LocalFileSelection>> {
    let profile_id = profile_id_for_session(&sessions, &bastion, request.session_id).await?;
    let mut dialog = app.dialog().file();
    if let Some(directory) = history.find(profile_id, &request.remote_directory).await? {
        let is_directory = tokio::fs::metadata(&directory)
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        if is_directory {
            dialog = dialog.set_directory(directory);
        }
    }
    let Some(paths) = dialog.blocking_pick_files() else {
        return Ok(Vec::new());
    };
    let mut selections = Vec::with_capacity(paths.len());
    for path in paths {
        selections.push(
            grants
                .grant_upload(path.into_path().map_err(|_| AppError::LocalFileInvalid)?)
                .await?,
        );
    }
    Ok(selections)
}

#[tauri::command]
pub async fn sftp_list_upload_directories(
    request: SessionRequest,
    history: State<'_, UploadDirectoryHistoryService>,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
) -> AppResult<Vec<UploadDirectoryHistoryEntry>> {
    let profile_id = profile_id_for_session(&sessions, &bastion, request.session_id).await?;
    history.list(profile_id).await
}

#[tauri::command]
pub async fn sftp_accept_latest_upload_drop(
    grants: State<'_, LocalFileGrantService>,
) -> AppResult<Vec<LocalFileSelection>> {
    grants.accept_latest_upload_drop().await
}

#[tauri::command]
pub async fn sftp_select_download_target(
    request: SelectDownloadTargetRequest,
    app: AppHandle,
    grants: State<'_, LocalFileGrantService>,
) -> AppResult<Option<LocalFileSelection>> {
    let Some(path) = app
        .dialog()
        .file()
        .set_file_name(&request.suggested_name)
        .blocking_save_file()
    else {
        return Ok(None);
    };
    grants
        .grant_download(
            path.into_path().map_err(|_| AppError::LocalFileInvalid)?,
            &request.suggested_name,
        )
        .await
        .map(Some)
}

#[tauri::command]
pub async fn sftp_start_upload(
    request: StartUploadRequest,
    grants: State<'_, LocalFileGrantService>,
    history: State<'_, UploadDirectoryHistoryService>,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TransferJob> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    let grant = grants
        .consume(request.grant_id, LocalFileGrantKind::UploadSource)
        .await?;
    let local_directory = grant.path.parent().map(ToOwned::to_owned);
    let profile_id = profile_id_for_session(&sessions, &bastion, request.session_id).await?;
    let remote_directory = request.remote_directory.clone();
    let sftp = if bastion.has(request.session_id).await {
        bastion.sftp_channel(request.session_id).await?
    } else {
        sessions.sftp_channel(request.session_id).await?
    };
    let job = sessions
        .transfer_queue()
        .enqueue_upload(
            request.session_id,
            sftp,
            grant,
            request.remote_directory,
            request.overwrite,
        )
        .await?;
    if let Some(local_directory) = local_directory {
        if history
            .remember(profile_id, remote_directory, local_directory)
            .await
            .is_err()
        {
            tracing::warn!(profile_id = %profile_id, "failed to persist upload directory history");
        }
    }
    Ok(job)
}

#[tauri::command]
pub async fn sftp_start_download(
    request: StartDownloadRequest,
    grants: State<'_, LocalFileGrantService>,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TransferJob> {
    authorize(
        &policies,
        &sessions,
        &bastion,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    let grant = grants
        .consume(request.grant_id, LocalFileGrantKind::DownloadTarget)
        .await?;
    let sftp = if bastion.has(request.session_id).await {
        bastion.sftp_channel(request.session_id).await?
    } else {
        sessions.sftp_channel(request.session_id).await?
    };
    sessions
        .transfer_queue()
        .enqueue_download(request.session_id, sftp, grant, request.remote_path)
        .await
}

#[tauri::command]
pub async fn sftp_transfer_list(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
) -> AppResult<Vec<TransferJob>> {
    profile_id_for_session(&sessions, &bastion, request.session_id).await?;
    Ok(sessions
        .transfer_queue()
        .list_for_session(request.session_id)
        .await)
}

#[tauri::command]
pub async fn sftp_transfer_subscribe(
    request: SessionRequest,
    on_event: Channel<TransferEvent>,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
) -> AppResult<()> {
    profile_id_for_session(&sessions, &bastion, request.session_id).await?;
    sessions
        .transfer_queue()
        .subscribe(request.session_id, on_event)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn sftp_transfer_cancel(
    request: TransferJobRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
) -> AppResult<()> {
    profile_id_for_session(&sessions, &bastion, request.session_id).await?;
    let belongs_to_session = sessions
        .transfer_queue()
        .list_for_session(request.session_id)
        .await
        .into_iter()
        .any(|job| job.id == request.job_id);
    if !belongs_to_session {
        return Err(AppError::TransferNotFound);
    }
    sessions.transfer_queue().cancel(request.job_id).await
}

#[tauri::command]
pub async fn sftp_transfer_retry(
    request: RetryTransferRequest,
    sessions: State<'_, ServerSessionManager>,
    bastion: State<'_, BastionSessionBridge>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TransferJob> {
    let job = sessions
        .transfer_queue()
        .list_for_session(request.session_id)
        .await
        .into_iter()
        .find(|job| job.id == request.job_id)
        .ok_or(AppError::TransferNotFound)?;
    let action = match job.direction {
        TransferDirection::Upload => CloudPolicyAction::WriteFiles,
        TransferDirection::Download => CloudPolicyAction::ReadFiles,
    };
    authorize(&policies, &sessions, &bastion, job.session_id, action).await?;
    sessions
        .transfer_queue()
        .retry(request.job_id, request.overwrite)
        .await
}
