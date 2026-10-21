use crate::domain::{AppError, AppResult, JavaProjectConfig};

use super::validation::{
    service_slug, validate_identifier, validate_port, validate_relative_path, validate_remote_path,
};

fn invalid<T>() -> AppResult<T> {
    Err(AppError::InvalidOperation)
}

/// Paths and options end up in a systemd unit and in shell arguments, so Java projects accept
/// only an unambiguous character set (no spaces, quotes, `$`, `;` or `%`).
fn is_safe_config_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'+' | b'@')
}

pub(crate) fn validate_java_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    if path.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

/// `java` is resolved on the remote PATH at deploy time; anything else must be the absolute
/// `bin/java` of an installed JDK/JRE as reported by the runtime scan.
pub(crate) fn validate_java_bin(value: &str) -> AppResult<()> {
    if value == "java" {
        return Ok(());
    }
    if value.len() <= 256 && value.ends_with("/bin/java") {
        return validate_java_path(value);
    }
    invalid()
}

pub(crate) fn validate_java_jar_path(jar_path: &str) -> AppResult<()> {
    if !jar_path.ends_with(".jar") || jar_path.len() > 512 {
        return invalid();
    }
    if jar_path.starts_with('/') {
        validate_java_path(jar_path)
    } else {
        validate_relative_path(jar_path)?;
        if jar_path.bytes().all(is_safe_config_char) {
            Ok(())
        } else {
            invalid()
        }
    }
}

fn is_option_char(byte: u8) -> bool {
    is_safe_config_char(byte) || matches!(byte, b'=' | b':' | b',')
}

/// Space-separated tokens, each limited to `is_option_char`. JVM options must start with `-`;
/// heap sizes have dedicated fields and debug/agent switches are not accepted.
pub(crate) fn validate_java_jvm_args(args: &str) -> AppResult<()> {
    let tokens = args.split_whitespace().collect::<Vec<_>>();
    if tokens.len() > 32 || args.len() > 1024 {
        return invalid();
    }
    for token in tokens {
        let lower = token.to_ascii_lowercase();
        if !token.starts_with('-')
            || token.len() > 256
            || token.contains("..")
            || !token.bytes().all(is_option_char)
            || lower.starts_with("-xmx")
            || lower.starts_with("-xms")
            || lower.starts_with("-agent")
            || lower.starts_with("-xrun")
            || lower.starts_with("-xdebug")
            || lower.starts_with("-dserver.port")
        {
            return invalid();
        }
    }
    Ok(())
}

pub(crate) fn validate_java_app_args(args: &str) -> AppResult<()> {
    let tokens = args.split_whitespace().collect::<Vec<_>>();
    if tokens.len() > 32 || args.len() > 1024 {
        return invalid();
    }
    for token in tokens {
        if token.len() > 256 || token.contains("..") || !token.bytes().all(is_option_char) {
            return invalid();
        }
    }
    Ok(())
}

pub(crate) fn validate_java_heap(xms_mb: u32, xmx_mb: u32) -> AppResult<()> {
    if (16..=262_144).contains(&xms_mb) && (16..=262_144).contains(&xmx_mb) && xms_mb <= xmx_mb {
        Ok(())
    } else {
        invalid()
    }
}

pub(crate) fn validate_java_config(config: &JavaProjectConfig) -> AppResult<()> {
    validate_java_bin(&config.java_bin)?;
    validate_java_jar_path(&config.jar_path)?;
    validate_identifier(&config.run_user)?;
    validate_port(config.port)?;
    validate_java_heap(config.xms_mb, config.xmx_mb)?;
    validate_java_jvm_args(&config.jvm_args)?;
    validate_java_app_args(&config.app_args)
}

/// setup_step is the next screen to display. Runtime settings are only complete at the final
/// step, so drafts need just a valid JDK and jar path from step 3.
pub(crate) fn validate_java_config_partial(
    config: &JavaProjectConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 3 {
        return Ok(());
    }
    validate_java_bin(&config.java_bin)?;
    validate_java_jar_path(&config.jar_path)
}

pub(crate) fn java_service_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-java-{}.service", service_slug(name)?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::JavaBuildTool;

    pub(crate) fn sample_config() -> JavaProjectConfig {
        JavaProjectConfig {
            java_bin: "java".into(),
            jar_path: "target/app.jar".into(),
            port: 8080,
            xms_mb: 256,
            xmx_mb: 512,
            jvm_args: "-XX:+UseG1GC -Dspring.profiles.active=prod".into(),
            app_args: "--spring.config.location=conf/".into(),
            run_user: "www".into(),
            build_tool: JavaBuildTool::None,
            auto_start: true,
        }
    }

    #[test]
    fn java_binary_is_path_lookup_or_absolute_jdk_path() {
        for ok in ["java", "/usr/lib/jvm/java-17-openjdk-amd64/bin/java"] {
            assert!(validate_java_bin(ok).is_ok(), "{ok}");
        }
        for bad in [
            "javac",
            "java;id",
            "/usr/bin/python3",
            "bin/java",
            "/opt/my jdk/bin/java",
            "/opt/../bin/java",
        ] {
            assert!(validate_java_bin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn jar_path_must_be_a_plain_jar_file() {
        for ok in ["app.jar", "target/app-1.0.jar", "/srv/app/app.jar"] {
            assert!(validate_java_jar_path(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "app.war",
            "../app.jar",
            "a b.jar",
            "$HOME/app.jar",
            "a;b.jar",
        ] {
            assert!(validate_java_jar_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn jvm_options_reject_shell_metacharacters_and_reserved_switches() {
        assert!(validate_java_jvm_args("").is_ok());
        assert!(validate_java_jvm_args("-XX:+UseG1GC -Duser.timezone=Asia/Shanghai").is_ok());
        for bad in [
            "XX:+UseG1GC",
            "-Dx=$(id)",
            "-Dx=a;b",
            "-Xmx1g",
            "-agentlib:jdwp=transport=dt_socket",
            "-Dserver.port=1",
            "-Dx=\"a\"",
        ] {
            assert!(validate_java_jvm_args(bad).is_err(), "{bad}");
        }
        assert!(validate_java_app_args("--server.address=127.0.0.1 --debug").is_ok());
        assert!(validate_java_app_args("--x=a|b").is_err());
    }

    #[test]
    fn config_validates_heap_port_and_user() {
        let mut config = sample_config();
        assert!(validate_java_config(&config).is_ok());
        config.xms_mb = 1024;
        assert!(validate_java_config(&config).is_err());
        config.xms_mb = 256;
        config.xmx_mb = 8;
        assert!(validate_java_config(&config).is_err());
        config.xmx_mb = 512;
        config.port = 0;
        assert!(validate_java_config(&config).is_err());
        config.port = 8080;
        config.run_user = "bad user".into();
        assert!(validate_java_config(&config).is_err());
    }

    #[test]
    fn service_name_uses_java_prefix() {
        assert_eq!(
            java_service_name("My API").unwrap(),
            "runory-java-my-api.service"
        );
        assert!(java_service_name("!!!").is_err());
    }
}
