use crate::domain::AppError;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin package is invalid")]
    InvalidPackage,
    #[error("manifest validation failed: {0}")]
    InvalidManifest(String),
    #[error("plugin is incompatible with this Runory version")]
    IncompatibleVersion,
    #[error("plugin is already installed")]
    AlreadyInstalled,
    #[error("plugin was not found")]
    NotFound,
    #[error("plugin is disabled")]
    Disabled,
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("workflow not found: {0}")]
    WorkflowNotFound(String),
    #[error("workflow validation failed: {0}")]
    WorkflowValidation(String),
    #[error("action not found: {0}")]
    ActionNotFound(String),
    #[error("policy denied operation")]
    PolicyDenied,
    #[error("workflow cancelled")]
    Cancelled,
    #[error("remote execution failed: {0}")]
    RemoteExecution(String),
    #[error("plugin package signature is missing")]
    SignatureMissing,
    #[error("plugin package signature is invalid")]
    SignatureInvalid,
    #[error("plugin signing key is unknown")]
    SigningKeyUnknown,
    #[error("plugin signing key is revoked")]
    SigningKeyRevoked,
    #[error("plugin publisher does not match signer")]
    PublisherMismatch,
    #[error("plugin is revoked")]
    PluginRevoked,
    #[error("plugin version is revoked")]
    PluginVersionRevoked,
    #[error("plugin update requires permission review")]
    UpdatePermissionReviewRequired,
    #[error("plugin plan is stale")]
    PlanStale,
    #[error("plugin marketplace is unavailable")]
    MarketplaceUnavailable,
    #[error("plugin marketplace hash mismatch")]
    HashMismatch,
    #[error("plugin storage failed")]
    Storage,
}

impl PluginError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidPackage => "PLUGIN_PACKAGE_INVALID",
            Self::InvalidManifest(_) => "PLUGIN_MANIFEST_INVALID",
            Self::IncompatibleVersion => "PLUGIN_VERSION_INCOMPATIBLE",
            Self::AlreadyInstalled => "PLUGIN_ALREADY_INSTALLED",
            Self::NotFound => "PLUGIN_NOT_FOUND",
            Self::Disabled => "PLUGIN_DISABLED",
            Self::PermissionDenied(_) => "PLUGIN_PERMISSION_DENIED",
            Self::WorkflowNotFound(_) => "PLUGIN_WORKFLOW_NOT_FOUND",
            Self::WorkflowValidation(_) => "PLUGIN_WORKFLOW_INVALID",
            Self::ActionNotFound(_) => "PLUGIN_ACTION_NOT_FOUND",
            Self::PolicyDenied => "PLUGIN_POLICY_DENIED",
            Self::Cancelled => "PLUGIN_CANCELLED",
            Self::RemoteExecution(_) => "PLUGIN_EXECUTION_FAILED",
            Self::SignatureMissing => "PLUGIN_SIGNATURE_MISSING",
            Self::SignatureInvalid => "PLUGIN_SIGNATURE_INVALID",
            Self::SigningKeyUnknown => "PLUGIN_SIGNING_KEY_UNKNOWN",
            Self::SigningKeyRevoked => "PLUGIN_SIGNING_KEY_REVOKED",
            Self::PublisherMismatch => "PLUGIN_PUBLISHER_MISMATCH",
            Self::PluginRevoked => "PLUGIN_REVOKED",
            Self::PluginVersionRevoked => "PLUGIN_VERSION_REVOKED",
            Self::UpdatePermissionReviewRequired => "PLUGIN_UPDATE_PERMISSION_REVIEW_REQUIRED",
            Self::PlanStale => "PLUGIN_PLAN_STALE",
            Self::MarketplaceUnavailable => "PLUGIN_MARKETPLACE_UNAVAILABLE",
            Self::HashMismatch => "PLUGIN_HASH_MISMATCH",
            Self::Storage => "STORAGE_ERROR",
        }
    }
}

impl From<PluginError> for AppError {
    fn from(value: PluginError) -> Self {
        match value {
            PluginError::InvalidPackage => AppError::PluginPackageInvalid,
            PluginError::InvalidManifest(_) => AppError::PluginManifestInvalid,
            PluginError::IncompatibleVersion => AppError::PluginVersionIncompatible,
            PluginError::AlreadyInstalled => AppError::PluginAlreadyInstalled,
            PluginError::NotFound => AppError::PluginNotFound,
            PluginError::Disabled => AppError::PluginDisabled,
            PluginError::PermissionDenied(_) => AppError::PluginPermissionDenied,
            PluginError::WorkflowNotFound(_) => AppError::PluginWorkflowNotFound,
            PluginError::WorkflowValidation(_) => AppError::PluginWorkflowInvalid,
            PluginError::ActionNotFound(_) => AppError::PluginActionNotFound,
            PluginError::PolicyDenied => AppError::PluginPolicyDenied,
            PluginError::Cancelled => AppError::PluginCancelled,
            PluginError::RemoteExecution(_) => AppError::PluginExecutionFailed,
            PluginError::SignatureMissing => AppError::PluginSignatureMissing,
            PluginError::SignatureInvalid => AppError::PluginSignatureInvalid,
            PluginError::SigningKeyUnknown => AppError::PluginSigningKeyUnknown,
            PluginError::SigningKeyRevoked => AppError::PluginSigningKeyRevoked,
            PluginError::PublisherMismatch => AppError::PluginPublisherMismatch,
            PluginError::PluginRevoked => AppError::PluginRevoked,
            PluginError::PluginVersionRevoked => AppError::PluginVersionRevoked,
            PluginError::UpdatePermissionReviewRequired => {
                AppError::PluginUpdatePermissionReviewRequired
            }
            PluginError::PlanStale => AppError::PluginPlanStale,
            PluginError::MarketplaceUnavailable => AppError::PluginMarketplaceUnavailable,
            PluginError::HashMismatch => AppError::PluginHashMismatch,
            PluginError::Storage => AppError::Storage,
        }
    }
}

pub type PluginResult<T> = Result<T, PluginError>;
