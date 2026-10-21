use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::ZipArchive;

use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::manifest::{
    parse_manifest_json, validate_compatibility, validate_manifest, PluginManifest,
};

pub const MAX_PACKAGE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_EXTRACTED_BYTES: u64 = 200 * 1024 * 1024;
pub const MAX_SINGLE_FILE_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_FILE_COUNT: usize = 2_000;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPackageInspection {
    pub manifest: PluginManifest,
    pub package_path: String,
    pub file_count: usize,
    pub compressed_size: u64,
}

pub fn inspect_package(path: &Path, runory_version: &str) -> PluginResult<PluginPackageInspection> {
    let meta = fs::metadata(path).map_err(|_| PluginError::InvalidPackage)?;
    if !meta.is_file() || meta.len() > MAX_PACKAGE_BYTES {
        return Err(PluginError::InvalidPackage);
    }
    let file = File::open(path).map_err(|_| PluginError::InvalidPackage)?;
    let mut archive = ZipArchive::new(file).map_err(|_| PluginError::InvalidPackage)?;
    let mut manifest_bytes = None;
    let mut entry_names = std::collections::HashSet::new();
    let mut extracted_estimate = 0u64;
    let file_count = archive.len();
    if file_count == 0 || file_count > MAX_FILE_COUNT {
        return Err(PluginError::InvalidPackage);
    }
    for index in 0..file_count {
        let entry = archive
            .by_index(index)
            .map_err(|_| PluginError::InvalidPackage)?;
        validate_zip_entry_name(entry.name())?;
        // ZIP aliases can make the inspected manifest differ from the extracted one.
        let normalized = entry.name().trim_end_matches('/').to_ascii_lowercase();
        if !entry_names.insert(normalized) {
            return Err(PluginError::InvalidPackage);
        }
        if entry.is_symlink() {
            return Err(PluginError::InvalidPackage);
        }
        let size = entry.size();
        if size > MAX_SINGLE_FILE_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        extracted_estimate = extracted_estimate.saturating_add(size);
        if extracted_estimate > MAX_EXTRACTED_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        if entry.name() == "manifest.json" {
            let mut bytes = Vec::new();
            let mut limited = entry.take(MAX_SINGLE_FILE_BYTES.saturating_add(1));
            limited
                .read_to_end(&mut bytes)
                .map_err(|_| PluginError::InvalidPackage)?;
            if bytes.len() as u64 > MAX_SINGLE_FILE_BYTES {
                return Err(PluginError::InvalidPackage);
            }
            manifest_bytes = Some(bytes);
        }
    }
    let bytes = manifest_bytes.ok_or(PluginError::InvalidPackage)?;
    let manifest = parse_manifest_json(&bytes)?;
    validate_manifest(&manifest)?;
    validate_compatibility(&manifest, runory_version)?;
    if let Some(logo) = &manifest.logo {
        validate_logo_in_archive(path, logo)?;
    }
    Ok(PluginPackageInspection {
        manifest,
        package_path: path.display().to_string(),
        file_count,
        compressed_size: meta.len(),
    })
}

fn validate_logo_in_archive(package: &Path, logo: &str) -> PluginResult<()> {
    use crate::plugins::manifest::logo::{validate_logo_path, MAX_LOGO_BYTES};
    validate_logo_path(logo)?;
    let file = File::open(package).map_err(|_| PluginError::InvalidPackage)?;
    let mut archive = ZipArchive::new(file).map_err(|_| PluginError::InvalidPackage)?;
    let normalized = logo.replace('\\', "/");
    let mut found = false;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| PluginError::InvalidPackage)?;
        let name = entry.name().trim_start_matches("./");
        if name == normalized || name.ends_with(&format!("/{normalized}")) {
            if entry.size() > MAX_LOGO_BYTES {
                return Err(PluginError::InvalidManifest(format!(
                    "logo exceeds {MAX_LOGO_BYTES} bytes"
                )));
            }
            found = true;
            break;
        }
    }
    if !found {
        return Err(PluginError::InvalidManifest(format!(
            "logo file missing: {logo}"
        )));
    }
    Ok(())
}

