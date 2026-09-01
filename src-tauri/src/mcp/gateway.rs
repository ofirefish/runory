use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult, CredentialInput, CredentialKind};
use crate::storage::JsonRepository;

const MAX_MCP_SERVERS: usize = 16;
const MAX_MCP_TOOLS: usize = 128;
const MAX_MCP_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_MCP_LIST_PAGES: usize = 8;
const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";
const CLIENT_NAME: &str = "runory";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpConfigureRequest {
    pub id: Uuid,
    pub label: String,
    pub endpoint: String,
    pub token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpToolPermission {
    pub name: String,
    pub description: String,
    pub read_only: bool,
    pub enabled: bool,
    #[serde(default)]
    pub requires_arguments: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum McpTransportEra {
    Modern,
    Legacy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServerConfig {
    pub id: Uuid,
    pub label: String,
    pub endpoint: String,
    pub connected: bool,
    #[serde(default)]
    pub enabled: bool,
    pub tools: Vec<McpToolPermission>,
    #[serde(default)]
    pub transport_era: Option<McpTransportEra>,
    #[serde(default)]
    pub protocol_version: Option<String>,
}

#[derive(Clone, Debug)]
enum McpConnection {
    Modern {
        protocol_version: String,
    },
    Legacy {
        protocol_version: String,
        session_id: Option<String>,
    },
}

impl McpConnection {
    fn era(&self) -> McpTransportEra {
        match self {
            Self::Modern { .. } => McpTransportEra::Modern,
            Self::Legacy { .. } => McpTransportEra::Legacy,
        }
    }

    fn protocol_version(&self) -> &str {
        match self {
            Self::Modern { protocol_version }
            | Self::Legacy {
                protocol_version, ..
            } => protocol_version,
        }
    }
}

struct McpHttpResponse {
    status: reqwest::StatusCode,
    content_type: Option<String>,
    session_id: Option<String>,
    body: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct McpConfigRepository {
    repository: JsonRepository<Vec<McpServerConfig>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpAuditRecord {
    id: Uuid,
    server_id: Uuid,
    tool_name: String,
    risk: String,
    mutability: String,
    status: String,
    error_code: Option<String>,
    occurred_at_epoch_ms: u64,
}
impl McpConfigRepository {
    pub(crate) fn new(repository: JsonRepository<Vec<McpServerConfig>>) -> Self {
        Self { repository }
    }
    async fn load(&self) -> AppResult<Vec<McpServerConfig>> {
        self.repository.load_or_default().await
    }
    async fn save(&self, value: &[McpServerConfig]) -> AppResult<()> {
        self.repository.save_atomic(&value.to_vec()).await
    }
}

pub(crate) struct McpGateway {
    repository: McpConfigRepository,
    audit_repository: JsonRepository<Vec<McpAuditRecord>>,
    audit_lock: Mutex<()>,
    configs: Arc<RwLock<Vec<McpServerConfig>>>,
    connections: Arc<RwLock<HashMap<Uuid, McpConnection>>>,
    client: Client,
}

impl McpGateway {
    pub(crate) fn new(
        repository: McpConfigRepository,
        audit_repository: JsonRepository<Vec<McpAuditRecord>>,
    ) -> Self {
        Self {
            repository,
            audit_repository,
            audit_lock: Mutex::new(()),
            configs: Arc::new(RwLock::new(Vec::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap_or_default(),
        }
    }

    pub(crate) async fn load(&self) -> AppResult<()> {
        let mut configs = self.repository.load().await?;
        configs.retain(|item| valid_endpoint(&item.endpoint) && valid_label(&item.label));
        for item in &mut configs {
            item.connected = false;
            item.transport_era = None;
            item.protocol_version = None;
        }
        self.connections.write().await.clear();
        *self.configs.write().await = configs;
        Ok(())
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<McpServerConfig>> {
        Ok(self.configs.read().await.clone())
    }

    pub(crate) async fn configure(
        &self,
        request: McpConfigureRequest,
        credentials: &CredentialService,
    ) -> AppResult<McpServerConfig> {
        if !valid_endpoint(&request.endpoint) || !valid_label(&request.label) {
            return Err(AppError::InvalidOperation);
        }
        if let Some(token) = request.token {
            if token.is_empty() || token.len() > 16 * 1024 {
                return Err(AppError::InvalidOperation);
            }
            credentials
                .remember(request.id, CredentialKind::McpToken, Zeroizing::new(token))
                .await?;
        }
        let token = credentials
            .resolve_for_profile(
                request.id,
                CredentialKind::McpToken,
                CredentialInput::Stored,
                false,
            )
            .await
            .ok()
            .map(|item| item.secret);
        let connection = self
            .connect(&request.endpoint, token.as_deref().map(String::as_str))
            .await?;
        let tools = self
            .discover(
                &request.endpoint,
                token.as_deref().map(String::as_str),
                &connection,
            )
            .await?;
        let config = McpServerConfig {
            id: request.id,
            label: request.label,
            endpoint: request.endpoint,
            connected: true,
            enabled: true,
            tools,
            transport_era: Some(connection.era()),
            protocol_version: Some(connection.protocol_version().to_owned()),
        };
        let mut configs = self.configs.write().await;
        let previous_configs = configs.clone();
        if let Some(existing) = configs.iter_mut().find(|item| item.id == config.id) {
            *existing = config.clone();
        } else if configs.len() < MAX_MCP_SERVERS {
            configs.push(config.clone());
        } else {
            return Err(AppError::InvalidOperation);
        }
        if let Err(error) = self.repository.save(&configs).await {
            *configs = previous_configs;
            return Err(error);
        }
        self.connections.write().await.insert(config.id, connection);
        Ok(config)
    }

    pub(crate) async fn remove(&self, id: Uuid, credentials: &CredentialService) -> AppResult<()> {
        let mut configs = self.configs.write().await;
        let previous_configs = configs.clone();
        let previous = configs.len();
        configs.retain(|item| item.id != id);
        if configs.len() == previous {
            return Err(AppError::InvalidOperation);
        }
        if let Err(error) = self.repository.save(&configs).await {
            *configs = previous_configs;
            return Err(error);
        }
        self.connections.write().await.remove(&id);
        credentials.forget(id, CredentialKind::McpToken).await
    }

    pub(crate) async fn set_tool_enabled(
        &self,
        id: Uuid,
        tool_name: &str,
        enabled: bool,
    ) -> AppResult<McpServerConfig> {
        let mut configs = self.configs.write().await;
        let previous_configs = configs.clone();
        let config = configs
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or(AppError::InvalidOperation)?;
        let tool = config
            .tools
            .iter_mut()
            .find(|item| item.name == tool_name)
            .ok_or(AppError::InvalidOperation)?;
        if enabled && !tool.read_only {
            return Err(AppError::InvalidOperation);
        }
        tool.enabled = enabled;
        let output = config.clone();
        if let Err(error) = self.repository.save(&configs).await {
            *configs = previous_configs;
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) async fn set_enabled(&self, id: Uuid, enabled: bool) -> AppResult<McpServerConfig> {
        let mut configs = self.configs.write().await;
        let previous_configs = configs.clone();
        let config = configs
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or(AppError::InvalidOperation)?;
        config.enabled = enabled;
        let output = config.clone();
        if let Err(error) = self.repository.save(&configs).await {
            *configs = previous_configs;
            return Err(error);
        }
        Ok(output)
    }

    pub(crate) async fn read_context(
        &self,
        id: Uuid,
        tool_name: &str,
        arguments: Value,
        credentials: &CredentialService,
    ) -> AppResult<Value> {
        if !arguments.is_object()
            || serde_json::to_vec(&arguments)
                .map_err(|_| AppError::InvalidOperation)?
                .len()
                > 64 * 1024
        {
            return Err(AppError::InvalidOperation);
        }
        let config = self
            .configs
            .read()
            .await
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or(AppError::InvalidOperation)?;
        let permitted = config
            .tools
            .iter()
            .any(|item| item.name == tool_name && item.read_only && item.enabled);
        if !config.enabled || !permitted {
            return Err(AppError::InvalidOperation);
        }
        let token = credentials
            .resolve_for_profile(id, CredentialKind::McpToken, CredentialInput::Stored, false)
            .await
            .ok()
            .map(|item| item.secret);
        let connection = self
            .connection_for(id, &config.endpoint, token.as_deref().map(String::as_str))
            .await?;
        let result = self
            .rpc(
                &config.endpoint,
                token.as_deref().map(String::as_str),
                &connection,
                "tools/call",
                json!({"name":tool_name,"arguments":arguments}),
            )
            .await;
        if result.is_err() {
            self.connections.write().await.remove(&id);
            let mut configs = self.configs.write().await;
            if let Some(config) = configs.iter_mut().find(|config| config.id == id) {
                config.connected = false;
                config.transport_era = None;
                config.protocol_version = None;
                if let Err(error) = self.repository.save(&configs).await {
                    tracing::warn!(server_id = %id, error_code = error.code(), "failed to persist disconnected MCP state");
                }
            }
        }
        let _audit_guard = self.audit_lock.lock().await;
        let mut audit = self.audit_repository.load_or_default().await?;
        audit.push(McpAuditRecord {
            id: Uuid::new_v4(),
            server_id: id,
            tool_name: tool_name.to_owned(),
            risk: "R1".into(),
            mutability: "read".into(),
            status: if result.is_ok() {
                "succeeded".into()
            } else {
                "failed".into()
            },
            error_code: result.as_ref().err().map(|error| error.code().to_owned()),
            occurred_at_epoch_ms: now_ms(),
        });
        if audit.len() > 2_000 {
            audit.drain(..audit.len() - 2_000);
        }
        self.audit_repository.save_atomic(&audit).await?;
        result
    }

    async fn discover(
        &self,
        endpoint: &str,
        token: Option<&str>,
        connection: &McpConnection,
    ) -> AppResult<Vec<McpToolPermission>> {
        let mut names = BTreeSet::new();
        let mut output = Vec::new();
        let mut cursor = None::<String>;
        for _ in 0..MAX_MCP_LIST_PAGES {
            let params = cursor
                .as_ref()
                .map(|cursor| json!({"cursor":cursor}))
                .unwrap_or_else(|| json!({}));
            let value = self
                .rpc(endpoint, token, connection, "tools/list", params)
                .await?;
            let tools = value
                .get("tools")
                .and_then(Value::as_array)
                .ok_or(AppError::InvalidOperation)?;
            if output.len().saturating_add(tools.len()) > MAX_MCP_TOOLS {
                return Err(AppError::InvalidOperation);
            }
            for item in tools {
                let Some(input_schema) = item.get("inputSchema").and_then(Value::as_object) else {
                    continue;
                };
                // The current gateway does not mirror x-mcp-header values. Excluding those tools
                // is safer than making a non-conforming call with missing security headers.
                if contains_x_mcp_header(item.get("inputSchema").unwrap_or(&Value::Null)) {
                    continue;
                }
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|value| valid_tool_name(value))
                    .ok_or(AppError::InvalidOperation)?
                    .to_owned();
                if !names.insert(name.clone()) {
                    return Err(AppError::InvalidOperation);
                }
                let read_only = item
                    .pointer("/annotations/readOnlyHint")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                output.push(McpToolPermission {
                    name,
                    description: item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .chars()
                        .take(512)
                        .collect(),
                    read_only,
                    enabled: false,
                    requires_arguments: input_schema
                        .get("required")
                        .and_then(Value::as_array)
                        .is_some_and(|required| !required.is_empty()),
                });
            }
            cursor = value
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty() && value.len() <= 1024)
                .map(str::to_owned);
            if cursor.is_none() {
                return Ok(output);
            }
        }
        Err(AppError::InvalidOperation)
    }

    async fn connection_for(
        &self,
        id: Uuid,
        endpoint: &str,
        token: Option<&str>,
    ) -> AppResult<McpConnection> {
        if let Some(connection) = self.connections.read().await.get(&id).cloned() {
            return Ok(connection);
        }
        let connection = self.connect(endpoint, token).await?;
        self.connections
            .write()
            .await
            .insert(id, connection.clone());
        let mut configs = self.configs.write().await;
        let previous_configs = configs.clone();
        if let Some(config) = configs.iter_mut().find(|config| config.id == id) {
            config.connected = true;
            config.transport_era = Some(connection.era());
            config.protocol_version = Some(connection.protocol_version().to_owned());
            if let Err(error) = self.repository.save(&configs).await {
                *configs = previous_configs;
                drop(configs);
                self.connections.write().await.remove(&id);
                return Err(error);
            }
        }
        Ok(connection)
    }

    async fn connect(&self, endpoint: &str, token: Option<&str>) -> AppResult<McpConnection> {
        let probe_id = Uuid::new_v4().to_string();
        let probe_connection = McpConnection::Modern {
            protocol_version: MODERN_PROTOCOL_VERSION.into(),
        };
        let probe = self
            .send_http(
                endpoint,
                token,
                Some(&probe_connection),
                "server/discover",
                json!({}),
                Some(&probe_id),
            )
            .await?;
        if probe.status.is_success() {
            let result = parse_rpc_result(&probe, &probe_id)?;
            let supported = result
                .get("supportedVersions")
                .and_then(Value::as_array)
                .ok_or(AppError::InvalidOperation)?;
            if supported
                .iter()
                .any(|version| version == MODERN_PROTOCOL_VERSION)
            {
                return Ok(probe_connection);
            }
            if supported
                .iter()
                .any(|version| version == LEGACY_PROTOCOL_VERSION)
            {
                return self.connect_legacy(endpoint, token).await;
            }
            return Err(AppError::InvalidOperation);
        }
        if matches!(probe.status.as_u16(), 400 | 404 | 405) && !recognized_modern_error(&probe.body)
        {
            return self.connect_legacy(endpoint, token).await;
        }
        Err(AppError::ConnectionRefused)
    }

    async fn connect_legacy(
        &self,
        endpoint: &str,
        token: Option<&str>,
    ) -> AppResult<McpConnection> {
        let request_id = Uuid::new_v4().to_string();
        let response = self
            .send_http(
                endpoint,
                token,
                None,
                "initialize",
                json!({
                    "protocolVersion": LEGACY_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name":CLIENT_NAME,"version":CLIENT_VERSION}
                }),
                Some(&request_id),
            )
            .await?;
        if !response.status.is_success() {
            return Err(AppError::ConnectionRefused);
        }
        let result = parse_rpc_result(&response, &request_id)?;
        let protocol_version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .filter(|version| supported_legacy_version(version))
            .ok_or(AppError::InvalidOperation)?
            .to_owned();
        let connection = McpConnection::Legacy {
            protocol_version,
            session_id: response.session_id,
        };
        let initialized = self
            .send_http(
                endpoint,
                token,
                Some(&connection),
                "notifications/initialized",
                json!({}),
                None,
            )
            .await?;
        if !initialized.status.is_success() {
            return Err(AppError::ConnectionRefused);
        }
        Ok(connection)
    }

    async fn rpc(
        &self,
        endpoint: &str,
        token: Option<&str>,
        connection: &McpConnection,
        method: &str,
        params: Value,
    ) -> AppResult<Value> {
        let request_id = Uuid::new_v4().to_string();
        let response = self
            .send_http(
                endpoint,
                token,
                Some(connection),
                method,
                params,
                Some(&request_id),
            )
            .await?;
        if !response.status.is_success() {
            return Err(AppError::ConnectionRefused);
        }
        parse_rpc_result(&response, &request_id)
    }

    async fn send_http(
        &self,
        endpoint: &str,
        token: Option<&str>,
        connection: Option<&McpConnection>,
        method: &str,
        mut params: Value,
        request_id: Option<&str>,
    ) -> AppResult<McpHttpResponse> {
        if let Some(McpConnection::Modern { protocol_version }) = connection {
            let object = params.as_object_mut().ok_or(AppError::InvalidOperation)?;
            object.insert(
                "_meta".into(),
                json!({
                    "io.modelcontextprotocol/protocolVersion": protocol_version,
                    "io.modelcontextprotocol/clientInfo": {"name":CLIENT_NAME,"version":CLIENT_VERSION},
                    "io.modelcontextprotocol/clientCapabilities": {}
                }),
            );
        }
        let request_name = params
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut body = json!({"jsonrpc":"2.0","method":method,"params":params});
        if let Some(request_id) = request_id {
            body["id"] = Value::String(request_id.to_owned());
        }
        let mut request = self
            .client
            .post(endpoint)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(connection) = connection {
            request = request.header("MCP-Protocol-Version", connection.protocol_version());
            match connection {
                McpConnection::Modern { .. } => {
                    request = request.header("Mcp-Method", method);
                    if let Some(name) = request_name.as_deref() {
                        request = request.header("Mcp-Name", name);
                    }
                }
                McpConnection::Legacy {
                    session_id: Some(session_id),
                    ..
                } => request = request.header("Mcp-Session-Id", session_id),
                McpConnection::Legacy {
                    session_id: None, ..
                } => {}
            }
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| AppError::ConnectionRefused)?;
        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let session_id = match response.headers().get("Mcp-Session-Id") {
            Some(value) => {
                let value = value.to_str().map_err(|_| AppError::InvalidOperation)?;
                if !valid_session_id(value) {
                    return Err(AppError::InvalidOperation);
                }
                Some(value.to_owned())
            }
            None => None,
        };
        if response
            .content_length()
            .is_some_and(|size| size > MAX_MCP_RESPONSE_BYTES as u64)
        {
            return Err(AppError::ExecOutputLimit);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AppError::ConnectionRefused)?
        {
            if body.len().saturating_add(chunk.len()) > MAX_MCP_RESPONSE_BYTES {
                return Err(AppError::ExecOutputLimit);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(McpHttpResponse {
            status,
            content_type,
            session_id,
            body,
        })
    }
}

fn parse_rpc_result(response: &McpHttpResponse, request_id: &str) -> AppResult<Value> {
    let messages = parse_response_messages(response)?;
    let message = messages
        .into_iter()
        .find(|message| message.get("id").and_then(Value::as_str) == Some(request_id))
        .ok_or(AppError::InvalidOperation)?;
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(AppError::InvalidOperation);
    }
    if message.get("error").is_some() {
        return Err(AppError::ExecFailed);
    }
    message
        .get("result")
        .cloned()
        .ok_or(AppError::InvalidOperation)
}

fn parse_response_messages(response: &McpHttpResponse) -> AppResult<Vec<Value>> {
    let content_type = response.content_type.as_deref().unwrap_or_default();
    if content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return serde_json::from_slice::<Value>(&response.body)
            .map(|value| vec![value])
            .map_err(|_| AppError::InvalidOperation);
    }
    if content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("text/event-stream"))
    {
        let text = std::str::from_utf8(&response.body).map_err(|_| AppError::InvalidOperation)?;
        let normalized = text.replace("\r\n", "\n");
        let mut messages = Vec::new();
        for event in normalized.split("\n\n") {
            let data = event
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                messages.push(serde_json::from_str(&data).map_err(|_| AppError::InvalidOperation)?);
            }
        }
        return Ok(messages);
    }
    Err(AppError::InvalidOperation)
}

fn recognized_modern_error(body: &[u8]) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.pointer("/error/code").and_then(Value::as_i64))
        .is_some_and(|code| matches!(code, -32020..=-32018 | -32022))
}

fn supported_legacy_version(value: &str) -> bool {
    matches!(value, "2025-11-25" | "2025-06-18" | "2025-03-26")
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

fn contains_x_mcp_header(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case("x-mcp-header") || contains_x_mcp_header(value)
        }),
        Value::Array(items) => items.iter().any(contains_x_mcp_header),
        _ => false,
    }
}

fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn valid_tool_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
}
fn valid_endpoint(value: &str) -> bool {
    if value.len() > 2048
        || value.contains('@')
        || value
            .chars()
            .any(|item| item.is_control() || item.is_whitespace())
    {
        return false;
    }
    value.starts_with("https://")
        || value.starts_with("http://127.0.0.1:")
        || value.starts_with("http://localhost:")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::CredentialVault;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    struct EmptyVault;
    impl CredentialVault for EmptyVault {
        fn is_initialized(&self) -> bool {
            false
        }
        fn is_unlocked(&self) -> bool {
            false
        }
        fn unlock(&self, _: Zeroizing<String>) -> AppResult<()> {
            Err(AppError::VaultLocked)
        }
        fn lock(&self) -> AppResult<()> {
            Ok(())
        }
        fn contains(&self, _: Uuid, _: CredentialKind) -> AppResult<bool> {
            Ok(false)
        }
        fn get(&self, _: Uuid, _: CredentialKind) -> AppResult<Zeroizing<String>> {
            Err(AppError::CredentialNotFound)
        }
        fn put(&self, _: Uuid, _: CredentialKind, _: Zeroizing<String>) -> AppResult<()> {
            Err(AppError::VaultLocked)
        }
        fn delete(&self, _: Uuid, _: CredentialKind) -> AppResult<()> {
            Err(AppError::VaultLocked)
        }
        fn put_private_key(&self, _: Uuid, _: Zeroizing<Vec<u8>>) -> AppResult<()> {
            Err(AppError::VaultLocked)
        }
        fn get_private_key(&self, _: Uuid) -> AppResult<Zeroizing<Vec<u8>>> {
            Err(AppError::CredentialNotFound)
        }
        fn delete_private_key(&self, _: Uuid) -> AppResult<()> {
            Err(AppError::VaultLocked)
        }
    }
    #[test]
    fn endpoint_policy_requires_https_or_explicit_loopback() {
        assert!(valid_endpoint("https://mcp.example.test/rpc"));
        assert!(valid_endpoint("http://127.0.0.1:3456/rpc"));
        assert!(!valid_endpoint("http://example.test/rpc"));
        assert!(!valid_endpoint("https://token@example.test/rpc"));
        assert!(valid_session_id("opaque-session_123"));
        assert!(!valid_session_id("session\nheader"));
    }

    #[test]
    fn persisted_configuration_has_no_token_field() {
        let config = McpServerConfig {
            id: Uuid::new_v4(),
            label: "example".into(),
            endpoint: "https://mcp.example.test/rpc".into(),
            connected: false,
            enabled: false,
            tools: Vec::new(),
            transport_era: None,
            protocol_version: None,
        };
        let serialized = serde_json::to_string(&config).expect("serialize");
        assert!(!serialized.to_ascii_lowercase().contains("token"));
        assert!(!serialized.contains("Authorization"));
    }

    #[tokio::test]
    async fn read_only_http_gateway_discovers_authorizes_calls_and_audits() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test address");
        let server = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().expect("accept");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("timeout");
                let text = read_http_request(&mut stream);
                assert!(text
                    .to_ascii_lowercase()
                    .contains("mcp-protocol-version: 2026-07-28"));
                let request: Value =
                    serde_json::from_str(text.split("\r\n\r\n").nth(1).expect("request body"))
                        .expect("request json");
                let id = request["id"].as_str().expect("request id");
                let method = request["method"].as_str().expect("method");
                assert_eq!(
                    request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
                    MODERN_PROTOCOL_VERSION
                );
                let result = match method {
                    "server/discover" => json!({
                        "supportedVersions":[MODERN_PROTOCOL_VERSION],
                        "capabilities":{"tools":{}},
                        "resultType":"complete"
                    }),
                    "tools/list" => json!({"tools":[{
                        "name":"issues.list",
                        "description":"List issues",
                        "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
                        "annotations":{"readOnlyHint":true}
                    }]}),
                    "tools/call" => json!({"content":[{"type":"text","text":"untrusted result"}]}),
                    _ => panic!("unexpected method"),
                };
                let body = serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"result":result}))
                    .expect("response");
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write response");
            }
        });
        let directory = tempfile::tempdir().expect("temporary directory");
        let audit_path = directory.path().join("mcp-audit.json");
        let gateway = McpGateway::new(
            McpConfigRepository::new(JsonRepository::new(
                directory.path().join("mcp-config.json"),
            )),
            JsonRepository::new(audit_path.clone()),
        );
        let credentials = CredentialService::new(Arc::new(EmptyVault));
        let id = Uuid::new_v4();
        gateway
            .configure(
                McpConfigureRequest {
                    id,
                    label: "test".into(),
                    endpoint: format!("http://127.0.0.1:{}/rpc", address.port()),
                    token: None,
                },
                &credentials,
            )
            .await
            .expect("configure");
        gateway
            .set_tool_enabled(id, "issues.list", true)
            .await
            .expect("enable");
        gateway.set_enabled(id, true).await.expect("enable server");
        let result = gateway
            .read_context(id, "issues.list", json!({}), &credentials)
            .await
            .expect("call");
        assert_eq!(result["content"][0]["text"], "untrusted result");
        let audit = tokio::fs::read_to_string(audit_path).await.expect("audit");
        assert!(audit.contains("issues.list"));
        assert!(!audit.contains("untrusted result"));
        server.join().expect("server thread");
    }

    #[tokio::test]
    async fn legacy_gateway_initializes_uses_session_and_accepts_sse_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("test address");
        let server = std::thread::spawn(move || {
            for index in 0..5 {
                let (mut stream, _) = listener.accept().expect("accept");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("timeout");
                let text = read_http_request(&mut stream);
                let request: Value =
                    serde_json::from_str(text.split("\r\n\r\n").nth(1).expect("request body"))
                        .expect("request json");
                let method = request["method"].as_str().expect("method");
                match index {
                    0 => {
                        assert_eq!(method, "server/discover");
                        let id = request["id"].as_str().expect("id");
                        let body = serde_json::to_string(&json!({
                            "jsonrpc":"2.0","id":id,
                            "error":{"code":-32601,"message":"method not found"}
                        }))
                        .expect("response");
                        write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write");
                    }
                    1 => {
                        assert_eq!(method, "initialize");
                        let id = request["id"].as_str().expect("id");
                        let body = serde_json::to_string(&json!({
                            "jsonrpc":"2.0","id":id,
                            "result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"legacy","version":"1"}}
                        })).expect("response");
                        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: safe-session-1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write");
                    }
                    2 => {
                        assert_eq!(method, "notifications/initialized");
                        assert!(text
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: safe-session-1"));
                        write!(stream, "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").expect("write");
                    }
                    3 => {
                        assert_eq!(method, "tools/list");
                        assert!(text
                            .to_ascii_lowercase()
                            .contains("mcp-protocol-version: 2025-06-18"));
                        assert!(text
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: safe-session-1"));
                        let id = request["id"].as_str().expect("id");
                        let body = serde_json::to_string(&json!({
                            "jsonrpc":"2.0","id":id,"result":{"tools":[{
                                "name":"legacy.read","description":"Legacy read",
                                "inputSchema":{"type":"object","properties":{}},
                                "annotations":{"readOnlyHint":true}
                            }]}
                        }))
                        .expect("response");
                        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write");
                    }
                    4 => {
                        assert_eq!(method, "tools/call");
                        let id = request["id"].as_str().expect("id");
                        let json = serde_json::to_string(&json!({
                            "jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":"legacy untrusted"}]}
                        })).expect("response");
                        let body = format!("event: message\ndata: {json}\n\n");
                        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).expect("write");
                    }
                    _ => unreachable!(),
                }
            }
        });
        let directory = tempfile::tempdir().expect("temporary directory");
        let gateway = McpGateway::new(
            McpConfigRepository::new(JsonRepository::new(
                directory.path().join("mcp-config.json"),
            )),
            JsonRepository::new(directory.path().join("mcp-audit.json")),
        );
        let credentials = CredentialService::new(Arc::new(EmptyVault));
        let id = Uuid::new_v4();
        let config = gateway
            .configure(
                McpConfigureRequest {
                    id,
                    label: "legacy".into(),
                    endpoint: format!("http://127.0.0.1:{}/rpc", address.port()),
                    token: None,
                },
                &credentials,
            )
            .await
            .expect("configure");
        assert_eq!(config.transport_era, Some(McpTransportEra::Legacy));
        assert_eq!(config.protocol_version.as_deref(), Some("2025-06-18"));
        gateway
            .set_tool_enabled(id, "legacy.read", true)
            .await
            .expect("enable");
        let result = gateway
            .read_context(id, "legacy.read", json!({}), &credentials)
            .await
            .expect("call");
        assert_eq!(result["content"][0]["text"], "legacy untrusted");
        server.join().expect("server thread");
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let size = stream.read(&mut buffer).expect("read request");
            assert!(size > 0, "request closed before body completed");
            request.extend_from_slice(&buffer[..size]);
            let Some(header_end) = request.windows(4).position(|item| item == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .expect("content length");
            if request.len() >= header_end + 4 + content_length {
                return String::from_utf8(request).expect("utf8 request");
            }
        }
    }
}
