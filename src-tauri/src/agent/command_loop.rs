//! Detect redundant command proposals so Runtime V2 cannot spin on the same
//! investigation query after evidence has already been collected.
//!
//! Similarity is intentionally generic: it does not hard-code journalctl or any
//! single tool. Echo banners, whitespace, and near-duplicate pipelines are
//! normalized away before comparison.

use super::reasoner::Observation;

pub const COMMAND_LOOP_DETECTED: &str = "COMMAND_LOOP_DETECTED";

const MIN_TOKEN_LEN: usize = 3;
const HIGH_OVERLAP: f32 = 0.82;
const SUBSTRING_RATIO: f32 = 0.88;

/// True when `candidate` is the same investigation as a prior attempt.
pub fn is_redundant_command(candidate: &str, prior_commands: &[String]) -> bool {
    let normalized = normalize_command(candidate);
    if normalized.is_empty() {
        return false;
    }
    prior_commands.iter().any(|prior| {
        let other = normalize_command(prior);
        !other.is_empty() && commands_overlap(&normalized, &other)
    })
}

/// Collect previously attempted commands from observation details/summaries.
pub fn prior_commands_from_observations(observations: &[Observation]) -> Vec<String> {
    observations
        .iter()
        .filter_map(extract_attempted_command)
        .collect()
}

pub fn loop_rejection_detail(repeat_count: usize) -> String {
    if repeat_count >= 2 {
        "This command (or a near-identical variant) was already attempted and did not add new evidence. Evidence has plateaued: propose a materially different command or evidence source, or answer with the best-supported conclusion including what remains unknown.".into()
    } else {
        "This command (or a near-identical variant) was already attempted. Do not repeat it. Change the query, time window, or evidence source, or answer if the goal is already supported.".into()
    }
}

fn extract_attempted_command(observation: &Observation) -> Option<String> {
    observation
        .detail
        .as_deref()
        .and_then(command_line_from_detail)
}

fn command_line_from_detail(detail: &str) -> Option<String> {
    for line in detail.lines() {
        let line = line.trim();
        if let Some(command) = line.strip_prefix("Command:") {
            let command = command.trim();
            if !command.is_empty() {
                return Some(command.to_owned());
            }
        }
    }
    None
}

/// Collapse cosmetic wrappers so `echo '==='; journalctl ... | tail` compares
/// equal to a later `journalctl ... | head` when the core query matches.
pub(crate) fn normalize_command(command: &str) -> String {
    let mut value = command.trim().to_ascii_lowercase();
    value = collapse_whitespace(&value);
    // Drop decorative echo banners the model often prepends.
    loop {
        let trimmed = strip_leading_echo(&value);
        if trimmed == value {
            break;
        }
        value = trimmed;
    }
    value = strip_output_cosmetics(&value);
    collapse_whitespace(&value)
}

/// Remove pager/output-format/line-count cosmetics that do not change the
/// investigation target (same journal boot/window with a different `tail`).
/// Keeps substantive filters such as `grep` / `awk`.
fn strip_output_cosmetics(command: &str) -> String {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut cleaned = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if matches!(token, "--no-pager" | "-q" | "--quiet") {
            index += 1;
            continue;
        }
        if matches!(token, "-o" | "--output") {
            index += 2;
            continue;
        }
        if token.starts_with("--output=") {
            index += 1;
            continue;
        }
        if token == "|" && index + 1 < tokens.len() && matches!(tokens[index + 1], "head" | "tail")
        {
            index += 2;
            if index < tokens.len() && matches!(tokens[index], "-n" | "-c") {
                index += 2;
            } else if index < tokens.len()
                && tokens[index]
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit())
                && !tokens[index].is_empty()
            {
                index += 1;
            }
            continue;
        }
        cleaned.push(token);
        index += 1;
    }
    cleaned.join(" ")
}

fn strip_leading_echo(command: &str) -> String {
    let trimmed = command.trim_start();
    if !trimmed.starts_with("echo ") {
        return trimmed.to_owned();
    }
    let after_echo = &trimmed["echo ".len()..];
    let rest = match after_echo.chars().next() {
        Some('\'') => skip_quoted(after_echo, '\''),
        Some('"') => skip_quoted(after_echo, '"'),
        _ => after_echo
            .split_once([';', '\n'])
            .map(|(_, rest)| rest)
            .unwrap_or(""),
    };
    let rest = rest.trim_start_matches([' ', ';', '\n', '\t']);
    if rest.is_empty() {
        trimmed.to_owned()
    } else {
        rest.to_owned()
    }
}

