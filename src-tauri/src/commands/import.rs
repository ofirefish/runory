use tauri::State;

use crate::cloud::CloudPolicyService;
use crate::credentials::CredentialService;
use crate::domain::AppResult;
use crate::groups::GroupService;
use crate::import::{
    apply_catalog_import, preview_catalog_import, CatalogImportApplyRequest,
    CatalogImportApplyResult, CatalogImportPreview, CatalogImportPreviewRequest,
};
use crate::profiles::ProfileService;

#[tauri::command]
pub async fn catalog_import_preview(
    request: CatalogImportPreviewRequest,
    profiles: State<'_, ProfileService>,
) -> AppResult<CatalogImportPreview> {
    let existing = profiles.list().await?;
    preview_catalog_import(request, &existing).await
}

#[tauri::command]
pub async fn catalog_import_apply(
    request: CatalogImportApplyRequest,
    groups: State<'_, GroupService>,
    profiles: State<'_, ProfileService>,
    credentials: State<'_, CredentialService>,
    policies: State<'_, CloudPolicyService>,
) -> AppResult<CatalogImportApplyResult> {
    let result = apply_catalog_import(request, &groups, &profiles, &credentials).await?;
    if result.profiles_created > 0 || result.profiles_updated > 0 {
        if let Ok(list) = profiles.list().await {
            if let Err(error) = policies
                .reconcile_profiles(list.into_iter().map(|profile| profile.id).collect())
                .await
            {
                tracing::warn!(
                    error_code = error.code(),
                    "could not reconcile cloud policy after catalog import"
                );
            }
        }
    }
    Ok(result)
}
