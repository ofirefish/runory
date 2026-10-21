//! Renders the nginx site for a reverse proxy project.

use crate::domain::{AppResult, ProxyHostHeader, ReverseProxyConfig};

use super::proxy_validation::{proxy_nginx_conf_name, validate_proxy_config};
use super::validation::validate_app_name;

pub(super) fn render_proxy_site(name: &str, config: &ReverseProxyConfig) -> AppResult<String> {
    validate_app_name(name)?;
    validate_proxy_config(config)?;
    let conf_name = proxy_nginx_conf_name(name)?;
    let server_name = if config.server_name.trim().is_empty() {
        "_".to_string()
    } else {
        config
            .server_name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let host = match config.host_header {
        ProxyHostHeader::Original => "$host",
        ProxyHostHeader::Target => "$proxy_host",
    };
    // `Connection "upgrade"` is sent on every request so no http-level `map` is needed; a second
    // site defining the same map variable would make nginx refuse the configuration.
    let websocket = if config.websocket {
        "        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection \"upgrade\";\n"
    } else {
        ""
    };
    let sni = if config.target_url.starts_with("https://") {
        "        proxy_ssl_server_name on;\n"
    } else {
        ""
    };
    Ok(format!(
        "# runory-proxy:{conf_name}\nserver {{\n    listen {port};\n    server_name {server_name};\n    client_max_body_size {body}m;\n\n    location / {{\n        proxy_pass {target};\n        proxy_http_version 1.1;\n        proxy_set_header Host {host};\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n{websocket}        proxy_connect_timeout {timeout}s;\n        proxy_send_timeout {timeout}s;\n        proxy_read_timeout {timeout}s;\n{sni}    }}\n}}\n",
        port = config.port,
        body = config.max_body_mb,
        target = config.target_url,
        timeout = config.timeout_seconds,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::proxy_validation::tests::sample_config;
    use super::*;

    #[test]
    fn renders_proxy_with_forwarding_headers_and_websocket() {
        let conf = render_proxy_site("API", &sample_config()).unwrap();
        assert!(conf.starts_with("# runory-proxy:runory-proxy-api.conf\n"));
        assert!(conf.contains("listen 80;"));
        assert!(conf.contains("server_name api.example.com;"));
        assert!(conf.contains("proxy_pass http://127.0.0.1:3000;"));
        assert!(conf.contains("proxy_set_header Host $host;"));
        assert!(conf.contains("proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;"));
        assert!(conf.contains("proxy_set_header Upgrade $http_upgrade;"));
        assert!(conf.contains("client_max_body_size 50m;"));
        assert!(conf.contains("proxy_read_timeout 60s;"));
        assert!(!conf.contains("proxy_ssl_server_name"));
    }

    #[test]
    fn target_host_header_https_and_no_websocket() {
        let mut config = sample_config();
        config.host_header = ProxyHostHeader::Target;
        config.websocket = false;
        config.server_name = String::new();
        config.target_url = "https://api.internal/v1/".into();
        let conf = render_proxy_site("api", &config).unwrap();
        assert!(conf.contains("server_name _;"));
        assert!(conf.contains("proxy_set_header Host $proxy_host;"));
        assert!(conf.contains("proxy_pass https://api.internal/v1/;"));
        assert!(conf.contains("proxy_ssl_server_name on;"));
        assert!(!conf.contains("Upgrade"));
    }

    #[test]
    fn injection_attempts_fail_before_rendering() {
        let mut config = sample_config();
        config.target_url = "http://x; proxy_pass http://evil".into();
        assert!(render_proxy_site("api", &config).is_err());
        let mut config = sample_config();
        config.server_name = "a.com; root /;".into();
        assert!(render_proxy_site("api", &config).is_err());
    }
}
