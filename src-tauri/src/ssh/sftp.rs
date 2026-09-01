use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use russh_sftp::client::fs::Metadata;
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;

use crate::domain::{
    AppError, AppResult, RemoteImagePreview, RemoteTextPreview, SftpDirectory, SftpEntry,
    SftpEntryKind, SftpMetadata,
};

const MAX_REMOTE_PATH_BYTES: usize = 4096;
const MAX_DELETE_DEPTH: usize = 64;
const MAX_DELETE_ENTRIES: usize = 10_000;
const MAX_AI_FILE_BYTES: usize = 512 * 1024;
const MAX_IMAGE_PREVIEW_BYTES: usize = 12 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_TEXT_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEXT_PREVIEW_LINES: usize = 50_000;

pub(crate) struct SftpChannel {
    session: Arc<SftpSession>,
    current_directory: RwLock<String>,
}

impl SftpChannel {
    pub async fn new(session: SftpSession) -> AppResult<Self> {
        session.set_timeout(15);
        let current_directory = session.canonicalize(".").await.map_err(map_sftp_error)?;
        validate_remote_path(&current_directory)?;
        Ok(Self {
            session: Arc::new(session),
            current_directory: RwLock::new(current_directory),
        })
    }

    pub(crate) fn session(&self) -> Arc<SftpSession> {
        Arc::clone(&self.session)
    }

