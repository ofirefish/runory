use crate::domain::{AppError, AppResult, GoProjectConfig};

use super::validation::{
    service_slug, validate_identifier, validate_port, validate_relative_path, validate_remote_path,
};

fn invalid<T>() -> AppResult<T> {
    Err(AppError::InvalidOperation)
}

/// Paths and options end up in a systemd unit and in shell arguments, so Go projects accept
/// only an unambiguous character set (no spaces, quotes, `$`, `;` or `%`).
fn is_safe_config_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'+' | b'@')
}

pub(crate) fn validate_go_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    if path.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

/// `go` is resolved on the remote PATH when building; anything else must be an absolute `bin/go`.
pub(crate) fn validate_go_bin(value: &str) -> AppResult<()> {
    if value == "go" {
        return Ok(());
    }
    if value.len() <= 256 && value.ends_with("/bin/go") {
        return validate_go_path(value);
    }
    invalid()
}

/// Executable relative to the project or absolute. It must name a file, not a directory.
pub(crate) fn validate_go_executable(executable: &str) -> AppResult<()> {
    if executable.is_empty() || executable.len() > 512 || executable.ends_with('/') {
        return invalid();
    }
    if executable.starts_with('/') {
        validate_go_path(executable)
    } else {
        validate_relative_path(executable)?;
        if executable.bytes().all(is_safe_config_char) && !executable.starts_with('-') {
            Ok(())
        } else {
            invalid()
        }
    }
}

/// Package path handed to `go build`; a leading `-` would be parsed as a flag, so it is refused.
pub(crate) fn validate_go_build_package(package: &str) -> AppResult<()> {
    if package == "." {
        return Ok(());
    }
    if package.len() > 256 || package.starts_with('-') {
        return invalid();
    }
    validate_relative_path(package)?;
    if package.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

fn is_option_char(byte: u8) -> bool {
    is_safe_config_char(byte) || matches!(byte, b'=' | b':' | b',')
}

pub(crate) fn validate_go_app_args(args: &str) -> AppResult<()> {
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

/// `KEY=value` pairs. `PORT` has its own field; `PATH` and `LD_*` would let a value change which
/// binaries or libraries the service loads, so they are not accepted.
pub(crate) fn validate_go_env_vars(env_vars: &str) -> AppResult<()> {
    let pairs = env_vars.split_whitespace().collect::<Vec<_>>();
    if pairs.len() > 32 || env_vars.len() > 2048 {
        return invalid();
    }
    for pair in pairs {
        let Some((key, value)) = pair.split_once('=') else {
            return invalid();
        };
        if key.is_empty()
            || key.len() > 64
            || !key.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || matches!(key, "PORT" | "PATH")
            || key.starts_with("LD_")
            || value.len() > 256
            || !value.bytes().all(is_option_char)
        {
            return invalid();
        }
    }
    Ok(())
}

pub(crate) fn validate_go_config(config: &GoProjectConfig) -> AppResult<()> {
    validate_go_executable(&config.executable)?;
    validate_go_bin(&config.go_bin)?;
    validate_go_build_package(&config.build_package)?;
    validate_identifier(&config.run_user)?;
    validate_port(config.port)?;
    validate_go_app_args(&config.app_args)?;
    validate_go_env_vars(&config.env_vars)
}

/// setup_step is the next screen to display. Runtime settings are only complete at the final
/// step, so drafts need just a valid executable and build settings from step 3.
pub(crate) fn validate_go_config_partial(
    config: &GoProjectConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 3 {
        return Ok(());
    }
    validate_go_executable(&config.executable)?;
    validate_go_bin(&config.go_bin)?;
    validate_go_build_package(&config.build_package)
}

pub(crate) fn go_service_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-go-{}.service", service_slug(name)?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample_config() -> GoProjectConfig {
        GoProjectConfig {
            executable: "bin/app".into(),
            build_enabled: true,
            go_bin: "go".into(),
            build_package: "./cmd/server".into(),
            port: 8080,
            app_args: "--config=conf/app.yaml".into(),
            env_vars: "GIN_MODE=release APP_ENV=prod".into(),
            run_user: "www".into(),
            auto_start: true,
        }
    }

    #[test]
    fn go_binary_is_path_lookup_or_absolute_go_path() {
        for ok in ["go", "/usr/local/go/bin/go", "/home/a/sdk/go1.22.1/bin/go"] {
            assert!(validate_go_bin(ok).is_ok(), "{ok}");
        }
        for bad in [
            "gofmt",
            "go;id",
            "bin/go",
            "/usr/bin/python3",
            "/opt/my go/bin/go",
            "/opt/../bin/go",
        ] {
            assert!(validate_go_bin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn executable_and_package_are_plain_paths() {
        for ok in ["app", "bin/app", "/opt/app/server"] {
            assert!(validate_go_executable(ok).is_ok(), "{ok}");
        }
        for bad in ["", "bin/", "../app", "a b", "-app", "$HOME/app", "a;b"] {
            assert!(validate_go_executable(bad).is_err(), "{bad}");
        }
        for ok in [".", "./cmd/server", "cmd/api"] {
            assert!(validate_go_build_package(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-o", "/abs", "../x", "a b", "./a;b"] {
            assert!(validate_go_build_package(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn env_vars_reject_reserved_keys_and_metacharacters() {
        assert!(validate_go_env_vars("").is_ok());
        assert!(validate_go_env_vars("A=1 B_2=x/y:z,w").is_ok());
        for bad in [
            "NOEQUALS",
            "=x",
            "1A=x",
            "PORT=1",
            "PATH=/x",
            "LD_PRELOAD=/x.so",
            "A=$HOME",
            "A=\"x\"",
            "A=a;b",
        ] {
            assert!(validate_go_env_vars(bad).is_err(), "{bad}");
        }
        assert!(validate_go_app_args("--x=1 --v").is_ok());
        assert!(validate_go_app_args("--x=a|b").is_err());
    }

    #[test]
    fn config_validates_port_user_and_nested_fields() {
        let mut config = sample_config();
        assert!(validate_go_config(&config).is_ok());
        config.port = 0;
        assert!(validate_go_config(&config).is_err());
        config.port = 8080;
        config.run_user = "bad user".into();
        assert!(validate_go_config(&config).is_err());
        config.run_user = "www".into();
        config.build_package = "-x".into();
        assert!(validate_go_config(&config).is_err());
    }

    #[test]
    fn service_name_uses_go_prefix() {
        assert_eq!(
            go_service_name("My Svc").unwrap(),
            "runory-go-my-svc.service"
        );
        assert!(go_service_name("!!!").is_err());
    }
}
