use crate::{
    credentials::CredentialService,
    domain::{
        AppError, AppResult, AuthMethod, CredentialInput, CredentialKind, KeySource,
        SshAuthentication, SshConnectionRequest,
    },
    known_hosts::KnownHostService,
    profiles::ProfileService,
};
use uuid::Uuid;
use zeroize::Zeroizing;

pub(super) struct PreparedConnection {
    pub(super) profile_id: Uuid,
    pub(super) detect_os_distribution: bool,
    pub(super) identity: crate::ssh::ConnectionIdentity,
    credential_kind: CredentialKind,
    secret_to_remember: Option<Zeroizing<String>>,
    pub(super) expected_fingerprint: String,
    pub(super) request: Option<SshConnectionRequest>,
}

pub(super) async fn prepare_connection(
    profile_id: Uuid,
    verification_attempt_id: Uuid,
    credential: CredentialInput,
    known_hosts: &KnownHostService,
    profiles: &ProfileService,
    credentials: &CredentialService,
) -> AppResult<(PreparedConnection, String)> {
    prepare_connection_scoped(
        profile_id,
        verification_attempt_id,
        credential,
        "direct",
        known_hosts,
        profiles,
        credentials,
    )
    .await
}

pub(super) async fn prepare_connection_scoped(
    profile_id: Uuid,
    verification_attempt_id: Uuid,
    credential: CredentialInput,
    route_scope: &str,
    known_hosts: &KnownHostService,
    profiles: &ProfileService,
    credentials: &CredentialService,
) -> AppResult<(PreparedConnection, String)> {
    let profile = profiles.get(profile_id).await?;
    let (credential_kind, allow_empty, auth_method) = match profile.auth_method {
        AuthMethod::Password => (CredentialKind::Password, false, "password"),
        AuthMethod::PrivateKey => (CredentialKind::KeyPassphrase, true, "private-key"),
    };
    let expected_fingerprint = known_hosts
        .consume_scoped(
            verification_attempt_id,
            route_scope,
            &profile.host,
            profile.port,
        )
        .await?;
    let resolved = credentials
        .resolve_for_profile(profile.id, credential_kind, credential, allow_empty)
        .await?;
    let secret_to_remember = resolved
        .remember_after_auth
        .then(|| Zeroizing::new(resolved.secret.to_string()));
    let identity = crate::ssh::ConnectionIdentity::from(&profile);
    let authentication = match profile.auth_method {
        AuthMethod::Password => SshAuthentication::Password {
            password: resolved.secret,
        },
        AuthMethod::PrivateKey => match profile.key_source {
            Some(KeySource::File { path }) => SshAuthentication::PrivateKeyFile {
                path,
                passphrase: (!resolved.secret.is_empty()).then_some(resolved.secret),
            },
            Some(KeySource::Vault { key_id }) => SshAuthentication::PrivateKeyData {
                content: credentials.private_key(key_id).await?,
                passphrase: (!resolved.secret.is_empty()).then_some(resolved.secret),
            },
            None => return Err(AppError::InvalidProfile),
        },
    };
    Ok((
        PreparedConnection {
            identity,
            profile_id: profile.id,
            detect_os_distribution: profile.os_distribution.is_none(),
            credential_kind,
            secret_to_remember,
            expected_fingerprint,
            request: Some(SshConnectionRequest {
                host: profile.host,
                port: profile.port,
                username: profile.username,
                authentication,
            }),
        },
        auth_method.to_owned(),
    ))
}

pub(super) async fn remember_after_success(
    prepared: &mut PreparedConnection,
    credentials: &CredentialService,
) -> bool {
    let Some(secret) = prepared.secret_to_remember.take() else {
        return true;
    };
    match credentials
        .remember(prepared.profile_id, prepared.credential_kind, secret)
        .await
    {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(profile_id = %prepared.profile_id, error_code = error.code(), "could not remember credential");
            false
        }
    }
}
