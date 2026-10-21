//! Terminus / Tabby `config.yaml` (+ `ssh-profiles.yaml`) host importer.
//!
//! Security invariants:
//! - Plaintext `options.password` is imported into CredentialVault on apply only.
//! - Passwords never appear in preview IPC or Profile JSON.
//! - Private keys are referenced by local file path only (after `file://` normalize).
//! - Jump hosts are wired only when both ends are imported in the same apply.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::Deserialize;

use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult, AuthMethod, ServerProfile};
use crate::groups::GroupService;
use crate::profiles::ProfileService;

use super::common::{
    apply_parsed_profiles, decode_config_text, endpoint_importable, mark_already_exists,
    preview_from_parsed, read_file_bytes, take_plaintext_password, validate_absolute_path,
    CatalogImportApplyResult, CatalogImportPreview, ParsedImportProfile, MAX_IMPORT_PROFILES,
};

#[derive(Debug, Deserialize)]
struct TerminusConfigFile {
    #[serde(default)]
    profiles: Vec<TerminusProfileEntry>,
    #[serde(default)]
    groups: Vec<TerminusGroupEntry>,
}

#[derive(Debug, Deserialize)]
struct TerminusGroupEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerminusProfileEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type", default)]
    profile_type: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default, alias = "isBuiltin")]
    is_builtin: Option<bool>,
    #[serde(default, alias = "isTemplate")]
    is_template: Option<bool>,
    #[serde(default)]
    options: Option<TerminusSshOptions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminusSshOptions {
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    auth: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    private_keys: Option<Vec<String>>,
    #[serde(default)]
    jump_host: Option<String>,
    #[serde(default)]
    proxy_command: Option<String>,
    #[serde(default)]
    socks_proxy_host: Option<String>,
    #[serde(default)]
    http_proxy_host: Option<String>,
}

pub async fn preview_terminus_import(
    path: &str,
    existing: &[ServerProfile],
) -> AppResult<CatalogImportPreview> {
    let parsed = load_and_parse(path, existing).await?;
    Ok(preview_from_parsed(parsed))
}

pub async fn apply_terminus_import(
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
    let path = validate_terminus_path(path)?;
    let bytes = read_file_bytes(&path).await?;
    let text = decode_config_text(&bytes)?;
    parse_terminus_yaml(&text, existing)
}

fn validate_terminus_path(path: &str) -> AppResult<PathBuf> {
    let candidate = validate_absolute_path(path)?;
    let extension = candidate
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("yaml" | "yml") => Ok(candidate),
        _ => Err(AppError::ImportInvalid),
    }
}

