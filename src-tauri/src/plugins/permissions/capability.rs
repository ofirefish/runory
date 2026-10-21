use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    HostRead,
    HostMetadataRead,
    SshExec,
    SshExecSudo,
    SshShell,
    RemoteFsRead,
    RemoteFsWrite,
    RemoteFsUpload,
    RemoteFsDelete,
    SecretReference,
    NetworkHttps,
}

impl PluginCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HostRead => "host.read",
            Self::HostMetadataRead => "host.metadata.read",
            Self::SshExec => "ssh.exec",
            Self::SshExecSudo => "ssh.exec.sudo",
            Self::SshShell => "ssh.shell",
            Self::RemoteFsRead => "remote.fs.read",
            Self::RemoteFsWrite => "remote.fs.write",
            Self::RemoteFsUpload => "remote.fs.upload",
            Self::RemoteFsDelete => "remote.fs.delete",
            Self::SecretReference => "secret.reference",
            Self::NetworkHttps => "network.https",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "host.read" => Ok(Self::HostRead),
            "host.metadata.read" => Ok(Self::HostMetadataRead),
            "ssh.exec" => Ok(Self::SshExec),
            "ssh.exec.sudo" => Ok(Self::SshExecSudo),
            "ssh.shell" => Ok(Self::SshShell),
            "remote.fs.read" => Ok(Self::RemoteFsRead),
            "remote.fs.write" => Ok(Self::RemoteFsWrite),
            "remote.fs.upload" => Ok(Self::RemoteFsUpload),
            "remote.fs.delete" => Ok(Self::RemoteFsDelete),
            "secret.reference" => Ok(Self::SecretReference),
            "network.https" => Ok(Self::NetworkHttps),
            other => Err(format!("unknown permission: {other}")),
        }
    }
}
