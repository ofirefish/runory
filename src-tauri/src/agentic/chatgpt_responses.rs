//! ChatGPT account transport. SSE stays in Rust; only completed text reaches
//! the reasoner, where the existing decision and approval checks still apply.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::StatusCode;
use serde_json::{json, Value};

use crate::domain::{AppError, AppResult};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
type ModelResponse = (String, u32, u32);

pub(super) fn chatgpt_account_id(access_token: &str) -> Option<String> {
    // This claim is only a routing hint to the issuer, never local authorization.
    // Opaque tokens remain usable without the optional account header.
    let payload = access_token.split('.').nth(1)?;
    if payload.len() > 32 * 1024 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .pointer("/https:~1~1api.openai.com~1auth/chatgpt_account_id")?
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

pub(super) fn chatgpt_request_body(
    model: &str,
    instructions: &str,
    messages: &[Value],
) -> AppResult<Value> {
    let input = messages
        .iter()
        .map(|message| {
            let role = message["role"].as_str().ok_or(AppError::ModelInvalid)?;
            let content = message["content"].as_str().ok_or(AppError::ModelInvalid)?;
            let content_type = match role {
                "assistant" => "output_text",
                "user" | "system" | "developer" => "input_text",
                _ => return Err(AppError::ModelInvalid),
            };
            Ok(json!({
                "type": "message", "role": role,
                "content": [{ "type": content_type, "text": content }]
            }))
        })
        .collect::<AppResult<Vec<_>>>()?;
    // The account-backed Codex endpoint uses streaming and does not accept
    // the API-key path's max_output_tokens / JSON-object formatting options.
    // JSON is requested by the instructions and validated by the reasoner.
    Ok(json!({
        "model": model, "instructions": instructions, "input": input,
        "store": false, "stream": true
    }))
}

pub(super) async fn parse_chatgpt_responses(
    mut response: reqwest::Response,
) -> AppResult<ModelResponse> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(AppError::ModelAuthFailed),
        StatusCode::TOO_MANY_REQUESTS => return Err(AppError::ModelRateLimited),
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => return Err(AppError::ModelInvalid),
        status if !status.is_success() => return Err(AppError::ModelUnavailable),
        _ => {}
    }
    let mut parser = ResponseStream::default();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::ModelUnavailable)?
    {
        if let Some(result) = parser.push(&chunk)? {
            return Ok(result);
        }
    }
    // A text delta or output item alone is never proof of successful completion.
    Err(AppError::ModelResponseInvalid)
}

#[derive(Default)]
struct ResponseStream {
    bytes_received: usize,
    line: Vec<u8>,
    data: String,
    output: String,
}

impl ResponseStream {
    fn push(&mut self, chunk: &[u8]) -> AppResult<Option<ModelResponse>> {
        self.bytes_received = self.bytes_received.saturating_add(chunk.len());
        if self.bytes_received > MAX_RESPONSE_BYTES {
            return Err(AppError::ModelResponseInvalid);
        }
        for &byte in chunk {
            if byte != b'\n' {
                self.line.push(byte);
                continue;
            }
            let line = std::mem::take(&mut self.line);
            let line = std::str::from_utf8(&line).map_err(|_| AppError::ModelResponseInvalid)?;
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                if let Some(result) = self.finish_event()? {
                    return Ok(Some(result));
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
                self.data.push('\n');
            }
        }
        Ok(None)
    }