    pub async fn list_directory(&self, path: &str) -> AppResult<SftpDirectory> {
        let path = self.resolve_directory(path).await?;
        let entries = self
            .session
            .read_dir(path.clone())
            .await
            .map_err(map_sftp_error)?;
        let mut entries = entries
            .filter(|entry| !matches!(entry.file_name().as_str(), "." | ".."))
            .map(|entry| {
                let metadata = entry.metadata();
                SftpEntry {
                    name: entry.file_name(),
                    path: entry.path(),
                    kind: entry_kind(entry.file_type()),
                    size: metadata.size,
                    modified: metadata.mtime,
                    permissions: metadata.permissions,
                }
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            entry_rank(&left.kind)
                .cmp(&entry_rank(&right.kind))
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        Ok(SftpDirectory { path, entries })
    }

    pub async fn stat(&self, path: &str) -> AppResult<SftpMetadata> {
        let path = self.resolve_path(path).await?;
        let metadata = self
            .session
            .metadata(path.clone())
            .await
            .map_err(map_sftp_error)?;
        Ok(metadata_response(path, metadata))
    }

    pub async fn change_directory(&self, path: &str) -> AppResult<SftpDirectory> {
        let directory = self.list_directory(path).await?;
        *self.current_directory.write().await = directory.path.clone();
        Ok(directory)
    }

    pub async fn refresh(&self) -> AppResult<SftpDirectory> {
        let current = self.current_directory.read().await.clone();
        self.list_directory(&current).await
    }

    pub async fn create_directory(&self, parent: &str, name: &str) -> AppResult<SftpDirectory> {
        let parent = self.resolve_directory(parent).await?;
        let path = destination_in_directory(&parent, name)?;
        self.session
            .create_dir(path)
            .await
            .map_err(map_sftp_error)?;
        self.list_directory(&parent).await
    }

    pub async fn rename(&self, path: &str, new_name: &str) -> AppResult<SftpDirectory> {
        let source = self.resolve_entry(path).await?;
        let parent = remote_parent(&source)?;
        let destination = destination_in_directory(&parent, new_name)?;
        if self
            .session
            .try_exists(destination.clone())
            .await
            .map_err(map_sftp_error)?
        {
            return Err(AppError::SftpAlreadyExists);
        }
        self.session
            .rename(source, destination)
            .await
            .map_err(map_sftp_error)?;
        self.list_directory(&parent).await
    }

    pub async fn delete(&self, path: &str, recursive: bool) -> AppResult<SftpDirectory> {
        let path = self.resolve_entry(path).await?;
        let parent = remote_parent(&path)?;
        let metadata = self
            .session
            .symlink_metadata(path.clone())
            .await
            .map_err(map_sftp_error)?;
        let kind = metadata
            .permissions
            .map(russh_sftp::protocol::FileType::from)
            .unwrap_or(russh_sftp::protocol::FileType::Other);
        if kind.is_dir() {
            if recursive {
                let mut removed = 0usize;
                self.delete_directory_tree(path, 0, &mut removed).await?;
            } else {
                self.session
                    .remove_dir(path)
                    .await
                    .map_err(map_sftp_error)?;
            }
        } else {
            // symlink_metadata ensures a symlink is removed rather than following it to its target.
            self.session
                .remove_file(path)
                .await
                .map_err(map_sftp_error)?;
        }
        self.list_directory(&parent).await
    }

    pub(crate) async fn resolve_upload_destination(
        &self,
        directory: &str,
        name: &str,
    ) -> AppResult<String> {
        let directory = self.resolve_directory(directory).await?;
        destination_in_directory(&directory, name)
    }

    pub(crate) async fn resolve_download_source(&self, path: &str) -> AppResult<(String, u64)> {
        let path = self.resolve_entry(path).await?;
        let metadata = self
            .session
            .metadata(path.clone())
            .await
            .map_err(map_sftp_error)?;
        let kind = metadata
            .permissions
            .map(russh_sftp::protocol::FileType::from)
            .unwrap_or(russh_sftp::protocol::FileType::Other);
        if !kind.is_file() {
            return Err(AppError::SftpNotFile);
        }
        Ok((path, metadata.size.unwrap_or(0)))
    }

    pub(crate) async fn read_text(&self, path: &str) -> AppResult<(String, String)> {
        let (path, size) = self.resolve_download_source(path).await?;
        if size > MAX_AI_FILE_BYTES as u64 {
            return Err(AppError::InvalidOperation);
        }
        let mut file = self.session.open(&path).await.map_err(map_sftp_error)?;
        let mut bytes = Vec::with_capacity(size as usize);
        file.read_to_end(&mut bytes)
            .await
            .map_err(|_| AppError::SftpOperationFailed)?;
        if bytes.len() > MAX_AI_FILE_BYTES {
            return Err(AppError::InvalidOperation);
        }
        let content = String::from_utf8(bytes).map_err(|_| AppError::InvalidOperation)?;
        Ok((path, content))
    }

    pub(crate) async fn read_image_preview(&self, path: &str) -> AppResult<RemoteImagePreview> {
        let (path, size) = self.resolve_download_source(path).await?;
        if size > MAX_IMAGE_PREVIEW_BYTES as u64 {
            return Err(AppError::SftpImageTooLarge);
        }
        let mut file = self.session.open(&path).await.map_err(map_sftp_error)?;
        let initial_capacity = usize::try_from(size)
            .unwrap_or(MAX_IMAGE_PREVIEW_BYTES)
            .min(MAX_IMAGE_PREVIEW_BYTES);
        let mut bytes = Vec::with_capacity(initial_capacity);
        (&mut file)
            .take((MAX_IMAGE_PREVIEW_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| AppError::SftpOperationFailed)?;
        if bytes.len() > MAX_IMAGE_PREVIEW_BYTES {
            return Err(AppError::SftpImageTooLarge);
        }
        let image = inspect_image(&bytes)?;
        validate_image_dimensions(image.width, image.height)?;
        Ok(RemoteImagePreview {
            name: remote_name(&path)?,
            path,
            mime_type: image.mime_type.to_owned(),
            width: image.width,
            height: image.height,
            size: bytes.len() as u64,
            data_base64: BASE64.encode(bytes),
        })
    }

    pub(crate) async fn read_text_preview(&self, path: &str) -> AppResult<RemoteTextPreview> {
        let (path, size) = self.resolve_download_source(path).await?;
        if size > MAX_TEXT_PREVIEW_BYTES as u64 {
            return Err(AppError::SftpTextTooLarge);
        }
        let mut file = self.session.open(&path).await.map_err(map_sftp_error)?;
        let initial_capacity = usize::try_from(size)
            .unwrap_or(MAX_TEXT_PREVIEW_BYTES)
            .min(MAX_TEXT_PREVIEW_BYTES);
        let mut bytes = Vec::with_capacity(initial_capacity);
        (&mut file)
            .take((MAX_TEXT_PREVIEW_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| AppError::SftpOperationFailed)?;
        if bytes.len() > MAX_TEXT_PREVIEW_BYTES {
            return Err(AppError::SftpTextTooLarge);
        }
        let (encoding, content) = decode_text(&bytes)?;
        let line_count = if content.is_empty() {
            0
        } else {
            content.bytes().filter(|byte| *byte == b'\n').count() + 1
        };
        if line_count > MAX_TEXT_PREVIEW_LINES {
            return Err(AppError::SftpTextTooLarge);
        }
        Ok(RemoteTextPreview {
            name: remote_name(&path)?,
            language: text_language(&path).to_owned(),
            path,
            encoding: encoding.to_owned(),
            size: bytes.len() as u64,
            line_count,
            content,
        })
    }

    pub(crate) async fn write_text(&self, path: &str, content: &str) -> AppResult<(String, u64)> {
        if content.len() > MAX_AI_FILE_BYTES || content.contains('\0') {
            return Err(AppError::InvalidOperation);
        }
        let destination = self.resolve_write_destination(path).await?;
        let parent = remote_parent(&destination)?;
        let temporary = destination_in_directory(
            &parent,
            &format!(".runory-agent-{}.tmp", uuid::Uuid::new_v4()),
        )?;
        let mut file = self
            .session
            .create(&temporary)
            .await
            .map_err(map_sftp_error)?;
        let result = async {
            file.write_all(content.as_bytes())
                .await
                .map_err(|_| AppError::SftpOperationFailed)?;
            file.flush()
                .await
                .map_err(|_| AppError::SftpOperationFailed)?;
            file.sync_all()
                .await
                .map_err(|_| AppError::SftpOperationFailed)?;
            file.close()
                .await
                .map_err(|_| AppError::SftpOperationFailed)?;
            if self.session.try_exists(&destination).await.unwrap_or(false) {
                self.session
                    .remove_file(&destination)
                    .await
                    .map_err(map_sftp_error)?;
            }
            self.session
                .rename(&temporary, &destination)
                .await
                .map_err(map_sftp_error)
        }
        .await;
        if result.is_err() {
            let _ = self.session.remove_file(&temporary).await;
        }
        result?;
        Ok((destination, content.len() as u64))
    }

    pub async fn close(&self) {
        let _ = self.session.close().await;
    }

    async fn resolve_directory(&self, path: &str) -> AppResult<String> {
        let resolved = self.resolve_path(path).await?;
        let metadata = self
            .session
            .metadata(resolved.clone())
            .await
            .map_err(map_sftp_error)?;
        let kind = metadata
            .permissions
            .map(russh_sftp::protocol::FileType::from)
            .unwrap_or(russh_sftp::protocol::FileType::Other);
        if !kind.is_dir() {
            return Err(AppError::SftpNotDirectory);
        }
        Ok(resolved)
    }

    async fn resolve_path(&self, path: &str) -> AppResult<String> {
        validate_remote_path(path)?;
        let current = self.current_directory.read().await.clone();
        let candidate = remote_candidate(&current, path);
        let canonical = self
            .session
            .canonicalize(candidate)
            .await
            .map_err(map_sftp_error)?;
        validate_remote_path(&canonical)?;
        Ok(canonical)
    }

    async fn resolve_entry(&self, path: &str) -> AppResult<String> {
        validate_remote_path(path)?;
        let current = self.current_directory.read().await.clone();
        let candidate = remote_candidate(&current, path);
        let name = remote_name(&candidate)?;
        let parent = remote_parent(&candidate)?;
        let parent = self
            .session
            .canonicalize(parent)
            .await
            .map_err(map_sftp_error)?;
        let resolved = destination_in_directory(&parent, &name)?;
        self.session
            .symlink_metadata(resolved.clone())
            .await
            .map_err(map_sftp_error)?;
        Ok(resolved)
    }

    async fn resolve_write_destination(&self, path: &str) -> AppResult<String> {
        validate_remote_path(path)?;
        let current = self.current_directory.read().await.clone();
        let candidate = remote_candidate(&current, path);
        let name = remote_name(&candidate)?;
        let parent = remote_parent(&candidate)?;
        let parent = self
            .session
            .canonicalize(parent)
            .await
            .map_err(map_sftp_error)?;
        destination_in_directory(&parent, &name)
    }

    fn delete_directory_tree<'a>(
        &'a self,
        path: String,
        depth: usize,
        removed: &'a mut usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send + 'a>> {
        Box::pin(async move {
            if depth >= MAX_DELETE_DEPTH || *removed >= MAX_DELETE_ENTRIES {
                return Err(AppError::SftpDeleteLimitExceeded);
            }
            let entries = self
                .session
                .read_dir(path.clone())
                .await
                .map_err(map_sftp_error)?;
            for entry in entries.filter(|entry| !matches!(entry.file_name().as_str(), "." | "..")) {
                *removed += 1;
                if *removed > MAX_DELETE_ENTRIES {
                    return Err(AppError::SftpDeleteLimitExceeded);
                }
                let entry_path = destination_in_directory(&path, &entry.file_name())?;
                if entry.file_type().is_dir() {
                    self.delete_directory_tree(entry_path, depth + 1, removed)
                        .await?;
                } else {
                    self.session
                        .remove_file(entry_path)
                        .await
                        .map_err(map_sftp_error)?;
                }
            }
            self.session
                .remove_dir(path)
                .await
                .map_err(map_sftp_error)?;
            Ok(())
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ImageInfo {
    mime_type: &'static str,
    width: u32,
    height: u32,
}

fn inspect_image(bytes: &[u8]) -> AppResult<ImageInfo> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
            return Err(AppError::SftpImageInvalid);
        }
        return Ok(ImageInfo {
            mime_type: "image/png",
            width: u32::from_be_bytes(
                bytes[16..20]
                    .try_into()
                    .map_err(|_| AppError::SftpImageInvalid)?,
            ),
            height: u32::from_be_bytes(
                bytes[20..24]
                    .try_into()
                    .map_err(|_| AppError::SftpImageInvalid)?,
            ),
        });
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        if bytes.len() < 10 {
            return Err(AppError::SftpImageInvalid);
        }
        return Ok(ImageInfo {
            mime_type: "image/gif",
            width: u16::from_le_bytes([bytes[6], bytes[7]]) as u32,
            height: u16::from_le_bytes([bytes[8], bytes[9]]) as u32,
        });
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        return inspect_jpeg(bytes);
    }
    if bytes.len() >= 30 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return inspect_webp(bytes);
    }
    Err(AppError::SftpImageUnsupported)
}

fn inspect_jpeg(bytes: &[u8]) -> AppResult<ImageInfo> {
    let mut position = 2usize;
    while position + 1 < bytes.len() {
        if bytes[position] != 0xff {
            return Err(AppError::SftpImageInvalid);
        }
        while position < bytes.len() && bytes[position] == 0xff {
            position += 1;
        }
        if position >= bytes.len() {
            break;
        }
        let marker = bytes[position];
        position += 1;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        if position + 2 > bytes.len() {
            return Err(AppError::SftpImageInvalid);
        }
        let segment_length = u16::from_be_bytes([bytes[position], bytes[position + 1]]) as usize;
        if segment_length < 2 || position + segment_length > bytes.len() {
            return Err(AppError::SftpImageInvalid);
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if segment_length < 7 {
                return Err(AppError::SftpImageInvalid);
            }
            return Ok(ImageInfo {
                mime_type: "image/jpeg",
                height: u16::from_be_bytes([bytes[position + 3], bytes[position + 4]]) as u32,
                width: u16::from_be_bytes([bytes[position + 5], bytes[position + 6]]) as u32,
            });
        }
        position += segment_length;
    }
    Err(AppError::SftpImageInvalid)
}

fn inspect_webp(bytes: &[u8]) -> AppResult<ImageInfo> {
    let (width, height) = match &bytes[12..16] {
        b"VP8X" => (
            1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]),
            1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]),
        ),
        b"VP8L" if bytes[20] == 0x2f => (
            1 + u32::from(bytes[21]) + ((u32::from(bytes[22]) & 0x3f) << 8),
            1 + (u32::from(bytes[22]) >> 6)
                + (u32::from(bytes[23]) << 2)
                + ((u32::from(bytes[24]) & 0x0f) << 10),
        ),
        b"VP8 " if bytes.len() >= 30 && bytes[23..26] == [0x9d, 0x01, 0x2a] => (
            u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff),
            u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff),
        ),
        _ => return Err(AppError::SftpImageInvalid),
    };
    Ok(ImageInfo {
        mime_type: "image/webp",
        width,
        height,
    })
}

