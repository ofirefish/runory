use crate::domain::{
    AppError, AppResult, PythonProjectConfig, PythonProtocol, PythonStartupMethod,
};

// systemd parses its own quoting and specifiers; shell escaping is insufficient.
pub(super) fn unit_value(value: &str, exec: bool) -> AppResult<String> {
    if value.chars().any(char::is_control) {
        return Err(AppError::InvalidOperation);
    }
    let mut escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    if exec {
        escaped = escaped.replace('$', "$$");
    }
    Ok(format!("\"{escaped}\""))
}

pub(super) fn render_python_unit(
    name: &str,
    project_path: &str,
    venv_path: &str,
    python: &PythonProjectConfig,
    service: &str,
) -> AppResult<String> {
    super::validation::validate_python_config(python)?;
    super::validation::validate_remote_path(project_path)?;
    super::validation::validate_remote_path(venv_path)?;
    super::validation::validate_app_name(name)?;
    let bin = format!("{}/bin", venv_path.trim_end_matches('/'));
    let exec = match python.startup {
        PythonStartupMethod::Gunicorn => {
            let worker = if python.protocol == PythonProtocol::Asgi {
                "uvicorn.workers.UvicornWorker"
            } else {
                "gthread"
            };
            format!(
                "{} -k {worker} --workers {} --threads {} -b 127.0.0.1:{} {}",
                unit_value(&format!("{bin}/gunicorn"), true)?,
                python.processes,
                python.threads,
                python.port,
                unit_value(&python.application, true)?
            )
        }
        PythonStartupMethod::Uwsgi => {
            let callable = if python.application.is_empty() {
                "application"
            } else {
                &python.application
            };
            format!("{} --master --die-on-term --processes {} --threads {} --http 127.0.0.1:{} --chdir {} --wsgi-file {} --callable {}",
                unit_value(&format!("{bin}/uwsgi"), true)?, python.processes, python.threads,
                python.port, unit_value(project_path, true)?, unit_value(&python.entry_file, true)?, callable)
        }
        PythonStartupMethod::Command => {
            let args = python
                .startup_command
                .split_whitespace()
                .map(|arg| unit_value(arg, true))
                .collect::<AppResult<Vec<_>>>()?
                .join(" ");
            format!("{} {args}", unit_value(&format!("{bin}/python"), true)?)
        }
    };
    Ok(format!(
        "[Unit]\nDescription=Runory Python {}\nAfter=network.target\n\n[Service]\nType=simple\nUser={}\nWorkingDirectory={}\nEnvironment={}\nEnvironment=PYTHONUNBUFFERED=1\nExecStart={}\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\n# runory-unit:{}\n",
        name.replace('%', "%%"), python.run_user, unit_value(project_path, false)?,
        unit_value(&format!("PATH={bin}:/usr/bin:/bin"), false)?, exec, service
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_systemd_values_and_blocks_control_characters() {
        assert_eq!(
            unit_value("/srv/my app/%i/$HOME", true).unwrap(),
            "\"/srv/my app/%%i/$$HOME\""
        );
        assert_eq!(unit_value("a\"b\\c", false).unwrap(), "\"a\\\"b\\\\c\"");
        assert!(unit_value("/srv/app\nExecStart=id", true).is_err());
    }

    fn legacy_config() -> PythonProjectConfig {
        serde_json::from_str(r#"{"port":8000,"pythonBin":"python3","startup":"uwsgi","entryFile":"app.py","protocol":"wsgi","application":"","runUser":"www","requirementsPath":""}"#).unwrap()
    }

    #[test]
    fn legacy_projects_keep_defaults_and_custom_wsgi_callable_is_used() {
        let mut config = legacy_config();
        assert!(config.auto_start);
        assert_eq!((config.processes, config.threads), (1, 1));
        config.application = "app".into();
        config.processes = 4;
        config.threads = 2;
        config.auto_start = false;
        let restored: PythonProjectConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored, config);
        let unit = render_python_unit(
            "demo",
            "/srv/my app",
            "/srv/my app/.venv",
            &config,
            "runory-py-demo.service",
        )
        .unwrap();
        assert!(unit.contains("--processes 4 --threads 2"));
        assert!(unit.contains("--callable app"));
        assert!(unit.contains("WorkingDirectory=\"/srv/my app\""));
        assert!(unit.contains("--master --die-on-term"));
    }

    #[test]
    fn invalid_limits_and_callable_fail_before_rendering() {
        let mut config = legacy_config();
        config.processes = 0;
        assert!(render_python_unit(
            "demo",
            "/srv/demo",
            "/srv/demo/.venv",
            &config,
            "runory-py-demo.service"
        )
        .is_err());
        config.processes = 1;
        config.threads = 65;
        assert!(super::super::validation::validate_python_config(&config).is_err());
        config.threads = 1;
        config.application = "app --daemonize".into();
        assert!(super::super::validation::validate_python_config(&config).is_err());
    }
}
