//! Renders the nginx site for a static HTML project.

use crate::domain::{AppResult, HtmlProjectConfig};

use super::html_validation::{html_nginx_conf_name, validate_html_config, validate_html_path};
use super::validation::validate_app_name;

fn web_root(project_path: &str, root_dir: &str) -> String {
    let base = project_path.trim_end_matches('/');
    if root_dir.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{}", root_dir.trim_matches('/'))
    }
}

pub(super) fn render_html_site(
    name: &str,
    project_path: &str,
    config: &HtmlProjectConfig,
) -> AppResult<String> {
    validate_app_name(name)?;
    validate_html_path(project_path)?;
    validate_html_config(config)?;
    let conf_name = html_nginx_conf_name(name)?;
    let server_name = if config.server_name.trim().is_empty() {
        "_".to_string()
    } else {
        config
            .server_name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let index_files = config.index_files.split_whitespace().collect::<Vec<_>>();
    let try_files = if config.spa_fallback {
        format!("try_files $uri $uri/ /{};", index_files[0])
    } else {
        "try_files $uri $uri/ =404;".to_string()
    };
    let gzip = if config.gzip {
        "    gzip on;\n    gzip_min_length 1024;\n    gzip_types text/plain text/css text/xml application/javascript application/json application/xml image/svg+xml;\n\n"
    } else {
        ""
    };
    let cache = if config.cache_static {
        "    location ~* \\.(?:css|js|mjs|png|jpe?g|gif|ico|svg|webp|avif|woff2?|ttf|eot)$ {\n        expires 7d;\n        add_header Cache-Control \"public\";\n        access_log off;\n    }\n\n"
    } else {
        ""
    };
    Ok(format!(
        "# runory-html:{conf_name}\nserver {{\n    listen {port};\n    server_name {server_name};\n    root {root};\n    index {index};\n    charset utf-8;\n\n{gzip}    location / {{\n        {try_files}\n    }}\n\n{cache}    location ~ /\\.(?!well-known) {{\n        deny all;\n    }}\n}}\n",
        port = config.port,
        root = web_root(project_path, &config.root_dir),
        index = index_files.join(" "),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::html_validation::tests::sample_config;
    use super::*;

    #[test]
    fn renders_spa_site_with_gzip_and_cache() {
        let conf = render_html_site("Docs", "/srv/docs", &sample_config()).unwrap();
        assert!(conf.starts_with("# runory-html:runory-html-docs.conf\n"));
        assert!(conf.contains("listen 8080;"));
        assert!(conf.contains("server_name example.com *.example.com;"));
        assert!(conf.contains("root /srv/docs/dist;"));
        assert!(conf.contains("index index.html index.htm;"));
        assert!(conf.contains("try_files $uri $uri/ /index.html;"));
        assert!(conf.contains("gzip on;"));
        assert!(conf.contains("expires 7d;"));
        assert!(conf.contains("deny all;"));
    }

    #[test]
    fn plain_site_has_no_fallback_gzip_or_cache() {
        let mut config = sample_config();
        config.spa_fallback = false;
        config.gzip = false;
        config.cache_static = false;
        config.server_name = String::new();
        config.root_dir = String::new();
        let conf = render_html_site("docs", "/srv/docs/", &config).unwrap();
        assert!(conf.contains("server_name _;"));
        assert!(conf.contains("root /srv/docs;"));
        assert!(conf.contains("try_files $uri $uri/ =404;"));
        assert!(!conf.contains("gzip"));
        assert!(!conf.contains("expires"));
    }

    #[test]
    fn spa_fallback_uses_the_first_index_file() {
        let mut config = sample_config();
        config.index_files = "app.html index.html".into();
        let conf = render_html_site("docs", "/srv/docs", &config).unwrap();
        assert!(conf.contains("try_files $uri $uri/ /app.html;"));
    }

    #[test]
    fn injection_attempts_fail_before_rendering() {
        let mut config = sample_config();
        config.server_name = "a.com; root /;".into();
        assert!(render_html_site("docs", "/srv/docs", &config).is_err());
        let mut config = sample_config();
        config.root_dir = "dist; autoindex on".into();
        assert!(render_html_site("docs", "/srv/docs", &config).is_err());
        assert!(render_html_site("docs", "/srv/my docs", &sample_config()).is_err());
    }
}
