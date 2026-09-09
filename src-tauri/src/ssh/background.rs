use super::{service::authenticate, service::HostKeyHandler, ForwardTransport};
use crate::domain::{AppResult, AuthMethod, KeySource, ServerProfile, SshConnectionRequest};
use russh::{client, Disconnect};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, PartialEq)]
pub(crate) struct ConnectionIdentity {
    pub profile_id: Uuid,
    host: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    key_source: Option<KeySource>,
}

impl From<&ServerProfile> for ConnectionIdentity {
    fn from(profile: &ServerProfile) -> Self {
        Self {
            profile_id: profile.id,
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            auth_method: profile.auth_method,
            key_source: profile.key_source.clone(),
        }
    }
}

/// Dedicated authenticated transport: no terminal channel, PTY, shell or SFTP.
/// Only running tunnel tasks retain ownership; the pool holds weak references.
pub(crate) struct BackgroundConnection {
    pub id: Uuid,
    client: Arc<client::Handle<HostKeyHandler>>,
}

impl BackgroundConnection {
    pub async fn connect(request: SshConnectionRequest, fingerprint: String) -> AppResult<Self> {
        let client = authenticate(request, fingerprint).await?;
        Ok(Self {
            id: Uuid::new_v4(),
            client: Arc::new(client),
        })
    }

    pub fn transport(&self) -> ForwardTransport {
        ForwardTransport::new(&self.client)
    }
}

impl Drop for BackgroundConnection {
    fn drop(&mut self) {
        let client = Arc::clone(&self.client);
        // Release SSH even when the final listener fails without another UI poll.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    client.disconnect(Disconnect::ByApplication, "forwarding complete", "en"),
                )
                .await;
            });
        }
    }
}