fn validate_image_dimensions(width: u32, height: u32) -> AppResult<()> {
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        Err(AppError::SftpImageTooLarge)
    } else {
        Ok(())
    }
}

fn decode_text(bytes: &[u8]) -> AppResult<(&'static str, String)> {
    let (encoding, content) = if let Some(content) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (
            "UTF-8 BOM",
            String::from_utf8(content.to_vec())
                .map_err(|_| AppError::SftpTextEncodingUnsupported)?,
        )
    } else if let Some(content) = bytes.strip_prefix(&[0xff, 0xfe]) {
        ("UTF-16 LE", decode_utf16(content, true)?)
    } else if let Some(content) = bytes.strip_prefix(&[0xfe, 0xff]) {
        ("UTF-16 BE", decode_utf16(content, false)?)
    } else {
        (
            "UTF-8",
            String::from_utf8(bytes.to_vec()).map_err(|_| AppError::SftpTextEncodingUnsupported)?,
        )
    };
    if content.chars().any(|character| {
        character == '\0' || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    }) {
        return Err(AppError::SftpTextUnsupported);
    }
    Ok((encoding, content))
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> AppResult<String> {
    if bytes.len() % 2 != 0 {
        return Err(AppError::SftpTextEncodingUnsupported);
    }
    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    String::from_utf16(&units.collect::<Vec<_>>())
        .map_err(|_| AppError::SftpTextEncodingUnsupported)
}

