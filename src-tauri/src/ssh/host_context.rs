use std::time::Duration;

use serde::Serialize;

use super::{RemoteCommand, ServerSessionManager};
use crate::domain::SessionId;

// Fixed metadata probe only: no model/user arguments, elevation, or PTY writes.
// Never source os-release: a remote file is data, not executable configuration.
const PROBE: &str = r#"
printf 'KERNEL='; uname -r 2>/dev/null
printf 'ARCH='; uname -m 2>/dev/null
printf 'USER='; id -un 2>/dev/null
printf 'UID='; id -u 2>/dev/null
cat /etc/os-release 2>/dev/null
"#;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostSystemInfo {
    pub os_release: Option<String>,
    pub version_id: Option<String>,
    pub kernel: Option<String>,
    pub architecture: Option<String>,
    pub login_user: Option<String>,
    pub login_uid: Option<u32>,
    pub login_is_root: Option<bool>,
}

impl ServerSessionManager {
    pub(crate) async fn host_system_info(&self, session_id: SessionId) -> HostSystemInfo {
        match self
            .exec(
                session_id,
                RemoteCommand::script(PROBE, vec![])
                    .with_timeout(Duration::from_secs(4))
                    .with_output_limit(16 * 1024),
            )
            .await
        {
            // Non-Linux hosts may have no os-release; preserve successful id/uname fields.
            Ok(result) => parse(&result.stdout),
            Err(_) => HostSystemInfo::default(),
        }
    }
}

fn parse(output: &str) -> HostSystemInfo {
    let value = |key: &str| {
        output.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            if name != key {
                return None;
            }
            let value = value.trim().trim_matches(['"', '\'']);
            if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return None;
            }
            let (_, sensitive) = crate::agentic::context::redact_secrets(value);
            (!sensitive).then(|| value.to_owned())
        })
    };
    let login_uid = value("UID").and_then(|value| value.parse::<u32>().ok());
    HostSystemInfo {
        os_release: value("PRETTY_NAME").or_else(|| value("NAME")),
        version_id: value("VERSION_ID"),
        kernel: value("KERNEL"),
        architecture: value("ARCH"),
        login_user: value("USER"),
        login_uid,
        login_is_root: login_uid.map(|uid| uid == 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_and_uses_uid_instead_of_username_for_root() {
        let info = parse("PRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nVERSION_ID=\"24.04\"\nKERNEL=6.8.0\nARCH=x86_64\nUSER=admin\nUID=0\n");
        assert_eq!(info.os_release.as_deref(), Some("Ubuntu 24.04.1 LTS"));
        assert_eq!(info.version_id.as_deref(), Some("24.04"));
        assert_eq!(info.kernel.as_deref(), Some("6.8.0"));
        assert_eq!(info.architecture.as_deref(), Some("x86_64"));
        assert_eq!(info.login_is_root, Some(true));
        assert_eq!(parse("USER=root\nUID=1000\n").login_is_root, Some(false));
        assert_eq!(parse("USER=root\nUID=invalid\n").login_is_root, None);
    }

    #[test]
    fn missing_unsafe_and_unrequested_values_are_not_context() {
        assert_eq!(parse(""), HostSystemInfo::default());
        let info =
            parse("PRETTY_NAME=TOKEN=secret\nKERNEL=bad\u{1b}[31m\nPASSWORD=example\nUID=501\n");
        assert_eq!(info.os_release, None);
        assert_eq!(info.kernel, None);
        assert_eq!(info.login_is_root, Some(false));
        assert_eq!(
            parse(&format!("NAME={}\n", "a".repeat(129))).os_release,
            None
        );
    }
}
