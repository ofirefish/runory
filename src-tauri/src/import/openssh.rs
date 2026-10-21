//! OpenSSH `config` host importer.
//!
//! Supports Host / HostName / User / Port / IdentityFile / ProxyJump.
//! Wildcard Host patterns and Include directives are skipped (with warnings).

use std::collections::HashSet;
use std::path::PathBuf;

use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult, AuthMethod, ServerProfile};
use crate::groups::GroupService;
use crate::profiles::ProfileService;

use super::common::{
    apply_parsed_profiles, decode_config_text, endpoint_importable, mark_already_exists,
    preview_from_parsed, read_file_bytes, validate_absolute_path, CatalogImportApplyResult,
    CatalogImportPreview, ParsedImportProfile, MAX_IMPORT_PROFILES,
};

pub async fn preview_openssh_import(
    path: &str,
    existing: &[ServerProfile],
) -> AppResult<CatalogImportPreview> {
    let parsed = load_and_parse(path, existing).await?;
    Ok(preview_from_parsed(parsed))
}

pub async fn apply_openssh_import(
    path: &str,
    selected_keys: &[String],
    groups: &GroupService,
    profiles: &ProfileService,
    credentials: &CredentialService,
) -> AppResult<CatalogImportApplyResult> {
    let existing = profiles.list().await?;
    let parsed = load_and_parse(path, &existing).await?;
    apply_parsed_profiles(parsed, selected_keys, groups, profiles, credentials).await
}

async fn load_and_parse(
    path: &str,
    existing: &[ServerProfile],
) -> AppResult<Vec<ParsedImportProfile>> {
    let path = validate_openssh_path(path)?;
    let bytes = read_file_bytes(&path).await?;
    let text = decode_config_text(&bytes)?;
    parse_openssh_config(&text, existing)
}

fn validate_openssh_path(path: &str) -> AppResult<PathBuf> {
    validate_absolute_path(path)
}

fn parse_openssh_config(
    text: &str,
    existing: &[ServerProfile],
) -> AppResult<Vec<ParsedImportProfile>> {
    let mut parsed = Vec::new();
    let mut used_keys = HashSet::new();
    let mut current: Option<OpenSshHost> = None;
    let mut include_seen = false;

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = split_keyword(line);
        let key_lower = key.to_ascii_lowercase();
        match key_lower.as_str() {
            "host" => {
                if let Some(host) = current.take() {
                    push_host(host, &mut parsed, &mut used_keys, existing)?;
                }
                current = Some(OpenSshHost {
                    patterns: value.split_whitespace().map(str::to_string).collect(),
                    hostname: None,
                    user: None,
                    port: None,
                    identity_file: None,
                    proxy_jump: None,
                });
            }
            "include" => {
                include_seen = true;
            }
            "hostname" => {
                if let Some(host) = current.as_mut() {
                    host.hostname = Some(value.to_string());
                }
            }
            "user" => {
                if let Some(host) = current.as_mut() {
                    host.user = Some(value.to_string());
                }
            }
            "port" => {
                if let Some(host) = current.as_mut() {
                    if let Ok(port) = value.parse::<u16>() {
                        host.port = Some(port);
                    }
                }
            }
            "identityfile" => {
                if let Some(host) = current.as_mut() {
                    if host.identity_file.is_none() {
                        host.identity_file = Some(value.to_string());
                    }
                }
            }
            "proxyjump" => {
                if let Some(host) = current.as_mut() {
                    host.proxy_jump = Some(value.to_string());
                }
            }
            _ => {}
        }
    }
    if let Some(host) = current.take() {
        push_host(host, &mut parsed, &mut used_keys, existing)?;
    }

    if include_seen && parsed.is_empty() {
        // Still a valid empty-ish config; Include alone is not fatal.
    }
    if include_seen {
        for item in &mut parsed {
            if !item.warnings.iter().any(|w| w == "includeSkipped") {
                item.warnings.push("includeSkipped".into());
            }
        }
    }

    Ok(parsed)
}

struct OpenSshHost {
    patterns: Vec<String>,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
    proxy_jump: Option<String>,
}

fn push_host(
    host: OpenSshHost,
    parsed: &mut Vec<ParsedImportProfile>,
    used_keys: &mut HashSet<String>,
    existing: &[ServerProfile],
) -> AppResult<()> {
    let aliases: Vec<String> = host
        .patterns
        .iter()
        .filter(|pattern| !pattern_is_wildcard(pattern))
        .cloned()
        .collect();
    if aliases.is_empty() {
        return Ok(());
    }

    for alias in aliases {
        let mut warnings = Vec::new();
        let endpoint = host
            .hostname
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(alias.as_str())
            .to_string();
        let username = host
            .user
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("root")
            .to_string();
        let port = host.port.unwrap_or(22);
        let name = alias.clone();
        let key_path = host
            .identity_file
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(expand_tilde);
        let auth_method = if key_path.is_some() {
            AuthMethod::PrivateKey
        } else {
            AuthMethod::Password
        };
        if host
            .proxy_jump
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            warnings.push("jumpDeferred".into());
        }

        let importable = endpoint_importable(&name, &endpoint, port, &username);
        if !importable {
            warnings.push("invalidEndpoint".into());
        }
        mark_already_exists(
            &mut warnings,
            importable,
            existing,
            &endpoint,
            port,
            &username,
        );

        let import_key = format!("openssh:{alias}");
        if !used_keys.insert(import_key.clone()) {
            continue;
        }

        parsed.push(ParsedImportProfile {
            import_key,
            source_id: Some(alias),
            name,
            host: endpoint,
            port,
            username,
            group_name: None,
            auth_method,
            key_path,
            password: None,
            jump_source_id: None,
            importable,
            warnings,
        });
        if parsed.len() > MAX_IMPORT_PROFILES {
            return Err(AppError::ImportInvalid);
        }
    }
    Ok(())
}

fn pattern_is_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('!')
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home)
                .join(rest)
                .to_string_lossy()
                .into_owned();
        }
    }
    path.to_string()
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(index) => &line[..index],
        None => line,
    }
}

fn split_keyword(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
        index += 1;
    }
    let key = &line[..index];
    let mut rest = &line[index..];
    rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    (key, rest.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_hosts_and_skips_wildcards() {
        let text = r#"
Host *.internal
  User nobody

Host web1 web2
  HostName 10.0.0.8
  User deploy
  Port 2222
  IdentityFile ~/.ssh/id_ed25519

Host db
  HostName db.example
  User root
  ProxyJump bastion
"#;
        let parsed = parse_openssh_config(text, &[]).expect("parse");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].name, "web1");
        assert_eq!(parsed[0].host, "10.0.0.8");
        assert_eq!(parsed[0].port, 2222);
        assert_eq!(parsed[0].auth_method, AuthMethod::PrivateKey);
        assert!(parsed[2].warnings.iter().any(|w| w == "jumpDeferred"));
    }

    #[test]
    fn marks_include_skipped() {
        let text = r#"
Include ~/.ssh/other
Host app
  HostName app.example
  User ops
"#;
        let parsed = parse_openssh_config(text, &[]).expect("parse");
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].warnings.iter().any(|w| w == "includeSkipped"));
    }
}
