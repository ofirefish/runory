//! Renders the three files a PHP project needs on the remote host: a dedicated
//! php-fpm pool configuration, the systemd unit that supervises it, and the nginx site.
//!
//! The renderers emit two placeholders that only the remote host can resolve:
//! `@FPM_BIN@` (absolute php-fpm path) and `@NGINX_USER@` (socket owner).

use crate::domain::{AppResult, PhpProcessManager, PhpProjectConfig, PhpRewriteMode};

use super::php_validation::{php_service_name, validate_php_config, validate_php_path};
use super::validation::validate_app_name;

pub(super) const FPM_BIN_PLACEHOLDER: &str = "@FPM_BIN@";
pub(super) const NGINX_USER_PLACEHOLDER: &str = "@NGINX_USER@";

/// Marker line that separates files when they are streamed to the remote installer.
pub(super) const FILE_MARKER: &str = "#--runory-file:";

pub(super) struct PhpFiles {
    pub fpm_conf: String,
    pub unit: String,
    pub nginx_conf: String,
}

impl PhpFiles {
    /// Single stdin payload; the remote script splits it on `FILE_MARKER` lines.
    pub fn bundle(&self) -> String {
        format!(
            "{FILE_MARKER}fpm.conf\n{}\n{FILE_MARKER}unit\n{}\n{FILE_MARKER}nginx.conf\n{}\n",
            self.fpm_conf.trim_end(),
            self.unit.trim_end(),
            self.nginx_conf.trim_end()
        )
    }
}

fn socket_path(service: &str) -> String {
    format!("/run/{}.sock", service.trim_end_matches(".service"))
}

fn web_root(project_path: &str, run_dir: &str) -> String {
    let base = project_path.trim_end_matches('/');
    if run_dir.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{}", run_dir.trim_matches('/'))
    }
}

/// php-fpm requires `start_servers` and both spare limits to stay within `max_children`.
fn dynamic_limits(max_children: u16) -> (u16, u16, u16) {
    let start = (max_children / 4).clamp(1, max_children);
    let max_spare = (max_children / 2).clamp(start, max_children);
    (start, 1, max_spare)
}

pub(super) fn render_php_files(
    name: &str,
    project_path: &str,
    config: &PhpProjectConfig,
) -> AppResult<PhpFiles> {
    validate_app_name(name)?;
    validate_php_path(project_path)?;
    validate_php_config(config)?;
    let service = php_service_name(name)?;
    Ok(PhpFiles {
        fpm_conf: render_fpm_conf(project_path, config, &service),
        unit: render_unit(name, project_path, &service),
        nginx_conf: render_nginx_site(project_path, config, &service),
    })
}

fn render_fpm_conf(project_path: &str, config: &PhpProjectConfig, service: &str) -> String {
    let project = project_path.trim_end_matches('/');
    let mut pool = String::new();
    match config.process_manager {
        PhpProcessManager::Static => {
            pool.push_str(&format!(
                "pm = static\npm.max_children = {}\n",
                config.max_children
            ));
        }
        PhpProcessManager::Ondemand => {
            pool.push_str(&format!(
                "pm = ondemand\npm.max_children = {}\npm.process_idle_timeout = 10s\n",
                config.max_children
            ));
        }
        PhpProcessManager::Dynamic => {
            let (start, min_spare, max_spare) = dynamic_limits(config.max_children);
            pool.push_str(&format!(
                "pm = dynamic\npm.max_children = {}\npm.start_servers = {start}\npm.min_spare_servers = {min_spare}\npm.max_spare_servers = {max_spare}\n",
                config.max_children
            ));
        }
    }
    let open_basedir = if config.open_basedir {
        format!("php_admin_value[open_basedir] = {project}/:/tmp/:/usr/share/php/:/dev/urandom\n")
    } else {
        String::new()
    };
    format!(
        "[global]\ndaemonize = no\nerror_log = /proc/self/fd/2\n\n[runory]\nuser = {user}\nlisten = {socket}\nlisten.owner = {NGINX_USER_PLACEHOLDER}\nlisten.group = {NGINX_USER_PLACEHOLDER}\nlisten.mode = 0660\n{pool}chdir = {project}\ncatch_workers_output = yes\nphp_admin_value[memory_limit] = {memory}M\nphp_admin_value[max_execution_time] = {exec}\nphp_admin_value[upload_max_filesize] = {upload}M\nphp_admin_value[post_max_size] = {upload}M\n{open_basedir}",
        user = config.run_user,
        socket = socket_path(service),
        memory = config.memory_limit_mb,
        exec = config.max_execution_seconds,
        upload = config.upload_max_mb,
    )
}

fn render_unit(name: &str, project_path: &str, service: &str) -> String {
    let project = project_path.trim_end_matches('/');
    // The master runs as root and drops to `user` per pool; the fpm config lives with the project.
    format!(
        "[Unit]\nDescription=Runory PHP {}\nAfter=network.target\n\n[Service]\nType=simple\nExecStart={FPM_BIN_PLACEHOLDER} --nodaemonize --fpm-config {project}/.runory/fpm.conf\nExecReload=/bin/kill -USR2 $MAINPID\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\n# runory-unit:{service}\n",
        name.replace('%', "%%").replace(['\n', '\r'], " "),
    )
}

