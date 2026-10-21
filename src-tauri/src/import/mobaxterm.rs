//! MobaXterm `.mxtsessions` bookmark importer.
//!
//! Only SSH session type `0` is imported. Encrypted credentials are not present as plaintext.

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

pub async fn preview_mobaxterm_import(
    path: &str,
    existing: &[ServerProfile],
) -> AppResult<CatalogImportPreview> {
    let parsed = load_and_parse(path, existing).await?;
    Ok(preview_from_parsed(parsed))
}

pub async fn apply_mobaxterm_import(
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
    let path = validate_mobaxterm_path(path)?;
    let bytes = read_file_bytes(&path).await?;
    let text = decode_config_text(&bytes)?;
    parse_mxtsessions(&text, existing)
}

fn validate_mobaxterm_path(path: &str) -> AppResult<PathBuf> {
    let candidate = validate_absolute_path(path)?;
    let extension = candidate
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("mxtsessions") => Ok(candidate),
        _ => Err(AppError::ImportInvalid),
    }
}

fn parse_mxtsessions(
    text: &str,
    existing: &[ServerProfile],
) -> AppResult<Vec<ParsedImportProfile>> {
    let mut parsed = Vec::new();
    let mut used_keys = HashSet::new();
    let mut current_group: Option<String> = None;
    let mut index = 0usize;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current_group = None;
            continue;
        }
        if let Some(value) = line.strip_prefix("SubRep=") {
            let name = value.trim();
            current_group = if name.is_empty() {
                None
            } else {
                Some(name.replace('\\', "/"))
            };
            continue;
        }
        if line.starts_with("ImgNum=") {
            continue;
        }
        if let Some((name, payload)) = line.split_once('=') {
            let name = name.trim();
            if name.is_empty() || !payload.contains('#') {
                continue;
            }
            if let Some(item) =
                parse_session_line(name, payload, current_group.as_deref(), index, existing)
            {
                if used_keys.insert(item.import_key.clone()) {
                    parsed.push(item);
                    index += 1;
                    if parsed.len() > MAX_IMPORT_PROFILES {
                        return Err(AppError::ImportInvalid);
                    }
                }
            }
        }
    }

    if parsed.is_empty() {
        return Err(AppError::ImportInvalid);
    }
    Ok(parsed)
}

fn parse_session_line(
    name: &str,
    payload: &str,
    group_name: Option<&str>,
    index: usize,
    existing: &[ServerProfile],
) -> Option<ParsedImportProfile> {
    // Sessions look like: #109#0%host%port%user%...
    let parts: Vec<&str> = payload.split('#').collect();
    if parts.len() < 3 {
        return None;
    }
    // parts[0] may be empty or "; logout"
    let ssh_block = parts.get(2).copied().unwrap_or("");
    let fields: Vec<&str> = ssh_block.split('%').collect();
    if fields.is_empty() {
        return None;
    }
    let session_type = fields.first().copied().unwrap_or("").trim();
    if session_type != "0" {
        return None;
    }
    let host = fields.get(1).copied().unwrap_or("").trim().to_string();
    if host.is_empty() {
        return None;
    }
    let port = fields
        .get(2)
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(22);
    let username_raw = fields.get(3).copied().unwrap_or("").trim();
    let username = if username_raw.is_empty() || username_raw == "<default>" {
        "root".to_string()
    } else {
        username_raw.to_string()
    };

    let mut warnings = Vec::new();
    let mut key_path = fields
        .get(14)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| value.replace("_CurrentDrive_", "C:"));

    let gateway = fields.get(8).copied().unwrap_or("").trim();
    if !gateway.is_empty() {
        warnings.push("jumpDeferred".into());
    }
    let proxy_type = fields.get(19).copied().unwrap_or("0").trim();
    if proxy_type != "0" && !proxy_type.is_empty() {
        warnings.push("proxySkipped".into());
    }

    if key_path.as_deref() == Some("<none>") {
        key_path = None;
    }

    let display_name = name.trim().to_string();
    let auth_method = if key_path.is_some() {
        AuthMethod::PrivateKey
    } else {
        AuthMethod::Password
    };
    let importable = endpoint_importable(&display_name, &host, port, &username);
    if !importable {
        warnings.push("invalidEndpoint".into());
    }
    mark_already_exists(&mut warnings, importable, existing, &host, port, &username);

    Some(ParsedImportProfile {
        import_key: format!("moba:{index}:{display_name}"),
        source_id: Some(display_name.clone()),
        name: display_name,
        host,
        port,
        username,
        group_name: group_name.map(str::to_string),
        auth_method,
        key_path,
        password: None,
        jump_source_id: None,
        importable,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_bookmarks() {
        let text = r#"
[Bookmarks]
SubRep=
ImgNum=42
Deck=#109#0%192.168.137.40%22%deck%%0%0%%%%%0%0%0%%%-1%0%0%0%%1080%%0%0%1%#MobaFont%10%0%0%-1%15%236,236,236%30,30,30%180,180,192%0%-1%0%%xterm%-1%0%_Std_Colors_0_%80%24%0%1%-1%<none>%%0%0%-1%-1#0# #-1
[Bookmarks_1]
SubRep=Lab\Web
ImgNum=41
Api=#109#0%10.0.0.5%2222%deploy%%0%0%%%%%0%0%0%%%-1%0%0%0%%1080%%0%0%1%#MobaFont#0# #-1
Rdp=#109#4%10.0.0.9%3389%admin%%0%0%#MobaFont#0# #-1
"#;
        let parsed = parse_mxtsessions(text, &[]).expect("parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "Deck");
        assert_eq!(parsed[0].host, "192.168.137.40");
        assert_eq!(parsed[1].name, "Api");
        assert_eq!(parsed[1].port, 2222);
        assert_eq!(parsed[1].group_name.as_deref(), Some("Lab/Web"));
    }
}
