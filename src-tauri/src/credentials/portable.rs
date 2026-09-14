use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult, CredentialKind};

use super::CredentialVault;

const VAULT_VERSION: u8 = 1;
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const VAULT_AAD: &[u8] = b"runory-mobile-credential-vault-v1";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultEnvelope {
    version: u8,
    salt: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

struct UnlockedVault {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; SALT_LENGTH],
    values: BTreeMap<String, Vec<u8>>,
}

impl Drop for UnlockedVault {
    fn drop(&mut self) {
        for value in self.values.values_mut() {
            value.zeroize();
        }
    }
}

/// Pure-Rust encrypted vault used on mobile so Android/iOS builds do not depend
/// on a host shell or a cross-compiled libsodium artifact.
pub struct PortableCredentialVault {
    path: PathBuf,
    unlocked: Mutex<Option<UnlockedVault>>,
}

impl PortableCredentialVault {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            unlocked: Mutex::new(None),
        }
    }

    fn record_key(profile_id: Uuid, kind: CredentialKind) -> String {
        let prefix = match kind {
            CredentialKind::Password => "password",
            CredentialKind::KeyPassphrase => "key-passphrase",
            CredentialKind::McpToken => "mcp-token",
            CredentialKind::LlmApiKey => "llm-api-key",
            CredentialKind::BastionToken => "bastion-token",
            CredentialKind::BastionAccessKey => "bastion-access-key",
            CredentialKind::BastionTargetPassword => "bastion-target-password",
        };
        format!("{prefix}:{profile_id}")
    }

    fn private_key_record(key_id: Uuid) -> String {
        format!("private-key:{key_id}")
    }

    fn with_unlocked<T>(
        &self,
        operation: impl FnOnce(&UnlockedVault) -> AppResult<T>,
    ) -> AppResult<T> {
        let guard = self.unlocked.lock().map_err(|_| AppError::VaultInvalid)?;
        operation(guard.as_ref().ok_or(AppError::VaultLocked)?)
    }

    fn with_unlocked_mut<T>(
        &self,
        operation: impl FnOnce(&mut UnlockedVault) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut guard = self.unlocked.lock().map_err(|_| AppError::VaultInvalid)?;
        operation(guard.as_mut().ok_or(AppError::VaultLocked)?)
    }

    fn derive_key(password: &str, salt: &[u8; SALT_LENGTH]) -> AppResult<Zeroizing<[u8; 32]>> {
        let mut key = Zeroizing::new([0_u8; 32]);
        Argon2::default()
            .hash_password_into(password.as_bytes(), salt, key.as_mut())
            .map_err(|_| AppError::VaultInvalid)?;
        Ok(key)
    }

    fn persist(path: &Path, state: &UnlockedVault) -> AppResult<()> {
        let cipher =
            Aes256Gcm::new_from_slice(state.key.as_ref()).map_err(|_| AppError::VaultInvalid)?;
        let mut nonce = [0_u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut nonce);
        let plaintext =
            Zeroizing::new(serde_json::to_vec(&state.values).map_err(|_| AppError::VaultInvalid)?);
        let nonce_value = Nonce::from(nonce);
        let ciphertext = cipher
            .encrypt(
                &nonce_value,
                Payload {
                    msg: plaintext.as_slice(),
                    aad: VAULT_AAD,
                },
            )
            .map_err(|_| AppError::VaultInvalid)?;
        let envelope = VaultEnvelope {
            version: VAULT_VERSION,
            salt: state.salt.to_vec(),
            nonce: nonce.to_vec(),
            ciphertext,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|_| AppError::Storage)?;
        let parent = path.parent().ok_or(AppError::Storage)?;
        std::fs::create_dir_all(parent).map_err(|_| AppError::Storage)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| AppError::Storage)?;
        temporary.write_all(&bytes).map_err(|_| AppError::Storage)?;
        temporary.flush().map_err(|_| AppError::Storage)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| AppError::Storage)?;
        temporary.persist(path).map_err(|_| AppError::Storage)?;
        Ok(())
    }

    fn load(path: &Path, password: &str) -> AppResult<UnlockedVault> {
        let bytes = std::fs::read(path).map_err(|_| AppError::Storage)?;
        let envelope: VaultEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| AppError::VaultInvalid)?;
        if envelope.version != VAULT_VERSION
            || envelope.salt.len() != SALT_LENGTH
            || envelope.nonce.len() != NONCE_LENGTH
        {
            return Err(AppError::VaultInvalid);
        }
        let salt: [u8; SALT_LENGTH] = envelope
            .salt
            .try_into()
            .map_err(|_| AppError::VaultInvalid)?;
        let key = Self::derive_key(password, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| AppError::VaultInvalid)?;
        let nonce: [u8; NONCE_LENGTH] = envelope
            .nonce
            .try_into()
            .map_err(|_| AppError::VaultInvalid)?;
        let nonce = Nonce::from(nonce);
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce,
                    Payload {
                        msg: &envelope.ciphertext,
                        aad: VAULT_AAD,
                    },
                )
                .map_err(|_| AppError::VaultInvalid)?,
        );
        let values =
            serde_json::from_slice(plaintext.as_slice()).map_err(|_| AppError::VaultInvalid)?;
        Ok(UnlockedVault { key, salt, values })
    }
}

