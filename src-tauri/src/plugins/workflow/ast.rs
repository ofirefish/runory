use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    pub api_version: String,
    pub kind: String,
    pub metadata: WorkflowMetadata,
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub outputs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub id: String,
    pub uses: String,
    #[serde(default)]
    pub with: serde_json::Value,
    #[serde(rename = "if", default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub retry: Option<RetryPolicy>,
    #[serde(default)]
    pub on_failure: Option<FailurePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    pub max_attempts: u32,
    #[serde(default)]
    pub delay: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailurePolicy {
    #[default]
    Abort,
    Continue,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrivilegeMode {
    #[default]
    None,
    Sudo,
}

/// Environment value: literal string, `{ value: ... }`, or `{ secretRef: ... }`.
/// Secret plaintext must never be stored in workflow context or audit logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvValue {
    Literal(String),
    Value {
        value: String,
    },
    SecretRef {
        #[serde(rename = "secretRef")]
        secret_ref: String,
    },
}

impl EnvValue {
    pub fn audit_display(&self) -> String {
        match self {
            Self::Literal(_) | Self::Value { .. } => "<literal>".into(),
            Self::SecretRef { .. } => "********".into(),
        }
    }

    pub fn as_literal(&self) -> Option<&str> {
        match self {
            Self::Literal(v) | Self::Value { value: v } => Some(v.as_str()),
            Self::SecretRef { .. } => None,
        }
    }

    pub fn secret_ref(&self) -> Option<&str> {
        match self {
            Self::SecretRef { secret_ref } => Some(secret_ref.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedWorkflow {
    pub definition: WorkflowDefinition,
}
