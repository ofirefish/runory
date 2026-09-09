// Phase M0 crypto foundation. No cloud transport may sync credentials until later gates wire
// device trust, recovery, migration, and production review around this module.
mod auth_session;
#[allow(dead_code)]
mod crypto;
mod platform;
mod policy;
mod service;
mod state;

pub use auth_session::{CloudAuthSession, CloudAuthSessionStore};
pub use platform::CloudSyncKeyStore;
#[cfg(not(mobile))]
pub use platform::NativeCloudSyncKeyStore;
#[cfg(mobile)]
pub use platform::UnavailableCloudSyncKeyStore;
pub use policy::{
    CloudPolicyAction, CloudPolicyBindingRequest, CloudPolicyCredentialRequest,
    CloudPolicyOrganizationRequest, CloudPolicyService, CloudPolicyStatus,
};
pub use service::CloudSyncService;
pub use state::CloudSyncStateRepository;