fn skip_quoted(value: &str, quote: char) -> &str {
    let bytes = value.as_bytes();
    if bytes.first().copied() != Some(quote as u8) {
        return value;
    }
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == quote as u8 {
            return value.get(index + 1..).unwrap_or("");
        }
        if bytes[index] == b'\\' && index + 1 < bytes.len() {
            index += 2;
            continue;
        }
        index += 1;
    }
    ""
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn commands_overlap(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    if !shorter.is_empty()
        && longer.contains(shorter)
        && (shorter.len() as f32 / longer.len() as f32) >= SUBSTRING_RATIO
    {
        return true;
    }
    token_jaccard(left, right) >= HIGH_OVERLAP
}

fn significant_tokens(command: &str) -> Vec<&str> {
    command
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
        .filter(|token| token.len() >= MIN_TOKEN_LEN)
        .filter(|token| {
            !matches!(
                *token,
                "echo"
                    | "head"
                    | "tail"
                    | "pager"
                    | "none"
                    | "true"
                    | "false"
                    | "null"
                    | "and"
                    | "the"
                    | "for"
                    | "with"
            )
        })
        .collect()
}

fn token_jaccard(left: &str, right: &str) -> f32 {
    let left_tokens = significant_tokens(left);
    let right_tokens = significant_tokens(right);
    // Short commands rely on exact/substring match; Jaccard is for longer queries.
    if left_tokens.len() < 3 || right_tokens.len() < 3 {
        return 0.0;
    }
    let mut intersection = 0usize;
    for token in &left_tokens {
        if right_tokens.contains(token) {
            intersection += 1;
        }
    }
    let union = left_tokens.len() + right_tokens.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

/// Count how many prior attempts already overlap this candidate.
pub fn redundant_prior_count(candidate: &str, prior_commands: &[String]) -> usize {
    let normalized = normalize_command(candidate);
    if normalized.is_empty() {
        return 0;
    }
    prior_commands
        .iter()
        .filter(|prior| {
            let other = normalize_command(prior);
            !other.is_empty() && commands_overlap(&normalized, &other)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn observation(detail: &str) -> Observation {
        Observation {
            tool_call_id: Some(Uuid::new_v4()),
            tool_name: Some("agent.command".into()),
            success: true,
            error_code: None,
            summary: "Command completed".into(),
            detail: Some(detail.into()),
        }
    }

    #[test]
    fn exact_and_echo_wrapped_commands_are_redundant() {
        let prior = vec![
            "journalctl -b -1 --since '2026-09-18 11:00:00' --until '2026-09-18 12:40:00' --no-pager | tail -n 300"
                .into(),
        ];
        let wrapped = "echo '=== previous boot ==='; journalctl -b -1 --since '2026-09-18 11:00:00' --until '2026-09-18 12:40:00' --no-pager -o short-precise | head -n 400";
        assert!(is_redundant_command(wrapped, &prior));
        assert!(is_redundant_command(&prior[0], &prior));
    }

    #[test]
    fn materially_different_commands_are_allowed() {
        let prior = vec!["journalctl -b -1 --since '2026-09-18 11:00:00' | tail -n 100".into()];
        assert!(!is_redundant_command(
            "docker ps -a; docker events --since 60m --until 0s 2>/dev/null | tail -n 50",
            &prior
        ));
        assert!(!is_redundant_command("free -m; swapon --show", &prior));
    }

    #[test]
    fn prior_commands_are_parsed_from_observation_details() {
        let observations = vec![observation(
            "Command: uptime\nExit code: unavailable\nstdout:\n1 user\nstderr:\n",
        )];
        assert_eq!(
            prior_commands_from_observations(&observations),
            vec!["uptime".to_owned()]
        );
    }

    #[test]
    fn whitespace_and_case_differences_do_not_bypass_detection() {
        let prior = vec!["DF -h /var".into()];
        assert!(is_redundant_command("df   -h   /var", &prior));
    }

    #[test]
    fn output_cosmetics_do_not_bypass_loop_detection() {
        let prior = vec![
            "journalctl -b -1 --since '2026-09-18 12:00:00' --until '2026-09-18 12:40:00' --no-pager | tail -n 300"
                .into(),
        ];
        assert!(is_redundant_command(
            "journalctl -b -1 --since '2026-09-18 12:00:00' --until '2026-09-18 12:40:00' -o short-iso | head -n 120",
            &prior
        ));
        assert!(!is_redundant_command(
            "journalctl -b -1 --since '2026-09-18 12:00:00' --until '2026-09-18 12:40:00' --no-pager | grep -i oom",
            &prior
        ));
    }

    #[test]
    fn short_distinct_commands_are_not_collapsed_by_token_overlap() {
        let prior = vec!["free -m".into()];
        assert!(!is_redundant_command("free -h", &prior));
        assert!(!is_redundant_command("uptime", &prior));
    }
}
