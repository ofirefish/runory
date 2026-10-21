use crate::domain::{AppError, AppResult, HtmlProjectConfig};

use super::validation::{
    service_slug, validate_domain, validate_port, validate_relative_path, validate_remote_path,
};

fn invalid<T>() -> AppResult<T> {
    Err(AppError::InvalidOperation)
}

/// Paths are written into nginx configuration where `$`, quotes, `;` and whitespace are
/// meaningful, so HTML projects accept only an unambiguous character set.
fn is_safe_config_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b'+' | b'@')
}

pub(crate) fn validate_html_path(path: &str) -> AppResult<()> {
    validate_remote_path(path)?;
    if path.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

pub(crate) fn validate_html_root_dir(root_dir: &str) -> AppResult<()> {
    if root_dir.is_empty() {
        return Ok(());
    }
    validate_relative_path(root_dir)?;
    if root_dir.len() <= 256 && root_dir.bytes().all(is_safe_config_char) {
        Ok(())
    } else {
        invalid()
    }
}

/// Host names, `localhost`, or a `*.` wildcard in front of a domain.
pub(crate) fn validate_html_server_name(server_name: &str) -> AppResult<()> {
    let names = server_name.split_whitespace().collect::<Vec<_>>();
    if names.len() > 10 {
        return invalid();
    }
    for name in names {
        if name == "localhost" {
            continue;
        }
        validate_domain(name.strip_prefix("*.").unwrap_or(name))?;
    }
    Ok(())
}

/// Default document file names (no directories), tried in order.
pub(crate) fn validate_html_index_files(index_files: &str) -> AppResult<()> {
    let files = index_files.split_whitespace().collect::<Vec<_>>();
    if files.is_empty() || files.len() > 8 {
        return invalid();
    }
    for file in files {
        if file.len() > 64
            || file.starts_with('.')
            || !file
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return invalid();
        }
    }
    Ok(())
}

pub(crate) fn validate_html_config(config: &HtmlProjectConfig) -> AppResult<()> {
    validate_port(config.port)?;
    validate_html_server_name(&config.server_name)?;
    validate_html_root_dir(&config.root_dir)?;
    validate_html_index_files(&config.index_files)
}

/// setup_step is the next screen to display. Site settings are only complete at the final
/// step, so drafts need just a valid web root from step 3.
pub(crate) fn validate_html_config_partial(
    config: &HtmlProjectConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 3 {
        return Ok(());
    }
    validate_html_root_dir(&config.root_dir)?;
    validate_html_index_files(&config.index_files)
}

/// nginx include file for the project, derived from the app name.
pub(crate) fn html_nginx_conf_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-html-{}.conf", service_slug(name)?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample_config() -> HtmlProjectConfig {
        HtmlProjectConfig {
            port: 8080,
            server_name: "example.com *.example.com".into(),
            root_dir: "dist".into(),
            index_files: "index.html index.htm".into(),
            spa_fallback: true,
            gzip: true,
            cache_static: true,
        }
    }

    #[test]
    fn paths_reject_nginx_metacharacters() {
        assert!(validate_html_path("/srv/my-site_1").is_ok());
        for bad in [
            "/srv/my site",
            "/srv/$root",
            "/srv/a;b",
            "/srv/\"x\"",
            "/srv/../etc",
        ] {
            assert!(validate_html_path(bad).is_err(), "{bad}");
        }
        assert!(validate_html_root_dir("").is_ok());
        assert!(validate_html_root_dir("dist/site").is_ok());
        for bad in ["../dist", "/dist", "a b", "a;b"] {
            assert!(validate_html_root_dir(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn server_names_allow_wildcards_but_not_injection() {
        assert!(validate_html_server_name("").is_ok());
        assert!(validate_html_server_name("example.com *.example.com localhost").is_ok());
        for bad in [
            "bad host;",
            "a.com; root /;",
            "*",
            "*.",
            "*.com;x",
            "exa mple",
        ] {
            assert!(validate_html_server_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn index_files_are_plain_file_names() {
        assert!(validate_html_index_files("index.html default.htm").is_ok());
        for bad in ["", "dir/index.html", ".hidden", "a;b", "a b;c"] {
            assert!(validate_html_index_files(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn config_requires_a_valid_port_and_is_otherwise_permissive_when_defaults() {
        let mut config = sample_config();
        assert!(validate_html_config(&config).is_ok());
        config.port = 0;
        assert!(validate_html_config(&config).is_err());
    }

    #[test]
    fn conf_name_uses_html_prefix() {
        assert_eq!(
            html_nginx_conf_name("My Site").unwrap(),
            "runory-html-my-site.conf"
        );
        assert!(html_nginx_conf_name("!!!").is_err());
    }
}
