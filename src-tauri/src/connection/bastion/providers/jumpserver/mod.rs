//! JumpServer BastionProvider.
//!
//! Control plane: Access Key (HMAC signature) or username/password (+ MFA) Bearer.
//! Data plane: Connection Token → KoKo SSH. Upper layers never see JumpServer-specific types.

mod api;
mod auth;
mod koko;
mod provider;
mod signer;

pub use provider::JumpServerProvider;
pub use provider::attach_fingerprint;
pub use auth::{normalize_koko_host, normalize_koko_ssh_port, parse_api_base_url};
#[allow(unused_imports)]
pub use signer::JumpServerSigner;
