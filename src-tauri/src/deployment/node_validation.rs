use crate::domain::{AppError, AppResult, NodeProjectConfig, NodeStartMode};

use super::validation::{
    service_slug, validate_identifier, validate_port, validate_relative_path, validate_remote_path,
};

fn invalid<T>() -> AppResult<T> {
    Err(AppError::InvalidOperation)
}

/// Paths and options end up in a systemd unit and in shell arguments, so Node projects accept
/// only an unambiguous character set (no spaces, quotes, `$`, `;` or `%`).
fn is_safe_config_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'+' | b'@')
}

pub(crate) fn validate_node_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    if path.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

/// `node` is resolved on the remote PATH at deploy time; anything else must be an absolute
/// `bin/node` such as an nvm or distribution install reported by the runtime scan.
pub(crate) fn validate_node_bin(value: &str) -> AppResult<()> {
    if value == "node" {
        return Ok(());
    }
    if value.len() <= 256 && value.ends_with("/bin/node") {
        return validate_node_path(value);
    }
    invalid()
}

/// `package.json` script names such as `start` or `build:prod`.
pub(crate) fn validate_node_script(script: &str) -> AppResult<()> {
    if !script.is_empty()
        && script.len() <= 64
        && script.starts_with(|ch: char| ch.is_ascii_alphanumeric())
        && script
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
    {
        Ok(())
    } else {
        invalid()
    }
}

pub(crate) fn validate_node_entry_file(entry: &str) -> AppResult<()> {
    let extension_ok = [".js", ".mjs", ".cjs"]
        .iter()
        .any(|extension| entry.ends_with(extension));
    if !extension_ok || entry.len() > 512 {
        return invalid();
    }
    validate_relative_path(entry)?;
    if entry.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

fn is_option_char(byte: u8) -> bool {
    is_safe_config_char(byte) || matches!(byte, b'=' | b':' | b',')
}

pub(crate) fn validate_node_app_args(args: &str) -> AppResult<()> {
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

pub(crate) fn validate_node_env(value: &str) -> AppResult<()> {
    if !value.is_empty()
        && value.len() <= 32
        && value.starts_with(|ch: char| ch.is_ascii_lowercase())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
    {
        Ok(())
    } else {
        invalid()
    }
}

fn validate_start(config: &NodeProjectConfig) -> AppResult<()> {
    match config.start_mode {
        NodeStartMode::Script => validate_node_script(&config.script),
        NodeStartMode::Entry => validate_node_entry_file(&config.entry_file),
    }
}

pub(crate) fn validate_node_config(config: &NodeProjectConfig) -> AppResult<()> {
    validate_node_bin(&config.node_bin)?;
    validate_start(config)?;
    validate_identifier(&config.run_user)?;
    validate_port(config.port)?;
    validate_node_env(&config.node_env)?;
    if !config.build_script.is_empty() {
        validate_node_script(&config.build_script)?;
    }
    validate_node_app_args(&config.app_args)
}

/// setup_step is the next screen to display. Runtime settings are only complete at the final
/// step, so drafts need just a valid Node binary and start target from step 3.
pub(crate) fn validate_node_config_partial(
    config: &NodeProjectConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 3 {
        return Ok(());
    }
    validate_node_bin(&config.node_bin)?;
    validate_start(config)?;
    if !config.build_script.is_empty() {
        validate_node_script(&config.build_script)?;
    }
    Ok(())
}

pub(crate) fn node_service_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-node-{}.service", service_slug(name)?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::NodePackageManager;

    pub(crate) fn sample_config() -> NodeProjectConfig {
        NodeProjectConfig {
            node_bin: "node".into(),
            package_manager: NodePackageManager::Npm,
            start_mode: NodeStartMode::Script,
            script: "start".into(),
            entry_file: String::new(),
            app_args: String::new(),
            port: 3000,
            node_env: "production".into(),
            install_deps: true,
            build_script: "build".into(),
            run_user: "www".into(),
            auto_start: true,
        }
    }

    #[test]
    fn node_binary_is_path_lookup_or_absolute_node_path() {
        for ok in [
            "node",
            "/usr/bin/node",
            "/home/app/.nvm/versions/node/v20.11.0/bin/node",
        ] {
            assert!(validate_node_bin(ok).is_ok(), "{ok}");
        }
        for bad in [
            "nodejs",
            "node;id",
            "bin/node",
            "/usr/bin/python3",
            "/opt/my node/bin/node",
            "/opt/../bin/node",
        ] {
            assert!(validate_node_bin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn scripts_and_entry_files_are_plain_names() {
        for ok in ["start", "build:prod", "start.dev"] {
            assert!(validate_node_script(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-x", "a b", "a;b", "$(id)"] {
            assert!(validate_node_script(bad).is_err(), "{bad}");
        }
        for ok in ["server.js", "dist/main.mjs", "bin/www.cjs"] {
            assert!(validate_node_entry_file(ok).is_ok(), "{ok}");
        }
        for bad in ["", "server.ts", "/srv/server.js", "../server.js", "a b.js"] {
            assert!(validate_node_entry_file(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn env_and_args_reject_metacharacters() {
        assert!(validate_node_env("production").is_ok());
        assert!(validate_node_env("staging-2").is_ok());
        for bad in ["", "Production", "prod;id", "1x"] {
            assert!(validate_node_env(bad).is_err(), "{bad}");
        }
        assert!(validate_node_app_args("--host=127.0.0.1 --verbose").is_ok());
        assert!(validate_node_app_args("--x=a|b").is_err());
        assert!(validate_node_app_args("--x=$HOME").is_err());
    }

    #[test]
    fn config_requires_target_for_the_chosen_start_mode() {
        let mut config = sample_config();
        assert!(validate_node_config(&config).is_ok());
        config.start_mode = NodeStartMode::Entry;
        assert!(validate_node_config(&config).is_err());
        config.entry_file = "server.js".into();
        assert!(validate_node_config(&config).is_ok());
        config.port = 0;
        assert!(validate_node_config(&config).is_err());
        config.port = 3000;
        config.build_script = "a b".into();
        assert!(validate_node_config(&config).is_err());
        config.build_script = String::new();
        config.run_user = "bad user".into();
        assert!(validate_node_config(&config).is_err());
    }

    #[test]
    fn service_name_uses_node_prefix() {
        assert_eq!(
            node_service_name("My Web").unwrap(),
            "runory-node-my-web.service"
        );
        assert!(node_service_name("!!!").is_err());
    }
}
