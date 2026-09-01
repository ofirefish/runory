use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::context::ContextFreshness;
use crate::tools::{NativeToolInvocation, ToolResult};

const MAX_CACHE_ENTRIES: usize = 2_048;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ToolResultProjection {
    pub fields: Vec<String>,
    pub page: usize,
    pub page_size: usize,
    pub tail: Option<usize>,
    pub from_epoch_ms: Option<u64>,
    pub to_epoch_ms: Option<u64>,
    pub max_result_bytes: usize,
}

impl Default for ToolResultProjection {
    fn default() -> Self {
        Self {
            fields: Vec::new(),
            page: 0,
            page_size: 100,
            tail: None,
            from_epoch_ms: None,
            to_epoch_ms: None,
            max_result_bytes: 16 * 1024,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectedToolResult {
    pub value: Value,
    pub original_bytes: usize,
    pub projected_bytes: usize,
    pub truncated: bool,
}

#[derive(Clone)]
struct CacheEntry {
    target_id: Uuid,
    observed_at_epoch_ms: u64,
    ttl_ms: u64,
    source: String,
    result: ToolResult,
}

#[derive(Default)]
pub(crate) struct ObservationCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
}

impl ObservationCache {
    pub(crate) async fn get(
        &self,
        target_id: Uuid,
        invocation: &NativeToolInvocation,
        now_epoch_ms: u64,
    ) -> Option<ToolResult> {
        let key = cache_key(target_id, invocation)?;
        let mut entries = self.entries.lock().await;
        let entry = entries.get(&key)?;
        let expected_source = format!("tool.{}", invocation.name().as_str());
        let fresh = entry.target_id == target_id
            && entry.source == expected_source
            && ContextFreshness {
                observed_at_epoch_ms: entry.observed_at_epoch_ms,
                ttl_ms: entry.ttl_ms,
            }
            .is_fresh(now_epoch_ms);
        if !fresh {
            entries.remove(&key);
            return None;
        }
        Some(entry.result.clone())
    }

    pub(crate) async fn put(
        &self,
        target_id: Uuid,
        invocation: &NativeToolInvocation,
        observed_at_epoch_ms: u64,
        result: &ToolResult,
    ) {
        let Some(key) = cache_key(target_id, invocation) else {
            return;
        };
        let mut entries = self.entries.lock().await;
        entries.retain(|_, entry| {
            ContextFreshness {
                observed_at_epoch_ms: entry.observed_at_epoch_ms,
                ttl_ms: entry.ttl_ms,
            }
            .is_fresh(observed_at_epoch_ms)
        });
        if entries.len() >= MAX_CACHE_ENTRIES && !entries.contains_key(&key) {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.observed_at_epoch_ms)
                .map(|(key, _)| key.clone())
            {
                entries.remove(&oldest);
            }
        }
        entries.insert(
            key,
            CacheEntry {
                target_id,
                observed_at_epoch_ms,
                ttl_ms: cache_ttl_ms(invocation),
                source: format!("tool.{}", invocation.name().as_str()),
                result: result.clone(),
            },
        );
    }

    pub(crate) async fn invalidate_target(&self, target_id: Uuid) -> usize {
        let mut entries = self.entries.lock().await;
        let before = entries.len();
        entries.retain(|_, value| value.target_id != target_id);
        before.saturating_sub(entries.len())
    }

    #[cfg(test)]
    async fn sources(&self) -> Vec<String> {
        self.entries
            .lock()
            .await
            .values()
            .map(|entry| entry.source.clone())
            .collect()
    }
}

pub(crate) fn cache_key(target_id: Uuid, invocation: &NativeToolInvocation) -> Option<String> {
    let suffix = match invocation {
        NativeToolInvocation::SystemInfo => "system.info".into(),
        NativeToolInvocation::SystemDisk => "system.disk".into(),
        NativeToolInvocation::ServiceStatus { service } => format!("service.status:{service}"),
        NativeToolInvocation::ServiceLogs { service, lines } => {
            format!("service.logs:{service}:{lines}")
        }
        NativeToolInvocation::NetworkPortCheck { host, port } => {
            format!("network.port:{host}:{port}")
        }
        NativeToolInvocation::HttpRequest { url } => format!("http.request:{url}"),
        NativeToolInvocation::NginxTest => "nginx.test".into(),
        NativeToolInvocation::DnsResolve { host } => format!("dns.resolve:{host}"),
        NativeToolInvocation::TlsInspect { host, port } => format!("tls.inspect:{host}:{port}"),
        NativeToolInvocation::NetworkListeners => "network.listeners".into(),
        NativeToolInvocation::ProcessList => "process.list".into(),
        NativeToolInvocation::FileInspect { path } => format!("file.inspect:{path}"),
        NativeToolInvocation::SystemDirectoryUsage { path } => format!("system.du:{path}"),
        NativeToolInvocation::SystemLargeFiles {
            path,
            minimum_bytes,
        } => format!("system.large:{path}:{minimum_bytes}"),
        NativeToolInvocation::DockerList => "docker.list".into(),
        NativeToolInvocation::DockerInspect { container } => format!("docker.inspect:{container}"),
        NativeToolInvocation::DockerLogs { container, lines } => {
            format!("docker.logs:{container}:{lines}")
        }
        NativeToolInvocation::FilePatch { .. }
        | NativeToolInvocation::ServiceRestart { .. }
        | NativeToolInvocation::ServiceReload { .. }
        | NativeToolInvocation::NginxReload
        | NativeToolInvocation::DockerRestart { .. } => return None,
    };
    Some(format!("{target_id}:{suffix}"))
}

fn cache_ttl_ms(invocation: &NativeToolInvocation) -> u64 {
    match invocation {
        NativeToolInvocation::ServiceLogs { .. } | NativeToolInvocation::DockerLogs { .. } => 5_000,
        NativeToolInvocation::HttpRequest { .. }
        | NativeToolInvocation::NetworkPortCheck { .. }
        | NativeToolInvocation::NginxTest => 10_000,
        _ => 30_000,
    }
}

pub(crate) fn project_result(
    result: &ToolResult,
    projection: &ToolResultProjection,
) -> ProjectedToolResult {
    let raw = serde_json::to_value(result).unwrap_or(Value::Null);
    let original_bytes = serde_json::to_vec(&raw).map_or(0, |value| value.len());
    let mut value = project_value(raw, projection);
    let mut bytes = serde_json::to_vec(&value).map_or(0, |item| item.len());
    let truncated = bytes > projection.max_result_bytes;
    if truncated {
        value = serde_json::json!({
            "toolName": result.tool_name,
            "success": result.success,
            "summary": result.summary,
            "errorCode": result.error_code,
            "truncated": true
        });
        bytes = serde_json::to_vec(&value).map_or(0, |item| item.len());
    }
    ProjectedToolResult {
        value,
        original_bytes,
        projected_bytes: bytes,
        truncated,
    }
}

fn project_value(mut value: Value, projection: &ToolResultProjection) -> Value {
    if !projection.fields.is_empty() {
        if let Value::Object(map) = &value {
            value = Value::Object(
                projection
                    .fields
                    .iter()
                    .filter_map(|field| map.get(field).cloned().map(|value| (field.clone(), value)))
                    .collect(),
            );
        }
    }
    if let Value::Array(items) = value {
        let items = items
            .into_iter()
            .filter(|item| within_time_range(item, projection))
            .collect::<Vec<_>>();
        let end = items.len();
        let start = projection.tail.map_or(
            projection.page.saturating_mul(projection.page_size),
            |tail| end.saturating_sub(tail),
        );
        return Value::Array(
            items
                .into_iter()
                .skip(start)
                .take(projection.page_size)
                .collect(),
        );
    }
    value
}

fn within_time_range(value: &Value, projection: &ToolResultProjection) -> bool {
    if projection.from_epoch_ms.is_none() && projection.to_epoch_ms.is_none() {
        return true;
    }
    let observed = ["observedAtEpochMs", "startedAtEpochMs", "timestampEpochMs"]
        .iter()
        .find_map(|field| value.get(field).and_then(Value::as_u64));
    observed.is_some_and(|observed| {
        projection.from_epoch_ms.is_none_or(|from| observed >= from)
            && projection.to_epoch_ms.is_none_or(|to| observed <= to)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_invocations_never_have_cache_keys() {
        assert!(cache_key(
            Uuid::nil(),
            &NativeToolInvocation::ServiceRestart {
                service: "nginx".into()
            }
        )
        .is_none());
    }

    #[test]
    fn pagination_tail_and_time_range_are_applied_before_context_entry() {
        let value = serde_json::json!([
            {"observedAtEpochMs": 10, "message": "old"},
            {"observedAtEpochMs": 20, "message": "keep"},
            {"observedAtEpochMs": 30, "message": "tail"}
        ]);
        let projection = ToolResultProjection {
            tail: Some(2),
            from_epoch_ms: Some(15),
            to_epoch_ms: Some(30),
            page_size: 2,
            ..ToolResultProjection::default()
        };
        let projected = project_value(value, &projection);
        assert_eq!(projected.as_array().map(Vec::len), Some(2));
        assert_eq!(projected[0]["message"], "keep");
    }

    #[tokio::test]
    async fn invalidation_is_target_local() {
        let cache = ObservationCache::default();
        let result = ToolResult {
            invocation_id: Uuid::new_v4(),
            tool_name: crate::tools::NativeToolName::SystemInfo,
            success: true,
            summary: "ok",
            data: None,
            error_code: None,
            warnings: Vec::new(),
            started_at_epoch_ms: 10,
            duration_ms: 1,
            truncated: false,
            cancelled: false,
            untrusted_remote_data: true,
        };
        cache
            .put(Uuid::nil(), &NativeToolInvocation::SystemInfo, 10, &result)
            .await;
        assert_eq!(cache.sources().await, vec!["tool.system.info"]);
        assert_eq!(cache.invalidate_target(Uuid::nil()).await, 1);
    }
}
