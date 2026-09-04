mod platform;
#[cfg(any(mobile, test))]
mod portable;
mod service;
#[cfg(not(mobile))]
mod stronghold;

#[cfg(not(mobile))]
pub use platform::NativePlatformKeyStore;
pub use platform::PlatformKeyStore;
#[cfg(any(mobile, test))]
pub use platform::UnavailablePlatformKeyStore;
#[cfg(any(mobile, test))]
pub use portable::PortableCredentialVault;
pub use service::{CredentialService, CredentialVault};
#[cfg(not(mobile))]
pub use stronghold::StrongholdCredentialVault;
