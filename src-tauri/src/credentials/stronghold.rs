use std::path::PathBuf;
use std::sync::Mutex;

use tauri_plugin_stronghold::kdf::KeyDerivation;
use tauri_plugin_stronghold::stronghold::Stronghold;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult, CredentialKind};

use super::CredentialVault;

const CLIENT_ID: &[u8] = b"runory-credentials";

pub struct StrongholdCredentialVault {
    snapshot_path: PathBuf,
    salt_path: PathBuf,
    stronghold: Mutex<Option<Stronghold>>,
}

impl StrongholdCredentialVault {
    pub fn new(snapshot_path: PathBuf, salt_path: PathBuf) -> Self {
        Self {
            snapshot_path,
            salt_path,
            stronghold: Mutex::new(None),
        }
    }

    fn with_stronghold<T>(
        &self,
        operation: impl FnOnce(&Stronghold) -> AppResult<T>,
    ) -> AppResult<T> {
        let guard = self.stronghold.lock().map_err(|_| AppError::VaultInvalid)?;
        operation(guard.as_ref().ok_or(AppError::VaultLocked)?)
    }

    fn record_key(profile_id: Uuid, kind: CredentialKind) -> Vec<u8> {
        let prefix = match kind {
            CredentialKind::Password => "password",
            CredentialKind::KeyPassphrase => "key-passphrase",
            CredentialKind::McpToken => "mcp-token",
            CredentialKind::LlmApiKey => "llm-api-key",
            CredentialKind::BastionToken => "bastion-token",
            CredentialKind::BastionAccessKey => "bastion-access-key",
            CredentialKind::BastionTargetPassword => "bastion-target-password",
        };
        format!("{prefix}:{profile_id}").into_bytes()
    }

    fn private_key_record(key_id: Uuid) -> Vec<u8> {
        format!("private-key:{key_id}").into_bytes()
    }
}

impl CredentialVault for StrongholdCredentialVault {
    fn is_initialized(&self) -> bool {
        self.snapshot_path.is_file()
    }

    fn is_unlocked(&self) -> bool {
        self.stronghold.lock().is_ok_and(|guard| guard.is_some())
    }

    fn unlock(&self, mut master_password: Zeroizing<String>) -> AppResult<()> {
        let existed = self.snapshot_path.is_file();
        let key = KeyDerivation::argon2(master_password.as_str(), &self.salt_path);
        master_password.zeroize();
        let stronghold =
            Stronghold::new(&self.snapshot_path, key).map_err(|_| AppError::VaultInvalid)?;
        if existed {
            stronghold
                .load_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
        } else {
            stronghold
                .create_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            stronghold.save().map_err(|_| AppError::Storage)?;
        }
        *self.stronghold.lock().map_err(|_| AppError::VaultInvalid)? = Some(stronghold);
        Ok(())
    }

    fn lock(&self) -> AppResult<()> {
        let stronghold = self
            .stronghold
            .lock()
            .map_err(|_| AppError::VaultInvalid)?
            .take();
        if let Some(stronghold) = stronghold {
            stronghold.save().map_err(|_| AppError::Storage)?;
            stronghold.clear().map_err(|_| AppError::VaultInvalid)?;
        }
        Ok(())
    }

    fn contains(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<bool> {
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            client
                .store()
                .get(&Self::record_key(profile_id, kind))
                .map(|value| value.is_some())
                .map_err(|_| AppError::VaultInvalid)
        })
    }

    fn get(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<Zeroizing<String>> {
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            let bytes = client
                .store()
                .get(&Self::record_key(profile_id, kind))
                .map_err(|_| AppError::VaultInvalid)?
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
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            client
                .store()
                .insert(
                    Self::record_key(profile_id, kind),
                    secret.as_bytes().to_vec(),
                    None,
                )
                .map_err(|_| AppError::VaultInvalid)?;
            stronghold.save().map_err(|_| AppError::Storage)
        })
    }

    fn delete(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<()> {
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            client
                .store()
                .delete(&Self::record_key(profile_id, kind))
                .map_err(|_| AppError::VaultInvalid)?;
            stronghold.save().map_err(|_| AppError::Storage)
        })
    }

    fn put_private_key(&self, key_id: Uuid, content: Zeroizing<Vec<u8>>) -> AppResult<()> {
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            client
                .store()
                .insert(Self::private_key_record(key_id), content.to_vec(), None)
                .map_err(|_| AppError::VaultInvalid)?;
            stronghold.save().map_err(|_| AppError::Storage)
        })
    }

    fn get_private_key(&self, key_id: Uuid) -> AppResult<Zeroizing<Vec<u8>>> {
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            client
                .store()
                .get(&Self::private_key_record(key_id))
                .map_err(|_| AppError::VaultInvalid)?
                .map(Zeroizing::new)
                .ok_or(AppError::CredentialNotFound)
        })
    }

    fn delete_private_key(&self, key_id: Uuid) -> AppResult<()> {
        self.with_stronghold(|stronghold| {
            let client = stronghold
                .get_client(CLIENT_ID)
                .map_err(|_| AppError::VaultInvalid)?;
            client
                .store()
                .delete(&Self::private_key_record(key_id))
                .map_err(|_| AppError::VaultInvalid)?;
            stronghold.save().map_err(|_| AppError::Storage)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Argon2 snapshot roundtrip is intentionally covered outside the fast unit suite"]
    fn persists_credentials_only_while_unlocked_with_the_correct_password() {
        let directory = tempfile::tempdir().expect("temporary vault directory");
        let profile_id = Uuid::new_v4();
        let vault = StrongholdCredentialVault::new(
            directory.path().join("credentials.hold"),
            directory.path().join("credentials.salt"),
        );

        vault
            .unlock(Zeroizing::new("correct master password".to_owned()))
            .expect("initialize vault");
        vault
            .put(
                profile_id,
                CredentialKind::Password,
                Zeroizing::new("server password".to_owned()),
            )
            .expect("store credential");
        vault.lock().expect("lock vault");

        assert!(matches!(
            vault.get(profile_id, CredentialKind::Password),
            Err(AppError::VaultLocked)
        ));
        assert!(matches!(
            vault.unlock(Zeroizing::new("wrong master password".to_owned())),
            Err(AppError::VaultInvalid)
        ));

        vault
            .unlock(Zeroizing::new("correct master password".to_owned()))
            .expect("reopen vault");
        let credential = vault
            .get(profile_id, CredentialKind::Password)
            .expect("read credential");
        assert_eq!(credential.as_str(), "server password");
        vault
            .delete(profile_id, CredentialKind::Password)
            .expect("delete credential");
        assert!(matches!(
            vault.get(profile_id, CredentialKind::Password),
            Err(AppError::CredentialNotFound)
        ));
    }
}