pub fn unpack_package(package: &Path, destination: &Path) -> PluginResult<()> {
    if destination.exists() {
        return Err(PluginError::AlreadyInstalled);
    }
    let parent = destination.parent().ok_or(PluginError::Storage)?;
    fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
    let staging = parent.join(format!(
        ".tmp-{}-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin"),
        uuid::Uuid::new_v4()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|_| PluginError::Storage)?;
    }
    fs::create_dir_all(&staging).map_err(|_| PluginError::Storage)?;

    let result = (|| -> PluginResult<()> {
        let meta = fs::metadata(package).map_err(|_| PluginError::InvalidPackage)?;
        if meta.len() > MAX_PACKAGE_BYTES {
            return Err(PluginError::InvalidPackage);
        }
        let file = File::open(package).map_err(|_| PluginError::InvalidPackage)?;
        let mut archive = ZipArchive::new(file).map_err(|_| PluginError::InvalidPackage)?;
        let mut extracted = 0u64;
        if archive.len() > MAX_FILE_COUNT {
            return Err(PluginError::InvalidPackage);
        }
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| PluginError::InvalidPackage)?;
            let name = entry.name().to_owned();
            validate_zip_entry_name(&name)?;
            if entry.is_symlink() {
                return Err(PluginError::InvalidPackage);
            }
            let out_path = safe_join(&staging, &name)?;
            if name.ends_with('/') {
                fs::create_dir_all(&out_path).map_err(|_| PluginError::Storage)?;
                continue;
            }
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
            }
            let size = entry.size();
            if size > MAX_SINGLE_FILE_BYTES {
                return Err(PluginError::InvalidPackage);
            }
            extracted = extracted.saturating_add(size);
            if extracted > MAX_EXTRACTED_BYTES {
                return Err(PluginError::InvalidPackage);
            }
            let mut outfile = File::create(&out_path).map_err(|_| PluginError::Storage)?;
            let mut buffer = Vec::new();
            entry
                .take(MAX_SINGLE_FILE_BYTES.saturating_add(1))
                .read_to_end(&mut buffer)
                .map_err(|_| PluginError::InvalidPackage)?;
            if buffer.len() as u64 > MAX_SINGLE_FILE_BYTES {
                return Err(PluginError::InvalidPackage);
            }
            outfile
                .write_all(&buffer)
                .map_err(|_| PluginError::Storage)?;
        }
        let manifest_path = staging.join("manifest.json");
        if !manifest_path.is_file() {
            return Err(PluginError::InvalidPackage);
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            fs::rename(&staging, destination).map_err(|_| {
                let _ = fs::remove_dir_all(&staging);
                PluginError::Storage
            })?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

pub fn validate_zip_entry_name(name: &str) -> PluginResult<()> {
    if name.is_empty()
        || name.contains('\0')
        || name.contains('\\')
        || name
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || name.trim_end_matches('/').split('/').any(|part| {
            let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            part.is_empty()
                || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || ((base.starts_with("COM") || base.starts_with("LPT"))
                    && base.len() == 4
                    && matches!(base.as_bytes()[3], b'1'..=b'9'))
        })
        || name
            .split('/')
            .any(|part| part == "." || part == ".." || part.ends_with('.') || part.ends_with(' '))
    {
        return Err(PluginError::InvalidPackage);
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return Err(PluginError::InvalidPackage);
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PluginError::InvalidPackage);
            }
        }
    }
    // Reject Windows drive-style absolute paths inside ZIP names.
    if name.chars().nth(1) == Some(':') {
        return Err(PluginError::InvalidPackage);
    }
    Ok(())
}

pub fn safe_join(root: &Path, relative: &str) -> PluginResult<PathBuf> {
    validate_zip_entry_name(relative)?;
    let joined = root.join(relative);
    let root_canon = root;
    if !joined.starts_with(root_canon) {
        return Err(PluginError::InvalidPackage);
    }
    Ok(joined)
}

