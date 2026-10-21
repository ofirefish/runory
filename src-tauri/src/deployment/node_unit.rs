use crate::domain::{AppResult, NodePackageManager, NodeProjectConfig, NodeStartMode};

use super::node_validation::{validate_node_config, validate_node_path};
use super::python_unit::unit_value;
use super::validation::validate_app_name;

pub(super) fn package_manager_label(manager: NodePackageManager) -> &'static str {
    match manager {
        NodePackageManager::Npm => "npm",
        NodePackageManager::Yarn => "yarn",
        NodePackageManager::Pnpm => "pnpm",
    }
}

pub(super) fn resolve_entry_path(project_path: &str, entry_file: &str) -> String {
    format!("{}/{entry_file}", project_path.trim_end_matches('/'))
}

/// `node_exec` and `package_manager_exec` are absolute paths resolved on the remote host; systemd
/// cannot search PATH for `ExecStart`. They are validated again before reaching the unit.
pub(super) fn render_node_unit(
    name: &str,
    project_path: &str,
    node_exec: &str,
    package_manager_exec: Option<&str>,
    node: &NodeProjectConfig,
    service: &str,
) -> AppResult<String> {
    validate_node_config(node)?;
    validate_node_path(project_path)?;
    validate_node_path(node_exec)?;
    validate_app_name(name)?;
    let args = node.app_args.split_whitespace().collect::<Vec<_>>();
    let mut words = Vec::new();
    match node.start_mode {
        NodeStartMode::Script => {
            let manager = package_manager_exec.ok_or(crate::domain::AppError::InvalidOperation)?;
            validate_node_path(manager)?;
            words.push(unit_value(manager, true)?);
            words.push(unit_value("run", true)?);
            words.push(unit_value(&node.script, true)?);
            if !args.is_empty() {
                // npm only forwards arguments placed after `--`; yarn and pnpm forward them directly.
                if node.package_manager == NodePackageManager::Npm {
                    words.push(unit_value("--", true)?);
                }
            }
        }
        NodeStartMode::Entry => {
            words.push(unit_value(node_exec, true)?);
            words.push(unit_value(
                &resolve_entry_path(project_path, &node.entry_file),
                true,
            )?);
        }
    }
    for arg in args {
        words.push(unit_value(arg, true)?);
    }
    // Scripts spawn `node` through PATH, so the selected installation must come first.
    let node_dir = node_exec.rsplit_once('/').map_or("", |(dir, _)| dir);
    let path_env = format!("PATH={node_dir}:/usr/local/bin:/usr/bin:/bin");
    // 143 is the normal exit status after SIGTERM, so `systemctl stop` is not a failure.
    Ok(format!(
        "[Unit]\nDescription=Runory Node {}\nAfter=network.target\n\n[Service]\nType=simple\nUser={}\nWorkingDirectory={}\nEnvironment={}\nEnvironment={}\nEnvironment={}\nExecStart={}\nSuccessExitStatus=143\nRestart=on-failure\nRestartSec=3\n\n[Install]\nWantedBy=multi-user.target\n# runory-unit:{}\n",
        name.replace('%', "%%"),
        node.run_user,
        unit_value(project_path, false)?,
        unit_value(&path_env, false)?,
        unit_value(&format!("NODE_ENV={}", node.node_env), false)?,
        unit_value(&format!("PORT={}", node.port), false)?,
        words.join(" "),
        service
    ))
}

#[cfg(test)]
mod tests {
    use super::super::node_validation::tests::sample_config;
    use super::*;

    #[test]
    fn renders_script_unit_with_resolved_package_manager() {
        let unit = render_node_unit(
            "web",
            "/srv/web",
            "/usr/local/n/bin/node",
            Some("/usr/local/n/bin/npm"),
            &sample_config(),
            "runory-node-web.service",
        )
        .unwrap();
        assert!(unit.contains("WorkingDirectory=\"/srv/web\""));
        assert!(unit.contains("Environment=\"PATH=/usr/local/n/bin:/usr/local/bin:/usr/bin:/bin\""));
        assert!(unit.contains("Environment=\"NODE_ENV=production\""));
        assert!(unit.contains("Environment=\"PORT=3000\""));
        assert!(unit.contains("ExecStart=\"/usr/local/n/bin/npm\" \"run\" \"start\"\n"));
        assert!(unit.contains("User=www"));
        assert!(unit.ends_with("# runory-unit:runory-node-web.service\n"));
    }

    #[test]
    fn npm_forwards_arguments_after_double_dash_but_yarn_does_not() {
        let mut config = sample_config();
        config.app_args = "--host=127.0.0.1".into();
        let npm = render_node_unit(
            "w",
            "/srv/w",
            "/usr/bin/node",
            Some("/usr/bin/npm"),
            &config,
            "s",
        )
        .unwrap();
        assert!(npm.contains("\"run\" \"start\" \"--\" \"--host=127.0.0.1\""));
        config.package_manager = NodePackageManager::Yarn;
        let yarn = render_node_unit(
            "w",
            "/srv/w",
            "/usr/bin/node",
            Some("/usr/bin/yarn"),
            &config,
            "s",
        )
        .unwrap();
        assert!(yarn.contains("\"run\" \"start\" \"--host=127.0.0.1\""));
    }

    #[test]
    fn renders_entry_unit_without_a_package_manager() {
        let mut config = sample_config();
        config.start_mode = NodeStartMode::Entry;
        config.entry_file = "dist/server.js".into();
        let unit =
            render_node_unit("api", "/srv/api", "/usr/bin/node", None, &config, "s").unwrap();
        assert!(unit.contains("ExecStart=\"/usr/bin/node\" \"/srv/api/dist/server.js\"\n"));
    }

    #[test]
    fn invalid_input_fails_before_rendering() {
        let config = sample_config();
        // Script mode needs a resolved package manager.
        assert!(render_node_unit("w", "/srv/w", "/usr/bin/node", None, &config, "s").is_err());
        assert!(render_node_unit(
            "w",
            "/srv/w",
            "/usr/bin/node;id",
            Some("/usr/bin/npm"),
            &config,
            "s"
        )
        .is_err());
        let mut bad = sample_config();
        bad.script = "a b".into();
        assert!(render_node_unit(
            "w",
            "/srv/w",
            "/usr/bin/node",
            Some("/usr/bin/npm"),
            &bad,
            "s"
        )
        .is_err());
    }
}
