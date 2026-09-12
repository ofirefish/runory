//! Stdio proxy transport (Teleport `tsh proxy ssh`).

use std::path::PathBuf;

use crate::domain::{AppError, AppResult};
use crate::helper::{
    DefaultExternalHelperManager, ExternalHelperManager, HelperError, ProcessSpec,
};

use super::{CommandSpec, OpenedTransport, TransportCleanup, TransportContext};

pub async fn open(command: &CommandSpec, _ctx: &TransportContext) -> AppResult<OpenedTransport> {
    if command.executable.trim().is_empty() {
        return Err(AppError::InvalidProfile);
    }
    let manager = DefaultExternalHelperManager::new();
    open_with_manager(&manager, command).await
}

pub async fn open_with_manager(
    manager: &dyn ExternalHelperManager,
    command: &CommandSpec,
) -> AppResult<OpenedTransport> {
    let spec = ProcessSpec {
        executable: PathBuf::from(&command.executable),
        args: command.args.clone(),
        working_directory: command.working_directory.as_ref().map(PathBuf::from),
        env: command.env.clone(),
        stdin: true,
        stdout: true,
        stderr: true,
    };
    let handle = manager
        .spawn_stdio_proxy(spec)
        .await
        .map_err(map_helper_error)?;
    let (process, transport) = handle.into_transport();
    Ok(OpenedTransport {
        stream: Box::new(transport),
        cleanup: Some(TransportCleanup::HelperProcess(process)),
        helper_ready_payload: None,
    })
}

fn map_helper_error(error: HelperError) -> AppError {
    match error {
        HelperError::Missing | HelperError::VersionMismatch => AppError::BastionUnavailable,
        HelperError::Cancelled => AppError::BastionUnavailable,
        _ => AppError::BastionUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_executable_rejected() {
        let cmd = CommandSpec::new("", vec!["proxy".into(), "ssh".into()]);
        let err = open(&cmd, &TransportContext::default())
            .await
            .expect_err("empty exe");
        assert!(matches!(err, AppError::InvalidProfile));
    }
}
