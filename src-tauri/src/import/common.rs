//! Shared catalog import types and apply pipeline.
//!
//! Security invariants:
//! - Plaintext passwords are held only inside Rust during parse/apply, then written to CredentialVault.
//! - Passwords never appear in preview IPC DTOs, Profile JSON, or logs.
//! - Encrypted third-party password blobs are not decrypted.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult, AuthMethod, CredentialKind, KeySource, ServerProfile};
use crate::groups::GroupService;
use crate::profiles::ProfileService;

pub const MAX_IMPORT_PROFILES: usize = 500;
pub const MAX_CONFIG_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_DIRECTORY_FILES: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportSource {
    Terminus,
    Openssh,
    Xshell,
    Securecrt,
    Mobaxterm,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImportPreviewRequest {
    pub source: ImportSource,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImportApplyRequest {
    pub source: ImportSource,
    pub path: String,
    pub selected_keys: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImportPreview {
    pub profile_count: usize,
    pub importable_count: usize,
    pub profiles: Vec<CatalogImportProfilePreview>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImportProfilePreview {
    pub import_key: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group_name: Option<String>,
    pub auth_method: AuthMethod,
    pub key_path: Option<String>,
    pub has_password: bool,
    pub importable: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogImportApplyResult {
    pub groups_created: usize,
    pub profiles_created: usize,
    pub profiles_updated: usize,
    pub jumps_wired: usize,
    pub passwords_stored: usize,
    pub skipped: usize,
}

pub struct ParsedImportProfile {
    pub import_key: String,
    pub source_id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group_name: Option<String>,
    pub auth_method: AuthMethod,
    pub key_path: Option<String>,
    pub password: Option<Zeroizing<String>>,
    pub jump_source_id: Option<String>,
    pub importable: bool,
    pub warnings: Vec<String>,
}

impl fmt::Debug for ParsedImportProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedImportProfile")
            .field("import_key", &self.import_key)
            .field("source_id", &self.source_id)
            .field("name", &self.name)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("group_name", &self.group_name)
            .field("auth_method", &self.auth_method)
            .field("key_path", &self.key_path)
            .field("has_password", &self.password.is_some())
            .field("jump_source_id", &self.jump_source_id)
            .field("importable", &self.importable)
            .field("warnings", &self.warnings)
            .finish()
    }
}

impl ParsedImportProfile {
    pub fn into_preview(self) -> CatalogImportProfilePreview {
        CatalogImportProfilePreview {
            import_key: self.import_key,
            name: self.name,
            host: self.host,
            port: self.port,
            username: self.username,
            group_name: self.group_name,
            auth_method: self.auth_method,
            key_path: self.key_path,
            has_password: self.password.is_some(),
            importable: self.importable,
            warnings: self.warnings,
        }
    }
}

pub fn take_plaintext_password(raw: Option<String>) -> Option<Zeroizing<String>> {
    let value = raw?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(Zeroizing::new(value))
    }
}

pub fn endpoint_importable(name: &str, host: &str, port: u16, username: &str) -> bool {
    !host.is_empty()
        && !host.contains(char::is_whitespace)
        && port != 0
        && !username.is_empty()
        && !name.is_empty()
        && name.len() <= 100
        && host.len() <= 255
        && username.len() <= 255
}

pub fn mark_already_exists(
    warnings: &mut Vec<String>,
    importable: bool,
    existing: &[ServerProfile],
    host: &str,
    port: u16,
    username: &str,
) {
    if importable
        && existing.iter().any(|profile| {
            profile.host == host && profile.port == port && profile.username == username
        })
    {
        warnings.push("willOverwrite".into());
    }
}

pub fn find_matching_profile<'a>(
    existing: &'a [ServerProfile],
    host: &str,
    port: u16,
    username: &str,
) -> Option<&'a ServerProfile> {
    existing.iter().find(|profile| {
        profile.host == host && profile.port == port && profile.username == username
    })
}

pub fn validate_absolute_path(path: &str) -> AppResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::LocalFileInvalid);
    }
    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err(AppError::LocalFileInvalid);
    }
    Ok(candidate)
}

pub fn extension_matches(path: &Path, allowed: &[&str]) -> bool {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        return allowed.iter().any(|item| *item == "");
    };
    let lower = ext.to_ascii_lowercase();
    allowed.iter().any(|item| *item == lower)
}

pub async fn read_file_bytes(path: &Path) -> AppResult<Vec<u8>> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|_| AppError::LocalFileInvalid)?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(AppError::LocalFileInvalid);
    }
    tokio::fs::read(path)
        .await
        .map_err(|_| AppError::LocalFileInvalid)
}

pub fn decode_config_text(bytes: &[u8]) -> AppResult<String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_le(&bytes[2..]);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_be(&bytes[2..]);
    }
    // UTF-16 LE without BOM: many null high bytes in even positions
    if bytes.len() >= 4 && bytes.len() % 2 == 0 {
        let nullish = bytes.chunks_exact(2).filter(|pair| pair[1] == 0).count();
        if nullish * 2 >= bytes.len() / 2 {
            return decode_utf16_le(bytes);
        }
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| AppError::ImportInvalid)
}