fn text_language(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" => return "dockerfile",
        "makefile" | "gnumakefile" => return "makefile",
        ".env" | ".gitignore" | ".dockerignore" => return "plaintext",
        _ => {}
    }
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("ts" | "tsx") => "typescript",
        Some("js" | "jsx" | "mjs" | "cjs") => "javascript",
        Some("rs") => "rust",
        Some("py") => "python",
        Some("go") => "go",
        Some("java" | "kt" | "kts") => "jvm",
        Some("c" | "h" | "cc" | "cpp" | "hpp") => "c-cpp",
        Some("sh" | "bash" | "zsh") => "shell",
        Some("ps1" | "psm1") => "powershell",
        Some("json" | "jsonc") => "json",
        Some("yaml" | "yml") => "yaml",
        Some("toml") => "toml",
        Some("xml" | "svg" | "html" | "htm") => "markup",
        Some("md" | "markdown") => "markdown",
        Some("sql") => "sql",
        Some("csv" | "tsv") => "delimited",
        Some("log") => "log",
        Some("ini" | "conf" | "cfg" | "properties") => "config",
        _ => "plaintext",
    }
}

pub(crate) fn validate_entry_name(name: &str) -> AppResult<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\0')
        || name.len() > 255
    {
        Err(AppError::SftpPathInvalid)
    } else {
        Ok(())
    }
}

