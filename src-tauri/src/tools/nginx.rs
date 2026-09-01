use crate::domain::{AppError, AppResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::NginxTestData;

const NGINX_TEST_SCRIPT: &str = "command -v nginx >/dev/null 2>&1 || exit 90; nginx -t";
const MAX_RAW_SUMMARY_BYTES: usize = 16 * 1024;
const MAX_ERROR_MESSAGE_BYTES: usize = 4 * 1024;

pub(super) async fn test(
    sessions: &ServerSessionManager,
    session_id: SessionId,
) -> AppResult<NginxTestData> {
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(NGINX_TEST_SCRIPT, Vec::new()).with_output_limit(64 * 1024),
        )
        .await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    let combined = match (result.stdout.trim(), result.stderr.trim()) {
        ("", stderr) => stderr.to_owned(),
        (stdout, "") => stdout.to_owned(),
        (stdout, stderr) => format!("{stdout}\n{stderr}"),
    };
    parse_nginx_test(result.exit_code == 0, &combined)
}

fn parse_nginx_test(valid: bool, output: &str) -> AppResult<NginxTestData> {
    if output.contains('\0') {
        return Err(AppError::ExecFailed);
    }
    let config_file = output.lines().find_map(extract_config_file);
    let (error_file, error_line) = if valid {
        (None, None)
    } else {
        output.lines().find_map(extract_error_location).unzip()
    };
    let error_message = if valid {
        None
    } else {
        output
            .lines()
            .find(|line| line.contains("[emerg]") || line.contains("[error]"))
            .or_else(|| output.lines().find(|line| !line.trim().is_empty()))
            .map(|line| {
                truncate_utf8(line.trim(), MAX_ERROR_MESSAGE_BYTES)
                    .0
                    .to_owned()
            })
    };
    let raw_summary = truncate_utf8(output.trim(), MAX_RAW_SUMMARY_BYTES)
        .0
        .to_owned();
    Ok(NginxTestData {
        valid,
        config_file,
        error_file,
        error_line,
        error_message,
        raw_summary,
    })
}

fn extract_config_file(line: &str) -> Option<String> {
    let remainder = line.split_once("configuration file ")?.1;
    let path = remainder
        .split_once(" syntax")
        .or_else(|| remainder.split_once(" test"))
        .map(|(path, _)| path.trim())?;
    valid_remote_path(path).then(|| path.to_owned())
}

fn extract_error_location(line: &str) -> Option<(String, u32)> {
    let location = line.rsplit_once(" in ")?.1.trim();
    let (file, line) = location.rsplit_once(':')?;
    let line = line.parse::<u32>().ok().filter(|line| *line > 0)?;
    valid_remote_path(file).then(|| (file.to_owned(), line))
}

fn valid_remote_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (&str, bool) {
    if value.len() <= max_bytes {
        return (value, false);
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_successful_nginx_validation() {
        let parsed = parse_nginx_test(
            true,
            "nginx: the configuration file /etc/nginx/nginx.conf syntax is ok\nnginx: configuration file /etc/nginx/nginx.conf test is successful",
        )
        .expect("parse success");
        assert!(parsed.valid);
        assert_eq!(parsed.config_file.as_deref(), Some("/etc/nginx/nginx.conf"));
        assert!(parsed.error_file.is_none());
        assert!(parsed.error_line.is_none());
    }

    #[test]
    fn parses_failed_nginx_validation_without_treating_remote_text_as_instructions() {
        let parsed = parse_nginx_test(
            false,
            "nginx: [emerg] duplicate listen options for 0.0.0.0:443 in /etc/nginx/conf.d/api.conf:12\nnginx: configuration file /etc/nginx/nginx.conf test failed",
        )
        .expect("parse failure");
        assert!(!parsed.valid);
        assert_eq!(
            parsed.error_file.as_deref(),
            Some("/etc/nginx/conf.d/api.conf")
        );
        assert_eq!(parsed.error_line, Some(12));
        assert!(parsed
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("duplicate listen")));
    }

    #[test]
    fn bounds_remote_nginx_output() {
        let output = "服".repeat(MAX_RAW_SUMMARY_BYTES);
        let parsed = parse_nginx_test(false, &output).expect("parse bounded output");
        assert!(parsed.raw_summary.len() <= MAX_RAW_SUMMARY_BYTES);
        assert!(parsed
            .raw_summary
            .is_char_boundary(parsed.raw_summary.len()));
    }
}