pub fn cleanup_stale_temp_dirs(plugins_root: &Path) -> PluginResult<()> {
    if !plugins_root.exists() {
        return Ok(());
    }
    let entries = fs::read_dir(plugins_root).map_err(|_| PluginError::Storage)?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if text.starts_with(".tmp-") {
            let _ = fs::remove_dir_all(entry.path());
        }
        // Also scan version directories one level deeper for plugin id folders.
        if entry.path().is_dir() && !text.starts_with('.') {
            if let Ok(inner) = fs::read_dir(entry.path()) {
                for child in inner.flatten() {
                    let child_name = child.file_name();
                    if child_name.to_string_lossy().starts_with(".tmp-") {
                        let _ = fs::remove_dir_all(child.path());
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_hello_zip(path: &Path) {
        let file = File::create(path).expect("create");
        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("manifest.json", options).expect("start");
        zip.write_all(
            br#"{
  "schemaVersion": 1,
  "id": "runory.hello",
  "name": "Runory Hello",
  "version": "1.0.0",
  "publisher": { "id": "runory", "name": "Runory" },
  "apiVersion": "1",
  "minRunoryVersion": "0.1.0",
  "permissions": ["host.read", "ssh.exec"],
  "contributes": {
    "commands": [{ "id": "hello.inspect", "title": "Inspect Host" }],
    "hostMenus": [{ "command": "hello.inspect", "group": "runory", "when": "host.os == 'linux'" }],
    "workflows": [{ "id": "hello.inspect", "file": "workflows/hello.yaml" }]
  }
}"#,
        )
        .expect("write");
        zip.start_file("workflows/hello.yaml", options).expect("wf");
        zip.write_all(
            br#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: hello.inspect
  name: Inspect Linux Host
steps:
  - id: detect
    uses: system.detect
  - id: uname
    uses: ssh.command
    with:
      executable: uname
      args: ["-a"]
"#,
        )
        .expect("wf body");
        zip.finish().expect("finish");
    }

    #[test]
    fn inspects_valid_package() {
        let dir = tempfile::tempdir().expect("temp");
        let zip_path = dir.path().join("hello.runory-plugin");
        write_hello_zip(&zip_path);
        let inspection = inspect_package(&zip_path, "0.1.1").expect("inspect");
        assert_eq!(inspection.manifest.id, "runory.hello");
    }

    #[test]
    fn rejects_zip_slip() {
        let dir = tempfile::tempdir().expect("temp");
        let zip_path = dir.path().join("evil.zip");
        let file = File::create(&zip_path).expect("create");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        zip.start_file("../evil.txt", options).expect("start");
        zip.write_all(b"x").expect("write");
        zip.finish().expect("finish");
        assert!(matches!(
            inspect_package(&zip_path, "0.1.1"),
            Err(PluginError::InvalidPackage)
        ));
    }

    #[test]
    fn rejects_absolute_path_entry() {
        assert!(matches!(
            validate_zip_entry_name("/tmp/evil"),
            Err(PluginError::InvalidPackage)
        ));
        assert!(matches!(
            validate_zip_entry_name("C:/evil"),
            Err(PluginError::InvalidPackage)
        ));
    }

    #[test]
    fn unpack_is_atomic_on_success() {
        let dir = tempfile::tempdir().expect("temp");
        let zip_path = dir.path().join("hello.runory-plugin");
        write_hello_zip(&zip_path);
        let dest = dir.path().join("runory.hello").join("1.0.0");
        unpack_package(&zip_path, &dest).expect("unpack");
        assert!(dest.join("manifest.json").is_file());
    }

    #[test]
    fn rejects_cross_platform_path_aliases() {
        for path in [
            "./manifest.json",
            "folder/../manifest.json",
            "folder\\manifest.json",
            "manifest.json:stream",
            "folder./file",
            "folder /file",
            "folder//file",
            "NUL.txt",
            "folder/COM1",
            "file?",
        ] {
            assert!(validate_zip_entry_name(path).is_err(), "{path}");
        }
    }

    #[test]
    fn nested_manifest_cannot_override_root_manifest() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("nested.zip");
        write_hello_zip(&path);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open");
        let mut archive = ZipWriter::new_append(file).expect("append");
        archive
            .start_file("nested/manifest.json", SimpleFileOptions::default())
            .expect("file");
        archive.write_all(b"not a manifest").expect("write");
        archive.finish().expect("finish");
        assert_eq!(
            inspect_package(&path, "0.1.1")
                .expect("root manifest")
                .manifest
                .id,
            "runory.hello"
        );
    }

    #[test]
    fn rejects_case_aliases_before_extracting() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("alias.zip");
        write_hello_zip(&path);
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open");
        let mut archive = ZipWriter::new_append(file).expect("append");
        archive
            .start_file("MANIFEST.JSON", SimpleFileOptions::default())
            .expect("file");
        archive.write_all(b"different bytes").expect("write");
        archive.finish().expect("finish");
        assert!(matches!(
            inspect_package(&path, "0.1.1"),
            Err(PluginError::InvalidPackage)
        ));
        assert!(crate::plugins::signing::digest::read_zip_entries(&path).is_err());
    }
}