fn validate_remote_path(path: &str) -> AppResult<()> {
    if path.len() > MAX_REMOTE_PATH_BYTES || path.contains('\0') {
        Err(AppError::SftpPathInvalid)
    } else {
        Ok(())
    }
}

fn remote_candidate(current: &str, path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else if path.is_empty() || path == "." {
        current.to_owned()
    } else if current == "/" {
        format!("/{path}")
    } else {
        format!("{}/{path}", current.trim_end_matches('/'))
    }
}

fn remote_parent(path: &str) -> AppResult<String> {
    let trimmed = path.trim_end_matches('/');
    let index = trimmed.rfind('/').ok_or(AppError::SftpPathInvalid)?;
    if index == 0 {
        Ok("/".to_owned())
    } else {
        Ok(trimmed[..index].to_owned())
    }
}

fn remote_name(path: &str) -> AppResult<String> {
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .ok_or(AppError::SftpPathInvalid)?;
    validate_entry_name(name)?;
    Ok(name.to_owned())
}

fn destination_in_directory(directory: &str, name: &str) -> AppResult<String> {
    validate_entry_name(name)?;
    let path = if directory == "/" {
        format!("/{name}")
    } else {
        format!("{}/{name}", directory.trim_end_matches('/'))
    };
    validate_remote_path(&path)?;
    Ok(path)
}

fn entry_kind(file_type: russh_sftp::protocol::FileType) -> SftpEntryKind {
    if file_type.is_dir() {
        SftpEntryKind::Directory
    } else if file_type.is_file() {
        SftpEntryKind::File
    } else if file_type.is_symlink() {
        SftpEntryKind::Symlink
    } else {
        SftpEntryKind::Other
    }
}

