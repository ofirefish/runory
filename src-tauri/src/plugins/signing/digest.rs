use std::fs::File;
use std::io::Read;
use std::path::Path;

use ring::digest::{self, SHA256};
use zip::ZipArchive;

use super::model::SIGNATURE_ENTRY;
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::installer::package::{
    validate_zip_entry_name, MAX_EXTRACTED_BYTES, MAX_FILE_COUNT, MAX_PACKAGE_BYTES,
    MAX_SINGLE_FILE_BYTES,
};

pub fn read_zip_entries(path: &Path) -> PluginResult<Vec<(String, Vec<u8>)>> {
    let meta = std::fs::metadata(path).map_err(|_| PluginError::InvalidPackage)?;
    if !meta.is_file() || meta.len() > MAX_PACKAGE_BYTES {
        return Err(PluginError::InvalidPackage);
    }
    let file = File::open(path).map_err(|_| PluginError::InvalidPackage)?;
    let mut archive = ZipArchive::new(file).map_err(|_| PluginError::InvalidPackage)?;
    if archive.len() == 0 || archive.len() > MAX_FILE_COUNT {
        return Err(PluginError::InvalidPackage);
    }
    let mut entries = Vec::with_capacity(archive.len());
    let mut total = 0u64;
    let mut names = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| PluginError::InvalidPackage)?;
        let name = entry.name().to_owned();
        validate_zip_entry_name(&name)?;
        if entry.is_symlink() || !names.insert(name.trim_end_matches('/').to_ascii_lowercase()) {
            return Err(PluginError::InvalidPackage);
        }
        if name.ends_with('/') {
            continue;
        }
        if entry.size() > MAX_SINGLE_FILE_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        let mut bytes = Vec::new();
        entry
            .take(MAX_SINGLE_FILE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| PluginError::InvalidPackage)?;
        if bytes.len() as u64 > MAX_SINGLE_FILE_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        total = total.saturating_add(bytes.len() as u64);
        if total > MAX_EXTRACTED_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        entries.push((normalize_entry_name(&name), bytes));
    }
    Ok(entries)
}

fn normalize_entry_name(name: &str) -> String {
    name.trim_start_matches("./").replace('\\', "/")
}

pub fn is_signature_entry(path: &str) -> bool {
    path == SIGNATURE_ENTRY
}

/// Canonical package digest bytes (SHA-256).
pub fn canonical_digest(entries: &[(String, Vec<u8>)]) -> [u8; 32] {
    let mut sorted: Vec<&(String, Vec<u8>)> = entries
        .iter()
        .filter(|(path, _)| !is_signature_entry(path))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut ctx = digest::Context::new(&SHA256);
    for (path, bytes) in sorted {
        let path_bytes = path.as_bytes();
        ctx.update(&(path_bytes.len() as u64).to_le_bytes());
        ctx.update(path_bytes);
        ctx.update(&(bytes.len() as u64).to_le_bytes());
        ctx.update(bytes);
    }
    let finished = ctx.finish();
    let mut out = [0u8; 32];
    out.copy_from_slice(finished.as_ref());
    out
}

pub fn canonical_digest_hex(entries: &[(String, Vec<u8>)]) -> String {
    hex_encode(&canonical_digest(entries))
}

pub fn canonical_digest_from_zip(path: &Path) -> PluginResult<[u8; 32]> {
    Ok(canonical_digest(&read_zip_entries(path)?))
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(input: &str) -> PluginResult<Vec<u8>> {
    if input.len() % 2 != 0 {
        return Err(PluginError::SignatureInvalid);
    }
    (0..input.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&input[i..i + 2], 16).map_err(|_| PluginError::SignatureInvalid)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_order_independent_after_sort() {
        let a = vec![
            ("b.txt".into(), b"2".to_vec()),
            ("a.txt".into(), b"1".to_vec()),
        ];
        let b = vec![
            ("a.txt".into(), b"1".to_vec()),
            ("b.txt".into(), b"2".to_vec()),
        ];
        assert_eq!(canonical_digest(&a), canonical_digest(&b));
    }

    #[test]
    fn signature_entry_excluded() {
        let with_sig = vec![
            ("manifest.json".into(), b"{}".to_vec()),
            (SIGNATURE_ENTRY.into(), b"sig".to_vec()),
        ];
        let without = vec![("manifest.json".into(), b"{}".to_vec())];
        assert_eq!(canonical_digest(&with_sig), canonical_digest(&without));
    }

    #[test]
    fn nested_signature_named_content_is_covered_by_digest() {
        let path = "templates/META-INF/signature.json".to_owned();
        assert!(!is_signature_entry(&path));
        assert_ne!(
            canonical_digest(&[(path.clone(), b"original".to_vec())]),
            canonical_digest(&[(path, b"changed".to_vec())])
        );
    }
}
