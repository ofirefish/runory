use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("jump host configuration is invalid")]
    InvalidJumpHost,
    #[error("profile is referenced as a jump host")]
    ProfileInUseAsJumpHost,
    #[error("jump host preparation was not found or expired")]
    JumpPreparationExpired,
    #[error("jump host denied TCP forwarding")]
    JumpForwardingDenied,
    #[error("target is unreachable from the jump host")]
    JumpTargetUnreachable,
    #[error("bastion connection route is not available yet")]
    BastionUnavailable,
    /// TCP reached the configured KoKo port, but it did not speak SSH (wrong port / not exposed).
    #[error("bastion KoKo SSH gateway at {endpoint} is not reachable as SSH")]
    BastionKokoUnreachable { endpoint: String },
    /// TCP could not reach the bastion SSH gateway (refused, unreachable, or timed out).
    #[error("bastion SSH gateway at {endpoint} is unreachable")]
    BastionGatewayUnreachable { endpoint: String },
    #[error("bastion provider was not found")]
    BastionProviderNotFound,
    #[error("bastion authentication failed")]
    BastionAuthFailed,
    #[error("bastion authentication requires user input")]
    BastionAwaitingUser,
    #[error("bastion permission denied")]
    BastionPermissionDenied,
    #[error("external helper binary is missing")]
    HelperMissing,
    #[error("external helper version is incompatible")]
    HelperVersionMismatch,
    #[error("external helper proxy failed")]
    HelperProxyFailed,
    #[error("background SSH authentication is required")]
    TunnelConnectionRequired,
    #[error("invalid tunnel rule")]
    TunnelInvalid,
    #[error("tunnel not found")]
    TunnelNotFound,
    #[error("stop the tunnel before changing it")]
    TunnelRunning,
    #[error("tunnel is not running")]
    TunnelStopped,
    #[error("local port is already in use")]
    TunnelPortInUse,
    #[error("local listener could not be bound")]
    TunnelBindFailed,
    #[error("SSH forwarding is prohibited")]
    TunnelDenied,
    #[error("forwarding destination could not be reached")]
    TunnelTargetFailed,
    #[error("tunnel session does not match the saved profile")]
    TunnelSessionMismatch,
    #[error("tunnel resource limit reached")]
    TunnelLimit,
    #[error("invalid request")]
    InvalidProfile,
    #[error("invalid group")]
    InvalidGroup,
    #[error("profile not found")]
    ProfileNotFound,
    #[error("group not found")]
    GroupNotFound,
    #[error("connection refused")]
    ConnectionRefused,
    #[error("connection timed out")]
    ConnectionTimeout,
    #[error("authentication failed")]
    AuthFailed,
    #[error("private key is invalid or cannot be read")]
    PrivateKeyInvalid,
    #[error("private key passphrase is required")]
    PassphraseRequired,
    #[error("host key is unknown")]
    HostKeyUnknown,
    #[error("host key changed")]
    HostKeyChanged,
    #[error("host verification attempt is invalid or expired")]
    HostVerificationExpired,
    #[error("session not found")]
    SessionNotFound,
    #[error("connection lost")]
    ConnectionLost,
    #[error("SFTP channel is not open")]
    SftpNotOpen,
    #[error("SFTP operation failed")]
    SftpOperationFailed,
    #[error("invalid remote path")]
    SftpPathInvalid,
    #[error("remote path is not a directory")]
    SftpNotDirectory,
    #[error("remote path is not a file")]
    SftpNotFile,
    #[error("remote path was not found")]
    SftpNotFound,
    #[error("remote permission denied")]
    SftpPermissionDenied,
    #[error("remote destination already exists")]
    SftpAlreadyExists,
    #[error("recursive delete safety limit exceeded")]
    SftpDeleteLimitExceeded,
    #[error("remote image exceeds the preview safety limit")]
    SftpImageTooLarge,
    #[error("remote image format is unsupported")]
    SftpImageUnsupported,
    #[error("remote image data is invalid")]
    SftpImageInvalid,
    #[error("remote text file exceeds the preview safety limit")]
    SftpTextTooLarge,
    #[error("remote file is not text")]
    SftpTextUnsupported,
    #[error("remote text encoding is unsupported")]
    SftpTextEncodingUnsupported,
    #[error("local file selection is invalid or unavailable")]
    LocalFileInvalid,
    #[error("transfer job was not found")]
    TransferNotFound,
    #[error("transfer job state does not allow this operation")]
    TransferStateInvalid,
    #[error("transfer was cancelled")]
    TransferCancelled,
    #[error("transfer operation failed")]
    TransferFailed,
    #[error("remote command failed")]
    ExecFailed,
    #[error("remote command timed out")]
    ExecTimedOut,
    #[error("remote command output exceeded the safety limit")]
    ExecOutputLimit,
    #[error("remote platform or capability is unsupported")]
    UnsupportedRemote,
    #[error("invalid infrastructure operation")]
    InvalidOperation,
    #[error("multi-server target selection is invalid")]
    AgentFleetTargetInvalid,
    #[error("multi-server target does not match the live SSH session")]
    AgentFleetTargetMismatch,
    #[error("multi-server target limit exceeded")]
    AgentFleetTargetLimit,
    #[error("multi-server plan is invalid")]
    AgentFleetPlanInvalid,
    #[error("multi-server plan contains a dependency cycle")]
    AgentFleetPlanCycle,
    #[error("production parallel fleet execution is blocked")]
    AgentFleetProductionParallelBlocked,
    #[error("credential vault is locked")]
    VaultLocked,
    #[error("credential vault password is invalid")]
    VaultInvalid,
    #[error("platform secure storage is unavailable")]
    PlatformKeyStoreUnavailable,
    #[error("credential was not found")]
    CredentialNotFound,
    #[error("cloud sync request is invalid")]
    CloudInvalid,
    #[error("cloud sync encryption failed")]
    CloudCrypto,
    #[error("cloud sync payload could not be decrypted")]
    CloudDecrypt,
    #[error("cloud sync import was not found")]
    CloudImportNotFound,
    #[error("cloud sync recovery passphrase is required")]
    CloudRecoveryRequired,
    #[error("cloud access policy denied the operation")]
    CloudPolicyDenied,
    #[error("cloud access policy could not be evaluated")]
    CloudPolicyUnavailable,
    #[error("model provider configuration is invalid")]
    ModelInvalid,
    #[error("model provider is unavailable")]
    ModelUnavailable,
    #[error("model provider request timed out")]
    ModelTimeout,
    #[error("model provider authentication failed")]
    ModelAuthFailed,
    #[error("managed model credits are insufficient")]
    ModelCreditInsufficient,
    #[error("model provider rate limit exceeded")]
    ModelRateLimited,
    #[error("model provider returned an invalid response")]
    ModelResponseInvalid,
    #[error("model provider returned an empty response")]
    ModelResponseEmpty,
    #[error("model provider returned invalid or truncated JSON")]
    ModelJsonInvalid,
    #[error("model provider returned an invalid agent decision")]
    ModelDecisionInvalid,
    #[error("model provider returned an invalid command proposal")]
    ModelCommandInvalid,
    #[error("model provider omitted valid usage information")]
    ModelUsageInvalid,
    #[error("model provider returned an invalid response envelope")]
    ModelProviderResponseInvalid,
    #[error("model provider OAuth is unsupported on this platform")]
    ModelOauthUnsupported,
    #[error("model provider OAuth was cancelled or timed out")]
    ModelOauthCancelled,
    #[error("desktop updater is not configured")]
    UpdateNotConfigured,
    #[error("desktop update check failed")]
    UpdateCheckFailed,
    #[error("desktop update is not ready")]
    UpdateNotReady,
    #[error("desktop update download failed")]
    UpdateDownloadFailed,
    #[error("active sessions must be closed before installing an update")]
    UpdateBusy,
    #[error("desktop update installation failed")]
    UpdateInstallFailed,
    #[error("storage operation failed")]
    Storage,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload<'a> {
    code: &'a str,
    /// Safe bastion/SSH gateway endpoint (`host:port`) for UI interpolation. Never secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<&'a str>,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidJumpHost => "INVALID_JUMP_HOST",
            Self::ProfileInUseAsJumpHost => "PROFILE_IN_USE_AS_JUMP_HOST",
            Self::JumpPreparationExpired => "JUMP_PREPARATION_EXPIRED",
            Self::JumpForwardingDenied => "JUMP_FORWARDING_DENIED",
            Self::JumpTargetUnreachable => "JUMP_TARGET_UNREACHABLE",
            Self::BastionUnavailable => "BASTION_UNAVAILABLE",
            Self::BastionKokoUnreachable { .. } => "BASTION_KOKO_UNREACHABLE",
            Self::BastionGatewayUnreachable { .. } => "BASTION_GATEWAY_UNREACHABLE",
            Self::BastionProviderNotFound => "BASTION_PROVIDER_NOT_FOUND",
            Self::BastionAuthFailed => "BASTION_AUTH_FAILED",
            Self::BastionAwaitingUser => "BASTION_AWAITING_USER",
            Self::BastionPermissionDenied => "BASTION_PERMISSION_DENIED",
            Self::HelperMissing => "HELPER_MISSING",
            Self::HelperVersionMismatch => "HELPER_VERSION_MISMATCH",
            Self::HelperProxyFailed => "HELPER_PROXY_FAILED",
            Self::TunnelConnectionRequired => "TUNNEL_CONNECTION_REQUIRED",
            Self::TunnelInvalid => "TUNNEL_INVALID",
            Self::TunnelNotFound => "TUNNEL_NOT_FOUND",
            Self::TunnelRunning => "TUNNEL_RUNNING",
            Self::TunnelStopped => "TUNNEL_STOPPED",
            Self::TunnelPortInUse => "TUNNEL_PORT_IN_USE",
            Self::TunnelBindFailed => "TUNNEL_BIND_FAILED",
            Self::TunnelDenied => "TUNNEL_DENIED",
            Self::TunnelTargetFailed => "TUNNEL_TARGET_FAILED",
            Self::TunnelSessionMismatch => "TUNNEL_SESSION_MISMATCH",
            Self::TunnelLimit => "TUNNEL_LIMIT",
            Self::InvalidProfile => "INVALID_PROFILE",
            Self::InvalidGroup => "INVALID_GROUP",
            Self::ProfileNotFound => "PROFILE_NOT_FOUND",
            Self::GroupNotFound => "GROUP_NOT_FOUND",
            Self::ConnectionRefused => "CONNECTION_REFUSED",
            Self::ConnectionTimeout => "CONNECTION_TIMEOUT",
            Self::AuthFailed => "AUTH_FAILED",
            Self::PrivateKeyInvalid => "PRIVATE_KEY_INVALID",
            Self::PassphraseRequired => "PASSPHRASE_REQUIRED",
            Self::HostKeyUnknown => "HOST_KEY_UNKNOWN",
            Self::HostKeyChanged => "HOST_KEY_CHANGED",
            Self::HostVerificationExpired => "HOST_VERIFICATION_EXPIRED",
            Self::SessionNotFound => "SESSION_NOT_FOUND",
            Self::ConnectionLost => "CONNECTION_LOST",
            Self::SftpNotOpen => "SFTP_NOT_OPEN",
            Self::SftpOperationFailed => "SFTP_OPERATION_FAILED",
            Self::SftpPathInvalid => "SFTP_PATH_INVALID",
            Self::SftpNotDirectory => "SFTP_NOT_DIRECTORY",
            Self::SftpNotFile => "SFTP_NOT_FILE",
            Self::SftpNotFound => "SFTP_NOT_FOUND",
            Self::SftpPermissionDenied => "SFTP_PERMISSION_DENIED",
            Self::SftpAlreadyExists => "SFTP_ALREADY_EXISTS",
            Self::SftpDeleteLimitExceeded => "SFTP_DELETE_LIMIT_EXCEEDED",
            Self::SftpImageTooLarge => "SFTP_IMAGE_TOO_LARGE",
            Self::SftpImageUnsupported => "SFTP_IMAGE_UNSUPPORTED",
            Self::SftpImageInvalid => "SFTP_IMAGE_INVALID",
            Self::SftpTextTooLarge => "SFTP_TEXT_TOO_LARGE",
            Self::SftpTextUnsupported => "SFTP_TEXT_UNSUPPORTED",
            Self::SftpTextEncodingUnsupported => "SFTP_TEXT_ENCODING_UNSUPPORTED",
            Self::LocalFileInvalid => "LOCAL_FILE_INVALID",
            Self::TransferNotFound => "TRANSFER_NOT_FOUND",
            Self::TransferStateInvalid => "TRANSFER_STATE_INVALID",
            Self::TransferCancelled => "TRANSFER_CANCELLED",
            Self::TransferFailed => "TRANSFER_FAILED",
            Self::ExecFailed => "EXEC_FAILED",
            Self::ExecTimedOut => "EXEC_TIMED_OUT",
            Self::ExecOutputLimit => "EXEC_OUTPUT_LIMIT",
            Self::UnsupportedRemote => "UNSUPPORTED_REMOTE",
            Self::InvalidOperation => "INVALID_OPERATION",
            Self::AgentFleetTargetInvalid => "AGENT_FLEET_TARGET_INVALID",
            Self::AgentFleetTargetMismatch => "AGENT_FLEET_TARGET_MISMATCH",
            Self::AgentFleetTargetLimit => "AGENT_FLEET_TARGET_LIMIT",
            Self::AgentFleetPlanInvalid => "AGENT_FLEET_PLAN_INVALID",
            Self::AgentFleetPlanCycle => "AGENT_FLEET_PLAN_CYCLE",
            Self::AgentFleetProductionParallelBlocked => "AGENT_FLEET_PRODUCTION_PARALLEL_BLOCKED",
            Self::VaultLocked => "VAULT_LOCKED",
            Self::VaultInvalid => "VAULT_INVALID",
            Self::PlatformKeyStoreUnavailable => "PLATFORM_KEY_STORE_UNAVAILABLE",
            Self::CredentialNotFound => "CREDENTIAL_NOT_FOUND",
            Self::CloudInvalid => "CLOUD_INVALID",
            Self::CloudCrypto => "CLOUD_CRYPTO_ERROR",
            Self::CloudDecrypt => "CLOUD_DECRYPT_FAILED",
            Self::CloudImportNotFound => "CLOUD_IMPORT_NOT_FOUND",
            Self::CloudRecoveryRequired => "CLOUD_RECOVERY_REQUIRED",
            Self::CloudPolicyDenied => "CLOUD_POLICY_DENIED",
            Self::CloudPolicyUnavailable => "CLOUD_POLICY_UNAVAILABLE",
            Self::ModelInvalid => "MODEL_INVALID",
            Self::ModelUnavailable => "MODEL_UNAVAILABLE",
            Self::ModelTimeout => "MODEL_TIMEOUT",
            Self::ModelAuthFailed => "MODEL_AUTH_FAILED",
            Self::ModelCreditInsufficient => "MODEL_CREDIT_INSUFFICIENT",
            Self::ModelRateLimited => "MODEL_RATE_LIMITED",
            Self::ModelResponseInvalid => "MODEL_RESPONSE_INVALID",
            Self::ModelResponseEmpty => "MODEL_RESPONSE_EMPTY",
            Self::ModelJsonInvalid => "MODEL_JSON_INVALID",
            Self::ModelDecisionInvalid => "MODEL_DECISION_INVALID",
            Self::ModelCommandInvalid => "MODEL_COMMAND_INVALID",
            Self::ModelUsageInvalid => "MODEL_USAGE_INVALID",
            Self::ModelProviderResponseInvalid => "MODEL_PROVIDER_RESPONSE_INVALID",
            Self::ModelOauthUnsupported => "MODEL_OAUTH_UNSUPPORTED",
            Self::ModelOauthCancelled => "MODEL_OAUTH_CANCELLED",
            Self::UpdateNotConfigured => "UPDATE_NOT_CONFIGURED",
            Self::UpdateCheckFailed => "UPDATE_CHECK_FAILED",
            Self::UpdateNotReady => "UPDATE_NOT_READY",
            Self::UpdateDownloadFailed => "UPDATE_DOWNLOAD_FAILED",
            Self::UpdateBusy => "UPDATE_BUSY",
            Self::UpdateInstallFailed => "UPDATE_INSTALL_FAILED",
            Self::Storage => "STORAGE_ERROR",
        }
    }

    /// Content-free gateway endpoint for bastion SSH diagnostics (`host:port`).
    pub fn endpoint(&self) -> Option<&str> {
        match self {
            Self::BastionKokoUnreachable { endpoint }
            | Self::BastionGatewayUnreachable { endpoint } => Some(endpoint.as_str()),
            _ => None,
        }
    }
}

/// Format `host:port`, wrapping IPv6 hosts in brackets.
pub fn format_ssh_endpoint(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ErrorPayload {
            code: self.code(),
            endpoint: self.endpoint(),
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_ssh_endpoint_wraps_ipv6() {
        assert_eq!(format_ssh_endpoint("127.0.0.1", 2222), "127.0.0.1:2222");
        assert_eq!(format_ssh_endpoint("::1", 2222), "[::1]:2222");
        assert_eq!(format_ssh_endpoint("[::1]", 2222), "[::1]:2222");
    }

    #[test]
    fn bastion_gateway_errors_serialize_endpoint() {
        let error = AppError::BastionGatewayUnreachable {
            endpoint: "localhost:2222".into(),
        };
        let value = serde_json::to_value(&error).expect("serialize");
        assert_eq!(value["code"], "BASTION_GATEWAY_UNREACHABLE");
        assert_eq!(value["endpoint"], "localhost:2222");
    }

    #[test]
    fn ordinary_errors_omit_endpoint_field() {
        let value = serde_json::to_value(&AppError::ConnectionRefused).expect("serialize");
        assert_eq!(value["code"], "CONNECTION_REFUSED");
        assert!(value.get("endpoint").is_none());
    }
}
