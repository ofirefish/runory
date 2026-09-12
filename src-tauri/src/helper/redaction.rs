//! Secret redaction for helper stdout/stderr before logging or UI diagnostics.

/// Patterns that must never appear in logs / crash reports / Agent context.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "authorization",
    "access_key",
    "access-key",
    "private_key",
    "private-key",
    "passphrase",
    "otp",
    "session_id",
    "identity",
    "credential",
];

/// Redact likely secrets from a single helper output line.
pub fn redact_helper_output(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    for key in SECRET_KEYS {
        if lower.contains(key) {
            // Replace values after `=` / `:` when the key is present.
            return redact_key_values(line);
        }
    }
    // Always scrub bearer-like tokens.
    scrub_bearer_tokens(line)
}

fn redact_key_values(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for part in line.split_whitespace() {
        if let Some((key, _)) = part.split_once('=') {
            if SECRET_KEYS
                .iter()
                .any(|k| key.to_ascii_lowercase().contains(k))
            {
                out.push_str(key);
                out.push_str("=[REDACTED]");
                out.push(' ');
                continue;
            }
        }
        if let Some((key, _)) = part.split_once(':') {
            if SECRET_KEYS
                .iter()
                .any(|k| key.to_ascii_lowercase().contains(k))
            {
                out.push_str(key);
                out.push_str(":[REDACTED]");
                out.push(' ');
                continue;
            }
        }
        out.push_str(part);
        out.push(' ');
    }
    scrub_bearer_tokens(out.trim_end())
}

fn scrub_bearer_tokens(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if let Some(idx) = lower.find("bearer ") {
        let mut redacted = line[..idx].to_string();
        redacted.push_str("Bearer [REDACTED]");
        return redacted;
    }
    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_password_assignment() {
        let safe = redact_helper_output("login password=s3cret host=x");
        assert!(safe.contains("[REDACTED]"));
        assert!(!safe.contains("s3cret"));
    }

    #[test]
    fn redacts_bearer() {
        let safe = redact_helper_output("Authorization: Bearer abc.def.ghi");
        assert!(safe.contains("[REDACTED]"));
        assert!(!safe.contains("abc.def"));
    }
}
