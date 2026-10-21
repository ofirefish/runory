//! Plugin package signing (Plugin Platform PP4).
//!
//! Canonical digest (API 1):
//! For each package entry except `META-INF/signature.json`, sorted by path:
//!   u64le(path_len) || path_utf8 || u64le(file_len) || file_bytes
//! Digest = SHA-256 over that concatenation.
//! Signature = Ed25519 over the raw 32-byte digest.

pub mod digest;
pub mod keys;
pub mod model;
pub mod official_keys;
pub mod verify;

pub use digest::{canonical_digest_from_zip, canonical_digest_hex, read_zip_entries};
pub use keys::{generate_keypair, load_private_key, save_keypair, PublisherKeyPair};
pub use model::{SignatureMetadata, TrustStatus, SIGNATURE_ENTRY};
pub use official_keys::seed_official_publisher_keys;
pub use verify::{sign_package, verify_package, VerifyOptions, VerifyResult};
