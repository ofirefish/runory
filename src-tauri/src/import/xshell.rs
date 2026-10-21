//! Xshell `.xsh` session importer (single file or Sessions directory).
//!
//! Encrypted `Password=` values are not decrypted.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult, AuthMethod, ServerProfile};
use crate::groups::GroupService;
use crate::profiles::ProfileService;

use super::common::{
    apply_parsed_profiles, collect_files_with_extensions, decode_config_text, endpoint_importable,
    file_stem_name, mark_already_exists, preview_from_parsed, read_file_bytes, relative_group_name,
    validate_absolute_path, CatalogImportApplyResult, CatalogImportPreview, ParsedImportProfile,
    MAX_IMPORT_PROFILES,
};

pub async fn preview_xshell_import(
    path: &str,
    existing: &[ServerProfile],
) -> AppResult<CatalogImportPreview> {
    let parsed = load_and_parse(path, existing).await?;
    Ok(preview_from_parsed(parsed))
}

pub async fn apply_xshell_import(
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
    let root = validate_absolute_path(path)?;
    let files = collect_files_with_extensions(&root, &["xsh"]).await?;
    if files.is_empty() {
        return Err(AppError::ImportInvalid);
    }
    let mut parsed = Vec::new();
    let mut used_keys = HashSet::new();
    for file in files {
        let bytes = read_file_bytes(&file).await?;
        let text = decode_config_text(&bytes)?;
        if let Some(item) = parse_xsh(&text, &root, &file, existing)? {
            if used_keys.insert(item.import_key.clone()) {
                parsed.push(item);
                if parsed.len() > MAX_IMPORT_PROFILES {
                    return Err(AppError::ImportInvalid);
                }
            }
        }
    }
    Ok(parsed)
}

fn parse_xsh(
    text: &str,
    root: &Path,
    file: &Path,
    existing: &[ServerProfile],
) -> AppResult<Option<ParsedImportProfile>> {
    let sections = parse_ini_sections(text);
    let connection = sections.get("CONNECTION");
    let auth = sections.get("CONNECTION:AUTHENTICATION");

    let protocol = connection
        .and_then(|map| map.get("Protocol"))
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if !protocol.is_empty() && protocol != "ssh" && protocol != "2" {
        return Ok(None);
    }

    let host = connection
        .and_then(|map| map.get("Host"))
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if host.is_empty() {
        return Ok(None);
    }
    let port = connection
        .and_then(|map| map.get("Port"))
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(22);
    let username = auth
        .and_then(|map| map.get("UserName"))
        .or_else(|| auth.and_then(|map| map.get("Username")))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "root".into());

    let mut warnings = Vec::new();
    let key_path = auth
        .and_then(|map| {
            map.get("UserKey")
                .or_else(|| map.get("UserKeyPath"))
                .or_else(|| map.get("PublicKeyPath"))
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let encrypted_password = auth
        .and_then(|map| map.get("Password"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if encrypted_password.is_some() {
        warnings.push("passwordSkipped".into());
    }

    if sections.contains_key("CONNECTION:PROXY") {
        if let Some(proxy) = sections
            .get("CONNECTION:PROXY")
            .and_then(|map| map.get("Proxy"))
        {
            if !proxy.trim().is_empty() {
                warnings.push("proxySkipped".into());
            }
        }
    }

    let name = file_stem_name(file);
    let auth_method = if key_path.is_some() {
        AuthMethod::PrivateKey
    } else {
        AuthMethod::Password
    };
    let importable = endpoint_importable(&name, &host, port, &username);
    if !importable {
        warnings.push("invalidEndpoint".into());
    }
    mark_already_exists(&mut warnings, importable, existing, &host, port, &username);

    let group_name = if root.is_dir() {
        relative_group_name(root, file)
    } else {
        None
    };
    let import_key = file.to_string_lossy().into_owned();

    Ok(Some(ParsedImportProfile {
        import_key,
        source_id: Some(file.to_string_lossy().into_owned()),
        name,
        host,
        port,
        username,
        group_name,
        auth_method,
        key_path,
        password: None,
        jump_source_id: None,
        importable,
        warnings,
    }))
}

fn parse_ini_sections(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            current = name.trim().to_ascii_uppercase();
            sections.entry(current.clone()).or_default();
            continue;
        }
        if current.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_ssh_xsh() {
        let text = r#"
[CONNECTION]
Protocol=SSH
Host=10.1.2.3
Port=22
[CONNECTION:AUTHENTICATION]
UserName=alice
Password=encrypted-blob
UserKey=C:\Users\alice\.ssh\id_rsa
"#;
        let file = PathBuf::from("/tmp/Sessions/Lab/api.xsh");
        let root = PathBuf::from("/tmp/Sessions");
        let item = parse_xsh(text, &root, &file, &[])
            .expect("parse")
            .expect("ssh");
        assert_eq!(item.host, "10.1.2.3");
        assert_eq!(item.username, "alice");
        assert_eq!(item.group_name.as_deref(), Some("Lab"));
        assert_eq!(item.auth_method, AuthMethod::PrivateKey);
        assert!(item.warnings.iter().any(|w| w == "passwordSkipped"));
    }

    #[test]
    fn skips_non_ssh() {
        let text = "[CONNECTION]\nProtocol=TELNET\nHost=10.0.0.1\n";
        let file = PathBuf::from("/tmp/a.xsh");
        let root = PathBuf::from("/tmp");
        assert!(parse_xsh(text, &root, &file, &[]).expect("parse").is_none());
    }
}
