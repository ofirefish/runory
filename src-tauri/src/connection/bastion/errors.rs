use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum BastionError {
    #[error("network error talking to bastion")]
    Network,
    #[error("bastion authentication failed")]
    AuthenticationFailed,
    #[error("bastion authentication expired")]
    AuthenticationExpired,
    #[error("multi-factor authentication is required")]
    MfaRequired,
    #[error("bastion permission denied")]
    PermissionDenied,
    #[error("bastion asset was not found")]
    AssetNotFound,
    #[error("bastion account is not allowed")]
    AccountNotAllowed,
    #[error("requested protocol is unsupported")]
    ProtocolUnsupported,
    #[error("requested capability is unavailable")]
    CapabilityUnavailable,
    #[error("bastion session was rejected")]
    SessionRejected,
    #[error("bastion session expired")]
    SessionExpired,
    #[error("bastion provider is unavailable")]
    ProviderUnavailable,
    #[error("bastion provider version is unsupported")]
    ProviderVersionUnsupported,
    #[error("bastion provider protocol error")]
    ProviderProtocolError,
    #[error("bastion operation timed out")]
    Timeout,
    #[error("bastion operation was cancelled")]
    Cancelled,
    #[error("external helper binary is missing")]
    HelperMissing,
    #[error("external helper version is incompatible")]
    HelperVersionMismatch,
    #[error("external helper failed to start proxy")]
    HelperProxyFailed,
    #[error("internal bastion error")]
    Internal,
}

impl BastionError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Network => "BASTION_NETWORK",
            Self::AuthenticationFailed => "BASTION_AUTH_FAILED",
            Self::AuthenticationExpired => "BASTION_AUTH_EXPIRED",
            Self::MfaRequired => "BASTION_MFA_REQUIRED",
            Self::PermissionDenied => "BASTION_PERMISSION_DENIED",
            Self::AssetNotFound => "BASTION_ASSET_NOT_FOUND",
            Self::AccountNotAllowed => "BASTION_ACCOUNT_NOT_ALLOWED",
            Self::ProtocolUnsupported => "BASTION_PROTOCOL_UNSUPPORTED",
            Self::CapabilityUnavailable => "BASTION_CAPABILITY_UNAVAILABLE",
            Self::SessionRejected => "BASTION_SESSION_REJECTED",
            Self::SessionExpired => "BASTION_SESSION_EXPIRED",
            Self::ProviderUnavailable => "BASTION_PROVIDER_UNAVAILABLE",
            Self::ProviderVersionUnsupported => "BASTION_PROVIDER_VERSION_UNSUPPORTED",
            Self::ProviderProtocolError => "BASTION_PROVIDER_PROTOCOL_ERROR",
            Self::Timeout => "BASTION_TIMEOUT",
            Self::Cancelled => "BASTION_CANCELLED",
            Self::HelperMissing => "HELPER_MISSING",
            Self::HelperVersionMismatch => "HELPER_VERSION_MISMATCH",
            Self::HelperProxyFailed => "HELPER_PROXY_FAILED",
            Self::Internal => "BASTION_INTERNAL",
        }
    }

    /// Failures that are safe to retry without risking enterprise lockouts.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network | Self::Timeout | Self::ProviderUnavailable | Self::AuthenticationExpired
        )
    }
}

/// Optional vendor diagnostics that never contain secrets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BastionErrorDetail {
    pub provider_error_code: Option<String>,
    pub provider_message: Option<String>,
}
