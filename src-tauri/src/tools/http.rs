use crate::domain::{AppError, AppResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::HttpResponseData;

const HTTP_REQUEST_SCRIPT: &str = "command -v curl >/dev/null 2>&1 || exit 90; curl --silent --show-error --location --max-redirs 5 --proto '=http,https' --proto-redir '=http,https' --connect-timeout 5 --max-time 20 --range 0-262143 --max-filesize 262144 --output - --write-out '\\nRUNORY_HTTP_META\\t%{http_code}\\t%{content_type}\\t%{size_download}\\n' \"$1\"";
const HTTP_META_MARKER: &str = "\nRUNORY_HTTP_META\t";
const MAX_BODY_PREVIEW_BYTES: usize = 64 * 1024;

pub(super) async fn request(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    url: String,
) -> AppResult<(HttpResponseData, bool)> {
    validate_url(&url)?;
    let result = sessions
        .exec(
            session_id,
            RemoteCommand::script(HTTP_REQUEST_SCRIPT, vec![url]).with_output_limit(384 * 1024),
        )
        .await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    if result.exit_code != 0 {
        return Err(AppError::ExecFailed);
    }
    parse_http_response(&result.stdout)
}

fn validate_url(url: &str) -> AppResult<()> {
    if url.len() > 2048
        || url
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(AppError::InvalidOperation);
    }
    let authority = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .and_then(|remainder| remainder.split(['/', '?', '#']).next())
        .filter(|authority| !authority.is_empty() && !authority.contains('@'));
    if authority.is_none() {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn parse_http_response(output: &str) -> AppResult<(HttpResponseData, bool)> {
    let (body, metadata) = output
        .rsplit_once(HTTP_META_MARKER)
        .ok_or(AppError::ExecFailed)?;
    let fields = metadata.trim_end().split('\t').collect::<Vec<_>>();
    if fields.len() != 3 {
        return Err(AppError::ExecFailed);
    }
    let status_code = fields[0]
        .parse::<u16>()
        .ok()
        .filter(|status| (100..=599).contains(status))
        .ok_or(AppError::ExecFailed)?;
    let content_type = match fields[1].trim() {
        "" => None,
        value if value.len() <= 512 => Some(value.to_owned()),
        _ => return Err(AppError::ExecFailed),
    };
    let body_bytes = fields[2]
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as u64)
        .ok_or(AppError::ExecFailed)?;
    let (body_preview, preview_truncated) = truncate_utf8(body, MAX_BODY_PREVIEW_BYTES);
    Ok((
        HttpResponseData {
            status_code,
            content_type,
            body_preview: body_preview.to_owned(),
            body_bytes,
        },
        preview_truncated || body_bytes > MAX_BODY_PREVIEW_BYTES as u64,
    ))
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
    fn accepts_only_plain_http_urls_without_embedded_credentials() {
        assert!(validate_url("https://example.com/health?full=1").is_ok());
        for url in [
            "file:///etc/passwd",
            "https://user:secret@example.com/",
            "https://",
            "https://example.com/with space",
        ] {
            assert!(validate_url(url).is_err(), "URL should be rejected: {url}");
        }
    }

    #[test]
    fn parses_http_metadata_separately_from_untrusted_body() {
        let (parsed, truncated) =
            parse_http_response("untrusted body\nRUNORY_HTTP_META\t204\ttext/plain\t14\n")
                .expect("parse response");
        assert_eq!(parsed.status_code, 204);
        assert_eq!(parsed.content_type.as_deref(), Some("text/plain"));
        assert_eq!(parsed.body_preview, "untrusted body");
        assert_eq!(parsed.body_bytes, 14);
        assert!(!truncated);
    }
}
