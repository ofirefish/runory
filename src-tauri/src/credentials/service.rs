use std::sync::Arc;

use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::{AppError, AppResult, CredentialInput, CredentialKind, CredentialStatus};

pub trait CredentialVault: Send + Sync {
    fn is_initialized(&self) -> bool;
    fn is_unlocked(&self) -> bool;
    fn unlock(&self, master_password: Zeroizing<String>) -> AppResult<()>;
    fn lock(&self) -> AppResult<()>;
    fn contains(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<bool>;
    fn get(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<Zeroizing<String>>;
    fn put(
        &self,
        profile_id: Uuid,
        kind: CredentialKind,
        secret: Zeroizing<String>,
    ) -> AppResult<()>;
    fn delete(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<()>;
    fn put_private_key(&self, key_id: Uuid, content: Zeroizing<Vec<u8>>) -> AppResult<()>;
    fn get_private_key(&self, key_id: Uuid) -> AppResult<Zeroizing<Vec<u8>>>;
    fn delete_private_key(&self, key_id: Uuid) -> AppResult<()>;
}

pub struct ResolvedCredential {
    pub secret: Zeroizing<String>,
    pub remember_after_auth: bool,
}

#[derive(Clone)]
pub struct CredentialService {
    vault: Arc<dyn CredentialVault>,
}

impl CredentialService {
    pub fn new(vault: Arc<dyn CredentialVault>) -> Self {
        Self { vault }
    }

    pub async fn unlock(&self, master_password: String) -> AppResult<()> {
        validate_secret(&master_password, false)?;
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.unlock(Zeroizing::new(master_password)))
            .await
            .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn lock(&self) -> AppResult<()> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.lock())
            .await
            .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn status(
        &self,
        profile_id: Option<Uuid>,
        kind: Option<CredentialKind>,
    ) -> AppResult<CredentialStatus> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || {
            let initialized = vault.is_initialized();
            let unlocked = vault.is_unlocked();
            let has_credential = if unlocked {
                match (profile_id, kind) {
                    (Some(profile_id), Some(kind)) => vault.contains(profile_id, kind)?,
                    _ => false,
                }
            } else {
                false
            };
            Ok(CredentialStatus {
                vault_initialized: initialized,
                vault_unlocked: unlocked,
                has_credential,
            })
        })
        .await
        .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn resolve_for_profile(
        &self,
        profile_id: Uuid,
        kind: CredentialKind,
        input: CredentialInput,
        allow_empty: bool,
    ) -> AppResult<ResolvedCredential> {
        match input {
            CredentialInput::SessionOnly { secret } => {
                validate_secret(&secret, allow_empty)?;
                Ok(ResolvedCredential {
                    secret: Zeroizing::new(secret),
                    remember_after_auth: false,
                })
            }
            CredentialInput::RememberSecurely { secret } => {
                validate_secret(&secret, false)?;
                if !self.vault.is_unlocked() {
                    return Err(AppError::VaultLocked);
                }
                Ok(ResolvedCredential {
                    secret: Zeroizing::new(secret),
                    remember_after_auth: true,
                })
            }
            CredentialInput::Stored => {
                let vault = Arc::clone(&self.vault);
                let secret = tokio::task::spawn_blocking(move || vault.get(profile_id, kind))
                    .await
                    .map_err(|_| AppError::VaultInvalid)??;
                Ok(ResolvedCredential {
                    secret,
                    remember_after_auth: false,
                })
            }
        }
    }

    pub async fn remember(
        &self,
        profile_id: Uuid,
        kind: CredentialKind,
        secret: Zeroizing<String>,
    ) -> AppResult<()> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.put(profile_id, kind, secret))
            .await
            .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn forget(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<()> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.delete(profile_id, kind))
            .await
            .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn prepare_profile_delete(&self, profile_id: Uuid) -> AppResult<()> {
        if self.vault.is_initialized() && !self.vault.is_unlocked() {
            return Err(AppError::VaultLocked);
        }
        if self.vault.is_unlocked() {
            self.forget(profile_id, CredentialKind::Password).await?;
            self.forget(profile_id, CredentialKind::KeyPassphrase)
                .await?;
        }
        Ok(())
    }

    pub async fn import_private_key(&self, content: Vec<u8>) -> AppResult<Uuid> {
        if content.is_empty() || content.len() > 1024 * 1024 {
            return Err(AppError::PrivateKeyInvalid);
        }
        let encoded = std::str::from_utf8(&content).map_err(|_| AppError::PrivateKeyInvalid)?;
        if !encoded.contains("-----BEGIN ") || !encoded.contains("PRIVATE KEY-----") {
            return Err(AppError::PrivateKeyInvalid);
        }
        let key_id = Uuid::new_v4();
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.put_private_key(key_id, Zeroizing::new(content)))
            .await
            .map_err(|_| AppError::VaultInvalid)??;
        Ok(key_id)
    }

    pub async fn private_key(&self, key_id: Uuid) -> AppResult<Zeroizing<Vec<u8>>> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.get_private_key(key_id))
            .await
            .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn forget_private_key(&self, key_id: Uuid) -> AppResult<()> {
        let vault = Arc::clone(&self.vault);
        tokio::task::spawn_blocking(move || vault.delete_private_key(key_id))
            .await
            .map_err(|_| AppError::VaultInvalid)?
    }
}

