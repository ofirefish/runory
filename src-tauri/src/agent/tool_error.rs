//! Stable observation error taxonomy for Runtime V2 tool failures (AR2-F).

/// Maps native tool / domain error codes into a small runtime vocabulary.
pub(crate) fn classify_tool_error(code: &str) -> &'static str {
    match code {
        "EXEC_FAILED" | "SFTP_NOT_FOUND" | "UNSUPPORTED_REMOTE" => "TOOL_EXECUTION_FAILED",
        "INVALID_OPERATION" | "MODEL_RESPONSE_INVALID" => "TOOL_INPUT_INVALID",
        "SESSION_NOT_FOUND" | "SESSION_DISCONNECTED" | "SSH_NOT_CONNECTED" => {
            "TOOL_SESSION_UNAVAILABLE"
        }
        "TOOL_CANCELLED" => "TOOL_CANCELLED",
        "TOOL_RISK_CEILING_EXCEEDED" | "TOOL_POLICY_DENIED" | "AGENT_POLICY_DENIED" => {
            "TOOL_POLICY_BLOCKED"
        }
        _ => "TOOL_FAILED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exec_and_policy_failures() {
        assert_eq!(classify_tool_error("EXEC_FAILED"), "TOOL_EXECUTION_FAILED");
        assert_eq!(
            classify_tool_error("TOOL_POLICY_DENIED"),
            "TOOL_POLICY_BLOCKED"
        );
        assert_eq!(classify_tool_error("UNKNOWN"), "TOOL_FAILED");
    }
}
