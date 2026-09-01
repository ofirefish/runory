use zeroize::Zeroizing;

use crate::domain::{AppError, AppResult};

#[cfg(not(mobile))]
const KEYRING_SERVICE: &str = "com.runory.app";
#[cfg(not(mobile))]
const KEYRING_ACCOUNT: &str = "credential-vault-master-v1";
#[cfg(not(mobile))]
const KEYRING_PROBE_ACCOUNT: &str = "availability-probe-v1";

/// OS-owned storage for the opaque secret that unlocks the local credential Vault.
/// Implementations must never persist this secret in Runory repositories or logs.
pub trait PlatformKeyStore: Send + Sync {
    fn is_supported(&self) -> bool;
    /// Performs a read-only runtime check. A compiled backend can still be
    /// unavailable for the current desktop logon session.
    fn probe(&self) -> AppResult<()>;
    fn load_vault_secret(&self) -> AppResult<Option<Zeroizing<String>>>;
    fn store_vault_secret(&self, secret: &str) -> AppResult<()>;
    fn delete_vault_secret(&self) -> AppResult<()>;
}

#[cfg(not(mobile))]
pub struct NativePlatformKeyStore;

#[cfg(not(mobile))]
impl NativePlatformKeyStore {
    pub fn new() -> Self {
        Self
    }

    fn entry() -> AppResult<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)
    }

    fn probe_entry() -> AppResult<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_PROBE_ACCOUNT)
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)
    }
}

#[cfg(not(mobile))]
impl PlatformKeyStore for NativePlatformKeyStore {
    fn is_supported(&self) -> bool {
        true
    }

    fn probe(&self) -> AppResult<()> {
        match Self::probe_entry()?.get_secret() {
            Ok(secret) => {
                let _secret = Zeroizing::new(secret);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PlatformKeyStoreUnavailable),
        }
    }

    fn load_vault_secret(&self) -> AppResult<Option<Zeroizing<String>>> {
        let bytes = match Self::entry()?.get_secret() {
            Ok(bytes) => Zeroizing::new(bytes),
            Err(keyring::Error::NoEntry) => return Ok(None),
            Err(_) => return Err(AppError::PlatformKeyStoreUnavailable),
        };
        let secret = std::str::from_utf8(bytes.as_slice())
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)?;
        if secret.is_empty() {
            return Err(AppError::PlatformKeyStoreUnavailable);
        }
        Ok(Some(Zeroizing::new(secret.to_owned())))
    }

    fn store_vault_secret(&self, secret: &str) -> AppResult<()> {
        if secret.is_empty() {
            return Err(AppError::PlatformKeyStoreUnavailable);
        }
        Self::entry()?
            .set_secret(secret.as_bytes())
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)
    }

    fn delete_vault_secret(&self) -> AppResult<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PlatformKeyStoreUnavailable),
        }
    }
}

#[cfg(any(mobile, test))]
pub struct UnavailablePlatformKeyStore;

#[cfg(any(mobile, test))]
impl PlatformKeyStore for UnavailablePlatformKeyStore {
    fn is_supported(&self) -> bool {
        false
    }

    fn probe(&self) -> AppResult<()> {
        Err(AppError::PlatformKeyStoreUnavailable)
    }

    fn load_vault_secret(&self) -> AppResult<Option<Zeroizing<String>>> {
        Ok(None)
    }

    fn store_vault_secret(&self, _secret: &str) -> AppResult<()> {
        Err(AppError::PlatformKeyStoreUnavailable)
    }

    fn delete_vault_secret(&self) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_store_never_accepts_a_secret() {
        let store = UnavailablePlatformKeyStore;
        assert!(!store.is_supported());
        assert!(matches!(
            store.probe(),
            Err(AppError::PlatformKeyStoreUnavailable)
        ));
        assert!(store.load_vault_secret().expect("load").is_none());
        assert!(matches!(
            store.store_vault_secret("secret"),
            Err(AppError::PlatformKeyStoreUnavailable)
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "writes and removes an isolated Windows Credential Manager test entry"]
    fn windows_credential_manager_round_trip() {
        struct Cleanup(keyring::Entry);

        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete_credential();
            }
        }

        let account = format!("credential-vault-test-{}", uuid::Uuid::new_v4());
        let entry = keyring::Entry::new("com.runory.app.test", &account)
            .expect("isolated Credential Manager entry");
        let cleanup = Cleanup(entry);
        cleanup
            .0
            .set_secret(b"runory-isolated-test-secret")
            .expect("write isolated secret");
        let loaded = Zeroizing::new(cleanup.0.get_secret().expect("read isolated secret"));
        assert_eq!(loaded.as_slice(), b"runory-isolated-test-secret");
    }
}
