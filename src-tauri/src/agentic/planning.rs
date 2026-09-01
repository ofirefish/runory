use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::changes::ChangeStepDraft;
use super::context::redact_secrets;
use super::model::model_evidence;
use super::state::Evidence;
use crate::domain::{AppError, AppResult};
use crate::tools::{NativeToolInvocation, NativeToolName, ToolData};

const MAX_TOOL_CALLS_PER_TURN: usize = 4;
const MAX_ANSWER_BYTES: usize = 16 * 1024;
const DEFAULT_LARGE_FILE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanningHints {
    pub service: Option<String>,
    pub http_url: Option<String>,
    pub include_nginx_test: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelToolCall {
    pub name: NativeToolName,
    #[serde(default = "empty_arguments")]
    pub arguments: Value,
}

#[derive(Clone, Debug)]
pub(crate) enum AgentDecision {
    ToolCalls(Vec<ModelToolCall>),
    Answer {
        text: String,
        evidence_ids: Vec<Uuid>,
        goal_achieved: bool,
    },
    Clarify(String),
    ProposeChange {
        title: String,
        summary: String,
        evidence_ids: Vec<Uuid>,
        steps: Vec<ChangeStepDraft>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct AgentModelTurn {
    pub decision: AgentDecision,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub estimated_cost_microusd: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteDecision {
    action: String,
    #[serde(default)]
    tool_calls: Vec<ModelToolCall>,
    answer: Option<String>,
    question: Option<String>,
    #[serde(default)]
    evidence_ids: Vec<Uuid>,
    #[serde(default)]
    goal_achieved: bool,
    title: Option<String>,
    summary: Option<String>,
    #[serde(default)]
    steps: Vec<ChangeStepDraft>,
}

pub(crate) fn parse_remote_decision(
    content: &str,
    allowed_evidence_ids: &[Uuid],
) -> AppResult<AgentDecision> {
    let normalized = normalized_json(content);
    let remote: RemoteDecision =
        serde_json::from_str(normalized).map_err(|_| AppError::ModelResponseInvalid)?;
    let action = remote.action.trim().to_ascii_lowercase().replace('_', "-");
    match action.as_str() {
        "tool-calls" => {
            if remote.tool_calls.is_empty()
                || remote.tool_calls.len() > MAX_TOOL_CALLS_PER_TURN
                || remote.answer.is_some()
                || remote.question.is_some()
                || remote.title.is_some()
                || remote.summary.is_some()
                || !remote.steps.is_empty()
                || !remote.evidence_ids.is_empty()
            {
                return Err(AppError::ModelResponseInvalid);
            }
            // Conversion is intentionally performed here, before any session is touched.
            for call in &remote.tool_calls {
                NativeToolInvocation::from_model_read_call(call.name, call.arguments.clone())?;
            }
            Ok(AgentDecision::ToolCalls(remote.tool_calls))
        }
        "answer" | "final" => {
            if !remote.tool_calls.is_empty()
                || remote.question.is_some()
                || remote.title.is_some()
                || remote.summary.is_some()
                || !remote.steps.is_empty()
            {
                return Err(AppError::ModelResponseInvalid);
            }
            let answer = valid_text(remote.answer, MAX_ANSWER_BYTES)?;
            validate_evidence_ids(&remote.evidence_ids, allowed_evidence_ids)?;
            Ok(AgentDecision::Answer {
                text: answer,
                evidence_ids: remote.evidence_ids,
                goal_achieved: remote.goal_achieved,
            })
        }
        "clarify" | "question" => {
            if !remote.tool_calls.is_empty()
                || !remote.evidence_ids.is_empty()
                || remote.answer.is_some()
                || remote.title.is_some()
                || remote.summary.is_some()
                || !remote.steps.is_empty()
            {
                return Err(AppError::ModelResponseInvalid);
            }
            Ok(AgentDecision::Clarify(valid_text(remote.question, 1_024)?))
        }
        "propose-change" => {
            if !remote.tool_calls.is_empty()
                || remote.question.is_some()
                || remote.steps.is_empty()
                || remote.steps.len() > MAX_TOOL_CALLS_PER_TURN
                || remote.evidence_ids.is_empty()
            {
                return Err(AppError::ModelResponseInvalid);
            }
            validate_evidence_ids(&remote.evidence_ids, allowed_evidence_ids)?;
            Ok(AgentDecision::ProposeChange {
                title: valid_text(remote.title, 256)?,
                summary: valid_text(remote.summary.or(remote.answer), 4_096)?,
                evidence_ids: remote.evidence_ids,
                steps: remote.steps,
            })
        }
        _ => Err(AppError::ModelResponseInvalid),
    }
}

fn normalized_json(content: &str) -> &str {
    let trimmed = content.trim();
    let without_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim()
}

pub(crate) fn invocations(calls: Vec<ModelToolCall>) -> AppResult<Vec<NativeToolInvocation>> {
    calls
        .into_iter()
        .map(|call| NativeToolInvocation::from_model_read_call(call.name, call.arguments))
        .collect()
}

pub(crate) fn local_turn(
    user_request: &str,
    evidence: &[Evidence],
    hints: &PlanningHints,
) -> AgentModelTurn {
    let decision = if evidence.is_empty() {
        local_initial_decision(user_request, hints)
    } else {
        local_answer(user_request, evidence)
    };
    AgentModelTurn {
        input_tokens: ((user_request.len()
            + serde_json::to_vec(&model_evidence(evidence)).map_or(0, |value| value.len()))
        .div_ceil(4))
        .min(u32::MAX as usize) as u32,
        output_tokens: 96,
        estimated_cost_microusd: Some(0),
        decision,
    }
}

fn local_initial_decision(request: &str, hints: &PlanningHints) -> AgentDecision {
    let lower = request.to_ascii_lowercase();
    let call = |name, arguments| ModelToolCall { name, arguments };
    let mut hinted = Vec::new();
    if let Some(service) = &hints.service {
        hinted.push(call(
            NativeToolName::ServiceStatus,
            json!({"service":service}),
        ));
        hinted.push(call(
            NativeToolName::ServiceLogs,
            json!({"service":service,"lines":100}),
        ));
    }
    if let Some(url) = &hints.http_url {
        hinted.push(call(NativeToolName::HttpRequest, json!({"url":url})));
    }
    if hints.include_nginx_test {
        hinted.push(call(NativeToolName::NginxTest, json!({})));
    }
    if !hinted.is_empty() {
        hinted.truncate(MAX_TOOL_CALLS_PER_TURN);
        return AgentDecision::ToolCalls(hinted);
    }
    if contains_any(
        &lower,
        &["large file", "large-file", "大文件", "文件大小", "超过"],
    ) {
        let minimum_bytes = parse_size_bytes(&lower).unwrap_or(DEFAULT_LARGE_FILE_BYTES);
        return AgentDecision::ToolCalls(vec![call(
            NativeToolName::SystemLargeFiles,
            json!({
                "path": explicit_path(request).unwrap_or_else(|| "/".into()),
                "minimumBytes": minimum_bytes.clamp(1_048_576, 1_099_511_627_776)
            }),
        )]);
    }
    if contains_any(&lower, &["disk", "磁盘", "空间", "storage"]) {
        return AgentDecision::ToolCalls(vec![call(NativeToolName::SystemDisk, json!({}))]);
    }
    if contains_any(&lower, &["nginx"]) {
        return AgentDecision::ToolCalls(vec![call(NativeToolName::NginxTest, json!({}))]);
    }
    if contains_any(&lower, &["docker", "容器"]) {
        return AgentDecision::ToolCalls(vec![call(NativeToolName::DockerList, json!({}))]);
    }
    if contains_any(&lower, &["process", "cpu", "进程", "负载"]) {
        return AgentDecision::ToolCalls(vec![call(NativeToolName::ProcessList, json!({}))]);
    }
    if contains_any(&lower, &["listener", "listening", "监听", "端口"]) {
        return AgentDecision::ToolCalls(vec![call(NativeToolName::NetworkListeners, json!({}))]);
    }
    AgentDecision::ToolCalls(vec![call(NativeToolName::SystemInfo, json!({}))])
}

fn local_answer(request: &str, evidence: &[Evidence]) -> AgentDecision {
    if explicit_change_intent(request) {
        if let Some(decision) = local_change_proposal(request, evidence) {
            return decision;
        }
    }
    evidence_answer(request, evidence)
}

pub(crate) fn evidence_answer(request: &str, evidence: &[Evidence]) -> AgentDecision {
    let chinese = request
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character));
    let mut lines = Vec::new();
    for item in evidence {
        if !item.result.success {
            lines.push(if chinese {
                format!(
                    "{} 执行失败（{}）。",
                    item.result.tool_name.as_str(),
                    item.result.error_code.unwrap_or("UNKNOWN")
                )
            } else {
                format!(
                    "{} failed ({}).",
                    item.result.tool_name.as_str(),
                    item.result.error_code.unwrap_or("UNKNOWN")
                )
            });
            continue;
        }
        match item.result.data.as_ref() {
            Some(ToolData::SystemInfo(data)) => lines.push(if chinese {
                format!(
                    "目标主机为 {}，系统 {}，内核 {}，架构 {}。",
                    data.hostname, data.operating_system, data.kernel_release, data.architecture
                )
            } else {
                format!(
                    "Host {} runs {} with kernel {} on {}.",
                    data.hostname, data.operating_system, data.kernel_release, data.architecture
                )
            }),
            Some(ToolData::SystemDisk(data)) => {
                let mut disks = data.disks.iter().collect::<Vec<_>>();
                disks.sort_by(|left, right| right.usage_percent.total_cmp(&left.usage_percent));
                if let Some(highest) = disks.first() {
                    lines.push(if highest.usage_percent >= 95.0 {
                        if chinese {
                            format!("发现严重磁盘容量风险：{} 已使用 {:.1}%。建议继续检查该挂载点的目录占用和大文件。", highest.mount, highest.usage_percent)
                        } else {
                            format!("Critical disk capacity risk: {} is {:.1}% used. Inspect directory usage and large files on this mount next.", highest.mount, highest.usage_percent)
                        }
                    } else if highest.usage_percent >= 85.0 {
                        if chinese {
                            format!("发现磁盘容量预警：{} 已使用 {:.1}%。建议关注增长趋势并定位主要占用目录。", highest.mount, highest.usage_percent)
                        } else {
                            format!("Disk capacity warning: {} is {:.1}% used. Monitor growth and identify the largest directories.", highest.mount, highest.usage_percent)
                        }
                    } else if chinese {
                        format!("未发现明显的磁盘容量异常；最高使用率为 {} 的 {:.1}%。", highest.mount, highest.usage_percent)
                    } else {
                        format!("No material disk capacity issue was found; the highest usage is {:.1}% on {}.", highest.usage_percent, highest.mount)
                    });
                } else {
                    lines.push(if chinese {
                        "磁盘检查成功，但目标没有返回文件系统条目。".into()
                    } else {
                        "The disk check succeeded but returned no filesystem entries.".into()
                    });
                }
                for disk in disks.into_iter().take(12) {
                    lines.push(if chinese {
                        format!(
                            "{}：已使用 {:.1}%（{} / {}）。",
                            disk.mount,
                            disk.usage_percent,
                            human_bytes(disk.used_bytes),
                            human_bytes(disk.total_bytes)
                        )
                    } else {
                        format!(
                            "{}: {:.1}% used ({} / {}).",
                            disk.mount,
                            disk.usage_percent,
                            human_bytes(disk.used_bytes),
                            human_bytes(disk.total_bytes)
                        )
                    });
                }
            }
            Some(ToolData::Diagnostic(data)) if data.category == "large-files" => {
                let files = data.fields.get("files").and_then(Value::as_array);
                if files.is_none_or(Vec::is_empty) {
                    lines.push(if chinese {
                        "未发现符合条件的文件。".into()
                    } else {
                        "No matching files were found.".into()
                    });
                } else if let Some(files) = files {
                    lines.push(if chinese {
                        "发现以下大文件：".into()
                    } else {
                        "Found these large files:".into()
                    });
                    for file in files.iter().take(30) {
                        let path = file.get("path").and_then(Value::as_str).unwrap_or("?");
                        let bytes = file.get("sizeBytes").and_then(Value::as_u64).unwrap_or(0);
                        lines.push(format!("- {} — {}", path, human_bytes(bytes)));
                    }
                }
            }
            Some(ToolData::Diagnostic(data)) => {
                let rendered =
                    serde_json::to_string_pretty(&data.fields).unwrap_or_else(|_| "{}".into());
                let rendered = rendered.chars().take(4_000).collect::<String>();
                lines.push(format!("{}\n```json\n{}\n```", data.category, rendered));
            }
            Some(ToolData::NginxTest(data)) => lines.push(if chinese {
                if data.valid {
                    "Nginx 配置检查通过。".into()
                } else {
                    format!(
                        "Nginx 配置无效：{}",
                        data.error_message.as_deref().unwrap_or("未提供错误详情")
                    )
                }
            } else if data.valid {
                "The Nginx configuration is valid.".into()
            } else {
                format!(
                    "The Nginx configuration is invalid: {}",
                    data.error_message.as_deref().unwrap_or("no error detail")
                )
            }),
            Some(data) => {
                let rendered = serde_json::to_string(data).unwrap_or_else(|_| "{}".into());
                lines.push(rendered.chars().take(4_000).collect());
            }
            None => {}
        }
    }
    let success = evidence.iter().all(|item| item.result.success);
    AgentDecision::Answer {
        text: if lines.is_empty() {
            if chinese {
                "调查已完成，但没有取得可展示的结果。".into()
            } else {
                "The investigation completed without displayable results.".into()
            }
        } else {
            lines.join("\n")
        },
        evidence_ids: evidence.iter().map(|item| item.id).collect(),
        goal_achieved: success,
    }
}

pub(crate) fn change_proposal_is_evidence_bound(
    request: &str,
    evidence: &[Evidence],
    evidence_ids: &[Uuid],
    steps: &[ChangeStepDraft],
) -> bool {
    if !explicit_change_intent(request) || evidence_ids.is_empty() || steps.is_empty() {
        return false;
    }
    let referenced = evidence
        .iter()
        .filter(|item| evidence_ids.contains(&item.id) && item.result.success)
        .collect::<Vec<_>>();
    if referenced.len() != evidence_ids.len() {
        return false;
    }
    steps.iter().all(|step| {
        match step {
        ChangeStepDraft::FilePatch {
            path,
            expected,
            replacement,
        } => {
            let sensitive = redact_secrets(expected).1 || redact_secrets(replacement).1;
            !sensitive
                && request.contains(expected)
                && request.contains(replacement)
                && referenced.iter().any(|item| {
                    matches!(
                        item.result.data.as_ref(),
                        Some(ToolData::Diagnostic(data))
                            if data.category == "file"
                                && data.fields.get("path").and_then(Value::as_str) == Some(path)
                                && data.fields.get("contentPreview").and_then(Value::as_str)
                                    .is_some_and(|content| content.contains(expected))
                    )
                })
        }
        ChangeStepDraft::ServiceRestart { service }
        | ChangeStepDraft::ServiceReload { service } => referenced.iter().any(|item| {
            matches!(
                item.result.data.as_ref(),
                Some(ToolData::ServiceStatus(data)) if data.service.name == *service
            ) || matches!(
                item.result.data.as_ref(),
                Some(ToolData::ServiceLogs(data)) if data.service == *service
            )
        }),
        ChangeStepDraft::NginxReload => referenced.iter().any(|item| {
            matches!(item.result.data.as_ref(), Some(ToolData::NginxTest(data)) if data.valid)
        }),
        ChangeStepDraft::DockerRestart { container } => referenced.iter().any(|item| {
            matches!(
                item.result.data.as_ref(),
                Some(ToolData::Diagnostic(data))
                    if data.category == "docker-inspect"
                        && data.fields.get("container").and_then(Value::as_str) == Some(container)
            )
        }),
    }
    })
}

fn local_change_proposal(request: &str, evidence: &[Evidence]) -> Option<AgentDecision> {
    let chinese = request
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character));
    for item in evidence.iter().filter(|item| item.result.success) {
        match item.result.data.as_ref() {
            Some(ToolData::NginxTest(data)) if data.valid => {
                return Some(AgentDecision::ProposeChange {
                    title: if chinese {
                        "重新加载 Nginx".into()
                    } else {
                        "Reload Nginx".into()
                    },
                    summary: if chinese {
                        "配置检查已通过，可以通过受审批的 ChangeSet 安全重新加载 Nginx。".into()
                    } else {
                        "The configuration check passed, so Nginx can be reloaded through an approved ChangeSet.".into()
                    },
                    evidence_ids: vec![item.id],
                    steps: vec![ChangeStepDraft::NginxReload],
                });
            }
            Some(ToolData::ServiceStatus(data))
                if !matches!(data.service.status, crate::domain::ServiceStatus::Active) =>
            {
                return Some(AgentDecision::ProposeChange {
                    title: if chinese {
                        format!("重启 {} 服务", data.service.name)
                    } else {
                        format!("Restart {} service", data.service.name)
                    },
                    summary: if chinese {
                        "服务当前未处于 active 状态，建议通过受审批的 ChangeSet 重启并验证。".into()
                    } else {
                        "The service is not active. Restart it through an approved ChangeSet and verify the result.".into()
                    },
                    evidence_ids: vec![item.id],
                    steps: vec![ChangeStepDraft::ServiceRestart {
                        service: data.service.name.clone(),
                    }],
                });
            }
            _ => {}
        }
    }
    None
}

