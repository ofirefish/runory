//! JumpServer Access Key HTTP Signature (HMAC-SHA256).
//!
//! JumpServer server-side `SignatureAuthentication.required_headers` is
//! `["(request-target)", "date"]` (see apps/common/auth/signature.py).
//! Signing only those headers avoids Accept-header drift behind proxies.
//! Secrets never appear in Debug or logs.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use ring::hmac;
use zeroize::Zeroizing;

/// Signs JumpServer REST requests with an Access Key.
pub struct JumpServerSigner {
    key_id: String,
    secret: Zeroizing<String>,
}

impl JumpServerSigner {
    pub fn new(key_id: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            key_id: key_id.into(),
            secret: Zeroizing::new(secret.into()),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Build `Authorization` and matching `Date` values for a request.
    ///
    /// `path_and_query` must match Django `request.get_full_path()` exactly
    /// (e.g. `/api/v1/perms/users/self/assets/?limit=20`).
    pub fn sign(
        &self,
        method: &str,
        path_and_query: &str,
        date: Option<&str>,
    ) -> JumpServerSignedHeaders {
        let date = date
            .map(str::to_string)
            .unwrap_or_else(http_date_gmt_now);
        let method_lower = method.trim().to_ascii_lowercase();
        let path = if path_and_query.starts_with('/') {
            path_and_query.to_string()
        } else {
            format!("/{path_and_query}")
        };
        // Must match httpsig.generate_message for headers=["(request-target)", "date"].
        let signing_string = format!("(request-target): {method_lower} {path}\ndate: {date}");
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.secret.as_bytes());
        let tag = hmac::sign(&key, signing_string.as_bytes());
        let signature = base64::engine::general_purpose::STANDARD.encode(tag.as_ref());
        let authorization = format!(
            "Signature keyId=\"{}\",algorithm=\"hmac-sha256\",headers=\"(request-target) date\",signature=\"{}\"",
            self.key_id, signature
        );
        JumpServerSignedHeaders {
            accept: "application/json".to_string(),
            date,
            authorization,
        }
    }
}

impl fmt::Debug for JumpServerSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JumpServerSigner")
            .field("key_id", &self.key_id)
            .field("secret", &"**REDACTED**")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JumpServerSignedHeaders {
    pub accept: String,
    pub date: String,
    pub authorization: String,
}

/// Split JumpServer `id:secret` paste format (`AccessKey.get_full_value()`).
pub fn split_access_key_material(key_id: &str, secret: &str) -> (String, String) {
    let key_id = key_id.trim();
    let secret = secret.trim();
    if secret.is_empty() {
        if let Some((id, sec)) = key_id.split_once(':') {
            let id = id.trim();
            let sec = sec.trim();
            if !id.is_empty() && !sec.is_empty() {
                return (id.to_string(), sec.to_string());
            }
        }
    }
    (key_id.to_string(), secret.to_string())
}

/// RFC 7231 IMF-fixdate in GMT, e.g. `Thu, 10 Sep 2026 07:30:00 GMT`.
pub fn http_date_gmt_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    http_date_gmt_from_unix(secs)
}

fn http_date_gmt_from_unix(secs: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // 1970-01-01 was Thursday.
    let days = secs / 86_400;
    let tod = secs % 86_400;
    let hour = tod / 3_600;
    let minute = (tod % 3_600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAYS[(days % 7) as usize],
        day,
        MONTHS[(month - 1) as usize],
        year,
        hour,
        minute,
        second
    )
}

/// Algorithm from Howard Hinnant / chrono civil_from_days (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_get_request_target_with_query() {
        let signer = JumpServerSigner::new("AKTEST", "secret-value");
        let signed = signer.sign(
            "GET",
            "/api/v1/perms/users/self/assets/?limit=20&offset=0",
            Some("Thu, 10 Sep 2026 07:30:00 GMT"),
        );
        assert_eq!(signed.accept, "application/json");
        assert_eq!(signed.date, "Thu, 10 Sep 2026 07:30:00 GMT");
        assert!(signed.authorization.starts_with(
            "Signature keyId=\"AKTEST\",algorithm=\"hmac-sha256\",headers=\"(request-target) date\",signature=\""
        ));
        let expected_sig = {
            let signing = "(request-target): get /api/v1/perms/users/self/assets/?limit=20&offset=0\ndate: Thu, 10 Sep 2026 07:30:00 GMT";
            let key = hmac::Key::new(hmac::HMAC_SHA256, b"secret-value");
            let tag = hmac::sign(&key, signing.as_bytes());
            base64::engine::general_purpose::STANDARD.encode(tag.as_ref())
        };
        assert!(signed.authorization.ends_with(&format!("signature=\"{expected_sig}\"")));
    }

    #[test]
    fn signs_post_request_target() {
        let signer = JumpServerSigner::new("AKPOST", "post-secret");
        let signed = signer.sign(
            "POST",
            "/api/v1/authentication/connection-token/",
            Some("Thu, 10 Sep 2026 07:30:00 GMT"),
        );
        let expected_sig = {
            let signing = "(request-target): post /api/v1/authentication/connection-token/\ndate: Thu, 10 Sep 2026 07:30:00 GMT";
            let key = hmac::Key::new(hmac::HMAC_SHA256, b"post-secret");
            let tag = hmac::sign(&key, signing.as_bytes());
            base64::engine::general_purpose::STANDARD.encode(tag.as_ref())
        };
        assert!(signed.authorization.contains(&expected_sig));
    }

    #[test]
    fn splits_combined_access_key_paste() {
        let (id, secret) = split_access_key_material(
            "11111111-1111-1111-1111-111111111111:abcdefghijklmnopqrstuvwxyz0123456789",
            "",
        );
        assert_eq!(id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(secret, "abcdefghijklmnopqrstuvwxyz0123456789");
    }

    #[test]
    fn debug_redacts_secret() {
        let signer = JumpServerSigner::new("AK", "super-secret");
        let debug = format!("{signer:?}");
        assert!(debug.contains("**REDACTED**"));
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn http_date_epoch_thursday() {
        assert_eq!(http_date_gmt_from_unix(0), "Thu, 01 Jan 1970 00:00:00 GMT");
    }
}