    fn finish_event(&mut self) -> AppResult<Option<ModelResponse>> {
        let data = std::mem::take(&mut self.data);
        if data.is_empty() {
            return Ok(None);
        }
        let event: Value =
            serde_json::from_str(&data).map_err(|_| AppError::ModelResponseInvalid)?;
        match event["type"].as_str() {
            Some("response.output_item.done") => {
                append_message_text(&event["item"], &mut self.output);
                Ok(None)
            }
            Some("response.completed") => {
                let response = &event["response"];
                if response["status"]
                    .as_str()
                    .is_some_and(|status| status != "completed")
                    || !response["error"].is_null()
                {
                    return Err(AppError::ModelResponseInvalid);
                }
                let mut text = String::new();
                if let Some(items) = response["output"].as_array() {
                    for item in items {
                        append_message_text(item, &mut text);
                    }
                }
                if text.is_empty() {
                    text = std::mem::take(&mut self.output);
                }
                if text.trim().is_empty() {
                    return Err(AppError::ModelResponseInvalid);
                }
                Ok(Some((
                    text,
                    token_count(response, "input_tokens"),
                    token_count(response, "output_tokens"),
                )))
            }
            Some("response.failed") | Some("error") => {
                let error = if event["type"] == "response.failed" {
                    &event["response"]["error"]
                } else {
                    &event["error"]
                };
                // Never forward provider messages: they may echo context or credentials.
                let code = error["code"].as_str().or_else(|| event["code"].as_str());
                Err(match code {
                    Some("rate_limit_exceeded" | "usage_limit_reached" | "insufficient_quota") => {
                        AppError::ModelRateLimited
                    }
                    Some("invalid_api_key" | "authentication_error" | "token_expired") => {
                        AppError::ModelAuthFailed
                    }
                    Some("invalid_request_error" | "model_not_found" | "unsupported_parameter") => {
                        AppError::ModelInvalid
                    }
                    _ => AppError::ModelUnavailable,
                })
            }
            Some("response.incomplete") => Err(AppError::ModelResponseInvalid),
            _ => Ok(None),
        }
    }
}

fn append_message_text(item: &Value, text: &mut String) {
    if item["type"] != "message" || item["role"] != "assistant" {
        return;
    }
    if let Some(parts) = item["content"].as_array() {
        for part in parts {
            if part["type"] == "output_text" {
                if let Some(value) = part["text"].as_str() {
                    text.push_str(value);
                }
            }
        }
    }
}

