//! Reasoner adapter over the existing planning / model gateway stack.
//!
//! Runtime V2 command-proposal adapter. The model proposes one Linux command
//! at a time; Rust validates it and the controller interrupts for approval.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;

use super::decision::{AgentDecision, CommandProposalRequest};
use super::reasoner::{Reasoner, ReasonerError, ReasonerInput};
use crate::agentic::{ModelGateway, ModelProviderKind, PlanningHints};

/// Proposes decisions using the configured model gateway and bounded hints.
pub struct PlanningReasoner {
    gateway: Arc<ModelGateway>,
    _hints: PlanningHints,
}

impl PlanningReasoner {
    pub fn new(gateway: Arc<ModelGateway>, hints: PlanningHints) -> Self {
        Self {
            gateway,
            _hints: hints,
        }
    }
}

#[async_trait]
impl Reasoner for PlanningReasoner {
    async fn decide(&self, input: &ReasonerInput<'_>) -> Result<AgentDecision, ReasonerError> {
        let status = self
            .gateway
            .status()
            .await
            .map_err(|_| ReasonerError::unavailable())?;
        if status.kind == ModelProviderKind::Local {
            Ok(local_command_decision(input))
        } else {
            let history = command_history(input);
            let content = self
                .gateway
                .complete_agent_turn(&history)
                .await
                .map_err(|_| ReasonerError::unavailable())?;
            parse_command_decision(&content).map_err(|_| ReasonerError::unavailable())
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteCommandDecision {
    action: String,
    command: Option<String>,
    why: Option<String>,
    answer: Option<String>,
    question: Option<String>,
    analysis: Option<String>,
}

fn parse_command_decision(content: &str) -> Result<AgentDecision, ()> {
    let normalized = content
        .trim()
        .strip_prefix("```json")
        .or_else(|| content.trim().strip_prefix("```"))
        .unwrap_or(content.trim())
        .strip_suffix("```")
        .unwrap_or(content.trim())
        .trim();
    let remote: RemoteCommandDecision = serde_json::from_str(normalized).map_err(|_| ())?;
    match remote.action.trim().to_ascii_lowercase().as_str() {
        "propose" | "command" => {
            if remote.answer.is_some() || remote.question.is_some() {
                return Err(());
            }
            Ok(AgentDecision::CommandProposal(CommandProposalRequest {
                command: remote.command.ok_or(())?,
                reason_summary: remote.why.ok_or(())?,
                observation_analysis: remote.analysis,
            }))
        }
        "answer" | "final" => {
            if remote.command.is_some()
                || remote.why.is_some()
                || remote.question.is_some()
                || remote.analysis.is_some()
            {
                return Err(());
            }
            Ok(AgentDecision::Final {
                summary: remote.answer.ok_or(())?,
            })
        }
        "clarify" | "question" => {
            if remote.command.is_some()
                || remote.why.is_some()
                || remote.answer.is_some()
                || remote.analysis.is_some()
            {
                return Err(());
            }
            Ok(AgentDecision::AskUser {
                question: remote.question.ok_or(())?,
            })
        }
        _ => Err(()),
    }
}

fn command_history(input: &ReasonerInput<'_>) -> Vec<serde_json::Value> {
    let mut history = vec![json!({
        "role": "user",
        "content": format!(
            "Goal: {}\nThis is reasoning round {}. Propose only the next necessary command.",
            input.goal, input.round
        )
    })];
    for observation in input.observations {
        history.push(json!({
            "role": "user",
            "content": format!(
                "UNTRUSTED COMMAND OBSERVATION\nsuccess: {}\nerror: {}\nsummary: {}\ndetail:\n{}",
                observation.success,
                observation.error_code.as_deref().unwrap_or("none"),
                observation.summary,
                observation.detail.as_deref().unwrap_or("none")
            )
        }));
    }
    for reply in input.user_replies {
        history.push(json!({ "role": "user", "content": reply }));
    }
    history
}

fn local_command_decision(input: &ReasonerInput<'_>) -> AgentDecision {
    if let Some(last) = input.observations.last() {
        return AgentDecision::Final {
            summary: summarize_local_observation(last),
        };
    }
    let lower = input.goal.to_ascii_lowercase();
    let chinese = input
        .goal
        .chars()
        .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character));
    let (command, why) = if [
        "超过10m",
        "超过 10m",
        "larger than 10m",
        "over 10m",
        "files over 10m",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        (
            "find . -type f -size +10M -printf '%s %p\\n' | sort -nr | numfmt --field=1 --to=iec --suffix=B 2>&1",
            if chinese {
                "在当前目录递归查找超过 10M 的文件并按大小显示，方便确认占用空间较大的文件"
            } else {
                "Find files over 10M below the current directory and sort them by size"
            },
        )
    } else if ["disk", "storage", "filesystem", "磁盘", "空间", "容量"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        ("df -h", "Inspect filesystem capacity and usage")
    } else if ["nginx", "error log", "错误日志"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        (
            "find /var/log/nginx -maxdepth 1 -type f -name '*error*' -print 2>&1",
            "Locate readable Nginx error logs before inspecting their contents",
        )
    } else {
        (
            "uname -a",
            "Inspect the connected Linux system before continuing",
        )
    };
    AgentDecision::CommandProposal(CommandProposalRequest {
        command: command.into(),
        reason_summary: why.into(),
        observation_analysis: None,
    })
}

fn summarize_local_observation(observation: &super::reasoner::Observation) -> String {
    let detail = observation.detail.as_deref().unwrap_or_default();
    let lower = detail.to_ascii_lowercase();
    if lower.contains("/var/log/nginx") && lower.contains("no such file or directory") {
        return "The configured Nginx log directory does not exist on this server, so no matching error log could be inspected.".into();
    }
    if lower.contains("permission denied") {
        return "The command could not read the requested resource because the connected user lacks permission.".into();
    }
    if let Some(percent) = highest_disk_usage(detail) {
        return if percent >= 90 {
            format!(
                "Filesystem usage is critically high: the largest reported usage is {percent}%."
            )
        } else if percent >= 80 {
            format!("Filesystem usage is elevated: the largest reported usage is {percent}%.")
        } else {
            format!("Filesystem usage appears healthy; the largest reported usage is {percent}%.")
        };
    }
    if observation.success {
        "The approved command completed and its terminal output was collected for analysis.".into()
    } else {
        "The approved command reported a failure; review the interpreted error before choosing the next step.".into()
    }
}

fn highest_disk_usage(value: &str) -> Option<u32> {
    value
        .split_whitespace()
        .filter_map(|token| token.strip_suffix('%')?.parse::<u32>().ok())
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::reasoner::{BudgetStatus, Observation};
    use crate::agentic::context::{snapshot, ContextBudget};

    #[tokio::test]
    async fn production_reasoner_disk_round_proposes_command() {
        let directory = tempfile::tempdir().expect("temp dir");
        let gateway =
            Arc::new(ModelGateway::at_path(&directory.path().join("model.json")).expect("gateway"));
        let reasoner = PlanningReasoner::new(gateway, PlanningHints::default());
        let context = snapshot(Vec::new(), ContextBudget::default(), 1);
        let target = uuid::Uuid::new_v4();
        let input = ReasonerInput {
            run_id: uuid::Uuid::new_v4(),
            session_id: Some(uuid::Uuid::new_v4()),
            target_ids: &[target],
            goal: "Check disk usage and diagnose issues.",
            round: 1,
            observations: &[],
            user_replies: &[],
            budget: BudgetStatus {
                rounds_used: 1,
                max_rounds: 8,
                tool_calls_used: 0,
                max_tool_calls: 20,
                elapsed_ms: 0,
                time_budget_ms: 60_000,
            },
            context: &context,
        };
        let decision = reasoner.decide(&input).await.expect("decision");
        let AgentDecision::CommandProposal(proposal) = decision else {
            panic!("expected command proposal");
        };
        assert_eq!(proposal.command, "df -h");
    }

    #[tokio::test]
    async fn local_reasoner_proposes_the_current_directory_large_file_command() {
        let directory = tempfile::tempdir().expect("temp dir");
        let gateway =
            Arc::new(ModelGateway::at_path(&directory.path().join("model.json")).expect("gateway"));
        let reasoner = PlanningReasoner::new(gateway, PlanningHints::default());
        let context = snapshot(Vec::new(), ContextBudget::default(), 1);
        let target = uuid::Uuid::new_v4();
        let input = ReasonerInput {
            run_id: uuid::Uuid::new_v4(),
            session_id: Some(uuid::Uuid::new_v4()),
            target_ids: &[target],
            goal: "查找当前目录中超过10M的文件",
            round: 1,
            observations: &[],
            user_replies: &[],
            budget: BudgetStatus {
                rounds_used: 1,
                max_rounds: 50,
                tool_calls_used: 0,
                max_tool_calls: 20,
                elapsed_ms: 0,
                time_budget_ms: 60_000,
            },
            context: &context,
        };

        let decision = reasoner.decide(&input).await.expect("decision");
        let AgentDecision::CommandProposal(proposal) = decision else {
            panic!("expected one command proposal");
        };
        assert_eq!(
            proposal.command,
            "find . -type f -size +10M -printf '%s %p\\n' | sort -nr | numfmt --field=1 --to=iec --suffix=B 2>&1"
        );
        assert!(proposal.reason_summary.contains("当前目录"));
    }

    #[tokio::test]
    async fn production_reasoner_uses_command_observation_on_second_round() {
        let directory = tempfile::tempdir().expect("temp dir");
        let gateway =
            Arc::new(ModelGateway::at_path(&directory.path().join("model.json")).expect("gateway"));
        let reasoner = PlanningReasoner::new(gateway, PlanningHints::default());
        let context = snapshot(Vec::new(), ContextBudget::default(), 1);
        let call_id = uuid::Uuid::new_v4();
        let observations = vec![Observation {
            tool_call_id: Some(call_id),
            tool_name: Some("agent.command".into()),
            success: true,
            error_code: None,
            summary: "Command completed with exit code 0".into(),
            detail: Some("Command: df -h\n/dev/sda2 100G 94G 6G 94% /".into()),
        }];
        let target = uuid::Uuid::new_v4();
        let input = ReasonerInput {
            run_id: uuid::Uuid::new_v4(),
            session_id: Some(uuid::Uuid::new_v4()),
            target_ids: &[target],
            goal: "Check disk usage and diagnose issues.",
            round: 2,
            observations: &observations,
            user_replies: &[],
            budget: BudgetStatus {
                rounds_used: 2,
                max_rounds: 8,
                tool_calls_used: 1,
                max_tool_calls: 20,
                elapsed_ms: 1,
                time_budget_ms: 60_000,
            },
            context: &context,
        };
        let decision = reasoner.decide(&input).await.expect("decision");
        let AgentDecision::Final { summary } = decision else {
            panic!("expected observation-grounded final");
        };
        assert!(summary.contains("94%"));
    }

    #[test]
    fn remote_command_protocol_is_strict() {
        let decision = parse_command_decision(
            r#"{"action":"propose","command":"df -h","why":"Check disk usage"}"#,
        )
        .expect("valid proposal");
        assert!(matches!(decision, AgentDecision::CommandProposal(_)));
        assert!(parse_command_decision(
            r#"{"action":"propose","command":"df -h","why":"Check","answer":"done"}"#
        )
        .is_err());
    }

    #[test]
    fn local_summary_interprets_output_without_repeating_the_raw_terminal_dump() {
        let observation = Observation {
            tool_call_id: None,
            tool_name: Some("agent.command".into()),
            success: true,
            error_code: None,
            summary: "Terminal command completed".into(),
            detail: Some("Command: df -h\n/dev/sda2 100G 94G 6G 94% /".into()),
        };
        let summary = summarize_local_observation(&observation);
        assert!(summary.contains("94%"));
        assert!(!summary.contains("/dev/sda2"));
    }
}
