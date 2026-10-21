pub mod agent_tools;
pub mod host_menu;
pub mod workspace;

pub use agent_tools::{list_enabled_plugin_agent_tools, PluginAgentToolDto};
pub use host_menu::contribution_menus;
pub use workspace::{list_plugin_workspace_entries, PluginWorkspaceEntryDto};

use crate::plugins::error::PluginResult;
use crate::plugins::workflow::expressions::evaluate_when;

/// Evaluate a host-menu `when` expression against host facts (including tags).
pub fn host_menu_matches(
    when: Option<&str>,
    host_os: Option<&str>,
    host_arch: Option<&str>,
    tags: &[String],
) -> PluginResult<bool> {
    match when {
        None => Ok(true),
        Some(expr) => evaluate_when(expr, host_os, host_arch, tags),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_when_always_matches() {
        assert!(host_menu_matches(None, Some("linux"), None, &[]).unwrap());
    }

    #[test]
    fn linux_when() {
        assert!(host_menu_matches(Some("host.os == 'linux'"), Some("linux"), None, &[]).unwrap());
        assert!(
            !host_menu_matches(Some("host.os == 'linux'"), Some("windows"), None, &[]).unwrap()
        );
    }
}
