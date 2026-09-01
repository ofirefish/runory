use tauri::{ipc::Channel, State};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::credentials::CredentialService;
use crate::domain::{
    AppError, AppResult, AuthMethod, ConnectProfileRequest, ConnectRequest, ConnectResponse,
    CredentialInput, CredentialKind, KeySource, ResizeRequest, SessionRequest, SshAuthentication,
    SshConnectionRequest, TerminalEvent, TestConnectionProfileRequest, TestConnectionResponse,
    WriteRequest,
};
use crate::known_hosts::KnownHostService;
use crate::profiles::ProfileService;
use crate::ssh::{ServerSessionManager, SshService};

struct PreparedConnection {
    profile_id: Uuid,
    detect_os_distribution: bool,
    credential_kind: CredentialKind,
    secret_to_remember: Option<Zeroizing<String>>,
    expected_fingerprint: String,
    request: Option<SshConnectionRequest>,
}

async fn prepare_connection(
    profile_id: Uuid,
    verification_attempt_id: Uuid,
    credential: CredentialInput,
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
        .consume(verification_attempt_id, &profile.host, profile.port)
        .await?;
    let resolved = credentials
        .resolve_for_profile(profile.id, credential_kind, credential, allow_empty)
        .await?;
    let secret_to_remember = resolved
        .remember_after_auth
        .then(|| Zeroizing::new(resolved.secret.to_string()));
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

async fn remember_after_success(
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

async fn open_profile_session(
    request: ConnectProfileRequest,
    on_event: Channel<TerminalEvent>,
    sessions: &ServerSessionManager,
    known_hosts: &KnownHostService,
    profiles: &ProfileService,
    credentials: &CredentialService,
    reconnecting: bool,
) -> AppResult<ConnectResponse> {
    let (mut prepared, auth_method) = prepare_connection(
        request.profile_id,
        request.verification_attempt_id,
        request.credential,
        known_hosts,
        profiles,
        credentials,
    )
    .await?;
    tracing::info!(profile_id = %prepared.profile_id, auth_method, reconnecting, "starting SSH connection");
    let session_id = sessions
        .connect(
            prepared.profile_id,
            ConnectRequest {
                connection: prepared.request.take().ok_or(AppError::InvalidProfile)?,
                cols: request.cols,
                rows: request.rows,
            },
            std::mem::take(&mut prepared.expected_fingerprint),
            on_event,
        )
        .await?;
    let credential_saved = remember_after_success(&mut prepared, credentials).await;
    let detected_os = if prepared.detect_os_distribution {
        match sessions.detect_os_distribution(session_id).await {
            Ok(distribution) => distribution,
            Err(error) => {
                tracing::debug!(profile_id = %prepared.profile_id, error_code = error.code(), "could not detect remote operating system");
                None
            }
        }
    } else {
        None
    };
    if let Err(error) = profiles
        .mark_connected(prepared.profile_id, detected_os)
        .await
    {
        tracing::warn!(profile_id = %prepared.profile_id, error_code = error.code(), "could not update last connected metadata");
    }
    Ok(ConnectResponse {
        session_id,
        credential_saved,
    })
}

#[tauri::command]
pub async fn ssh_connect(
    request: ConnectProfileRequest,
    on_event: Channel<TerminalEvent>,
    sessions: State<'_, ServerSessionManager>,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<ConnectResponse> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    open_profile_session(
        request,
        on_event,
        &sessions,
        &known_hosts,
        &profiles,
        &credentials,
        false,
    )
    .await
}

#[tauri::command]
pub async fn ssh_reconnect(
    request: ConnectProfileRequest,
    on_event: Channel<TerminalEvent>,
    sessions: State<'_, ServerSessionManager>,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<ConnectResponse> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    open_profile_session(
        request,
        on_event,
        &sessions,
        &known_hosts,
        &profiles,
        &credentials,
        true,
    )
    .await
}

#[tauri::command]
pub async fn ssh_test(
    request: TestConnectionProfileRequest,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TestConnectionResponse> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    let (mut prepared, auth_method) = prepare_connection(
        request.profile_id,
        request.verification_attempt_id,
        request.credential,
        &known_hosts,
        &profiles,
        &credentials,
    )
    .await?;
    tracing::info!(profile_id = %prepared.profile_id, auth_method, "testing SSH connection");
    let expected_fingerprint = std::mem::take(&mut prepared.expected_fingerprint);
    SshService::test_connection(
        prepared.request.take().ok_or(AppError::InvalidProfile)?,
        expected_fingerprint,
    )
    .await?;
    Ok(TestConnectionResponse {
        credential_saved: remember_after_success(&mut prepared, &credentials).await,
    })
}

#[tauri::command]
pub async fn ssh_write(
    request: WriteRequest,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<()> {
    sessions.write(request.session_id, request.data).await
}

#[tauri::command]
pub async fn ssh_resize(
    request: ResizeRequest,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<()> {
    sessions
        .resize(request.session_id, request.cols, request.rows)
        .await
}

#[tauri::command]
pub async fn ssh_disconnect(
    request: SessionRequest,
    sessions: State<'_, ServerSessionManager>,
) -> AppResult<()> {
    sessions.disconnect(request.session_id).await
}
