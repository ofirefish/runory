use crate::plugins::permissions::capability::PluginCapability;

pub fn action_required_capabilities(
    action_id: &str,
    uses_sudo: bool,
) -> Result<Vec<PluginCapability>, String> {
    let mut caps = match action_id {
        "system.detect"
        | "system.command_exists"
        | "system.disk"
        | "system.memory"
        | "network.tcp_check"
        | "http.check" => vec![PluginCapability::HostRead],
        "system.preflight_report" => vec![PluginCapability::HostRead, PluginCapability::SshExec],
        "python.detect" => vec![PluginCapability::HostRead, PluginCapability::SshExec],
        "ssh.command" | "git.clone" | "git.rev_parse" | "git.fetch" | "git.checkout"
        | "python.venv" | "python.pip.install" | "database.migration" | "systemd.status" => {
            vec![PluginCapability::SshExec]
        }
        "changeset.checkpoint" => {
            vec![PluginCapability::SshExec, PluginCapability::RemoteFsRead]
        }
        "ssh.shell" => vec![PluginCapability::SshShell],
        "remote.fs.read" => vec![PluginCapability::RemoteFsRead],
        "remote.fs.write" | "remote.fs.mkdir" | "remote.fs.symlink" | "template.render" => {
            vec![PluginCapability::RemoteFsWrite]
        }
        "remote.fs.write_env" => vec![PluginCapability::RemoteFsWrite],
        "remote.fs.upload" => vec![PluginCapability::RemoteFsUpload],
        "remote.fs.remove" => vec![PluginCapability::RemoteFsDelete],
        "systemd.install" => vec![
            PluginCapability::RemoteFsWrite,
            PluginCapability::SshExecSudo,
        ],
        "release.activate" => vec![
            PluginCapability::RemoteFsWrite,
            PluginCapability::SshExec,
            PluginCapability::SshExecSudo,
            PluginCapability::HostRead,
        ],
        "systemd.daemon_reload"
        | "systemd.enable"
        | "systemd.restart"
        | "systemd.reload"
        | "nginx.test"
        | "nginx.reload" => vec![PluginCapability::SshExec, PluginCapability::SshExecSudo],
        other => return Err(format!("unknown action: {other}")),
    };
    if action_id == "ssh.command" && uses_sudo {
        caps.push(PluginCapability::SshExecSudo);
    }
    Ok(caps)
}
