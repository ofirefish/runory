//! Tool artifacts for Runtime V2 (AR2-H).
//!
//! Large tool outputs are stored here so model context receives only a
//! summary + reference. Optional search/read/tail helpers let the runtime
//! retrieve bounded slices without injecting full logs into the prompt.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agentic::context::redact_secrets;

/// Outputs larger than this are stored as artifacts instead of inline detail.
pub const ARTIFACT_INLINE_THRESHOLD_BYTES: usize = 4 * 1024;

/// Max bytes returned by a single artifact.read / artifact.tail call.
pub const ARTIFACT_READ_MAX_BYTES: usize = 4 * 1024;

/// Max search hits returned by artifact.search.
pub const ARTIFACT_SEARCH_MAX_HITS: usize = 20;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolArtifact {
    pub id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub tool_name: String,
    /// Redacted, bounded summary for model context.
    pub summary: String,
    /// Full redacted content (never injected wholesale into model context).
    pub content: String,
    pub byte_len: usize,
    pub created_at_epoch_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArtifactReference {
    pub artifact_id: Uuid,
    pub tool_name: String,
    pub summary: String,
    pub byte_len: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactSearchHit {
    pub line_number: usize,
    pub line: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactStoreError {
    NotFound,
    PersistenceFailed,
}

impl ArtifactStoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "ARTIFACT_NOT_FOUND",
            Self::PersistenceFailed => "ARTIFACT_PERSISTENCE_FAILED",
        }
    }
}

pub trait ArtifactStore: Send + Sync {
    fn put(&self, artifact: ToolArtifact) -> Result<(), ArtifactStoreError>;
    fn get(&self, artifact_id: Uuid) -> Result<Option<ToolArtifact>, ArtifactStoreError>;
    fn list_for_run(&self, run_id: Uuid) -> Result<Vec<ToolArtifact>, ArtifactStoreError>;
}

#[derive(Default)]
pub struct InMemoryArtifactStore {
    inner: Mutex<HashMap<Uuid, ToolArtifact>>,
}

impl ArtifactStore for InMemoryArtifactStore {
    fn put(&self, artifact: ToolArtifact) -> Result<(), ArtifactStoreError> {
        self.inner
            .lock()
            .map_err(|_| ArtifactStoreError::PersistenceFailed)?
            .insert(artifact.id, artifact);
        Ok(())
    }

    fn get(&self, artifact_id: Uuid) -> Result<Option<ToolArtifact>, ArtifactStoreError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| ArtifactStoreError::PersistenceFailed)?
            .get(&artifact_id)
            .cloned())
    }

    fn list_for_run(&self, run_id: Uuid) -> Result<Vec<ToolArtifact>, ArtifactStoreError> {
        let mut items: Vec<_> = self
            .inner
            .lock()
            .map_err(|_| ArtifactStoreError::PersistenceFailed)?
            .values()
            .filter(|item| item.run_id == run_id)
            .cloned()
            .collect();
        items.sort_by_key(|item| item.created_at_epoch_ms);
        Ok(items)
    }
}

/// Builds a redacted artifact from large tool output.
pub fn store_large_output(
    store: &dyn ArtifactStore,
    run_id: Uuid,
    tool_call_id: Uuid,
    tool_name: &str,
    summary: &str,
    raw_content: &str,
    created_at_epoch_ms: u64,
) -> Result<ToolArtifact, ArtifactStoreError> {
    let (content, _) = redact_secrets(raw_content);
    let (summary, _) = redact_secrets(summary);
    let artifact = ToolArtifact {
        id: Uuid::new_v4(),
        run_id,
        tool_call_id: Some(tool_call_id),
        tool_name: tool_name.to_owned(),
        summary: truncate_chars(&summary, 300),
        byte_len: content.len(),
        content,
        created_at_epoch_ms,
    };
    store.put(artifact.clone())?;
    Ok(artifact)
}

pub fn should_store_as_artifact(content: &str) -> bool {
    content.len() > ARTIFACT_INLINE_THRESHOLD_BYTES
}

pub fn artifact_reference(artifact: &ToolArtifact) -> ArtifactReference {
    ArtifactReference {
        artifact_id: artifact.id,
        tool_name: artifact.tool_name.clone(),
        summary: artifact.summary.clone(),
        byte_len: artifact.byte_len,
    }
}