fn token_count(response: &Value, field: &str) -> u32 {
    response["usage"][field]
        .as_u64()
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str) -> Value {
        json!({"type":"message", "role":"assistant", "content":[{"type":"output_text", "text":text}]})
    }

    fn event(value: Value) -> String {
        format!(
            "event: {}\r\ndata: {value}\r\n\r\n",
            value["type"].as_str().unwrap()
        )
    }

    fn completed(text: &str) -> String {
        event(json!({"type":"response.completed", "response":{
            "status":"completed", "output":[message(text)],
            "usage":{"input_tokens":37,"output_tokens":11}
        }}))
    }

    #[test]
    fn account_request_uses_streaming_and_correct_history_content_types() {
        let body = chatgpt_request_body(
            "gpt-5.4",
            "Return JSON",
            &[
                json!({"role":"user","content":"Inspect disk"}),
                json!({"role":"assistant","content":"{\"action\":\"propose\"}"}),
                json!({"role":"user","content":"Untrusted result"}),
            ],
        )
        .unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["store"], false);
        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["instructions"], "Return JSON");
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][1]["content"][0]["type"], "output_text");
        assert!(body.get("max_output_tokens").is_none());
        assert!(body.get("text").is_none());
        assert!(chatgpt_request_body("gpt-5.4", "JSON", &[json!({"role":"user"})]).is_err());
    }

    #[test]
    fn account_header_uses_only_the_issuer_account_claim() {
        let payload = URL_SAFE_NO_PAD.encode(
            json!({"https://api.openai.com/auth":{"chatgpt_account_id":"account-123"}}).to_string(),
        );
        assert_eq!(
            chatgpt_account_id(&format!("header.{payload}.signature")).as_deref(),
            Some("account-123")
        );
        assert_eq!(chatgpt_account_id("opaque-access-token"), None);
        assert_eq!(chatgpt_account_id("header.invalid.signature"), None);
    }

    #[test]
    fn stream_handles_split_utf8_crlf_and_ignores_reasoning() {
        let text = "{\"action\":\"answer\",\"answer\":\"正常\"}";
        let stream = format!(
            ": keepalive\r\n\r\n{}{}{}",
            event(json!({"type":"response.reasoning_text.delta","delta":"hidden"})),
            event(json!({"type":"response.output_item.done","item":message(text)})),
            completed(text)
        );
        let mut parser = ResponseStream::default();
        let mut result = None;
        for byte in stream.as_bytes() {
            if let Some(response) = parser.push(&[*byte]).unwrap() {
                result = Some(response);
            }
        }
        assert_eq!(result, Some((text.into(), 37, 11)));
    }

    #[test]
    fn completed_can_use_output_items_when_terminal_event_omits_output() {
        let mut parser = ResponseStream::default();
        let output =
            event(json!({"type":"response.output_item.done", "item":message("{\"ok\":true}")}));
        assert!(parser.push(output.as_bytes()).unwrap().is_none());
        let done = event(json!({"type":"response.completed", "response":{"status":"completed"}}));
        assert_eq!(
            parser.push(done.as_bytes()).unwrap(),
            Some(("{\"ok\":true}".into(), 0, 0))
        );
    }

    #[test]
    fn multiline_sse_data_is_supported() {
        let stream = "data: {\"type\":\"response.completed\",\n data: ignored\ndata: \"response\":{\"status\":\"completed\",\"output\":[]}}\n\n";
        // Valid SSE fields without output still cannot produce a model decision.
        assert_eq!(
            ResponseStream::default()
                .push(stream.as_bytes())
                .unwrap_err()
                .code(),
            "MODEL_RESPONSE_INVALID"
        );
        let stream = format!("data: {{\"type\":\"response.completed\",\ndata: \"response\":{{\"output\":[{}]}}}}\n\n", message("OK"));
        assert_eq!(
            ResponseStream::default().push(stream.as_bytes()).unwrap(),
            Some(("OK".into(), 0, 0))
        );
    }

    #[test]
    fn incomplete_failed_and_oversized_streams_never_return_partial_success() {
        for (value, code) in [
            (
                json!({"type":"response.incomplete"}),
                "MODEL_RESPONSE_INVALID",
            ),
            (
                json!({"type":"response.failed","response":{"error":{"code":"usage_limit_reached","message":"secret echo"}}}),
                "MODEL_RATE_LIMITED",
            ),
            (
                json!({"type":"error","code":"token_expired","message":"secret echo"}),
                "MODEL_AUTH_FAILED",
            ),
            (
                json!({"type":"response.completed","response":{"status":"incomplete","output":[message("partial")]}}),
                "MODEL_RESPONSE_INVALID",
            ),
        ] {
            let error = ResponseStream::default()
                .push(event(value).as_bytes())
                .unwrap_err();
            assert_eq!(error.code(), code);
            assert!(!error.to_string().contains("secret echo"));
        }
        let mut parser = ResponseStream::default();
        assert!(parser
            .push(event(json!({"type":"response.output_text.delta","delta":"partial"})).as_bytes())
            .unwrap()
            .is_none());
        assert!(parser.push(&vec![b'x'; MAX_RESPONSE_BYTES]).is_err());
        assert!(ResponseStream::default()
            .push(b"data: not-json\n\n")
            .is_err());
    }

    async fn http_response(status: &str, body: &str) -> reqwest::Response {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let wire = format!("HTTP/1.1 {status}\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 2048];
            let _ = socket.read(&mut buffer).await.unwrap();
            socket.write_all(wire.as_bytes()).await.unwrap();
        });
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn http_stream_returns_completion_and_rejects_disconnect_before_completion() {
        let response = http_response("200 OK", &completed("{\"ok\":true}")).await;
        assert_eq!(
            parse_chatgpt_responses(response).await.unwrap(),
            ("{\"ok\":true}".into(), 37, 11)
        );
        let partial = event(json!({"type":"response.output_item.done","item":message("partial")}));
        let response = http_response("200 OK", &partial).await;
        assert_eq!(
            parse_chatgpt_responses(response).await.unwrap_err().code(),
            "MODEL_RESPONSE_INVALID"
        );
    }

    #[tokio::test]
    async fn http_failures_use_stable_codes_without_provider_body() {
        for (status, code) in [
            ("400 Bad Request", "MODEL_INVALID"),
            ("401 Unauthorized", "MODEL_AUTH_FAILED"),
            ("429 Too Many Requests", "MODEL_RATE_LIMITED"),
            ("503 Unavailable", "MODEL_UNAVAILABLE"),
        ] {
            let response = http_response(status, "secret echo").await;
            let error = parse_chatgpt_responses(response).await.unwrap_err();
            assert_eq!(error.code(), code);
            assert!(!error.to_string().contains("secret echo"));
        }
    }
}