fn explicit_change_intent(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "fix",
            "repair",
            "restart",
            "reload",
            "remediate",
            "resolve",
            "apply",
            "replace",
            "modify",
            "change",
            "修复",
            "重启",
            "重载",
            "恢复",
            "处理",
            "替换",
            "修改",
            "执行修复",
            "应用变更",
        ],
    )
}

fn validate_evidence_ids(ids: &[Uuid], allowed: &[Uuid]) -> AppResult<()> {
    let allowed = allowed.iter().copied().collect::<HashSet<_>>();
    if ids.len() > allowed.len() || ids.iter().any(|id| !allowed.contains(id)) {
        Err(AppError::ModelResponseInvalid)
    } else {
        Ok(())
    }
}

fn valid_text(value: Option<String>, maximum: usize) -> AppResult<String> {
    let value = value
        .map(|value| value.trim().to_owned())
        .unwrap_or_default();
    if value.is_empty() || value.len() > maximum || value.chars().any(|item| item == '\0') {
        Err(AppError::ModelResponseInvalid)
    } else {
        Ok(value)
    }
}

fn empty_arguments() -> Value {
    json!({})
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn explicit_path(request: &str) -> Option<String> {
    request
        .split_whitespace()
        .map(|item| item.trim_matches(|character: char| ",，。；;：:'\"()[]{}".contains(character)))
        .find(|item| item.starts_with('/') && item.len() <= 4_096)
        .map(str::to_owned)
}

fn parse_size_bytes(value: &str) -> Option<u64> {
    let chars = value.as_bytes();
    for start in 0..chars.len() {
        if !chars[start].is_ascii_digit() {
            continue;
        }
        let mut end = start;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }
        let number = value[start..end].parse::<u64>().ok()?;
        let suffix = value[end..].trim_start();
        let multiplier = if suffix.starts_with("gb") || suffix.starts_with('g') {
            1024_u64.pow(3)
        } else if suffix.starts_with("mb") || suffix.starts_with('m') || suffix.starts_with('兆') {
            1024_u64.pow(2)
        } else if suffix.starts_with("kb") || suffix.starts_with('k') {
            1024
        } else {
            continue;
        };
        return number.checked_mul(multiplier);
    }
    None
}

