use std::fs;
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use getrandom::fill;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::plugins::error::{PluginError, PluginResult};

const KEY_FORMAT: &str = "runory-plugin-key-v1";
const ENCRYPTED_MAGIC: &[u8] = b"RPK1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherKeyPair {
    pub format: String,
    pub key_id: String,
    pub publisher: String,
    pub algorithm: String,
    pub public_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
}

pub struct LoadedSigningKey {
    pub key_id: String,
    pub publisher: String,
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

pub fn generate_keypair(publisher: &str, key_id: &str) -> PluginResult<PublisherKeyPair> {
    validate_publisher_id(publisher)?;
    if key_id.trim().is_empty() || key_id.len() > 128 {
        return Err(PluginError::InvalidManifest("invalid key id".into()));
    }
    let mut seed = Zeroizing::new([0u8; 32]);
    fill(seed.as_mut()).map_err(|_| PluginError::Storage)?;
    let signing = SigningKey::from_bytes(&seed);
    let verifying = signing.verifying_key();
    Ok(PublisherKeyPair {
        format: KEY_FORMAT.into(),
        key_id: key_id.to_owned(),
        publisher: publisher.to_owned(),
        algorithm: "ed25519".into(),
        public_key: B64.encode(verifying.as_bytes()),
        private_key: Some(B64.encode(signing.to_bytes())),
    })
}

pub fn save_keypair(
    pair: &PublisherKeyPair,
    private_path: &Path,
    public_path: Option<&Path>,
    password: Option<&str>,
) -> PluginResult<()> {
    if let Some(parent) = private_path.parent() {
        fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
    }
    if let Some(password) = password {
        let private = pair
            .private_key
            .as_ref()
            .ok_or(PluginError::SigningKeyUnknown)?;
        let encrypted = encrypt_private_blob(pair, private, password)?;
        fs::write(private_path, encrypted).map_err(|_| PluginError::Storage)?;
    } else {
        let json = serde_json::to_vec_pretty(pair).map_err(|_| PluginError::Storage)?;
        fs::write(private_path, json).map_err(|_| PluginError::Storage)?;
    }
    if let Some(public_path) = public_path {
        let mut public = pair.clone();
        public.private_key = None;
        let json = serde_json::to_vec_pretty(&public).map_err(|_| PluginError::Storage)?;
        if let Some(parent) = public_path.parent() {
            fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
        }
        fs::write(public_path, json).map_err(|_| PluginError::Storage)?;
    }
    Ok(())
}

pub fn load_private_key(path: &Path, password: Option<&str>) -> PluginResult<LoadedSigningKey> {
    let bytes = fs::read(path).map_err(|_| PluginError::SigningKeyUnknown)?;
    let pair = if bytes.starts_with(ENCRYPTED_MAGIC) {
        let password = password.ok_or(PluginError::SigningKeyUnknown)?;
        decrypt_private_blob(&bytes, password)?
    } else {
        serde_json::from_slice(&bytes).map_err(|_| PluginError::SigningKeyUnknown)?
    };
    let private_b64 = pair
        .private_key
        .as_ref()
        .ok_or(PluginError::SigningKeyUnknown)?;
    let private_bytes = B64
        .decode(private_b64.as_bytes())
        .map_err(|_| PluginError::SigningKeyUnknown)?;
    let seed: [u8; 32] = private_bytes
        .as_slice()
        .try_into()
        .map_err(|_| PluginError::SigningKeyUnknown)?;
    let signing = SigningKey::from_bytes(&seed);
    Ok(LoadedSigningKey {
        key_id: pair.key_id,
        publisher: pair.publisher,
        verifying_key: signing.verifying_key(),
        signing_key: signing,
    })
}

pub fn load_public_key_bytes(public_key_b64: &str) -> PluginResult<[u8; 32]> {
    let bytes = B64
        .decode(public_key_b64.as_bytes())
        .map_err(|_| PluginError::SigningKeyUnknown)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| PluginError::SigningKeyUnknown)
}

pub fn verifying_key_from_b64(public_key_b64: &str) -> PluginResult<VerifyingKey> {
    let bytes = load_public_key_bytes(public_key_b64)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| PluginError::SigningKeyUnknown)
}

pub fn sign_digest(signing: &SigningKey, digest: &[u8; 32]) -> String {
    B64.encode(signing.sign(digest).to_bytes())
}

pub fn validate_publisher_id(id: &str) -> PluginResult<()> {
    let bytes = id.as_bytes();
    if !(2..=64).contains(&bytes.len()) {
        return Err(PluginError::InvalidManifest("invalid publisher id".into()));
    }
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return Err(PluginError::InvalidManifest("invalid publisher id".into()));
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(PluginError::InvalidManifest("invalid publisher id".into()));
    }
    Ok(())
}

fn encrypt_private_blob(
    pair: &PublisherKeyPair,
    private_b64: &str,
    password: &str,
) -> PluginResult<Vec<u8>> {
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    fill(&mut salt).map_err(|_| PluginError::Storage)?;
    fill(&mut nonce_bytes).map_err(|_| PluginError::Storage)?;
    let key = derive_key(password.as_bytes(), &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| PluginError::Storage)?;
    let plaintext = serde_json::to_vec(&serde_json::json!({
        "format": pair.format,
        "keyId": pair.key_id,
        "publisher": pair.publisher,
        "algorithm": pair.algorithm,
        "publicKey": pair.public_key,
        "privateKey": private_b64,
    }))
    .map_err(|_| PluginError::Storage)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|_| PluginError::Storage)?;
    let mut out = Vec::with_capacity(4 + 16 + 12 + ciphertext.len());
    out.extend_from_slice(ENCRYPTED_MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_private_blob(bytes: &[u8], password: &str) -> PluginResult<PublisherKeyPair> {
    if bytes.len() < 4 + 16 + 12 + 16 {
        return Err(PluginError::SigningKeyUnknown);
    }
    let salt = &bytes[4..20];
    let nonce_bytes = &bytes[20..32];
    let ciphertext = &bytes[32..];
    let key = derive_key(password.as_bytes(), salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| PluginError::SigningKeyUnknown)?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| PluginError::SigningKeyUnknown)?;
    serde_json::from_slice(&plaintext).map_err(|_| PluginError::SigningKeyUnknown)
}

fn derive_key(password: &[u8], salt: &[u8]) -> PluginResult<[u8; 32]> {
    let params = Params::new(19_456, 2, 1, Some(32)).map_err(|_| PluginError::Storage)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(password, salt, &mut key)
        .map_err(|_| PluginError::Storage)?;
    Ok(key)
}