fn render_nginx_site(project_path: &str, config: &PhpProjectConfig, service: &str) -> String {
    let server_name = if config.server_name.trim().is_empty() {
        "_".to_string()
    } else {
        config
            .server_name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let root = web_root(project_path, &config.run_dir);
    let try_files = match config.rewrite {
        PhpRewriteMode::None => "try_files $uri $uri/ =404;",
        PhpRewriteMode::FrontController => "try_files $uri $uri/ /index.php?$query_string;",
    };
    format!(
        "# runory-php:{service}\nserver {{\n    listen {port};\n    server_name {server_name};\n    root {root};\n    index index.php index.html;\n    client_max_body_size {upload}m;\n\n    location / {{\n        {try_files}\n    }}\n\n    location ~ \\.php$ {{\n        include fastcgi_params;\n        fastcgi_pass unix:{socket};\n        fastcgi_index index.php;\n        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;\n        fastcgi_param DOCUMENT_ROOT $realpath_root;\n    }}\n\n    location ~ /\\.(?!well-known) {{\n        deny all;\n    }}\n}}\n",
        port = config.port,
        upload = config.upload_max_mb,
        socket = socket_path(service),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PhpProjectConfig {
        serde_json::from_str(
            r#"{"phpBin":"php8.2","port":8080,"serverName":"example.com","runDir":"public","rewrite":"frontController","runUser":"www-data"}"#,
        )
        .unwrap()
    }

    #[test]
    fn missing_tuning_fields_use_defaults() {
        let config = config();
        assert_eq!(
            (
                config.max_children,
                config.memory_limit_mb,
                config.max_execution_seconds,
                config.upload_max_mb
            ),
            (8, 256, 60, 50)
        );
        assert!(config.open_basedir && config.composer_install && config.auto_start);
    }

    #[test]
    fn renders_pool_with_isolation_and_limits() {
        let files = render_php_files("Shop", "/srv/shop", &config()).unwrap();
        let conf = &files.fpm_conf;
        assert!(conf.contains("user = www-data"));
        assert!(conf.contains("listen = /run/runory-php-shop.sock"));
        assert!(conf.contains("pm = dynamic"));
        assert!(conf.contains("pm.max_children = 8"));
        assert!(conf.contains("pm.start_servers = 2"));
        assert!(conf.contains("pm.max_spare_servers = 4"));
        assert!(conf.contains("php_admin_value[memory_limit] = 256M"));
        assert!(conf.contains("php_admin_value[open_basedir] = /srv/shop/:/tmp/"));
        assert!(conf.contains(NGINX_USER_PLACEHOLDER));
    }

    #[test]
    fn open_basedir_can_be_disabled_and_static_pool_has_no_spare_limits() {
        let mut config = config();
        config.open_basedir = false;
        config.process_manager = PhpProcessManager::Static;
        let conf = render_php_files("shop", "/srv/shop", &config)
            .unwrap()
            .fpm_conf;
        assert!(!conf.contains("open_basedir"));
        assert!(conf.contains("pm = static"));
        assert!(!conf.contains("spare"));
    }

    #[test]
    fn dynamic_limits_stay_within_max_children() {
        for n in [1u16, 2, 3, 7, 8, 64, 512] {
            let (start, min_spare, max_spare) = dynamic_limits(n);
            assert!(
                min_spare <= start && start <= max_spare && max_spare <= n,
                "{n}"
            );
        }
    }

    #[test]
    fn renders_nginx_site_for_front_controller() {
        let nginx = render_php_files("shop", "/srv/shop/", &config())
            .unwrap()
            .nginx_conf;
        assert!(nginx.contains("listen 8080;"));
        assert!(nginx.contains("server_name example.com;"));
        assert!(nginx.contains("root /srv/shop/public;"));
        assert!(nginx.contains("try_files $uri $uri/ /index.php?$query_string;"));
        assert!(nginx.contains("fastcgi_pass unix:/run/runory-php-shop.sock;"));
        assert!(nginx.contains("client_max_body_size 50m;"));
    }

    #[test]
    fn renders_plain_site_without_rewrite_and_any_host() {
        let mut config = config();
        config.rewrite = PhpRewriteMode::None;
        config.server_name = String::new();
        config.run_dir = String::new();
        let nginx = render_php_files("shop", "/srv/shop", &config)
            .unwrap()
            .nginx_conf;
        assert!(nginx.contains("server_name _;"));
        assert!(nginx.contains("root /srv/shop;"));
        assert!(nginx.contains("try_files $uri $uri/ =404;"));
    }

    #[test]
    fn unit_runs_dedicated_master_from_project_config() {
        let unit = render_php_files("shop", "/srv/shop", &config())
            .unwrap()
            .unit;
        assert!(unit
            .contains("ExecStart=@FPM_BIN@ --nodaemonize --fpm-config /srv/shop/.runory/fpm.conf"));
        assert!(unit.contains("# runory-unit:runory-php-shop.service"));
    }

    #[test]
    fn unsafe_input_fails_before_rendering() {
        assert!(render_php_files("shop", "/srv/my shop", &config()).is_err());
        let mut bad = config();
        bad.run_dir = "../etc".into();
        assert!(render_php_files("shop", "/srv/shop", &bad).is_err());
        bad = config();
        bad.server_name = "a.com; root /;".into();
        assert!(render_php_files("shop", "/srv/shop", &bad).is_err());
    }

    #[test]
    fn bundle_separates_files_with_markers() {
        let bundle = render_php_files("shop", "/srv/shop", &config())
            .unwrap()
            .bundle();
        let names = bundle
            .lines()
            .filter_map(|line| line.strip_prefix(FILE_MARKER))
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["fpm.conf", "unit", "nginx.conf"]);
    }
}
