use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use base64::Engine;
use getrandom::rand_core::TryRng;
use getrandom::SysRng;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult, CredentialInput, CredentialKind, CredentialStatus};

use super::PlatformKeyStore;
#[cfg(test)]
use super::UnavailablePlatformKeyStore;

const PLATFORM_VAULT_SECRET_LENGTH: usize = 32;

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
    platform_key_store: Arc<dyn PlatformKeyStore>,
    platform_unlock_available: Arc<AtomicBool>,
    platform_unlock_configured: Arc<AtomicBool>,
}

impl CredentialService {
    #[cfg(test)]
    pub fn new(vault: Arc<dyn CredentialVault>) -> Self {
        Self::with_platform_key_store(vault, Arc::new(UnavailablePlatformKeyStore))
    }

    pub fn with_platform_key_store(
        vault: Arc<dyn CredentialVault>,
        platform_key_store: Arc<dyn PlatformKeyStore>,
    ) -> Self {
        Self {
            vault,
            platform_key_store,
            platform_unlock_available: Arc::new(AtomicBool::new(false)),
            platform_unlock_configured: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn refresh_platform_availability(&self) -> bool {
        if !self.platform_key_store.is_supported() {
            self.platform_unlock_available
                .store(false, Ordering::Release);
            return false;
        }
        let platform_key_store = Arc::clone(&self.platform_key_store);
        let available = tokio::task::spawn_blocking(move || platform_key_store.probe().is_ok())
            .await
            .unwrap_or(false);
        self.platform_unlock_available
            .store(available, Ordering::Release);
        available
    }

    pub async fn auto_unlock(&self) -> AppResult<bool> {
        if self.vault.is_unlocked() {
            return Ok(true);
        }
        let platform_available = self.refresh_platform_availability().await;
        if !self.vault.is_initialized() {
            self.platform_unlock_configured
                .store(false, Ordering::Release);
            return Ok(false);
        }
        if !platform_available {
            return Ok(false);
        }
        let vault = Arc::clone(&self.vault);
        let platform_key_store = Arc::clone(&self.platform_key_store);
        let available = Arc::clone(&self.platform_unlock_available);
        let configured = Arc::clone(&self.platform_unlock_configured);
        tokio::task::spawn_blocking(move || {
            let secret = match platform_key_store.load_vault_secret() {
                Ok(secret) => secret,
                Err(error) => {
                    available.store(false, Ordering::Release);
                    return Err(error);
                }
            };
            let Some(secret) = secret else {
                configured.store(false, Ordering::Release);
                return Ok(false);
            };
            configured.store(true, Ordering::Release);
            vault.unlock(secret)?;
            Ok(true)
        })
        .await
        .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn initialize_with_platform_key(&self) -> AppResult<()> {
        if self.vault.is_initialized() {
            return if self.auto_unlock().await? {
                Ok(())
            } else {
                Err(AppError::VaultLocked)
            };
        }
        if !self.refresh_platform_availability().await {
            return Err(AppError::PlatformKeyStoreUnavailable);
        }
        let secret = generate_platform_vault_secret()?;
        let vault = Arc::clone(&self.vault);
        let platform_key_store = Arc::clone(&self.platform_key_store);
        let available = Arc::clone(&self.platform_unlock_available);
        let configured = Arc::clone(&self.platform_unlock_configured);
        tokio::task::spawn_blocking(move || {
            if let Err(error) = platform_key_store.store_vault_secret(secret.as_str()) {
                available.store(false, Ordering::Release);
                return Err(error);
            }
            configured.store(true, Ordering::Release);
            if let Err(error) = vault.unlock(secret) {
                let _ = platform_key_store.delete_vault_secret();
                configured.store(false, Ordering::Release);
                return Err(error);
            }
            Ok(())
        })
        .await
        .map_err(|_| AppError::VaultInvalid)?
    }

    pub async fn unlock_with_platform_key(&self) -> AppResult<()> {
        if self.auto_unlock().await? {
            Ok(())
        } else {
            Err(AppError::PlatformKeyStoreUnavailable)
        }
    }

    pub async fn unlock(&self, master_password: String) -> AppResult<()> {
        validate_secret(&master_password, false)?;
        let platform_available = self.refresh_platform_availability().await;
        let vault = Arc::clone(&self.vault);
        let platform_key_store = Arc::clone(&self.platform_key_store);
        let available = Arc::clone(&self.platform_unlock_available);
        let configured = Arc::clone(&self.platform_unlock_configured);
        tokio::task::spawn_blocking(move || {
            let master_password = Zeroizing::new(master_password);
            vault.unlock(Zeroizing::new(master_password.to_string()))?;
            if platform_available {
                if let Err(error) = platform_key_store.store_vault_secret(master_password.as_str())
                {
                    let _ = vault.lock();
                    available.store(false, Ordering::Release);
                    configured.store(false, Ordering::Release);
                    return Err(error);
                }
                configured.store(true, Ordering::Release);
            }
            Ok(())
        })
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
        let platform_unlock_supported = self.platform_key_store.is_supported();
        let platform_unlock_available = self.platform_unlock_available.load(Ordering::Acquire);
        let platform_unlock_configured = self.platform_unlock_configured.load(Ordering::Acquire);
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
                platform_unlock_supported,
                platform_unlock_available,
                platform_unlock_configured,
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

fn generate_platform_vault_secret() -> AppResult<Zeroizing<String>> {
    let mut bytes = Zeroizing::new([0_u8; PLATFORM_VAULT_SECRET_LENGTH]);
    SysRng
        .try_fill_bytes(bytes.as_mut())
        .map_err(|_| AppError::VaultInvalid)?;
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes.as_ref());
    bytes.zeroize();
    Ok(Zeroizing::new(secret))
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

    struct MemoryVault {
        initialized: Mutex<bool>,
        unlocked: Mutex<bool>,
        values: Mutex<HashMap<(Uuid, CredentialKind), String>>,
        keys: Mutex<HashMap<Uuid, Vec<u8>>>,
    }

    impl Default for MemoryVault {
        fn default() -> Self {
            Self {
                initialized: Mutex::new(true),
                unlocked: Mutex::new(false),
                values: Mutex::new(HashMap::new()),
                keys: Mutex::new(HashMap::new()),
            }
        }
    }

    impl MemoryVault {
        fn uninitialized() -> Self {
            Self {
                initialized: Mutex::new(false),
                ..Self::default()
            }
        }
    }

    impl CredentialVault for MemoryVault {
        fn is_initialized(&self) -> bool {
            self.initialized.lock().is_ok_and(|value| *value)
        }
        fn is_unlocked(&self) -> bool {
            self.unlocked.lock().is_ok_and(|value| *value)
        }
        fn unlock(&self, _master_password: Zeroizing<String>) -> AppResult<()> {
            *self
                .initialized
                .lock()
                .map_err(|_| AppError::VaultInvalid)? = true;
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

    struct MemoryPlatformKeyStore {
        secret: Mutex<Option<String>>,
    }

    impl MemoryPlatformKeyStore {
        fn empty() -> Self {
            Self {
                secret: Mutex::new(None),
            }
        }
    }

    impl PlatformKeyStore for MemoryPlatformKeyStore {
        fn is_supported(&self) -> bool {
            true
        }

        fn probe(&self) -> AppResult<()> {
            Ok(())
        }

        fn load_vault_secret(&self) -> AppResult<Option<Zeroizing<String>>> {
            Ok(self
                .secret
                .lock()
                .map_err(|_| AppError::PlatformKeyStoreUnavailable)?
                .clone()
                .map(Zeroizing::new))
        }

        fn store_vault_secret(&self, secret: &str) -> AppResult<()> {
            *self
                .secret
                .lock()
                .map_err(|_| AppError::PlatformKeyStoreUnavailable)? = Some(secret.to_owned());
            Ok(())
        }

        fn delete_vault_secret(&self) -> AppResult<()> {
            *self
                .secret
                .lock()
                .map_err(|_| AppError::PlatformKeyStoreUnavailable)? = None;
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

    #[tokio::test]
    async fn legacy_password_unlock_migrates_to_platform_unlock() {
        let vault = Arc::new(MemoryVault::default());
        let platform = Arc::new(MemoryPlatformKeyStore::empty());
        let service = CredentialService::with_platform_key_store(vault, platform.clone());

        service
            .unlock("existing-vault-password".into())
            .await
            .expect("legacy unlock should migrate");
        let status = service.status(None, None).await.expect("status");
        assert!(status.platform_unlock_supported);
        assert!(status.platform_unlock_available);
        assert!(status.platform_unlock_configured);
        assert!(status.vault_unlocked);
        assert!(platform.secret.lock().expect("platform secret").is_some());

        service.lock().await.expect("lock");
        assert!(service.auto_unlock().await.expect("automatic unlock"));
        assert!(
            service
                .status(None, None)
                .await
                .expect("status")
                .vault_unlocked
        );
    }

    #[tokio::test]
    async fn new_vault_uses_random_platform_secret_without_user_password() {
        let vault = Arc::new(MemoryVault::uninitialized());
        let platform = Arc::new(MemoryPlatformKeyStore::empty());
        let service = CredentialService::with_platform_key_store(vault, platform.clone());

        service
            .initialize_with_platform_key()
            .await
            .expect("platform-backed initialization");
        let status = service.status(None, None).await.expect("status");
        assert!(status.vault_initialized);
        assert!(status.vault_unlocked);
        assert!(status.platform_unlock_available);
        assert!(status.platform_unlock_configured);
        let stored = platform.secret.lock().expect("platform secret");
        assert!(stored.as_ref().is_some_and(|secret| secret.len() >= 43));
        assert_ne!(stored.as_deref(), Some("existing-vault-password"));
    }

    #[tokio::test]
    async fn stale_platform_secret_never_creates_a_vault_during_startup() {
        let vault = Arc::new(MemoryVault::uninitialized());
        let platform = Arc::new(MemoryPlatformKeyStore {
            secret: Mutex::new(Some("stale-secret".into())),
        });
        let service = CredentialService::with_platform_key_store(vault, platform);

        assert!(!service.auto_unlock().await.expect("automatic unlock"));
        let status = service.status(None, None).await.expect("status");
        assert!(!status.vault_initialized);
        assert!(!status.vault_unlocked);
        assert!(!status.platform_unlock_configured);
    }

    #[tokio::test]
    async fn unavailable_platform_store_falls_back_to_session_password_unlock() {
        let service = CredentialService::new(Arc::new(MemoryVault::default()));

        service
            .unlock("existing-vault-password".into())
            .await
            .expect("password fallback should remain usable");
        let status = service.status(None, None).await.expect("status");
        assert!(status.vault_unlocked);
        assert!(!status.platform_unlock_supported);
        assert!(!status.platform_unlock_available);
        assert!(!status.platform_unlock_configured);
    }
}