impl CredentialVault for PortableCredentialVault {
    fn is_initialized(&self) -> bool {
        self.path.is_file()
    }

    fn is_unlocked(&self) -> bool {
        self.unlocked.lock().is_ok_and(|state| state.is_some())
    }

    fn unlock(&self, mut master_password: Zeroizing<String>) -> AppResult<()> {
        let state = if self.path.is_file() {
            Self::load(&self.path, master_password.as_str())?
        } else {
            let mut salt = [0_u8; SALT_LENGTH];
            OsRng.fill_bytes(&mut salt);
            let state = UnlockedVault {
                key: Self::derive_key(master_password.as_str(), &salt)?,
                salt,
                values: BTreeMap::new(),
            };
            Self::persist(&self.path, &state)?;
            state
        };
        master_password.zeroize();
        *self.unlocked.lock().map_err(|_| AppError::VaultInvalid)? = Some(state);
        Ok(())
    }

    fn lock(&self) -> AppResult<()> {
        self.unlocked
            .lock()
            .map_err(|_| AppError::VaultInvalid)?
            .take();
        Ok(())
    }

    fn contains(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<bool> {
        self.with_unlocked(|state| {
            Ok(state
                .values
                .contains_key(&Self::record_key(profile_id, kind)))
        })
    }

    fn get(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<Zeroizing<String>> {
        self.with_unlocked(|state| {
            let bytes = state
                .values
                .get(&Self::record_key(profile_id, kind))
                .cloned()
                .ok_or(AppError::CredentialNotFound)?;
            String::from_utf8(bytes)
                .map(Zeroizing::new)
                .map_err(|_| AppError::VaultInvalid)
        })
    }

    fn put(
        &self,
        profile_id: Uuid,
        kind: CredentialKind,
        secret: Zeroizing<String>,
    ) -> AppResult<()> {
        self.with_unlocked_mut(|state| {
            state.values.insert(
                Self::record_key(profile_id, kind),
                secret.as_bytes().to_vec(),
            );
            Self::persist(&self.path, state)
        })
    }

    fn delete(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<()> {
        self.with_unlocked_mut(|state| {
            if let Some(mut removed) = state.values.remove(&Self::record_key(profile_id, kind)) {
                removed.zeroize();
            }
            Self::persist(&self.path, state)
        })
    }

    fn put_private_key(&self, key_id: Uuid, content: Zeroizing<Vec<u8>>) -> AppResult<()> {
        self.with_unlocked_mut(|state| {
            state
                .values
                .insert(Self::private_key_record(key_id), content.to_vec());
            Self::persist(&self.path, state)
        })
    }

    fn get_private_key(&self, key_id: Uuid) -> AppResult<Zeroizing<Vec<u8>>> {
        self.with_unlocked(|state| {
            state
                .values
                .get(&Self::private_key_record(key_id))
                .cloned()
                .map(Zeroizing::new)
                .ok_or(AppError::CredentialNotFound)
        })
    }

    fn delete_private_key(&self, key_id: Uuid) -> AppResult<()> {
        self.with_unlocked_mut(|state| {
            if let Some(mut removed) = state.values.remove(&Self::private_key_record(key_id)) {
                removed.zeroize();
            }
            Self::persist(&self.path, state)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_credentials_and_private_keys_with_authenticated_encryption() {
        let directory = tempfile::tempdir().expect("temporary vault directory");
        let path = directory.path().join("credentials.mobile.vault");
        let profile_id = Uuid::new_v4();
        let key_id = Uuid::new_v4();
        let vault = PortableCredentialVault::new(path.clone());

        vault
            .unlock(Zeroizing::new("correct master password".to_owned()))
            .expect("initialize");
        vault
            .put(
                profile_id,
                CredentialKind::Password,
                Zeroizing::new("server password".to_owned()),
            )
            .expect("put password");
        vault
            .put_private_key(key_id, Zeroizing::new(b"private key bytes".to_vec()))
            .expect("put key");
        vault.lock().expect("lock");

        let encrypted = std::fs::read(&path).expect("read encrypted vault");
        assert!(!encrypted
            .windows(b"server password".len())
            .any(|value| value == b"server password"));
        assert!(matches!(
            vault.unlock(Zeroizing::new("wrong password".to_owned())),
            Err(AppError::VaultInvalid)
        ));
        vault
            .unlock(Zeroizing::new("correct master password".to_owned()))
            .expect("reopen");
        assert_eq!(
            vault
                .get(profile_id, CredentialKind::Password)
                .expect("get password")
                .as_str(),
            "server password"
        );
        assert_eq!(
            vault.get_private_key(key_id).expect("get key").as_slice(),
            b"private key bytes"
        );
    }
}
