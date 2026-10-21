//! Third-party host catalog importers.
//!
//! Parsers may hold plaintext passwords only in Rust memory during apply, then
//! write them to CredentialVault. Preview IPC never includes secrets.

mod common;
mod mobaxterm;
mod openssh;
mod securecrt;
mod terminus;
mod xshell;

pub use common::{
    CatalogImportApplyRequest, CatalogImportApplyResult, CatalogImportPreview,
    CatalogImportPreviewRequest, ImportSource,
};

use crate::credentials::CredentialService;
use crate::domain::{AppResult, ServerProfile};
use crate::groups::GroupService;
use crate::profiles::ProfileService;

pub async fn preview_catalog_import(
    request: CatalogImportPreviewRequest,
    existing: &[ServerProfile],
) -> AppResult<CatalogImportPreview> {
    match request.source {
        ImportSource::Terminus => terminus::preview_terminus_import(&request.path, existing).await,
        ImportSource::Openssh => openssh::preview_openssh_import(&request.path, existing).await,
        ImportSource::Xshell => xshell::preview_xshell_import(&request.path, existing).await,
        ImportSource::Securecrt => {
            securecrt::preview_securecrt_import(&request.path, existing).await
        }
        ImportSource::Mobaxterm => {
            mobaxterm::preview_mobaxterm_import(&request.path, existing).await
        }
    }
}

pub async fn apply_catalog_import(
    request: CatalogImportApplyRequest,
    groups: &GroupService,
    profiles: &ProfileService,
    credentials: &CredentialService,
) -> AppResult<CatalogImportApplyResult> {
    match request.source {
        ImportSource::Terminus => {
            terminus::apply_terminus_import(
                &request.path,
                &request.selected_keys,
                groups,
                profiles,
                credentials,
            )
            .await
        }
        ImportSource::Openssh => {
            openssh::apply_openssh_import(
                &request.path,
                &request.selected_keys,
                groups,
                profiles,
                credentials,
            )
            .await
        }
        ImportSource::Xshell => {
            xshell::apply_xshell_import(
                &request.path,
                &request.selected_keys,
                groups,
                profiles,
                credentials,
            )
            .await
        }
        ImportSource::Securecrt => {
            securecrt::apply_securecrt_import(
                &request.path,
                &request.selected_keys,
                groups,
                profiles,
                credentials,
            )
            .await
        }
        ImportSource::Mobaxterm => {
            mobaxterm::apply_mobaxterm_import(
                &request.path,
                &request.selected_keys,
                groups,
                profiles,
                credentials,
            )
            .await
        }
    }
}
