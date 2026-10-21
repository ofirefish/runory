use crate::domain::{AppError, AppResult, CronTask, PythonProjectConfig, RestartTarget};

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

pub(crate) fn validate_relative_path(path: &str) -> AppResult<()> {
    if !path.is_empty()
        && path.len() <= 1024
        && !path.starts_with('/')
        && !path.contains(['\0', '\n', '\r'])
        && !path.split(['/', '\\']).any(|part| part == "..")
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
    if url.is_empty() {
        return Ok(());
    }
    if url.len() <= 2048
        && !url.contains(['\0', '\n', '\r', ' '])
        && (url.starts_with("https://")
            || url.starts_with("http://")
            || url.starts_with("ssh://")
            || (url.starts_with("git@") && url.contains(':')))
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_identifier(value: &str) -> AppResult<()> {
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

pub(crate) fn validate_python_bin(value: &str) -> AppResult<()> {
    if value == "python3"
        || value == "python"
        || (value.starts_with("python3.")
            && value.len() <= 16
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')))
    {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_port(port: u16) -> AppResult<()> {
    if (1..=65535).contains(&port) {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

pub(crate) fn validate_startup_command(command: &str) -> AppResult<()> {
    let command = command.trim();
    if command.is_empty() || command.len() > 512 {
        return Err(AppError::InvalidOperation);
    }
    if command.contains([
        '\0', '\n', '\r', ';', '|', '&', '$', '`', '(', ')', '<', '>', '{', '}', '[', ']', '\\',
        '"', '\'',
    ]) || command.contains("..")
    {
        return Err(AppError::InvalidOperation);
    }
    let mut parts = command.split_whitespace();
    let first = parts.next().ok_or(AppError::InvalidOperation)?;
    if first.starts_with('/') {
        return Err(AppError::InvalidOperation);
    }
    validate_relative_path(first)?;
    for part in parts {
        if part.is_empty()
            || part.len() > 256
            || part.contains("..")
            || !part.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(b, b'_' | b'-' | b'.' | b'/' | b'=' | b':' | b'+' | b'@')
            })
        {
            return Err(AppError::InvalidOperation);
        }
    }
    Ok(())
}

pub(crate) fn validate_python_config(config: &PythonProjectConfig) -> AppResult<()> {
    if !(1..=64).contains(&config.processes) || !(1..=64).contains(&config.threads) {
        return Err(AppError::InvalidOperation);
    }
    validate_python_bin(&config.python_bin)?;
    validate_identifier(&config.run_user)?;
    if !config.requirements_path.is_empty() {
        if config.requirements_path.starts_with('/') {
            validate_remote_path(&config.requirements_path)?;
        } else {
            validate_relative_path(&config.requirements_path)?;
        }
    }

    match config.startup {
        crate::domain::PythonStartupMethod::Command => {
            validate_startup_command(&config.startup_command)?;
            Ok(())
        }
        crate::domain::PythonStartupMethod::Gunicorn => {
            validate_port(config.port)?;
            validate_identifier(&config.application)?;
            if !config.entry_file.is_empty() {
                validate_relative_path(&config.entry_file)?;
            }
            Ok(())
        }
        crate::domain::PythonStartupMethod::Uwsgi => {
            validate_port(config.port)?;
            validate_relative_path(&config.entry_file)?;
            if !config.application.is_empty()
                && (config.application.len() > 256
                    || !config
                        .application
                        .starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
                    || !config
                        .application
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            {
                return Err(AppError::InvalidOperation);
            }
            if matches!(config.protocol, crate::domain::PythonProtocol::Asgi) {
                return Err(AppError::InvalidOperation);
            }
            Ok(())
        }
    }
}

/// setup_step is the next screen to display, not a completed step.
/// Drafts may reach startup configuration before any startup fields are filled;
/// full validation belongs to the transition to Ready and remote provisioning.
pub(crate) fn validate_python_config_partial(
    config: &PythonProjectConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 3 {
        return Ok(());
    }
    if !config.python_bin.is_empty() {
        validate_python_bin(&config.python_bin)?;
    } else {
        return Err(AppError::InvalidOperation);
    }
    if !config.requirements_path.is_empty() {
        if config.requirements_path.starts_with('/') {
            validate_remote_path(&config.requirements_path)?;
        } else {
            validate_relative_path(&config.requirements_path)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_restart(target: &RestartTarget) -> AppResult<()> {
    match target {
        RestartTarget::None => Ok(()),
        RestartTarget::Systemd { service } => validate_identifier(service),
        RestartTarget::Pm2 { process } => validate_identifier(process),
        RestartTarget::DockerCompose { service } => validate_identifier(service),
    }
}

pub(crate) fn validate_app_name(name: &str) -> AppResult<()> {
    if !name.is_empty() && name.len() <= 128 && !name.contains(['\0', '\n', '\r']) {
        Ok(())
    } else {
        Err(AppError::InvalidOperation)
    }
}

/// Lowercase ASCII slug shared by the generated systemd unit names.
pub(crate) fn service_slug(name: &str) -> AppResult<String> {
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let collapsed = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() || collapsed.len() > 48 {
        return Err(AppError::InvalidOperation);
    }
    Ok(collapsed)
}

pub(crate) fn python_service_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-py-{}.service", service_slug(name)?))
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
    use crate::domain::{PythonProtocol, PythonStartupMethod};

    #[test]
    fn paths_and_branches_reject_traversal_and_metacharacters() {
        assert!(validate_remote_path("/srv/app").is_ok());
        assert!(validate_remote_path("/srv/../etc").is_err());
        assert!(validate_branch("main;id").is_err());
    }

    #[test]
    fn remote_urls_accept_http_https_ssh_and_git_scp() {
        assert!(validate_remote_url("https://example.com/app.git").is_ok());
        assert!(validate_remote_url("http://git.internal/api.git").is_ok());
        assert!(validate_remote_url("ssh://git@example.com/app.git").is_ok());
        assert!(validate_remote_url("git@example.com:org/app.git").is_ok());
        assert!(validate_remote_url("").is_ok());
        assert!(validate_remote_url("ftp://example.com/app.git").is_err());
        assert!(validate_remote_url("https://example.com/app.git ").is_err());
    }

    #[test]
    fn python_service_names_are_stable_and_safe() {
        assert_eq!(
            python_service_name("Demo App").unwrap(),
            "runory-py-demo-app.service"
        );
        assert!(python_service_name("!!!").is_err());
    }

    #[test]
    fn python_config_rejects_traversal_entry() {
        let mut config = PythonProjectConfig {
            port: 8000,
            python_bin: "python3".into(),
            startup: PythonStartupMethod::Gunicorn,
            entry_file: "app.py".into(),
            protocol: PythonProtocol::Asgi,
            application: "main:app".into(),
            run_user: "www".into(),
            requirements_path: "requirements.txt".into(),
            startup_command: String::new(),
            processes: 1,
            threads: 1,
            auto_start: true,
        };
        assert!(validate_python_config(&config).is_ok());
        config.application = "main;id".into();
        assert!(validate_python_config(&config).is_err());
    }

    #[test]
    fn command_startup_requires_safe_relative_command() {
        let mut config = PythonProjectConfig {
            port: 0,
            python_bin: "python3".into(),
            startup: PythonStartupMethod::Command,
            entry_file: String::new(),
            protocol: PythonProtocol::Wsgi,
            application: String::new(),
            run_user: "www".into(),
            requirements_path: String::new(),
            startup_command: "app.py --host 127.0.0.1".into(),
            processes: 1,
            threads: 1,
            auto_start: true,
        };
        assert!(validate_python_config(&config).is_ok());
        config.startup_command = "app.py; rm -rf /".into();
        assert!(validate_python_config(&config).is_err());
        config.startup_command = "/etc/passwd".into();
        assert!(validate_python_config(&config).is_err());
    }
}
