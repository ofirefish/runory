use crate::domain::{AppError, AppResult, ReverseProxyConfig};

use super::validation::{service_slug, validate_domain, validate_port};

fn invalid<T>() -> AppResult<T> {
    Err(AppError::InvalidOperation)
}

pub(crate) fn validate_proxy_server_name(server_name: &str) -> AppResult<()> {
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

/// Upstream host: `localhost`, an IPv4 address, a domain, or a single-label name such as a
/// container or service alias. Only characters that cannot alter an nginx directive are allowed.
fn validate_target_host(host: &str) -> AppResult<()> {
    if host.is_empty()
        || host.len() > 253
        || host.starts_with(['.', '-'])
        || host.ends_with(['.', '-'])
        || host.contains("..")
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
    {
        return invalid();
    }
    Ok(())
}

fn validate_target_path(path: &str) -> AppResult<()> {
    if path.len() > 256
        || path.contains("..")
        || !path.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b'+' | b'@')
        })
    {
        return invalid();
    }
    Ok(())
}

/// Accepts `http(s)://host[:port][/path]`. Credentials, queries and fragments are refused
/// because the value is written into a `proxy_pass` directive.
pub(crate) fn validate_proxy_target(target: &str) -> AppResult<()> {
    let rest = target
        .strip_prefix("http://")
        .or_else(|| target.strip_prefix("https://"))
        .ok_or(AppError::InvalidOperation)?;
    if target.len() > 512 {
        return invalid();
    }
    let (authority, path) = match rest.find('/') {
        Some(index) => rest.split_at(index),
        None => (rest, ""),
    };
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    validate_target_host(host)?;
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return invalid();
        }
        validate_port(
            port.parse::<u16>()
                .map_err(|_| AppError::InvalidOperation)?,
        )?;
    }
    validate_target_path(path)
}

fn in_range(value: u16, min: u16, max: u16) -> bool {
    (min..=max).contains(&value)
}

pub(crate) fn validate_proxy_config(config: &ReverseProxyConfig) -> AppResult<()> {
    validate_port(config.port)?;
    validate_proxy_server_name(&config.server_name)?;
    validate_proxy_target(&config.target_url)?;
    if !in_range(config.max_body_mb, 1, 2048) || !in_range(config.timeout_seconds, 1, 3600) {
        return invalid();
    }
    Ok(())
}

/// setup_step is the next screen to display. The target is entered on the first screen, so
/// drafts must carry a valid one from step 2.
pub(crate) fn validate_proxy_config_partial(
    config: &ReverseProxyConfig,
    setup_step: u8,
) -> AppResult<()> {
    if setup_step < 2 {
        return Ok(());
    }
    validate_proxy_target(&config.target_url)
}

/// nginx include file for the proxy, derived from the app name.
pub(crate) fn proxy_nginx_conf_name(name: &str) -> AppResult<String> {
    Ok(format!("runory-proxy-{}.conf", service_slug(name)?))
}

/// Where the site is installed. Stored as the app's path so the existing per-profile path
/// uniqueness also prevents two proxies from claiming the same site file.
pub(crate) fn proxy_site_path(name: &str) -> AppResult<String> {
    Ok(format!(
        "/etc/nginx/conf.d/{}",
        proxy_nginx_conf_name(name)?
    ))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::ProxyHostHeader;

    pub(crate) fn sample_config() -> ReverseProxyConfig {
        ReverseProxyConfig {
            port: 80,
            server_name: "api.example.com".into(),
            target_url: "http://127.0.0.1:3000".into(),
            host_header: ProxyHostHeader::Original,
            websocket: true,
            max_body_mb: 50,
            timeout_seconds: 60,
        }
    }

    #[test]
    fn targets_accept_http_urls_and_reject_directive_injection() {
        for ok in [
            "http://127.0.0.1:3000",
            "https://api.example.com",
            "http://backend:8080/v1/",
            "http://localhost",
            "https://a-b.example.com:8443/app_1/x.y",
        ] {
            assert!(validate_proxy_target(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "ftp://x.com",
            "127.0.0.1:3000",
            "http://",
            "http://user@host",
            "http://host:99999",
            "http://host:",
            "http://host/a b",
            "http://host/a;b",
            "http://host/$x",
            "http://host/a?x=1",
            "http://host/../etc",
            "http://-host",
            "http://ho st",
            "http://host\nproxy_pass http://evil",
            "http://[::1]:80",
        ] {
            assert!(validate_proxy_target(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn server_names_allow_wildcards_but_not_injection() {
        assert!(validate_proxy_server_name("").is_ok());
        assert!(validate_proxy_server_name("a.com *.a.com localhost").is_ok());
        assert!(validate_proxy_server_name("a.com; root /;").is_err());
    }

    #[test]
    fn config_validates_port_target_and_limits() {
        let mut config = sample_config();
        assert!(validate_proxy_config(&config).is_ok());
        config.port = 0;
        assert!(validate_proxy_config(&config).is_err());
        config.port = 80;
        config.max_body_mb = 0;
        assert!(validate_proxy_config(&config).is_err());
        config.max_body_mb = 50;
        config.timeout_seconds = 4000;
        assert!(validate_proxy_config(&config).is_err());
        config.timeout_seconds = 60;
        config.target_url = "http://x;y".into();
        assert!(validate_proxy_config(&config).is_err());
    }

    #[test]
    fn site_path_is_derived_from_the_name() {
        assert_eq!(
            proxy_site_path("My API").unwrap(),
            "/etc/nginx/conf.d/runory-proxy-my-api.conf"
        );
        assert!(proxy_site_path("!!!").is_err());
    }
}
