use crate::domain::{AppError, AppResult, CronTask, RestartTarget};

pub(crate) fn validate_environment_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    let name = path.rsplit('/').next().unwrap_or_default();
    if name == ".env" || name.starts_with(".env.") {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_remote_path(path: &str) -> AppResult<()> {
    if path.starts_with('/')
        && path.len() <= 4096
        && !path.contains(['\0', '\n', '\r'])
        && !path.split('/').any(|part| part == "..")
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_branch(branch: &str) -> AppResult<()> {
    if !branch.is_empty()
        && branch.len() <= 255
        && branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/'))
        && !branch.contains("..")
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_domain(domain: &str) -> AppResult<()> {
    if domain.len() <= 253
        && domain.split('.').count() >= 2
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_email(email: &str) -> AppResult<()> {
    if email.len() <= 254
        && email.matches('@').count() == 1
        && !email.contains(['\0', '\n', '\r', ' '])
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_remote_url(url: &str) -> AppResult<()> {
    if url.len() <= 2048
        && !url.contains(['\0', '\n', '\r', ' '])
        && (url.starts_with("https://")
            || url.starts_with("ssh://")
            || (url.starts_with("git@") && url.contains(':')))
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

fn validate_identifier(value: &str) -> AppResult<()> {
    if !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'@' | b':'))
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_restart(target: &RestartTarget) -> AppResult<()> {
    match target {
        RestartTarget::None => Ok(()),
        RestartTarget::Systemd { service } => validate_identifier(service),
        RestartTarget::Pm2 { process } => validate_identifier(process),
        RestartTarget::DockerCompose { service } => validate_identifier(service),
    }
}

pub(crate) fn validate_cron_task(task: &CronTask) -> AppResult<()> {
    match task {
        CronTask::Backup {
            source_path,
            destination_directory,
        } => {
            validate_remote_path(source_path)?;
            validate_remote_path(destination_directory)
        }
        CronTask::ServiceRestart { service } => validate_identifier(service),
        CronTask::GitPull {
            repository_path,
            branch,
        } => {
            validate_remote_path(repository_path)?;
            validate_branch(branch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_branches_reject_traversal_and_metacharacters() {
        assert!(validate_remote_path("/srv/app").is_ok());
        assert!(validate_remote_path("/srv/../etc").is_err());
        assert!(validate_branch("main;id").is_err());
    }
}
