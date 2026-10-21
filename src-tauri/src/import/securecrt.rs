//! SecureCRT session `.ini` importer (single file or Sessions directory).
//!
//! Encrypted `S:"Password"` values are not decrypted. SSH2 ports are hex-encoded.

use std::collections::HashSet;
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

const SKIP_NAMES: &[&str] = &["__FolderData__", "Default", "default"];

pub async fn preview_securecrt_import(
    path: &str,
    existing: &[ServerProfile],
) -> AppResult<CatalogImportPreview> {
    let parsed = load_and_parse(path, existing).await?;
    Ok(preview_from_parsed(parsed))
}

pub async fn apply_securecrt_import(
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
    let files = collect_files_with_extensions(&root, &["ini"]).await?;
    if files.is_empty() {
        return Err(AppError::ImportInvalid);
    }
    let mut parsed = Vec::new();
    let mut used_keys = HashSet::new();
    for file in files {
        let stem = file_stem_name(&file);
        if SKIP_NAMES
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&stem))
        {
            continue;
        }
        let bytes = read_file_bytes(&file).await?;
        let text = decode_config_text(&bytes)?;
        if let Some(item) = parse_securecrt_ini(&text, &root, &file, existing)? {
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

fn parse_securecrt_ini(
    text: &str,
    root: &Path,
    file: &Path,
    existing: &[ServerProfile],
) -> AppResult<Option<ParsedImportProfile>> {
    let mut values: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = parse_securecrt_line(line) {
            values.insert(key, value);
        }
    }

    let protocol = values
        .get("Protocol Name")
        .or_else(|| values.get("Terminal Protocol Name"))
        .map(|value| value.trim().to_ascii_uppercase())
        .unwrap_or_default();
    if !protocol.is_empty() && protocol != "SSH2" && protocol != "SSH1" && protocol != "SSH" {
        return Ok(None);
    }
    if values.get("Is Session").map(String::as_str) == Some("00000000") {
        return Ok(None);
    }

    let host = values
        .get("Hostname")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    if host.is_empty() {
        return Ok(None);
    }
    let username = values
        .get("Username")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "root".into());
    let port = values
        .get("[SSH2] Port")
        .or_else(|| values.get("Port"))
        .and_then(|value| parse_hex_u32(value))
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(22);

    let mut warnings = Vec::new();
    if values
        .get("Password")
        .is_some_and(|value| !value.trim().is_empty())
    {
        warnings.push("passwordSkipped".into());
    }
    if values
        .get("Firewall Name")
        .is_some_and(|value| !value.trim().is_empty() && !value.eq_ignore_ascii_case("None"))
    {
        warnings.push("proxySkipped".into());
    }

    let key_path = values
        .get("Identity Filename")
        .or_else(|| values.get("Identity Filename V2"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

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

    Ok(Some(ParsedImportProfile {
        import_key: file.to_string_lossy().into_owned(),
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

fn parse_securecrt_line(line: &str) -> Option<(String, String)> {
    // Formats: S:"Hostname"=value  or  D:"[SSH2] Port"=00000016
    let rest = line
        .strip_prefix("S:")
        .or_else(|| line.strip_prefix("D:"))
        .or_else(|| line.strip_prefix("B:"))?;
    let (key_part, value) = rest.split_once('=')?;
    let key = key_part.trim().trim_matches('"').to_string();
    Some((key, value.trim().to_string()))
}

fn parse_hex_u32(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    u32::from_str_radix(trimmed, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_ssh2_session() {
        let text = r#"
D:"Is Session"=00000001
S:"Protocol Name"=SSH2
S:"Hostname"=10.3.2.66
S:"Username"=xwu
D:"[SSH2] Port"=00000016
S:"Password"=uec99b0a4283ff19e5
S:"Identity Filename"=
"#;
        let file = PathBuf::from("/tmp/Sessions/Prod/api.ini");
        let root = PathBuf::from("/tmp/Sessions");
        let item = parse_securecrt_ini(text, &root, &file, &[])
            .expect("parse")
            .expect("ssh");
        assert_eq!(item.host, "10.3.2.66");
        assert_eq!(item.port, 22);
        assert_eq!(item.username, "xwu");
        assert_eq!(item.group_name.as_deref(), Some("Prod"));
        assert!(item.warnings.iter().any(|w| w == "passwordSkipped"));
    }

    #[test]
    fn skips_telnet() {
        let text = r#"
S:"Protocol Name"=Telnet
S:"Hostname"=10.0.0.1
D:"Port"=00000017
"#;
        let file = PathBuf::from("/tmp/a.ini");
        let root = PathBuf::from("/tmp");
        assert!(parse_securecrt_ini(text, &root, &file, &[])
            .expect("parse")
            .is_none());
    }
}