/// Case-insensitive line search over an artifact (bounded hits).
pub fn artifact_search(
    store: &dyn ArtifactStore,
    artifact_id: Uuid,
    query: &str,
) -> Result<Vec<ArtifactSearchHit>, ArtifactStoreError> {
    let artifact = store
        .get(artifact_id)?
        .ok_or(ArtifactStoreError::NotFound)?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let lower_query = query.to_ascii_lowercase();
    let mut hits = Vec::new();
    for (index, line) in artifact.content.lines().enumerate() {
        if line.to_ascii_lowercase().contains(&lower_query) {
            hits.push(ArtifactSearchHit {
                line_number: index + 1,
                line: truncate_chars(line, 400),
            });
            if hits.len() >= ARTIFACT_SEARCH_MAX_HITS {
                break;
            }
        }
    }
    Ok(hits)
}

/// Reads a byte window from an artifact (default from start).
pub fn artifact_read(
    store: &dyn ArtifactStore,
    artifact_id: Uuid,
    offset: usize,
    max_bytes: usize,
) -> Result<String, ArtifactStoreError> {
    let artifact = store
        .get(artifact_id)?
        .ok_or(ArtifactStoreError::NotFound)?;
    let max_bytes = max_bytes.min(ARTIFACT_READ_MAX_BYTES).max(1);
    if offset >= artifact.content.len() {
        return Ok(String::new());
    }
    let end = (offset + max_bytes).min(artifact.content.len());
    let mut end = end;
    while end > offset && !artifact.content.is_char_boundary(end) {
        end -= 1;
    }
    let mut start = offset;
    while start < end && !artifact.content.is_char_boundary(start) {
        start += 1;
    }
    Ok(artifact.content[start..end].to_owned())
}

/// Returns the last `max_bytes` of an artifact.
pub fn artifact_tail(
    store: &dyn ArtifactStore,
    artifact_id: Uuid,
    max_bytes: usize,
) -> Result<String, ArtifactStoreError> {
    let artifact = store
        .get(artifact_id)?
        .ok_or(ArtifactStoreError::NotFound)?;
    let max_bytes = max_bytes.min(ARTIFACT_READ_MAX_BYTES).max(1);
    if artifact.content.len() <= max_bytes {
        return Ok(artifact.content.clone());
    }
    let mut start = artifact.content.len() - max_bytes;
    while start < artifact.content.len() && !artifact.content.is_char_boundary(start) {
        start += 1;
    }
    Ok(artifact.content[start..].to_owned())
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_owned();
    }
    input.chars().take(max_chars).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_output_is_stored_redacted_and_searchable() {
        let store = InMemoryArtifactStore::default();
        let mut body = String::new();
        for i in 0..5000 {
            if i == 42 {
                body.push_str("Authorization: Bearer secret-token\n");
            } else {
                body.push_str(&format!("line {i} nginx error upstream\n"));
            }
        }
        assert!(should_store_as_artifact(&body));
        let artifact = store_large_output(
            &store,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "service.logs",
            "nginx journal",
            &body,
            1,
        )
        .expect("store");
        assert!(!artifact.content.contains("secret-token"));
        assert!(artifact.content.contains("[REDACTED]"));
        assert!(artifact.byte_len > ARTIFACT_INLINE_THRESHOLD_BYTES);

        let hits = artifact_search(&store, artifact.id, "upstream").expect("search");
        assert!(!hits.is_empty());
        assert!(hits.len() <= ARTIFACT_SEARCH_MAX_HITS);

        let head = artifact_read(&store, artifact.id, 0, 200).expect("read");
        assert!(head.starts_with("line 0"));
        assert!(head.len() <= ARTIFACT_READ_MAX_BYTES);

        let tail = artifact_tail(&store, artifact.id, 120).expect("tail");
        assert!(tail.contains("4999"));
        assert!(!tail.contains("line 0 nginx"));
    }

    #[test]
    fn full_artifact_content_is_never_the_reference_summary() {
        let store = InMemoryArtifactStore::default();
        let content = "x".repeat(8_000);
        let artifact = store_large_output(
            &store,
            Uuid::new_v4(),
            Uuid::new_v4(),
            "service.logs",
            "large log",
            &content,
            1,
        )
        .expect("store");
        let reference = artifact_reference(&artifact);
        assert_eq!(reference.byte_len, 8_000);
        assert!(reference.summary.len() < content.len());
    }
}
