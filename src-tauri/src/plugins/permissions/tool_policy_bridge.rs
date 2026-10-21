//! Bridge plugin planned actions onto Core `ToolPolicy` (fail-closed).
//! Plugins never receive ToolPolicy handles; only Core evaluates them.

use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::workflow::actions::PlannedChange;
use crate::plugins::workflow::ast::PrivilegeMode;
use crate::tools::{native_descriptors, NativeToolName, ToolPolicy, ToolPolicyDecision};

/// Map a planned change onto a native tool when one exists.
pub fn native_tool_for_planned_change(change: &PlannedChange) -> Option<NativeToolName> {
    match change {
        PlannedChange::RemoteFileWrite { .. } => None, // needs FilePatch semantics
        PlannedChange::RemoteFileDelete { .. } => None,
        PlannedChange::PackageInstall { .. } => None,
        PlannedChange::DatabaseMigration { .. } => None,
        PlannedChange::ServiceRestart { .. } => Some(NativeToolName::ServiceRestart),
        PlannedChange::RemoteCommand {
            executable,
            args,
            privilege: _,
        } => map_remote_command(executable, args),
    }
}

fn map_remote_command(executable: &str, args: &[String]) -> Option<NativeToolName> {
    let exe = executable.to_ascii_lowercase();
    match exe.as_str() {
        "nginx" if args.iter().any(|a| a == "-t" || a == "-T") => Some(NativeToolName::NginxTest),
        "nginx" if args.iter().any(|a| a == "-s") => Some(NativeToolName::NginxReload),
        "nginx" if args.iter().any(|a| a == "-v" || a == "-V") => {
            Some(NativeToolName::TerminalExecReadonly)
        }
        "systemctl" => {
            let action = args.first().map(|s| s.as_str())?;
            let service = args.get(1).map(|s| s.as_str());
            match action {
                "reload" if service == Some("nginx") => Some(NativeToolName::NginxReload),
                "reload" => Some(NativeToolName::ServiceReload),
                "restart" => Some(NativeToolName::ServiceRestart),
                "status" | "is-active" | "show" | "cat" | "--version" => {
                    Some(NativeToolName::ServiceStatus)
                }
                _ => None,
            }
        }
        "git" => match args.first().map(|s| s.as_str()) {
            Some("--version" | "rev-parse" | "status" | "log" | "show") => {
                Some(NativeToolName::TerminalExecReadonly)
            }
            _ => None,
        },
        "python" | "python3" | "pip" | "pip3" => {
            if args.iter().any(|a| a == "--version" || a == "-V")
                || args.first().map(|s| s.as_str()) == Some("--version")
            {
                Some(NativeToolName::TerminalExecReadonly)
            } else if is_manage_py_check(args) {
                Some(NativeToolName::TerminalExecReadonly)
            } else {
                None
            }
        }
        "uname" | "whoami" | "id" | "hostname" | "pwd" | "date" | "uptime" | "true" | "false"
        | "echo" | "cat" | "ls" | "which" | "journalctl" | "curl" | "nc" => {
            Some(NativeToolName::TerminalExecReadonly)
        }
        path if path.contains("/python")
            || path.ends_with("python3")
            || path.ends_with("python") =>
        {
            if is_manage_py_check(args) || args.iter().any(|a| a == "--version") {
                Some(NativeToolName::TerminalExecReadonly)
            } else {
                None
            }
        }
        exe if exe.starts_with("python") => {
            if args.iter().any(|a| a == "--version" || a == "-V") || is_manage_py_check(args) {
                Some(NativeToolName::TerminalExecReadonly)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_manage_py_check(args: &[String]) -> bool {
    args.first().map(|s| s.as_str()) == Some("manage.py")
        && args.get(1).map(|s| s.as_str()) == Some("check")
}

fn is_manage_py_collectstatic(args: &[String]) -> bool {
    args.first().map(|s| s.as_str()) == Some("manage.py")
        && args.get(1).map(|s| s.as_str()) == Some("collectstatic")
}

fn is_plugin_managed_python_path(executable: &str, args: &[String]) -> bool {
    let exe = executable.to_ascii_lowercase();
    (exe.contains("/python") || exe.ends_with("python") || exe.ends_with("python3"))
        && is_manage_py_collectstatic(args)
}

/// Whether this planned change is considered a write / mutative operation.
pub fn planned_change_is_mutating(change: &PlannedChange) -> bool {
    match change {
        PlannedChange::RemoteFileWrite { .. }
        | PlannedChange::RemoteFileDelete { .. }
        | PlannedChange::PackageInstall { .. }
        | PlannedChange::DatabaseMigration { .. }
        | PlannedChange::ServiceRestart { .. } => true,
        PlannedChange::RemoteCommand {
            executable,
            args,
            privilege,
        } => match map_remote_command(executable, args) {
            Some(
                NativeToolName::NginxTest
                | NativeToolName::TerminalExecReadonly
                | NativeToolName::ServiceStatus
                | NativeToolName::FileInspect,
            ) => false,
            Some(_) => true,
            None => {
                matches!(privilege, PrivilegeMode::Sudo)
                    || !matches!(
                        executable.to_ascii_lowercase().as_str(),
                        "uname"
                            | "whoami"
                            | "id"
                            | "hostname"
                            | "pwd"
                            | "date"
                            | "uptime"
                            | "true"
                            | "false"
                            | "echo"
                            | "cat"
                            | "ls"
                            | "which"
                            | "printf"
                            | "test"
                            | "command"
                            | "python3"
                            | "python"
                            | "git"
                            | "pip"
                            | "pip3"
                    ) && !executable.to_ascii_lowercase().starts_with("python")
            }
        },
    }
}

/// Evaluate ToolPolicy for a planned change.
///
/// - Read tools: `ToolPolicy::foundation()`
/// - Write tools at plan-time: `approved_repair()` allowlist check (must be known repair tool)
/// - Write tools at execute-time after ChangeSet approval: `approved_repair()`
/// - Plugin-owned mutative ops (`RemoteFileWrite`, `DatabaseMigration`, …) are
///   capability + risk gated and do not require a native ToolPolicy mapping.
pub fn require_tool_policy_for_change(
    change: &PlannedChange,
    _write_execution: bool,
) -> PluginResult<()> {
    match change {
        PlannedChange::RemoteFileWrite { .. }
        | PlannedChange::RemoteFileDelete { .. }
        | PlannedChange::PackageInstall { .. }
        | PlannedChange::DatabaseMigration { .. } => return Ok(()),
        PlannedChange::ServiceRestart { .. } | PlannedChange::RemoteCommand { .. } => {}
    }

    let mutating = planned_change_is_mutating(change);
    let Some(tool) = native_tool_for_planned_change(change) else {
        if mutating {
            // Allow known plugin-managed verbs that lack ChangeStepDrafts.
            if let PlannedChange::RemoteCommand {
                executable, args, ..
            } = change
            {
                if is_plugin_managed_systemctl(executable, args)
                    || is_plugin_managed_python_path(executable, args)
                    || is_plugin_managed_git(executable, args)
                    || is_plugin_managed_python_setup(executable, args)
                {
                    return Ok(());
                }
            }
            return Err(PluginError::PolicyDenied);
        }
        // Unmapped read-only remote commands remain capability-gated only.
        return Ok(());
    };

    let descriptor = native_descriptors()
        .into_iter()
        .find(|d| d.name == tool)
        .ok_or(PluginError::PolicyDenied)?;

    let policy = if mutating {
        ToolPolicy::approved_repair()
    } else {
        ToolPolicy::foundation()
    };

    match policy.evaluate(&descriptor) {
        ToolPolicyDecision::Allowed => Ok(()),
        ToolPolicyDecision::Blocked { .. } => Err(PluginError::PolicyDenied),
    }
}

fn is_plugin_managed_systemctl(executable: &str, args: &[String]) -> bool {
    if executable.to_ascii_lowercase() != "systemctl" {
        return false;
    }
    matches!(
        args.first().map(|s| s.as_str()),
        Some("enable" | "daemon-reload" | "disable" | "start")
    )
}

fn is_plugin_managed_git(executable: &str, args: &[String]) -> bool {
    executable.to_ascii_lowercase() == "git"
        && matches!(
            args.first().map(|s| s.as_str()),
            Some("clone" | "fetch" | "checkout" | "pull")
        )
}

fn is_plugin_managed_python_setup(executable: &str, args: &[String]) -> bool {
    let exe = executable.to_ascii_lowercase();
    let is_python =
        exe == "python" || exe == "python3" || exe.starts_with("python") || exe.contains("/python");
    if !is_python {
        return false;
    }
    // python -m venv PATH  OR  python -m pip install ...
    args.first().map(|s| s.as_str()) == Some("-m")
        && matches!(args.get(1).map(|s| s.as_str()), Some("venv" | "pip"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_test_maps_and_passes_foundation() {
        let change = PlannedChange::RemoteCommand {
            executable: "nginx".into(),
            args: vec!["-t".into()],
            privilege: PrivilegeMode::Sudo,
        };
        assert_eq!(
            native_tool_for_planned_change(&change),
            Some(NativeToolName::NginxTest)
        );
        assert!(!planned_change_is_mutating(&change));
        require_tool_policy_for_change(&change, false).expect("allowed");
    }

    #[test]
    fn systemctl_reload_nginx_is_mutating_and_allowlisted() {
        let change = PlannedChange::RemoteCommand {
            executable: "systemctl".into(),
            args: vec!["reload".into(), "nginx".into()],
            privilege: PrivilegeMode::Sudo,
        };
        assert!(planned_change_is_mutating(&change));
        require_tool_policy_for_change(&change, false).expect("repair allowlist");
    }

    #[test]
    fn unknown_mutating_command_denied() {
        let change = PlannedChange::RemoteCommand {
            executable: "rm".into(),
            args: vec!["-rf".into(), "/tmp/x".into()],
            privilege: PrivilegeMode::None,
        };
        // rm is mutating but not a native repair tool → PolicyDenied
        assert!(require_tool_policy_for_change(&change, false).is_err());
    }
}