fn decode_utf16_le(bytes: &[u8]) -> AppResult<String> {
    if bytes.len() % 2 != 0 {
        return Err(AppError::ImportInvalid);
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| AppError::ImportInvalid)
}

fn decode_utf16_be(bytes: &[u8]) -> AppResult<String> {
    if bytes.len() % 2 != 0 {
        return Err(AppError::ImportInvalid);
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| AppError::ImportInvalid)
}

pub async fn collect_files_with_extensions(
    root: &Path,
    extensions: &[&str],
) -> AppResult<Vec<PathBuf>> {
    let metadata = tokio::fs::metadata(root)
        .await
        .map_err(|_| AppError::LocalFileInvalid)?;
    if metadata.is_file() {
        if extension_matches(root, extensions) {
            return Ok(vec![root.to_path_buf()]);
        }
        return Err(AppError::ImportInvalid);
    }
    if !metadata.is_dir() {
        return Err(AppError::LocalFileInvalid);
    }

    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|_| AppError::LocalFileInvalid)?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|_| AppError::LocalFileInvalid)?
        {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|_| AppError::LocalFileInvalid)?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() && extension_matches(&path, extensions) {
                files.push(path);
                if files.len() > MAX_DIRECTORY_FILES {
                    return Err(AppError::ImportInvalid);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

pub fn relative_group_name(root: &Path, file: &Path) -> Option<String> {
    let parent = file.parent()?;
    let relative = parent.strip_prefix(root).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    let name = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub fn file_stem_name(path: &Path) -> String {
    path.file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Imported SSH".into())
}

pub async fn apply_parsed_profiles(
    parsed: Vec<ParsedImportProfile>,
    selected_keys: &[String],
    groups: &GroupService,
    profiles: &ProfileService,
    credentials: &CredentialService,
) -> AppResult<CatalogImportApplyResult> {
    if selected_keys.is_empty() {
        return Ok(CatalogImportApplyResult {
            groups_created: 0,
            profiles_created: 0,
            profiles_updated: 0,
            jumps_wired: 0,
            passwords_stored: 0,
            skipped: 0,
        });
    }
    if selected_keys.len() > MAX_IMPORT_PROFILES {
        return Err(AppError::ImportInvalid);
    }

    let selected: HashSet<&str> = selected_keys.iter().map(String::as_str).collect();
    let mut to_import: Vec<ParsedImportProfile> = parsed
        .into_iter()
        .filter(|item| item.importable && selected.contains(item.import_key.as_str()))
        .collect();

    if to_import.is_empty() {
        return Ok(CatalogImportApplyResult {
            groups_created: 0,
            profiles_created: 0,
            profiles_updated: 0,
            jumps_wired: 0,
            passwords_stored: 0,
            skipped: selected.len(),
        });
    }
    if to_import.len() > MAX_IMPORT_PROFILES {
        return Err(AppError::ImportInvalid);
    }

    if to_import.iter().any(|item| item.password.is_some()) {
        credentials.ensure_unlocked()?;
    }

    to_import.sort_by(|left, right| {
        let left_is_jump = left.jump_source_id.is_some();
        let right_is_jump = right.jump_source_id.is_some();
        left_is_jump
            .cmp(&right_is_jump)
            .then_with(|| left.name.cmp(&right.name))
    });

    let group_names: Vec<String> = to_import
        .iter()
        .filter_map(|item| item.group_name.clone())
        .collect();
    let (group_ids, groups_created) = groups.ensure_groups_by_name(&group_names).await?;

    let upserts = to_import
        .into_iter()
        .map(|item| {
            let group_id = item
                .group_name
                .as_ref()
                .and_then(|name| group_ids.get(name).copied());
            let key_source = match (&item.auth_method, &item.key_path) {
                (AuthMethod::PrivateKey, Some(path)) => {
                    Some(KeySource::File { path: path.clone() })
                }
                _ => None,
            };
            crate::profiles::ImportUpsertRequest {
                name: item.name,
                host: item.host,
                port: item.port,
                username: item.username,
                group_id,
                auth_method: item.auth_method,
                key_source,
                source_id: item.source_id,
                jump_source_id: item.jump_source_id,
                password: item.password,
            }
        })
        .collect::<Vec<_>>();

    let batch = profiles.upsert_import_batch(upserts).await?;

    let password_entries: Vec<_> = batch
        .outcomes
        .into_iter()
        .filter_map(|outcome| {
            outcome
                .password
                .map(|password| (outcome.profile_id, CredentialKind::Password, password))
        })
        .collect();
    let passwords_stored = password_entries.len();
    credentials.remember_many(password_entries).await?;

    let applied = batch.profiles_created + batch.profiles_updated;
    let skipped = selected.len().saturating_sub(applied);
    Ok(CatalogImportApplyResult {
        groups_created,
        profiles_created: batch.profiles_created,
        profiles_updated: batch.profiles_updated,
        jumps_wired: batch.jumps_wired,
        passwords_stored,
        skipped,
    })
}

pub fn preview_from_parsed(parsed: Vec<ParsedImportProfile>) -> CatalogImportPreview {
    CatalogImportPreview {
        profile_count: parsed.len(),
        importable_count: parsed.iter().filter(|item| item.importable).count(),
        profiles: parsed
            .into_iter()
            .map(ParsedImportProfile::into_preview)
            .collect(),
    }
}
