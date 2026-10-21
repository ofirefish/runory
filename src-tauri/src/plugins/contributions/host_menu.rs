use crate::plugins::manifest::model::{HostMenuContribution, PluginManifest};
use crate::plugins::registry::model::PluginHostMenuDto;

/// Evaluate the small safe `when` subset for host menu visibility.
pub fn host_menu_visible(
    when: Option<&str>,
    host_os: Option<&str>,
    host_arch: Option<&str>,
) -> bool {
    let Some(expr) = when else {
        return true;
    };
    let trimmed = expr.trim();
    match trimmed {
        "host.os == \"linux\"" | "host.os == 'linux'" => host_os
            .map(|value| value.eq_ignore_ascii_case("linux"))
            .unwrap_or(false),
        "host.os == \"windows\"" | "host.os == 'windows'" => host_os
            .map(|value| value.eq_ignore_ascii_case("windows"))
            .unwrap_or(false),
        "host.arch == \"x86_64\"" | "host.arch == 'x86_64'" => host_arch
            .map(|value| value == "x86_64" || value == "amd64")
            .unwrap_or(false),
        // Tag expression requires caller-supplied tags; V1 treats unknown as hidden.
        "host.tags contains \"production\"" | "host.tags contains 'production'" => false,
        _ => false,
    }
}

pub fn contribution_menus(
    manifest: &PluginManifest,
    host_os: Option<&str>,
    host_arch: Option<&str>,
) -> Vec<PluginHostMenuDto> {
    manifest
        .contributes
        .host_menus
        .iter()
        .filter(|menu| host_menu_visible(menu.when.as_deref(), host_os, host_arch))
        .map(|menu: &HostMenuContribution| {
            let title = manifest
                .contributes
                .commands
                .iter()
                .find(|command| command.id == menu.command)
                .map(|command| command.title.clone())
                .unwrap_or_else(|| menu.command.clone());
            let workflow_id = manifest
                .contributes
                .workflows
                .iter()
                .find(|workflow| workflow.id == menu.command)
                .map(|workflow| workflow.id.clone());
            PluginHostMenuDto {
                plugin_id: manifest.id.clone(),
                command: menu.command.clone(),
                group: menu.group.clone(),
                when: menu.when.clone(),
                title,
                workflow_id,
            }
        })
        .collect()
}
