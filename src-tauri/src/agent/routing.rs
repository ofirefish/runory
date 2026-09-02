//! Bounded routing hints extracted from natural-language goals (AR2-E).
//! Replaces frontend `inferDiagnosticInputs` heuristics.

use crate::agentic::PlanningHints;

pub fn planning_hints_from_goal(text: &str) -> PlanningHints {
    let lower = text.to_ascii_lowercase();
    let url = text.split_whitespace().find_map(|token| {
        let trimmed = token.trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            Some(trimmed.to_owned())
        } else {
            None
        }
    });
    let service = {
        let lower = text.to_ascii_lowercase();
        if let Some(rest) = lower.split("service:").nth(1) {
            rest.split_whitespace()
                .next()
                .map(|token| token.trim().to_owned())
        } else if let Some(rest) = lower.split("systemd:").nth(1) {
            rest.split_whitespace()
                .next()
                .map(|token| token.trim().to_owned())
        } else if let Some(rest) = text.split("服务").nth(1) {
            rest.split([':', '：'])
                .nth(1)
                .and_then(|segment| segment.split_whitespace().next())
                .map(|token| token.trim().to_owned())
        } else {
            None
        }
    };
    PlanningHints {
        service,
        http_url: url,
        include_nginx_test: ["nginx", "网站", "website", "gateway", "反向代理"]
            .iter()
            .any(|needle| lower.contains(needle)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_service_url_and_nginx_hint() {
        let hints = planning_hints_from_goal("Check service: nginx at https://example.com/health");
        assert_eq!(hints.service.as_deref(), Some("nginx"));
        assert_eq!(
            hints.http_url.as_deref(),
            Some("https://example.com/health")
        );
        assert!(hints.include_nginx_test);
    }

    #[test]
    fn does_not_invent_optional_targets() {
        let hints = planning_hints_from_goal("Why is this server slow?");
        assert!(hints.service.is_none());
        assert!(hints.http_url.is_none());
        assert!(!hints.include_nginx_test);
    }
}
