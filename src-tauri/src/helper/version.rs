//! Helper binary version probing.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::Command;

use super::HelperError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperVersion {
    pub raw: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionConstraint {
    pub min_major: u32,
    pub min_minor: u32,
}

impl VersionConstraint {
    pub fn at_least(major: u32, minor: u32) -> Self {
        Self {
            min_major: major,
            min_minor: minor,
        }
    }

    pub fn matches(&self, version: &HelperVersion) -> bool {
        version.major > self.min_major
            || (version.major == self.min_major && version.minor >= self.min_minor)
    }
}

pub fn check_version(
    binary: &PathBuf,
    constraint: &VersionConstraint,
) -> Result<HelperVersion, HelperError> {
    // Synchronous probe keeps provider init simple; callers may wrap in spawn_blocking.
    let output = std::process::Command::new(binary)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .or_else(|_| {
            std::process::Command::new(binary)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
        })
        .map_err(|_| HelperError::Missing)?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    };
    let version = parse_version(&combined).ok_or(HelperError::VersionMismatch)?;
    if !constraint.matches(&version) {
        return Err(HelperError::VersionMismatch);
    }
    Ok(version)
}

/// Async variant for callers already on the runtime.
#[allow(dead_code)]
pub async fn check_version_async(
    binary: &PathBuf,
    constraint: &VersionConstraint,
) -> Result<HelperVersion, HelperError> {
    let output = Command::new(binary)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;
    let output = match output {
        Ok(o) if !o.stdout.is_empty() || o.status.success() => o,
        _ => Command::new(binary)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|_| HelperError::Missing)?,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    };
    let version = parse_version(&combined).ok_or(HelperError::VersionMismatch)?;
    if !constraint.matches(&version) {
        return Err(HelperError::VersionMismatch);
    }
    Ok(version)
}

pub fn parse_version(text: &str) -> Option<HelperVersion> {
    // Find first `N.N` or `N.N.N` sequence.
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let candidate = &text[start..i];
            if let Some(version) = parse_semver_prefix(candidate) {
                return Some(version);
            }
        } else {
            i += 1;
        }
    }
    None
}

fn parse_semver_prefix(s: &str) -> Option<HelperVersion> {
    let mut parts = s.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next().and_then(|p| {
        let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    }).unwrap_or(0);
    Some(HelperVersion {
        raw: format!("{major}.{minor}.{patch}"),
        major,
        minor,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tsh_version_line() {
        let v = parse_version("Teleport v15.2.1 git:v15.2.1").expect("parse");
        assert_eq!(v.major, 15);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 1);
    }

    #[test]
    fn constraint_rejects_older() {
        let c = VersionConstraint::at_least(14, 0);
        let old = HelperVersion {
            raw: "13.0.0".into(),
            major: 13,
            minor: 0,
            patch: 0,
        };
        assert!(!c.matches(&old));
    }
}
