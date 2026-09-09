use crate::{
    cloud::{CloudPolicyAction, CloudPolicyService},
    credentials::CredentialService,
    domain::{AppError, AppResult, CredentialInput, ServerProfile, TestConnectionResponse},
    known_hosts::KnownHostService,
    profiles::ProfileService,
    ssh::{BackgroundConnection, ConnectionIdentity},
    tunnels::{SaveTunnelRequest, TunnelRule, TunnelService, TunnelView},
};
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TunnelRequest {
    id: Uuid,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartTunnelRequest {
    id: Uuid,
    profile_id: Uuid,
    verification_attempt_id: Uuid,
    credential: CredentialInput,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TunnelSessionRequest {
    session_id: Uuid,
}

#[tauri::command]
pub async fn tunnel_session_impact(
    request: TunnelSessionRequest,
    tunnels: State<'_, TunnelService>,
) -> AppResult<Vec<TunnelRule>> {
    Ok(tunnels.session_impact(request.session_id).await)
}

#[tauri::command]
pub async fn tunnel_list(tunnels: State<'_, TunnelService>) -> AppResult<Vec<TunnelView>> {
    tunnels.list().await
}
#[tauri::command]
pub async fn tunnel_save(
    request: SaveTunnelRequest,
    tunnels: State<'_, TunnelService>,
    profiles: State<'_, ProfileService>,
) -> AppResult<TunnelRule> {
    profiles.get(request.profile_id).await?;
    tunnels.save(request).await
}
#[tauri::command]
pub async fn tunnel_delete(
    request: TunnelRequest,
    tunnels: State<'_, TunnelService>,
) -> AppResult<()> {
    tunnels.delete(request.id).await
}
#[tauri::command]
pub async fn tunnel_stop(
    request: TunnelRequest,
    tunnels: State<'_, TunnelService>,
) -> AppResult<()> {
    tunnels.stop(request.id).await
}

async fn authorize(
    tunnels: &TunnelService,
    profiles: &ProfileService,
    policies: &CloudPolicyService,
    id: Uuid,
) -> AppResult<ServerProfile> {
    let rule = tunnels.rule(id).await?;
    let profile = profiles.get(rule.profile_id).await?;
    policies
        .authorize(rule.profile_id, CloudPolicyAction::Connect)
        .await?;
    policies
        .authorize(rule.profile_id, CloudPolicyAction::Operate)
        .await?;
    Ok(profile)
}

#[tauri::command]
pub async fn tunnel_start(
    request: TunnelRequest,
    tunnels: State<'_, TunnelService>,
    profiles: State<'_, ProfileService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<()> {
    let profile = authorize(&tunnels, &profiles, &policies, request.id).await?;
    tunnels
        .start_background(request.id, ConnectionIdentity::from(&profile), || async {
            Err(AppError::TunnelConnectionRequired)
        })
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn tunnel_connect_start(
    request: StartTunnelRequest,
    tunnels: State<'_, TunnelService>,
    profiles: State<'_, ProfileService>,
    policies: State<'_, CloudPolicyService>,
    known_hosts: State<'_, KnownHostService>,
    credentials: State<'_, CredentialService>,
) -> AppResult<TestConnectionResponse> {
    use super::connection::{prepare_connection, remember_after_success};
    let profile = authorize(&tunnels, &profiles, &policies, request.id).await?;
    if profile.id != request.profile_id {
        return Err(AppError::TunnelSessionMismatch);
    }
    let identity = ConnectionIdentity::from(&profile);
    let expected_identity = identity.clone();
    let result = tunnels
        .start_background(request.id, identity, || async {
            // Shared with terminal authentication; preparation is bounded and cancellable with this start.
            let (mut prepared, _) = prepare_connection(
                request.profile_id,
                request.verification_attempt_id,
                request.credential,
                &known_hosts,
                &profiles,
                &credentials,
            )
            .await?;
            if prepared.identity != expected_identity {
                return Err(AppError::TunnelSessionMismatch);
            }
            let connection = BackgroundConnection::connect(
                prepared.request.take().ok_or(AppError::InvalidProfile)?,
                std::mem::take(&mut prepared.expected_fingerprint),
            )
            .await?;
            let saved = remember_after_success(&mut prepared, &credentials).await;
            Ok((connection, saved))
        })
        .await;
    known_hosts.cancel(request.verification_attempt_id).await;
    result.map(|credential_saved| TestConnectionResponse { credential_saved })
}
#[tauri::command]
pub async fn tunnel_check(
    request: TunnelRequest,
    tunnels: State<'_, TunnelService>,
    profiles: State<'_, ProfileService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<()> {
    let profile = authorize(&tunnels, &profiles, &policies, request.id).await?;
    tunnels.check(request.id, profile.id).await
}