fn validate_secret(secret: &str, allow_empty: bool) -> AppResult<()> {
    if (!allow_empty && secret.is_empty()) || secret.len() > 16 * 1024 {
        Err(AppError::InvalidProfile)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MemoryVault {
        unlocked: Mutex<bool>,
        values: Mutex<HashMap<(Uuid, CredentialKind), String>>,
        keys: Mutex<HashMap<Uuid, Vec<u8>>>,
    }

    impl CredentialVault for MemoryVault {
        fn is_initialized(&self) -> bool {
            true
        }
        fn is_unlocked(&self) -> bool {
            self.unlocked.lock().is_ok_and(|value| *value)
        }
        fn unlock(&self, _master_password: Zeroizing<String>) -> AppResult<()> {
            *self.unlocked.lock().map_err(|_| AppError::VaultInvalid)? = true;
            Ok(())
        }
        fn lock(&self) -> AppResult<()> {
            *self.unlocked.lock().map_err(|_| AppError::VaultInvalid)? = false;
            Ok(())
        }
        fn contains(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<bool> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            Ok(self
                .values
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .contains_key(&(profile_id, kind)))
        }
        fn get(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<Zeroizing<String>> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            self.values
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .get(&(profile_id, kind))
                .cloned()
                .map(Zeroizing::new)
                .ok_or(AppError::CredentialNotFound)
        }
        fn put(
            &self,
            profile_id: Uuid,
            kind: CredentialKind,
            secret: Zeroizing<String>,
        ) -> AppResult<()> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            self.values
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .insert((profile_id, kind), secret.to_string());
            Ok(())
        }
        fn delete(&self, profile_id: Uuid, kind: CredentialKind) -> AppResult<()> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            self.values
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .remove(&(profile_id, kind));
            Ok(())
        }
        fn put_private_key(&self, key_id: Uuid, content: Zeroizing<Vec<u8>>) -> AppResult<()> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            self.keys
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .insert(key_id, content.to_vec());
            Ok(())
        }
        fn get_private_key(&self, key_id: Uuid) -> AppResult<Zeroizing<Vec<u8>>> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            self.keys
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .get(&key_id)
                .cloned()
                .map(Zeroizing::new)
                .ok_or(AppError::CredentialNotFound)
        }
        fn delete_private_key(&self, key_id: Uuid) -> AppResult<()> {
            if !self.is_unlocked() {
                return Err(AppError::VaultLocked);
            }
            self.keys
                .lock()
                .map_err(|_| AppError::VaultInvalid)?
                .remove(&key_id);
            Ok(())
        }
    }

    #[tokio::test]
    async fn stored_credentials_never_cross_the_status_boundary() {
        let profile_id = Uuid::new_v4();
        let service = CredentialService::new(Arc::new(MemoryVault::default()));
        service
            .unlock("master-password".into())
            .await
            .expect("unlock");
        service
            .remember(
                profile_id,
                CredentialKind::Password,
                Zeroizing::new("secret".into()),
            )
            .await
            .expect("remember");
        let status = service
            .status(Some(profile_id), Some(CredentialKind::Password))
            .await
            .expect("status");
        assert!(status.has_credential);
        let resolved = service
            .resolve_for_profile(
                profile_id,
                CredentialKind::Password,
                CredentialInput::Stored,
                false,
            )
            .await
            .expect("resolve");
        assert_eq!(resolved.secret.as_str(), "secret");
    }

    #[tokio::test]
    async fn locked_vault_cannot_resolve_or_delete_credentials() {
        let profile_id = Uuid::new_v4();
        let service = CredentialService::new(Arc::new(MemoryVault::default()));
        assert!(matches!(
            service
                .resolve_for_profile(
                    profile_id,
                    CredentialKind::Password,
                    CredentialInput::Stored,
                    false
                )
                .await,
            Err(AppError::VaultLocked)
        ));
        assert!(matches!(
            service.prepare_profile_delete(profile_id).await,
            Err(AppError::VaultLocked)
        ));
    }

    #[tokio::test]
    async fn private_key_content_never_crosses_the_import_boundary() {
        let service = CredentialService::new(Arc::new(MemoryVault::default()));
        service
            .unlock("master-password".into())
            .await
            .expect("unlock");
        let key =
            b"-----BEGIN OPENSSH PRIVATE KEY-----\nencoded\n-----END OPENSSH PRIVATE KEY-----\n";
        let key_id = service
            .import_private_key(key.to_vec())
            .await
            .expect("import key");
        assert_eq!(
            service.private_key(key_id).await.expect("load").as_slice(),
            key
        );
        service.forget_private_key(key_id).await.expect("forget");
        assert!(matches!(
            service.private_key(key_id).await,
            Err(AppError::CredentialNotFound)
        ));
    }
}