fn human_bytes(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentic::context::ContextTrust;
    use crate::domain::{DiskUsage, ServiceHealth, ServiceStatus};
    use crate::tools::{DiagnosticData, ServiceStatusData, SystemDiskData, ToolResult};

    #[test]
    fn local_planner_routes_large_file_goal_with_requested_threshold() {
        let turn = local_turn(
            "帮我查找 /var 下超过 100MB 的文件",
            &[],
            &PlanningHints::default(),
        );
        let AgentDecision::ToolCalls(calls) = turn.decision else {
            panic!("expected tool calls");
        };
        assert_eq!(calls[0].name, NativeToolName::SystemLargeFiles);
        assert_eq!(calls[0].arguments["path"], "/var");
        assert_eq!(calls[0].arguments["minimumBytes"], 100 * 1024 * 1024);
    }

    #[test]
    fn remote_plans_cannot_select_write_tools_or_invent_evidence() {
        let write = r#"{"action":"tool-calls","toolCalls":[{"name":"service.restart","arguments":{"service":"nginx"}}]}"#;
        assert!(parse_remote_decision(write, &[]).is_err());

        let invented = Uuid::new_v4();
        let answer = format!(
            r#"{{"action":"answer","answer":"done","evidenceIds":["{invented}"],"goalAchieved":true}}"#
        );
        assert!(parse_remote_decision(&answer, &[]).is_err());
    }

    #[test]
    fn remote_protocol_accepts_common_json_fence_and_action_spelling() {
        let decision = parse_remote_decision(
            "```json\n{\"action\":\"tool_calls\",\"toolCalls\":[{\"name\":\"system.info\",\"arguments\":{}}]}\n```",
            &[],
        )
        .expect("compatible response");
        assert!(matches!(decision, AgentDecision::ToolCalls(_)));
    }

    #[test]
    fn repeated_disk_call_can_fall_back_to_an_evidence_bound_diagnosis() {
        let disk = evidence(
            NativeToolName::SystemDisk,
            ToolData::SystemDisk(SystemDiskData {
                disks: vec![
                    DiskUsage {
                        mount: "/data".into(),
                        used_bytes: 96,
                        total_bytes: 100,
                        usage_percent: 96.0,
                    },
                    DiskUsage {
                        mount: "/".into(),
                        used_bytes: 40,
                        total_bytes: 100,
                        usage_percent: 40.0,
                    },
                ],
            }),
        );

        let AgentDecision::Answer {
            text,
            evidence_ids,
            goal_achieved,
        } = evidence_answer("检查磁盘使用情况并诊断问题", std::slice::from_ref(&disk))
        else {
            panic!("expected deterministic evidence answer");
        };
        assert!(text.contains("严重磁盘容量风险"));
        assert!(text.contains("/data"));
        assert!(text.contains("96.0%"));
        assert_eq!(evidence_ids, vec![disk.id]);
        assert!(goal_achieved);
    }

    #[test]
    fn change_proposals_require_explicit_user_intent_and_matching_evidence() {
        let step = ChangeStepDraft::ServiceRestart {
            service: "nginx".into(),
        };
        assert!(!change_proposal_is_evidence_bound(
            "check nginx",
            &[],
            &[],
            &[step]
        ));

        let evidence = evidence(
            NativeToolName::ServiceStatus,
            ToolData::ServiceStatus(ServiceStatusData {
                service: ServiceHealth {
                    name: "nginx".into(),
                    status: ServiceStatus::Failed,
                },
            }),
        );
        assert!(change_proposal_is_evidence_bound(
            "修复并重启 nginx",
            std::slice::from_ref(&evidence),
            &[evidence.id],
            &[ChangeStepDraft::ServiceRestart {
                service: "nginx".into()
            }]
        ));
        assert!(!change_proposal_is_evidence_bound(
            "修复并重启 sshd",
            std::slice::from_ref(&evidence),
            &[evidence.id],
            &[ChangeStepDraft::ServiceRestart {
                service: "sshd".into()
            }]
        ));
    }

    #[test]
    fn file_patch_requires_user_supplied_exact_non_secret_text_and_remote_match() {
        let evidence = evidence(
            NativeToolName::FileInspect,
            ToolData::Diagnostic(DiagnosticData {
                category: "file",
                fields: json!({"path":"/etc/app.conf","contentPreview":"workers=2\n"}),
            }),
        );
        let valid = ChangeStepDraft::FilePatch {
            path: "/etc/app.conf".into(),
            expected: "workers=2".into(),
            replacement: "workers=4".into(),
        };
        assert!(change_proposal_is_evidence_bound(
            "修改 /etc/app.conf，把 workers=2 替换为 workers=4",
            std::slice::from_ref(&evidence),
            &[evidence.id],
            &[valid]
        ));
        let secret = ChangeStepDraft::FilePatch {
            path: "/etc/app.conf".into(),
            expected: "PASSWORD=old".into(),
            replacement: "PASSWORD=new".into(),
        };
        assert!(!change_proposal_is_evidence_bound(
            "修改 PASSWORD=old 为 PASSWORD=new",
            std::slice::from_ref(&evidence),
            &[evidence.id],
            &[secret]
        ));
    }

    fn evidence(tool_name: NativeToolName, data: ToolData) -> Evidence {
        let invocation_id = Uuid::new_v4();
        Evidence {
            id: Uuid::new_v4(),
            source: format!("tool.{}", tool_name.as_str()),
            invocation_id,
            trust: ContextTrust::UntrustedRemoteData,
            summary: "test-evidence",
            result: ToolResult {
                invocation_id,
                tool_name,
                success: true,
                summary: "test-evidence",
                data: Some(data),
                error_code: None,
                warnings: Vec::new(),
                started_at_epoch_ms: 1,
                duration_ms: 1,
                truncated: false,
                cancelled: false,
                untrusted_remote_data: true,
            },
        }
    }
}
