use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::{AppError, AppResult};

#[cfg(not(mobile))]
const KEYRING_SERVICE: &str = "com.runory.app.cloud-sync";
#[cfg(not(mobile))]
const KEYRING_PROBE_ACCOUNT: &str = "availability-probe-v1";

/// OS-owned storage for an opaque per-organization cloud data key.
/// The recovery passphrase and decrypted infrastructure data never enter this store.
pub trait CloudSyncKeyStore: Send + Sync {
    fn is_supported(&self) -> bool;
    fn probe(&self) -> AppResult<()>;
    fn load(&self, organization_id: Uuid) -> AppResult<Option<Zeroizing<Vec<u8>>>>;
    fn store(&self, organization_id: Uuid, material: &[u8]) -> AppResult<()>;
    fn delete(&self, organization_id: Uuid) -> AppResult<()>;
}

#[cfg(not(mobile))]
pub struct NativeCloudSyncKeyStore;

#[cfg(not(mobile))]
impl NativeCloudSyncKeyStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(account: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)
    }

    fn account(organization_id: Uuid) -> String {
        format!("organization-{organization_id}-data-key-v1")
    }
}

#[cfg(not(mobile))]
impl CloudSyncKeyStore for NativeCloudSyncKeyStore {
    fn is_supported(&self) -> bool {
        true
    }

    fn probe(&self) -> AppResult<()> {
        match Self::entry(KEYRING_PROBE_ACCOUNT)?.get_secret() {
            Ok(secret) => {
                let _secret = Zeroizing::new(secret);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PlatformKeyStoreUnavailable),
        }
    }

    fn load(&self, organization_id: Uuid) -> AppResult<Option<Zeroizing<Vec<u8>>>> {
        match Self::entry(&Self::account(organization_id))?.get_secret() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::PlatformKeyStoreUnavailable),
        }
    }

    fn store(&self, organization_id: Uuid, material: &[u8]) -> AppResult<()> {
        if material.is_empty() {
            return Err(AppError::CloudCrypto);
        }
        Self::entry(&Self::account(organization_id))?
            .set_secret(material)
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)
    }

    fn delete(&self, organization_id: Uuid) -> AppResult<()> {
        match Self::entry(&Self::account(organization_id))?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PlatformKeyStoreUnavailable),
        }
    }
}

#[cfg(mobile)]
pub struct UnavailableCloudSyncKeyStore;

#[cfg(mobile)]
impl CloudSyncKeyStore for UnavailableCloudSyncKeyStore {
    fn is_supported(&self) -> bool {
        false
    }

    fn probe(&self) -> AppResult<()> {
        Err(AppError::PlatformKeyStoreUnavailable)
    }

    fn load(&self, _organization_id: Uuid) -> AppResult<Option<Zeroizing<Vec<u8>>>> {
        Ok(None)
    }

    fn store(&self, _organization_id: Uuid, _material: &[u8]) -> AppResult<()> {
        Err(AppError::PlatformKeyStoreUnavailable)
    }

    fn delete(&self, _organization_id: Uuid) -> AppResult<()> {
        Ok(())
    }
}
