use crate::domain::{AppError, AppResult, NginxAction, ResourceAction, ServiceStatus, SessionId};
use crate::operations::OperationsService;
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::{FilePatchData, ServiceChangeData};

pub(crate) async fn file_patch(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    path: String,
    expected: String,
    replacement: String,
) -> AppResult<FilePatchData> {
    if expected.is_empty()
        || expected == replacement
        || expected.len() > 512 * 1024
        || replacement.len() > 512 * 1024
    {
        return Err(AppError::InvalidOperation);
    }
    let (_, current) = sessions.sftp_read_text(session_id, path.clone()).await?;
    if current.matches(&expected).count() != 1 {
        return Err(AppError::InvalidOperation);
    }
    let updated = current.replacen(&expected, &replacement, 1);
    let (resolved, bytes) = sessions
        .sftp_write_text(session_id, path, updated.clone())
        .await?;
    let (_, verified) = sessions
        .sftp_read_text(session_id, resolved.clone())
        .await?;
    if verified != updated {
        let _ = sessions
            .sftp_write_text(session_id, resolved.clone(), current)
            .await;
        return Err(AppError::SftpOperationFailed);
    }
    Ok(FilePatchData {
        path: resolved,
        bytes,
        verified: true,
    })
}

pub(crate) async fn service_change(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    service: String,
    action: &'static str,
) -> AppResult<ServiceChangeData> {
    validate_service(&service)?;
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(
                "command -v systemctl >/dev/null 2>&1 || exit 90; systemctl \"$1\" \"$2\"",
                vec![action.into(), service.clone()],
            ),
        )
        .await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    if result.exit_code != 0 {
        return Err(AppError::ExecFailed);
    }
    let status = crate::tools::service::status(sessions, session_id, service.clone()).await?;
    let verified = match action {
        "restart" => matches!(status.service.status, ServiceStatus::Active),
        "reload" => matches!(status.service.status, ServiceStatus::Active),
        _ => false,
    };
    if !verified {
        return Err(AppError::ExecFailed);
    }
    Ok(ServiceChangeData {
        service,
        action,
        verified,
    })
}

pub(crate) async fn nginx_reload(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<ServiceChangeData> {
    let test = crate::tools::nginx::test(sessions, session_id).await?;
    if !test.valid {
        return Err(AppError::InvalidOperation);
    }
    let result = OperationsService::nginx_action(sessions, session_id, NginxAction::Reload).await?;
    if !result.success {
        return Err(AppError::ExecFailed);
    }
    let after = crate::tools::nginx::test(sessions, session_id).await?;
    if !after.valid {
        return Err(AppError::ExecFailed);
    }
    Ok(ServiceChangeData {
        service: "nginx".into(),
        action: "reload",
        verified: true,
    })
}

pub(crate) async fn docker_restart(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    container: String,
) -> AppResult<ServiceChangeData> {
    if container.is_empty()
        || container.len() > 256
        || !container.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'@' | b':' | b'/')
        })
    {
        return Err(AppError::InvalidOperation);
    }
    let result = OperationsService::docker_action(
        sessions,
        session_id,
        container.clone(),
        ResourceAction::Restart,
    )
    .await?;
    if !result.success {
        return Err(AppError::ExecFailed);
    }
    let containers = OperationsService::docker_list(sessions, session_id).await?;
    if !containers
        .iter()
        .any(|item| (item.id == container || item.name == container) && item.state == "running")
    {
        return Err(AppError::ExecFailed);
    }
    Ok(ServiceChangeData {
        service: container,
        action: "restart",
        verified: true,
    })
}

fn validate_service(value: &str) -> AppResult<()> {
    if !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'@' | b':' | b'-')
        })
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}
