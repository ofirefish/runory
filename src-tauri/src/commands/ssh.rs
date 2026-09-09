use std::sync::Arc;

use tauri::{ipc::Channel, State};

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::credentials::CredentialService;
use crate::domain::{
    AppError, AppResult, CancelJumpConnectionRequest, ConnectProfileRequest, ConnectRequest,
    ConnectResponse, ConnectionRoute, PrepareJumpConnectionRequest, PrepareJumpConnectionResponse,
    ResizeRequest, SessionRequest, TerminalEvent, TestConnectionProfileRequest,
    TestConnectionResponse, WriteRequest,
};
use crate::known_hosts::KnownHostService;
use crate::profiles::ProfileService;
use crate::ssh::{
    jump_route_scope, BackgroundConnection, JumpConnectionManager, ServerSessionManager, SshService,
};

use super::connection::{prepare_connection, prepare_connection_scoped, remember_after_success};

async fn authorize_jump_if_present(
    profile_id: uuid::Uuid,
    profiles: &ProfileService,
    policies: &CloudPolicyService,
) -> AppResult<()> {
    let profile = profiles.get(profile_id).await?;
    if let ConnectionRoute::JumpHost { profile_id } = profile.connection_route {
        let jump = profiles.get(profile_id).await?;
        if jump.connection_route != ConnectionRoute::Direct {
            return Err(AppError::InvalidJumpHost);
        }
        policies
            .authorize(profile_id, CloudPolicyAction::Connect)
            .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn open_profile_session(
    request: ConnectProfileRequest,
    on_event: Channel<TerminalEvent>,
    sessions: &ServerSessionManager,
    jumps: &JumpConnectionManager,
    known_hosts: &KnownHostService,
    profiles: &ProfileService,
    credentials: &CredentialService,
    reconnecting: bool,
) -> AppResult<ConnectResponse> {
    let profile = profiles.get(request.profile_id).await?;
    let (mut prepared, auth_method, route_owner) = match profile.connection_route.clone() {
        ConnectionRoute::Direct => {
            let (prepared, auth_method) = prepare_connection(
                request.profile_id,
                request.verification_attempt_id,
                request.credential,
                known_hosts,
                profiles,
                credentials,
            )
            .await?;
            (prepared, auth_method, None)
        }
        ConnectionRoute::JumpHost { profile_id } => {
            let jump = profiles.get(profile_id).await?;
            if jump.connection_route != ConnectionRoute::Direct {
                return Err(AppError::InvalidJumpHost);
            }
            let preparation_id = request
                .jump_preparation_id
                .ok_or(AppError::JumpPreparationExpired)?;
            let owner = jumps.consume(preparation_id, &profile, &jump).await?;
            let scope = jump_route_scope(&jump);
            let (prepared, auth_method) = prepare_connection_scoped(
                request.profile_id,
                request.verification_attempt_id,
                request.credential,
                &scope,
                known_hosts,
                profiles,
                credentials,
            )
            .await?;
            (prepared, auth_method, Some(owner))
        }
    };
    tracing::info!(profile_id = %prepared.profile_id, auth_method, reconnecting, "starting SSH connection");
    let connect_request = ConnectRequest {
        connection: prepared.request.take().ok_or(AppError::InvalidProfile)?,
        cols: request.cols,
        rows: request.rows,
    };
    let expected_fingerprint = std::mem::take(&mut prepared.expected_fingerprint);
    let session_id = if let Some(owner) = route_owner {
        sessions
            .connect_via(
                prepared.profile_id,
                owner,
                connect_request,
                expected_fingerprint,
                on_event,
            )
            .await?
    } else {
        sessions
            .connect(
                prepared.profile_id,
                connect_request,
                expected_fingerprint,
                on_event,
            )
            .await?
    };
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
pub async fn ssh_jump_prepare(
    request: PrepareJumpConnectionRequest,
    jumps: State<'_, JumpConnectionManager>,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<PrepareJumpConnectionResponse> {
    let target = profiles.get(request.target_profile_id).await?;
    let ConnectionRoute::JumpHost { profile_id } = target.connection_route else {
        return Err(AppError::InvalidJumpHost);
    };
    let jump = profiles.get(profile_id).await?;
    if jump.connection_route != ConnectionRoute::Direct {
        return Err(AppError::InvalidJumpHost);
    }
    policies
        .authorize(target.id, CloudPolicyAction::Connect)
        .await?;
    policies
        .authorize(jump.id, CloudPolicyAction::Connect)
        .await?;

    let (mut prepared_jump, auth_method) = prepare_connection(
        jump.id,
        request.jump_verification_attempt_id,
        request.jump_credential,
        &known_hosts,
        &profiles,
        &credentials,
    )
    .await?;
    tracing::info!(
        profile_id = %target.id,
        jump_profile_id = %jump.id,
        auth_method,
        "preparing jump-host SSH connection"
    );
    let connection = Arc::new(
        BackgroundConnection::connect(
            prepared_jump
                .request
                .take()
                .ok_or(AppError::InvalidProfile)?,
            std::mem::take(&mut prepared_jump.expected_fingerprint),
        )
        .await?,
    );
    let jump_credential_saved = remember_after_success(&mut prepared_jump, &credentials).await;
    let observed =
        SshService::scan_host_key_via(&connection.transport(), &target.host, target.port).await?;
    let scope = jump_route_scope(&jump);
    let target_verification = known_hosts
        .prepare_scoped(&scope, &target.host, target.port, observed)
        .await?;
    let preparation_id = jumps.store(&target, &jump, connection).await;
    Ok(PrepareJumpConnectionResponse {
        preparation_id,
        target_verification,
        jump_credential_saved,
    })
}

#[tauri::command]
pub async fn ssh_jump_cancel(
    request: CancelJumpConnectionRequest,
    jumps: State<'_, JumpConnectionManager>,
) -> AppResult<()> {
    jumps.cancel(request.preparation_id).await;
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn ssh_connect(
    request: ConnectProfileRequest,
    on_event: Channel<TerminalEvent>,
    sessions: State<'_, ServerSessionManager>,
    jumps: State<'_, JumpConnectionManager>,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<ConnectResponse> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    authorize_jump_if_present(request.profile_id, &profiles, &policies).await?;
    open_profile_session(
        request,
        on_event,
        &sessions,
        &jumps,
        &known_hosts,
        &profiles,
        &credentials,
        false,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn ssh_reconnect(
    request: ConnectProfileRequest,
    on_event: Channel<TerminalEvent>,
    sessions: State<'_, ServerSessionManager>,
    jumps: State<'_, JumpConnectionManager>,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<ConnectResponse> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    authorize_jump_if_present(request.profile_id, &profiles, &policies).await?;
    open_profile_session(
        request,
        on_event,
        &sessions,
        &jumps,
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
    jumps: State<'_, JumpConnectionManager>,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<TestConnectionResponse> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    authorize_jump_if_present(request.profile_id, &profiles, &policies).await?;
    let profile = profiles.get(request.profile_id).await?;
    let (mut prepared, auth_method, route_owner) = match profile.connection_route.clone() {
        ConnectionRoute::Direct => {
            let (prepared, auth_method) = prepare_connection(
                request.profile_id,
                request.verification_attempt_id,
                request.credential,
                &known_hosts,
                &profiles,
                &credentials,
            )
            .await?;
            (prepared, auth_method, None)
        }
        ConnectionRoute::JumpHost { profile_id } => {
            let jump = profiles.get(profile_id).await?;
            let owner = jumps
                .consume(
                    request
                        .jump_preparation_id
                        .ok_or(AppError::JumpPreparationExpired)?,
                    &profile,
                    &jump,
                )
                .await?;
            let (prepared, auth_method) = prepare_connection_scoped(
                request.profile_id,
                request.verification_attempt_id,
                request.credential,
                &jump_route_scope(&jump),
                &known_hosts,
                &profiles,
                &credentials,
            )
            .await?;
            (prepared, auth_method, Some(owner))
        }
    };
    tracing::info!(profile_id = %prepared.profile_id, auth_method, "testing SSH connection");
    let expected_fingerprint = std::mem::take(&mut prepared.expected_fingerprint);
    let connection = prepared.request.take().ok_or(AppError::InvalidProfile)?;
    if let Some(owner) = route_owner {
        SshService::test_connection_via(&owner.transport(), connection, expected_fingerprint)
            .await?;
    } else {
        SshService::test_connection(connection, expected_fingerprint).await?;
    }
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
    tunnels: State<'_, crate::tunnels::TunnelService>,
) -> AppResult<()> {
    let result = sessions.disconnect(request.session_id).await;
    tunnels.stop_session(request.session_id).await;
    result
}
