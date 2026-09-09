use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult};

#[cfg(not(mobile))]
const KEYRING_SERVICE: &str = "com.runory.app";
#[cfg(not(mobile))]
const KEYRING_ACCOUNT: &str = "supabase-auth-session-v1";
const MAX_TOKEN_LENGTH: usize = 16 * 1024;

#[derive(Deserialize, Serialize, Zeroize)]
#[serde(rename_all = "camelCase")]
#[zeroize(drop)]
pub struct CloudAuthSession {
    pub access_token: String,
    pub refresh_token: String,
}

impl CloudAuthSession {
    fn validate(&self) -> AppResult<()> {
        if self.access_token.is_empty()
            || self.refresh_token.is_empty()
            || self.access_token.len() > MAX_TOKEN_LENGTH
            || self.refresh_token.len() > MAX_TOKEN_LENGTH
        {
            return Err(AppError::InvalidOperation);
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct CloudAuthSessionStore;

impl CloudAuthSessionStore {
    pub async fn load(&self) -> AppResult<Option<CloudAuthSession>> {
        #[cfg(mobile)]
        {
            return Ok(None);
        }
        #[cfg(not(mobile))]
        tokio::task::spawn_blocking(|| {
            let bytes = match entry()?.get_secret() {
                Ok(bytes) => Zeroizing::new(bytes),
                Err(keyring::Error::NoEntry) => return Ok(None),
                Err(_) => return Err(AppError::PlatformKeyStoreUnavailable),
            };
            let session: CloudAuthSession =
                serde_json::from_slice(bytes.as_slice()).map_err(|_| AppError::Storage)?;
            session.validate()?;
            Ok(Some(session))
        })
        .await
        .map_err(|_| AppError::PlatformKeyStoreUnavailable)?
    }

    pub async fn save(&self, session: CloudAuthSession) -> AppResult<()> {
        session.validate()?;
        #[cfg(mobile)]
        {
            let _ = session;
            return Err(AppError::PlatformKeyStoreUnavailable);
        }
        #[cfg(not(mobile))]
        tokio::task::spawn_blocking(move || {
            let bytes =
                Zeroizing::new(serde_json::to_vec(&session).map_err(|_| AppError::Storage)?);
            entry()?
                .set_secret(bytes.as_slice())
                .map_err(|_| AppError::PlatformKeyStoreUnavailable)
        })
        .await
        .map_err(|_| AppError::PlatformKeyStoreUnavailable)?
    }

    pub async fn clear(&self) -> AppResult<()> {
        #[cfg(mobile)]
        {
            return Ok(());
        }
        #[cfg(not(mobile))]
        tokio::task::spawn_blocking(|| match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::PlatformKeyStoreUnavailable),
        })
        .await
        .map_err(|_| AppError::PlatformKeyStoreUnavailable)?
    }
}

#[cfg(not(mobile))]
fn entry() -> AppResult<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|_| AppError::PlatformKeyStoreUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_or_oversized_tokens() {
        let empty = CloudAuthSession {
            access_token: String::new(),
            refresh_token: "refresh".into(),
        };
        assert!(empty.validate().is_err());
        let oversized = CloudAuthSession {
            access_token: "a".repeat(MAX_TOKEN_LENGTH + 1),
            refresh_token: "refresh".into(),
        };
        assert!(oversized.validate().is_err());
    }
}