fn parse_terminus_yaml(
    text: &str,
    existing: &[ServerProfile],
) -> AppResult<Vec<ParsedImportProfile>> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|_| AppError::ImportInvalid)?;

    let (entries, groups) = match value {
        serde_yaml::Value::Sequence(sequence) => {
            let entries: Vec<TerminusProfileEntry> =
                serde_yaml::from_value(serde_yaml::Value::Sequence(sequence))
                    .map_err(|_| AppError::ImportInvalid)?;
            (entries, Vec::new())
        }
        serde_yaml::Value::Mapping(_) => {
            let file: TerminusConfigFile =
                serde_yaml::from_value(value).map_err(|_| AppError::ImportInvalid)?;
            (file.profiles, file.groups)
        }
        _ => return Err(AppError::ImportInvalid),
    };

    let mut group_names: HashMap<String, String> = HashMap::new();
    for group in groups {
        let Some(id) = group.id.filter(|value| !value.trim().is_empty()) else {
            continue;
        };
        let name = group
            .name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if let Some(name) = name {
            group_names.insert(id, name);
        }
    }

    let mut parsed = Vec::new();
    let mut used_keys = HashSet::new();

    for (index, entry) in entries.into_iter().enumerate() {
        if entry.is_builtin.unwrap_or(false) || entry.is_template.unwrap_or(false) {
            continue;
        }
        let profile_type = entry
            .profile_type
            .as_deref()
            .unwrap_or("ssh")
            .trim()
            .to_ascii_lowercase();
        if profile_type != "ssh" {
            continue;
        }
        let Some(options) = entry.options else {
            continue;
        };

        let mut warnings = Vec::new();
        let host = options.host.as_deref().unwrap_or("").trim().to_string();
        let username = options.user.as_deref().unwrap_or("root").trim().to_string();
        let port = options.port.unwrap_or(22);
        let name = entry
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                if host.is_empty() {
                    format!("Imported SSH {index}")
                } else {
                    host.clone()
                }
            });

        let group_name = resolve_group_name(entry.group.as_deref(), &group_names);
        let (auth_method, key_path, auth_warnings) = map_auth(&options);
        warnings.extend(auth_warnings);

        let password = take_plaintext_password(options.password);
        if password.is_some() {
            warnings.push("passwordWillImport".into());
        } else if auth_looks_password_backed(options.auth.as_deref()) {
            warnings.push("passwordSkipped".into());
        }

        if options
            .proxy_command
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            || options
                .socks_proxy_host
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            || options
                .http_proxy_host
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            warnings.push("proxySkipped".into());
        }

        let jump_source_id = options
            .jump_host
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if jump_source_id.is_some() {
            warnings.push("jumpDeferred".into());
        }

        let importable = endpoint_importable(&name, &host, port, &username);
        if !importable {
            warnings.push("invalidEndpoint".into());
        }
        mark_already_exists(&mut warnings, importable, existing, &host, port, &username);

        let source_id = entry
            .id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let import_key = source_id
            .clone()
            .unwrap_or_else(|| format!("index:{index}"));
        if !used_keys.insert(import_key.clone()) {
            continue;
        }

        parsed.push(ParsedImportProfile {
            import_key,
            source_id,
            name,
            host,
            port,
            username,
            group_name,
            auth_method,
            key_path,
            password,
            jump_source_id,
            importable,
            warnings,
        });

        if parsed.len() > MAX_IMPORT_PROFILES {
            return Err(AppError::ImportInvalid);
        }
    }

    Ok(parsed)
}

fn resolve_group_name(raw: Option<&str>, group_names: &HashMap<String, String>) -> Option<String> {
    let value = raw?.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(name) = group_names.get(value) {
        return Some(name.clone());
    }
    Some(value.to_string())
}

fn map_auth(options: &TerminusSshOptions) -> (AuthMethod, Option<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let auth = options
        .auth
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("password");
    let key_path = options
        .private_keys
        .as_ref()
        .and_then(|keys| keys.iter().find_map(|raw| normalize_key_path(raw)));

    match auth {
        "publicKey" => {
            if let Some(path) = key_path {
                (AuthMethod::PrivateKey, Some(path), warnings)
            } else {
                warnings.push("missingKeyPath".into());
                (AuthMethod::Password, None, warnings)
            }
        }
        "agent" => {
            warnings.push("agentMapped".into());
            if let Some(path) = key_path {
                (AuthMethod::PrivateKey, Some(path), warnings)
            } else {
                (AuthMethod::Password, None, warnings)
            }
        }
        "keyboardInteractive" => {
            warnings.push("keyboardInteractiveMapped".into());
            (AuthMethod::Password, None, warnings)
        }
        _ => {
            if let Some(path) = key_path {
                (AuthMethod::PrivateKey, Some(path), warnings)
            } else {
                (AuthMethod::Password, None, warnings)
            }
        }
    }
}

fn normalize_key_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = if let Some(rest) = trimmed.strip_prefix("file://") {
        rest
    } else {
        return Some(trimmed.to_string());
    };

    let path = if cfg!(windows) {
        let stripped = without_scheme.strip_prefix('/').unwrap_or(without_scheme);
        if stripped.len() >= 2 && stripped.as_bytes()[1] == b':' {
            stripped.to_string()
        } else if without_scheme.starts_with('/') {
            without_scheme.to_string()
        } else {
            format!("/{without_scheme}")
        }
    } else if without_scheme.starts_with('/') {
        without_scheme.to_string()
    } else {
        format!("/{without_scheme}")
    };

    let path = percent_decode_light(&path);
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

