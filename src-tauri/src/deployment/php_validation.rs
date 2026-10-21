use crate::domain::{AppError, AppResult, PhpProjectConfig};

use super::validation::{
    service_slug, validate_domain, validate_identifier, validate_port, validate_relative_path,
    validate_remote_path,
};

fn invalid<T>() -> AppResult<T> {
    Err(AppError::InvalidOperation)
}

/// Paths are written into nginx and php-fpm configuration where `$`, quotes, `;` and
/// whitespace are meaningful, so PHP projects accept only an unambiguous character set.
fn is_safe_config_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'+' | b'@')
}

pub(crate) fn validate_php_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    if path.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

pub(crate) fn validate_php_bin(value: &str) -> AppResult<()> {
    let version = value.strip_prefix("php");
    match version {
        Some("") => Ok(()),
        Some(rest)
            if rest.len() <= 6
                && rest.starts_with(|ch: char| ch.is_ascii_digit())
                && rest.bytes().all(|b| b.is_ascii_digit() || b == b'.') =>
        {
            Ok(())
        }
        _ => invalid(),
    }
}

pub(crate) fn validate_php_run_dir(run_dir: &str) -> AppResult<()> {
    if run_dir.is_empty() {
        return Ok(());
    }
    validate_relative_path(run_dir)?;
    if run_dir.len() <= 256 && run_dir.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

pub(crate) fn validate_php_server_name(server_name: &str) -> AppResult<()> {
    let names = server_name.split_whitespace().collect::<Vec<_>>();
    if names.len() > 10 {
        return invalid();
    }
    for name in names {
        if name != "localhost" {
            validate_domain(name)?;
        }
    }
    Ok(())
}

fn in_range(value: u16, min: u16, max: u16) -> bool {
    (min..=max).contains(&value)
}

pub(crate) fn validate_php_config(config: &PhpProjectConfig) -> AppResult<()> {
    validate_php_bin(&config.php_bin)?;
    validate_identifier(&config.run_user)?;
    validate_port(config.port)?;
    validate_php_server_name(&config.server_name)?;
    validate_php_run_dir(&config.run_dir)?;
    if !in_range(config.max_children, 1, 512)
        || !in_range(config.memory_limit_mb, 16, 8192)
        || !in_range(config.max_execution_seconds, 1, 3600)
        || !in_range(config.upload_max_mb, 1, 2048)
    {
        return invalid();
    }
    Ok(())
}

/// setup_step is the next screen to display. Site and process settings are only
/// complete at the final step, so drafts need just a valid interpreter from step 3.
pub(crate) fn validate_php_config_partial(
    config: &PhpProjectConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 3 {
        return Ok(());
    }
    validate_php_bin(&config.php_bin)?;
    validate_php_run_dir(&config.run_dir)
}

pub(crate) fn php_service_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-php-{}.service", service_slug(name)?))
}

/// nginx include file for the project, derived from the same slug as the service.
pub(crate) fn php_nginx_conf_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-php-{}.conf", service_slug(name)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample_config() -> PhpProjectConfig {
        PhpProjectConfig {
            php_bin: "php8.2".into(),
            port: 8080,
            server_name: "example.com www.example.com".into(),
            run_dir: "public".into(),
            rewrite: crate::domain::PhpRewriteMode::FrontController,
            run_user: "www-data".into(),
            process_manager: crate::domain::PhpProcessManager::Dynamic,
            max_children: 8,
            memory_limit_mb: 256,
            max_execution_seconds: 60,
            upload_max_mb: 50,
            open_basedir: true,
            composer_install: true,
            auto_start: true,
        }
    }

    #[test]
    fn php_binaries_are_limited_to_versioned_names() {
        for ok in ["php", "php8.2", "php82", "php7.4"] {
            assert!(validate_php_bin(ok).is_ok(), "{ok}");
        }
        for bad in [
            "python3",
            "php;id",
            "php-fpm",
            "php8.2.1.1.1",
            "/usr/bin/php",
        ] {
            assert!(validate_php_bin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn config_paths_reject_nginx_and_ini_metacharacters() {
        assert!(validate_php_path("/srv/my-site_1").is_ok());
        for bad in [
            "/srv/my site",
            "/srv/$root",
            "/srv/a;b",
            "/srv/\"x\"",
            "/srv/../etc",
        ] {
            assert!(validate_php_path(bad).is_err(), "{bad}");
        }
        assert!(validate_php_run_dir("").is_ok());
        assert!(validate_php_run_dir("public").is_ok());
        assert!(validate_php_run_dir("../public").is_err());
        assert!(validate_php_run_dir("/public").is_err());
    }

    #[test]
    fn config_validates_limits_and_hosts() {
        let mut config = sample_config();
        assert!(validate_php_config(&config).is_ok());
        config.server_name = "localhost".into();
        assert!(validate_php_config(&config).is_ok());
        config.server_name = "bad host;".into();
        assert!(validate_php_config(&config).is_err());
        config.server_name = String::new();
        config.max_children = 0;
        assert!(validate_php_config(&config).is_err());
        config.max_children = 8;
        config.upload_max_mb = 4096;
        assert!(validate_php_config(&config).is_err());
        config.upload_max_mb = 50;
        config.port = 0;
        assert!(validate_php_config(&config).is_err());
    }

    #[test]
    fn service_and_nginx_names_share_a_slug() {
        assert_eq!(
            php_service_name("My Site").unwrap(),
            "runory-php-my-site.service"
        );
        assert_eq!(
            php_nginx_conf_name("My Site").unwrap(),
            "runory-php-my-site.conf"
        );
        assert!(php_service_name("!!!").is_err());
    }
}
