use serde::Serialize;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
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
    #[error("cloud access policy denied the operation")]
    CloudPolicyDenied,
    #[error("cloud access policy could not be evaluated")]
    CloudPolicyUnavailable,
    #[error("model provider configuration is invalid")]
    ModelInvalid,
    #[error("model provider is unavailable")]
    ModelUnavailable,
    #[error("model provider authentication failed")]
    ModelAuthFailed,
    #[error("model provider rate limit exceeded")]
    ModelRateLimited,
    #[error("model provider returned an invalid response")]
    ModelResponseInvalid,
    #[error("storage operation failed")]
    Storage,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload<'a> {
    code: &'a str,
}

impl AppError {
    pub const fn code(&self) -> &'static str {
        match self {
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
            Self::VaultLocked => "VAULT_LOCKED",
            Self::VaultInvalid => "VAULT_INVALID",
            Self::PlatformKeyStoreUnavailable => "PLATFORM_KEY_STORE_UNAVAILABLE",
            Self::CredentialNotFound => "CREDENTIAL_NOT_FOUND",
            Self::CloudInvalid => "CLOUD_INVALID",
            Self::CloudCrypto => "CLOUD_CRYPTO_ERROR",
            Self::CloudDecrypt => "CLOUD_DECRYPT_FAILED",
            Self::CloudImportNotFound => "CLOUD_IMPORT_NOT_FOUND",
            Self::CloudPolicyDenied => "CLOUD_POLICY_DENIED",
            Self::CloudPolicyUnavailable => "CLOUD_POLICY_UNAVAILABLE",
            Self::ModelInvalid => "MODEL_INVALID",
            Self::ModelUnavailable => "MODEL_UNAVAILABLE",
            Self::ModelAuthFailed => "MODEL_AUTH_FAILED",
            Self::ModelRateLimited => "MODEL_RATE_LIMITED",
            Self::ModelResponseInvalid => "MODEL_RESPONSE_INVALID",
            Self::Storage => "STORAGE_ERROR",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ErrorPayload { code: self.code() }.serialize(serializer)
    }
}