fn percent_decode_light(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[index + 1]), from_hex(bytes[index + 2])) {
                out.push((hi << 4) | lo);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn auth_looks_password_backed(auth: Option<&str>) -> bool {
    matches!(
        auth.map(str::trim).filter(|value| !value.is_empty()),
        None | Some("password") | Some("keyboardInteractive")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ConnectionRoute;
    use uuid::Uuid;

    #[test]
    fn parses_tabby_config_and_keeps_password_internally() {
        let yaml = r#"
version: 7
groups:
  - id: g-prod
    name: Production
profiles:
  - type: local
    name: bash
    options:
      command: /bin/bash
  - type: ssh
    id: ssh:prod-api
    name: Prod API
    group: g-prod
    options:
      host: 10.0.0.5
      port: 2222
      user: deploy
      auth: password
      password: super-secret
  - type: ssh
    id: ssh:key-host
    name: Key Host
    group: g-prod
    options:
      host: 10.0.0.6
      user: root
      auth: publicKey
      privateKeys:
        - file:///home/runory/.ssh/id_ed25519
  - type: ssh
    isBuiltin: true
    name: template
    options:
      host: ignored.example
      user: root
"#;
        let parsed = parse_terminus_yaml(yaml, &[]).expect("parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "Prod API");
        assert_eq!(parsed[0].group_name.as_deref(), Some("Production"));
        assert_eq!(parsed[0].auth_method, AuthMethod::Password);
        assert_eq!(
            parsed[0].password.as_ref().map(|value| value.as_str()),
            Some("super-secret")
        );
        assert!(parsed[0].warnings.iter().any(|w| w == "passwordWillImport"));
        assert!(!format!("{parsed:?}").contains("super-secret"));

        let preview = preview_from_parsed(parsed);
        assert!(preview.profiles[0].has_password);
        assert!(!format!("{preview:?}").contains("super-secret"));

        assert_eq!(preview.profiles[1].auth_method, AuthMethod::PrivateKey);
        assert_eq!(
            preview.profiles[1].key_path.as_deref(),
            Some("/home/runory/.ssh/id_ed25519")
        );
    }

    #[test]
    fn parses_bare_ssh_profiles_list() {
        let yaml = r#"
- name: edge
  options:
    host: edge.example
    user: ops
    auth: publicKey
    privateKeys:
      - /Users/ops/.ssh/id_rsa
"#;
        let parsed = parse_terminus_yaml(yaml, &[]).expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].host, "edge.example");
        assert_eq!(
            parsed[0].key_path.as_deref(),
            Some("/Users/ops/.ssh/id_rsa")
        );
    }

    #[test]
    fn normalize_file_url_windows_drive() {
        let path = normalize_key_path("file:///C:/Users/a/.ssh/id_rsa").expect("path");
        if cfg!(windows) {
            assert_eq!(path, "C:/Users/a/.ssh/id_rsa");
        } else {
            assert!(path.ends_with("/Users/a/.ssh/id_rsa"));
        }
    }

    #[test]
    fn rejects_non_yaml_extension() {
        assert!(matches!(
            validate_terminus_path("/tmp/config.json"),
            Err(AppError::ImportInvalid)
        ));
    }

    #[test]
    fn marks_duplicate_against_existing_profiles() {
        let existing = [ServerProfile {
            id: Uuid::new_v4(),
            name: "existing".into(),
            host: "10.0.0.5".into(),
            port: 22,
            username: "root".into(),
            group_id: None,
            auth_method: AuthMethod::Password,
            key_source: None,
            connection_route: ConnectionRoute::Direct,
            sort_order: 0,
            created_at: "now".into(),
            updated_at: "now".into(),
            last_connected_at: None,
            os_distribution: None,
        }];
        let yaml = r#"
profiles:
  - type: ssh
    name: Dup
    options:
      host: 10.0.0.5
      user: root
"#;
        let parsed = parse_terminus_yaml(yaml, &existing).expect("parse");
        assert!(parsed[0].warnings.iter().any(|w| w == "willOverwrite"));
        assert!(parsed[0].warnings.iter().any(|w| w == "passwordSkipped"));
    }
}
