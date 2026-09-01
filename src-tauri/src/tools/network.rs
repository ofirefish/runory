use crate::domain::{AppError, AppResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::NetworkPortCheckData;

const PORT_CHECK_SCRIPT: &str = "command -v nc >/dev/null 2>&1 || exit 90; if nc -z -w 5 \"$1\" \"$2\" >/dev/null 2>&1; then reachable=true; else reachable=false; fi; printf 'PORT_CHECK\\t%s\\t%s\\t%s\\n' \"$1\" \"$2\" \"$reachable\"";

pub(super) async fn port_check(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    host: String,
    port: u16,
) -> AppResult<NetworkPortCheckData> {
    validate_host(&host)?;
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(PORT_CHECK_SCRIPT, vec![host.clone(), port.to_string()])
                .with_output_limit(8 * 1024),
        )
        .await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    if result.exit_code != 0 {
        return Err(AppError::ExecFailed);
    }
    parse_port_check(&result.stdout, host, port)
}

fn validate_host(host: &str) -> AppResult<()> {
    if host.is_empty()
        || host.len() > 255
        || host.starts_with('-')
        || host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn parse_port_check(output: &str, host: String, port: u16) -> AppResult<NetworkPortCheckData> {
    let fields = output.trim_end().split('\t').collect::<Vec<_>>();
    if fields.len() != 4
        || fields[0] != "PORT_CHECK"
        || fields[1] != host
        || fields[2] != port.to_string()
    {
        return Err(AppError::ExecFailed);
    }
    let reachable = match fields[3] {
        "true" => true,
        "false" => false,
        _ => return Err(AppError::ExecFailed),
    };
    Ok(NetworkPortCheckData {
        host,
        port,
        reachable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_matching_port_probe_only() {
        let parsed = parse_port_check("PORT_CHECK\t127.0.0.1\t22\ttrue\n", "127.0.0.1".into(), 22)
            .expect("parse probe");
        assert!(parsed.reachable);
        assert!(parse_port_check("PORT_CHECK\tother\t22\ttrue\n", "host".into(), 22).is_err());
    }

    #[test]
    fn rejects_ambiguous_remote_hosts() {
        for host in ["", "-proxy-command", "host name", "host\nname"] {
            assert!(
                validate_host(host).is_err(),
                "host should be rejected: {host:?}"
            );
        }
        assert!(validate_host("127.0.0.1").is_ok());
        assert!(validate_host("::1").is_ok());
    }
}
