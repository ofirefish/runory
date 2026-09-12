use serde::{Deserialize, Serialize};

/// Capability bits negotiated with a BastionProvider. UI / Runtime must branch
/// on these flags instead of `if provider == "jumpserver"`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BastionCapabilities(u64);

impl BastionCapabilities {
    pub const SSH: Self = Self(1 << 0);
    pub const SFTP: Self = Self(1 << 1);
    pub const SCP: Self = Self(1 << 2);
    pub const PORT_FORWARD: Self = Self(1 << 3);
    pub const ASSET_DISCOVERY: Self = Self(1 << 4);
    pub const ACCOUNT_DISCOVERY: Self = Self(1 << 5);
    pub const PASSWORD_AUTH: Self = Self(1 << 6);
    pub const KEY_AUTH: Self = Self(1 << 7);
    pub const TOKEN_AUTH: Self = Self(1 << 8);
    pub const MFA: Self = Self(1 << 9);
    pub const BROWSER_SSO: Self = Self(1 << 10);
    pub const SESSION_RECORDING: Self = Self(1 << 11);
    pub const COMMAND_AUDIT: Self = Self(1 << 12);
    pub const FILE_AUDIT: Self = Self(1 << 13);
    pub const DYNAMIC_CREDENTIAL: Self = Self(1 << 14);
    pub const TEMP_ACCESS: Self = Self(1 << 15);
    pub const SESSION_RESUME: Self = Self(1 << 16);
    pub const AGENT_FORWARDING: Self = Self(1 << 17);
    pub const EXTERNAL_HELPER: Self = Self(1 << 18);
    pub const MULTI_HOP: Self = Self(1 << 19);
    pub const SSH_CERTIFICATE: Self = Self(1 << 20);
    pub const SHORT_LIVED_CREDENTIALS: Self = Self(1 << 21);
    pub const MANAGED_CREDENTIALS: Self = Self(1 << 22);
    pub const ACCESS_REQUEST: Self = Self(1 << 23);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn from_bits_truncate(bits: u64) -> Self {
        Self(bits)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl std::ops::BitOr for BastionCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for BastionCapabilities {
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

/// Soft concurrency limits declared by a provider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProviderLimits {
    pub max_auth_sessions: Option<u32>,
    pub max_target_sessions: Option<u32>,
}

/// Shared timeout budget; providers may tighten individual stages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BastionTimeouts {
    pub probe: std::time::Duration,
    pub auth: std::time::Duration,
    pub discovery: std::time::Duration,
    pub connect: std::time::Duration,
    pub io_idle: Option<std::time::Duration>,
}

impl Default for BastionTimeouts {
    fn default() -> Self {
        Self {
            probe: std::time::Duration::from_secs(10),
            auth: std::time::Duration::from_secs(60),
            discovery: std::time::Duration::from_secs(30),
            connect: std::time::Duration::from_secs(30),
            io_idle: Some(std::time::Duration::from_secs(300)),
        }
    }
}
