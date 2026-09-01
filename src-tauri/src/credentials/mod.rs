#[cfg(any(mobile, test))]
mod portable;
mod service;
#[cfg(not(mobile))]
mod stronghold;

#[cfg(mobile)]
pub use portable::PortableCredentialVault;
pub use service::{CredentialService, CredentialVault};
#[cfg(not(mobile))]
pub use stronghold::StrongholdCredentialVault;