fn entry_rank(kind: &SftpEntryKind) -> u8 {
    match kind {
        SftpEntryKind::Directory => 0,
        SftpEntryKind::Symlink => 1,
        SftpEntryKind::File => 2,
        SftpEntryKind::Other => 3,
    }
}

fn metadata_response(path: String, metadata: Metadata) -> SftpMetadata {
    let kind = metadata
        .permissions
        .map(russh_sftp::protocol::FileType::from)
        .map(entry_kind)
        .unwrap_or(SftpEntryKind::Other);
    SftpMetadata {
        path,
        kind,
        size: metadata.size,
        modified: metadata.mtime,
        permissions: metadata.permissions,
    }
}

pub(crate) fn map_sftp_error(error: russh_sftp::client::error::Error) -> AppError {
    use russh_sftp::protocol::StatusCode;
    match error {
        russh_sftp::client::error::Error::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => AppError::SftpNotFound,
            StatusCode::PermissionDenied => AppError::SftpPermissionDenied,
            _ => AppError::SftpOperationFailed,
        },
        _ => AppError::SftpOperationFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_path_validation_rejects_nul_and_oversized_input() {
        assert!(matches!(
            validate_remote_path("bad\0path"),
            Err(AppError::SftpPathInvalid)
        ));
        assert!(matches!(
            validate_remote_path(&"x".repeat(MAX_REMOTE_PATH_BYTES + 1)),
            Err(AppError::SftpPathInvalid)
        ));
        assert!(validate_remote_path("/home/runory").is_ok());
    }

    #[test]
    fn destination_names_cannot_escape_the_selected_directory() {
        for invalid in ["", ".", "..", "nested/file", "bad\0name"] {
            assert!(matches!(
                destination_in_directory("/srv", invalid),
                Err(AppError::SftpPathInvalid)
            ));
        }
        assert_eq!(
            destination_in_directory("/srv", "release.zip").expect("valid destination"),
            "/srv/release.zip"
        );
    }

    #[test]
    fn root_and_nested_remote_parents_are_stable() {
        assert_eq!(remote_parent("/file").expect("root parent"), "/");
        assert_eq!(
            remote_parent("/home/runory/file").expect("nested parent"),
            "/home/runory"
        );
    }

    #[test]
    fn image_headers_are_inspected_without_trusting_extensions() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        let info = inspect_image(&png).expect("valid PNG header");
        assert_eq!(
            info,
            ImageInfo {
                mime_type: "image/png",
                width: 640,
                height: 480
            }
        );

        let gif = b"GIF89a\x20\x03\x58\x02";
        assert_eq!(inspect_image(gif).expect("valid GIF header").width, 800);
        assert!(matches!(
            inspect_image(b"<svg></svg>"),
            Err(AppError::SftpImageUnsupported)
        ));
    }

    #[test]
    fn image_dimension_limits_reject_decompression_bombs() {
        assert!(validate_image_dimensions(4_000, 3_000).is_ok());
        assert!(matches!(
            validate_image_dimensions(10_000, 10_000),
            Err(AppError::SftpImageTooLarge)
        ));
        assert!(matches!(
            validate_image_dimensions(0, 100),
            Err(AppError::SftpImageTooLarge)
        ));
    }

    #[test]
    fn text_preview_decodes_supported_encodings_and_rejects_binary_data() {
        let (encoding, utf8) = decode_text(b"\xef\xbb\xbfhello\nworld").expect("UTF-8 BOM");
        assert_eq!(encoding, "UTF-8 BOM");
        assert_eq!(utf8, "hello\nworld");
        let (_, utf16) = decode_text(&[0xff, 0xfe, b'h', 0, b'i', 0]).expect("UTF-16 LE");
        assert_eq!(utf16, "hi");
        assert!(matches!(
            decode_text(b"text\0binary"),
            Err(AppError::SftpTextUnsupported)
        ));
    }

    #[test]
    fn text_language_uses_filename_and_extension_without_executing_content() {
        assert_eq!(text_language("/srv/app/Dockerfile"), "dockerfile");
        assert_eq!(text_language("/srv/app/main.rs"), "rust");
        assert_eq!(text_language("/srv/app/README"), "plaintext");
    }
}
