//! Packaged plugin logo validation (PP1.1).
//!
//! Logos are package-local PNG/WebP only. Remote URLs, absolute paths,
//! path traversal, SVG, and oversized files are rejected.

use std::fs;
use std::path::Path;

use super::validator::validate_package_relative_path;
use crate::plugins::error::{PluginError, PluginResult};

pub const MAX_LOGO_BYTES: u64 = 512 * 1024;
const ALLOWED_EXTENSIONS: &[&str] = &["png", "webp"];

/// Structural logo path checks (no filesystem access).
pub fn validate_logo_path(raw: &str) -> PluginResult<()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PluginError::InvalidManifest(
            "logo path must be non-empty".into(),
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file:")
        || lower.contains("://")
    {
        return Err(PluginError::InvalidManifest(
            "logo remote or file URL is forbidden".into(),
        ));
    }
    validate_package_relative_path(trimmed, "logo")?;
    let path = Path::new(trimmed);
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match ext.as_deref() {
        Some(ext) if ALLOWED_EXTENSIONS.contains(&ext) => Ok(()),
        _ => Err(PluginError::InvalidManifest(
            "logo must be a PNG or WebP file".into(),
        )),
    }
}

/// Validate that a logo file exists under `root` and is within size limits.
pub fn validate_logo_file(root: &Path, relative: &str) -> PluginResult<()> {
    validate_logo_path(relative)?;
    let path = root.join(relative);
    let meta = fs::metadata(&path)
        .map_err(|_| PluginError::InvalidManifest(format!("logo file missing: {relative}")))?;
    if !meta.is_file() {
        return Err(PluginError::InvalidManifest(format!(
            "logo is not a file: {relative}"
        )));
    }
    if meta.len() > MAX_LOGO_BYTES {
        return Err(PluginError::InvalidManifest(format!(
            "logo exceeds {MAX_LOGO_BYTES} bytes"
        )));
    }
    Ok(())
}

pub fn logo_mime_type(relative: &str) -> Option<&'static str> {
    let ext = Path::new(relative)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;
    match ext.as_str() {
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn accepts_png_relative_path() {
        validate_logo_path("assets/logo.png").expect("png");
    }

    #[test]
    fn accepts_webp_relative_path() {
        validate_logo_path("assets/logo.webp").expect("webp");
    }

    #[test]
    fn rejects_svg() {
        assert!(validate_logo_path("assets/logo.svg").is_err());
    }

    #[test]
    fn rejects_remote_url() {
        assert!(validate_logo_path("https://example.com/logo.png").is_err());
        assert!(validate_logo_path("http://example.com/logo.png").is_err());
        assert!(validate_logo_path("file:///tmp/logo.png").is_err());
    }

    #[test]
    fn rejects_absolute_and_traversal() {
        assert!(validate_logo_path("/etc/passwd").is_err());
        assert!(validate_logo_path("../logo.png").is_err());
        assert!(validate_logo_path(r"C:\logo.png").is_err());
    }

    #[test]
    fn rejects_missing_and_oversized_file() {
        let dir = tempdir().expect("tempdir");
        assert!(validate_logo_file(dir.path(), "assets/logo.png").is_err());

        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).expect("mkdir");
        let logo = assets.join("logo.png");
        let mut file = fs::File::create(&logo).expect("create");
        let oversized = vec![0u8; (MAX_LOGO_BYTES as usize) + 1];
        file.write_all(&oversized).expect("write");
        assert!(validate_logo_file(dir.path(), "assets/logo.png").is_err());
    }

    #[test]
    fn accepts_existing_small_png() {
        let dir = tempdir().expect("tempdir");
        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).expect("mkdir");
        fs::write(assets.join("logo.png"), b"fake-png").expect("write");
        validate_logo_file(dir.path(), "assets/logo.png").expect("ok");
    }
}
