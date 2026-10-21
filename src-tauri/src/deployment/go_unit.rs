use crate::domain::{AppResult, GoProjectConfig};

use super::go_validation::{validate_go_config, validate_go_path};
use super::python_unit::unit_value;
use super::validation::validate_app_name;

pub(super) fn resolve_executable_path(project_path: &str, executable: &str) -> String {
    if executable.starts_with('/') {
        executable.to_string()
    } else {
        format!("{}/{executable}", project_path.trim_end_matches('/'))
    }
}

pub(super) fn render_go_unit(
    name: &str,
    project_path: &str,
    go: &GoProjectConfig,
    service: &str,
) -> AppResult<String> {
    validate_go_config(go)?;
    validate_go_path(project_path)?;
    validate_app_name(name)?;
    let executable = resolve_executable_path(project_path, &go.executable);
    let mut words = vec![unit_value(&executable, true)?];
    for arg in go.app_args.split_whitespace() {
        words.push(unit_value(arg, true)?);
    }
    let mut environment = vec![unit_value(&format!("PORT={}", go.port), false)?];
    for pair in go.env_vars.split_whitespace() {
        environment.push(unit_value(pair, false)?);
    }
    let environment = environment
        .into_iter()
        .map(|value| format!("Environment={value}\n"))
        .collect::<String>();
    // 143 is the normal exit status after SIGTERM, so `systemctl stop` is not a failure.
    Ok(format!(
        "[Unit]\nDescription=Runory Go {}\nAfter=network.target\n\n[Service]\nType=simple\nUser={}\nWorkingDirectory={}\n{}ExecStart={}\nSuccessExitStatus=143\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\n# runory-unit:{}\n",
        name.replace('%', "%%"),
        go.run_user,
        unit_value(project_path, false)?,
        environment,
        words.join(" "),
        service
    ))
}

#[cfg(test)]
mod tests {
    use super::super::go_validation::tests::sample_config;
    use super::*;

    #[test]
    fn renders_unit_with_environment_and_arguments() {
        let unit =
            render_go_unit("api", "/srv/api", &sample_config(), "runory-go-api.service").unwrap();
        assert!(unit.contains("WorkingDirectory=\"/srv/api\""));
        assert!(unit.contains("Environment=\"PORT=8080\"\n"));
        assert!(unit.contains("Environment=\"GIN_MODE=release\"\n"));
        assert!(unit.contains("Environment=\"APP_ENV=prod\"\n"));
        assert!(unit.contains("ExecStart=\"/srv/api/bin/app\" \"--config=conf/app.yaml\"\n"));
        assert!(unit.contains("User=www"));
        assert!(unit.contains("SuccessExitStatus=143"));
        assert!(unit.ends_with("# runory-unit:runory-go-api.service\n"));
    }

    #[test]
    fn absolute_executable_is_kept_and_invalid_input_fails_before_rendering() {
        let mut config = sample_config();
        config.executable = "/opt/app/server".into();
        let unit = render_go_unit("api", "/srv/api", &config, "s.service").unwrap();
        assert!(unit.contains("ExecStart=\"/opt/app/server\""));
        config.env_vars = "A=$HOME".into();
        assert!(render_go_unit("api", "/srv/api", &config, "s.service").is_err());
        assert!(render_go_unit("api", "/srv/my app", &sample_config(), "s.service").is_err());
    }
}
