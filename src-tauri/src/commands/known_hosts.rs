use tauri::State;

use crate::cloud::{CloudPolicyAction, CloudPolicyService};
use crate::domain::{
    AppError, AppResult, CancelHostVerificationRequest, ConnectionRoute, HostVerification,
    KnownHost, PrepareHostVerificationRequest, RemoveKnownHostRequest, TrustHostRequest,
};
use crate::known_hosts::KnownHostService;
use crate::profiles::ProfileService;
use crate::ssh::SshService;

#[tauri::command]
pub async fn known_host_prepare(
    request: PrepareHostVerificationRequest,
    known_hosts: State<'_, KnownHostService>,
    profiles: State<'_, ProfileService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<HostVerification> {
    policies
        .authorize(request.profile_id, CloudPolicyAction::Connect)
        .await?;
    let profile = profiles.get(request.profile_id).await?;
    if profile.connection_route != ConnectionRoute::Direct {
        return Err(AppError::InvalidJumpHost);
    }
    let observed = SshService::scan_host_key(&profile.host, profile.port).await?;
    known_hosts
        .prepare(&profile.host, profile.port, observed)
        .await
}

#[tauri::command]
pub async fn known_host_trust(
    request: TrustHostRequest,
    known_hosts: State<'_, KnownHostService>,
) -> AppResult<()> {
    known_hosts
        .trust(request.attempt_id, request.remember)
        .await
}

#[tauri::command]
pub async fn known_host_cancel(
    request: CancelHostVerificationRequest,
    known_hosts: State<'_, KnownHostService>,
) -> AppResult<()> {
    known_hosts.cancel(request.attempt_id).await;
    Ok(())
}

#[tauri::command]
pub async fn known_host_list(
    known_hosts: State<'_, KnownHostService>,
) -> AppResult<Vec<KnownHost>> {
    known_hosts.list().await
}

#[tauri::command]
pub async fn known_host_remove(
    request: RemoveKnownHostRequest,
    known_hosts: State<'_, KnownHostService>,
) -> AppResult<()> {
    known_hosts
        .remove_scoped(&request.route_scope, &request.host, request.port)
        .await
}
