use serde::{Deserialize, Serialize};

pub const SIGNATURE_ENTRY: &str = "META-INF/signature.json";
pub const SIGNATURE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureMetadata {
    pub format_version: u32,
    pub algorithm: String,
    pub publisher: String,
    pub key_id: String,
    pub package_sha256: String,
    /// Base64-encoded Ed25519 signature over the canonical digest bytes.
    pub signature: String,
}

impl SignatureMetadata {
    pub fn ed25519(
        publisher: impl Into<String>,
        key_id: impl Into<String>,
        package_sha256: impl Into<String>,
        signature_b64: impl Into<String>,
    ) -> Self {
        Self {
            format_version: SIGNATURE_FORMAT_VERSION,
            algorithm: "ed25519".into(),
            publisher: publisher.into(),
            key_id: key_id.into(),
            package_sha256: package_sha256.into(),
            signature: signature_b64.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrustStatus {
    Official,
    VerifiedPublisher,
    CommunitySigned,
    Unsigned,
    InvalidSignature,
    RevokedKey,
    UnknownPublisher,
}

impl TrustStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Official => "Official",
            Self::VerifiedPublisher => "Verified Publisher",
            Self::CommunitySigned => "Community Signed",
            Self::Unsigned => "Unsigned",
            Self::InvalidSignature => "Invalid Signature",
            Self::RevokedKey => "Revoked Key",
            Self::UnknownPublisher => "Unknown Publisher",
        }
    }

    pub fn allows_install(self, developer_mode: bool) -> bool {
        match self {
            Self::Official | Self::VerifiedPublisher | Self::CommunitySigned => true,
            Self::Unsigned => developer_mode,
            Self::InvalidSignature | Self::RevokedKey | Self::UnknownPublisher => false,
        }
    }
}
