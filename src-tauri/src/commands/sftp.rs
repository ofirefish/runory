use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::domain::{
    AppError, AppResult, LocalFileSelection, RemoteImagePreview, RemoteTextPreview,
    RetryTransferRequest, SelectDownloadTargetRequest, SessionRequest, SftpCreateDirectoryRequest,
    SftpDeleteRequest, SftpDirectory, SftpMetadata, SftpPathRequest, SftpRenameRequest,
    StartDownloadRequest, StartUploadRequest, TransferDirection, TransferEvent, TransferJob,
    TransferJobRequest,
};
use crate::ssh::ServerSessionManager;
use crate::transfers::{LocalFileGrantKind, LocalFileGrantService};

async fn authorize(
    policies: &CloudPolicyService,
    sessions: &ServerSessionManager,
    session_id: uuid::Uuid,
    action: CloudPolicyAction,
) -> AppResult<()> {
    policies
        .authorize(sessions.profile_id(session_id).await?, action)
        .await
}

#[tauri::command]
pub async fn sftp_open(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions.sftp_open(request.session_id).await
}

#[tauri::command]
pub async fn list_directory(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions
        .sftp_list_directory(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn stat(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpMetadata> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions.sftp_stat(request.session_id, request.path).await
}

#[tauri::command]
pub async fn sftp_preview_image(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<RemoteImagePreview> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions
        .sftp_read_image_preview(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn sftp_preview_text(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<RemoteTextPreview> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions
        .sftp_read_text_preview(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn change_directory(
    request: SftpPathRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions
        .sftp_change_directory(request.session_id, request.path)
        .await
}

#[tauri::command]
pub async fn refresh(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    sessions.sftp_refresh(request.session_id).await
}

#[tauri::command]
pub async fn create_directory(
    request: SftpCreateDirectoryRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    sessions
        .sftp_create_directory(request.session_id, request.parent, request.name)
        .await
}

#[tauri::command]
pub async fn rename(
    request: SftpRenameRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    sessions
        .sftp_rename(request.session_id, request.path, request.new_name)
        .await
}

#[tauri::command]
pub async fn delete(
    request: SftpDeleteRequest,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<SftpDirectory> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    sessions
        .sftp_delete(request.session_id, request.path, request.recursive)
        .await
}

#[tauri::command]
pub async fn sftp_select_upload_files(
    app: AppHandle,
    grants: State<'_, LocalFileGrantService>,
) -> AppResult<Vec<LocalFileSelection>> {
    let Some(paths) = app.dialog().file().blocking_pick_files() else {
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
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TransferJob> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::WriteFiles,
    )
    .await?;
    let grant = grants
        .consume(request.grant_id, LocalFileGrantKind::UploadSource)
        .await?;
    let sftp = sessions.sftp_channel(request.session_id).await?;
    sessions
        .transfer_queue()
        .enqueue_upload(
            request.session_id,
            sftp,
            grant,
            request.remote_directory,
            request.overwrite,
        )
        .await
}

#[tauri::command]
pub async fn sftp_start_download(
    request: StartDownloadRequest,
    grants: State<'_, LocalFileGrantService>,
    sessions: State<'_, ServerSessionManager>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TransferJob> {
    authorize(
        &policies,
        &sessions,
        request.session_id,
        CloudPolicyAction::ReadFiles,
    )
    .await?;
    let grant = grants
        .consume(request.grant_id, LocalFileGrantKind::DownloadTarget)
        .await?;
    let sftp = sessions.sftp_channel(request.session_id).await?;
    sessions
        .transfer_queue()
        .enqueue_download(request.session_id, sftp, grant, request.remote_path)
        .await
}

#[tauri::command]
pub async fn sftp_transfer_list(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<Vec<TransferJob>> {
    sessions.profile_id(request.session_id).await?;
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
) -> AppResult<()> {
    sessions.profile_id(request.session_id).await?;
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
) -> AppResult<()> {
    sessions.profile_id(request.session_id).await?;
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
    authorize(&policies, &sessions, job.session_id, action).await?;
    sessions
        .transfer_queue()
        .retry(request.job_id, request.overwrite)
        .await
}
