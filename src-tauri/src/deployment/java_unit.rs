use crate::domain::{AppResult, JavaProjectConfig};

use super::java_validation::{validate_java_config, validate_java_path};
use super::python_unit::unit_value;
use super::validation::validate_app_name;

pub(super) fn resolve_jar_path(project_path: &str, jar_path: &str) -> String {
    if jar_path.starts_with('/') {
        jar_path.to_string()
    } else {
        format!("{}/{jar_path}", project_path.trim_end_matches('/'))
    }
}

/// `java_exec` is the absolute interpreter resolved on the remote host; systemd cannot search PATH
/// for `ExecStart`, and the resolved path is validated again before it reaches the unit.
pub(super) fn render_java_unit(
    name: &str,
    project_path: &str,
    java_exec: &str,
    java: &JavaProjectConfig,
    service: &str,
) -> AppResult<String> {
    validate_java_config(java)?;
    validate_java_path(project_path)?;
    validate_java_path(java_exec)?;
    validate_app_name(name)?;
    let jar = resolve_jar_path(project_path, &java.jar_path);
    let mut words = vec![
        unit_value(java_exec, true)?,
        unit_value(&format!("-Xms{}m", java.xms_mb), true)?,
        unit_value(&format!("-Xmx{}m", java.xmx_mb), true)?,
        unit_value(&format!("-Dserver.port={}", java.port), true)?,
    ];
    for arg in java.jvm_args.split_whitespace() {
        words.push(unit_value(arg, true)?);
    }
    words.push(unit_value("-jar", true)?);
    words.push(unit_value(&jar, true)?);
    for arg in java.app_args.split_whitespace() {
        words.push(unit_value(arg, true)?);
    }
    // 143 is the JVM's normal exit status after SIGTERM, so `systemctl stop` is not a failure.
    Ok(format!(
        "[Unit]\nDescription=Runory Java {}\nAfter=network.target\n\n[Service]\nType=simple\nUser={}\nWorkingDirectory={}\nExecStart={}\nSuccessExitStatus=143\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\n# runory-unit:{}\n",
        name.replace('%', "%%"),
        java.run_user,
        unit_value(project_path, false)?,
        words.join(" "),
        service
    ))
}

#[cfg(test)]
mod tests {
    use super::super::java_validation::tests::sample_config;
    use super::*;

    #[test]
    fn renders_spring_boot_unit() {
        let unit = render_java_unit(
            "api",
            "/srv/api",
            "/usr/lib/jvm/java-17/bin/java",
            &sample_config(),
            "runory-java-api.service",
        )
        .unwrap();
        assert!(unit.contains("WorkingDirectory=\"/srv/api\""));
        assert!(unit.contains(
            "ExecStart=\"/usr/lib/jvm/java-17/bin/java\" \"-Xms256m\" \"-Xmx512m\" \"-Dserver.port=8080\" \"-XX:+UseG1GC\" \"-Dspring.profiles.active=prod\" \"-jar\" \"/srv/api/target/app.jar\" \"--spring.config.location=conf/\""
        ));
        assert!(unit.contains("User=www"));
        assert!(unit.contains("SuccessExitStatus=143"));
        assert!(unit.ends_with("# runory-unit:runory-java-api.service\n"));
    }

    #[test]
    fn absolute_jar_is_kept_and_invalid_config_fails_before_rendering() {
        let mut config = sample_config();
        config.jar_path = "/opt/app/app.jar".into();
        let unit =
            render_java_unit("api", "/srv/api", "/usr/bin/java", &config, "s.service").unwrap();
        assert!(unit.contains("\"-jar\" \"/opt/app/app.jar\""));
        config.xmx_mb = 1;
        assert!(
            render_java_unit("api", "/srv/api", "/usr/bin/java", &config, "s.service").is_err()
        );
        assert!(render_java_unit(
            "api",
            "/srv/api",
            "/usr/bin/java;id",
            &sample_config(),
            "s.service"
        )
        .is_err());
    }
}
