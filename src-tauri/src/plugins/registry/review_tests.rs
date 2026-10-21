use super::review::ConfirmPluginInstall;
use crate::plugins::error::PluginError;
use crate::plugins::test_support::{detect_only_yaml, write_plugin_zip, PluginTestHarness};

fn package(harness: &PluginTestHarness, version: &str, permissions: &[&str]) -> std::path::PathBuf {
    let path = harness
        ._root
        .path()
        .join(format!("{version}.runory-plugin"));
    write_plugin_zip(
        &path,
        "runory.review",
        version,
        permissions,
        detect_only_yaml(),
        "detect.only",
    );
    path
}

fn confirm(review_id: &str, permissions: &[&str]) -> ConfirmPluginInstall {
    ConfirmPluginInstall {
        review_id: review_id.into(),
        granted_permissions: permissions.iter().map(|p| (*p).into()).collect(),
    }
}

#[tokio::test]
async fn review_installs_snapshot_and_never_grants_omitted_permissions() {
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &["host.read", "ssh.exec"]);
    let review = harness
        .registry
        .prepare_install(&path, true, None)
        .await
        .expect("review");
    // Replacing the user-selected file cannot replace the reviewed bytes.
    write_plugin_zip(
        &path,
        "runory.attacker",
        "2.0.0",
        &["ssh.exec.sudo"],
        detect_only_yaml(),
        "detect.only",
    );
    let installed = harness
        .registry
        .confirm_install(confirm(&review.review_id, &["host.read"]), true)
        .await
        .expect("install snapshot");
    assert_eq!(installed.id, "runory.review");
    assert_eq!(installed.version, "1.0.0");
    assert!(installed
        .permissions
        .iter()
        .any(|p| p.permission == "ssh.exec" && !p.granted));
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &["host.read"]), true)
            .await,
        Err(PluginError::PlanStale)
    ));
}

#[tokio::test]
async fn unsigned_review_and_confirmation_obey_current_developer_setting() {
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &[]);
    assert!(matches!(
        harness.registry.prepare_install(&path, false, None).await,
        Err(PluginError::SignatureMissing)
    ));
    let review = harness
        .registry
        .prepare_install(&path, true, None)
        .await
        .expect("review");
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &[]), false)
            .await,
        Err(PluginError::SignatureMissing)
    ));
    assert!(harness
        .registry
        .list_installed()
        .await
        .expect("list")
        .is_empty());
}

#[tokio::test]
async fn package_identity_must_match_requested_marketplace_version() {
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &[]);
    for expected in [("other.plugin", "1.0.0"), ("runory.review", "2.0.0")] {
        assert!(matches!(
            harness
                .registry
                .prepare_install(&path, true, Some(expected))
                .await,
            Err(PluginError::InvalidPackage)
        ));
    }
}

#[tokio::test]
async fn cancelled_review_is_not_reusable() {
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &[]);
    let review = harness
        .registry
        .prepare_install(&path, true, None)
        .await
        .expect("review");
    harness
        .registry
        .discard_install_review(&review.review_id)
        .expect("discard");
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &[]), true)
            .await,
        Err(PluginError::PlanStale)
    ));
}

#[tokio::test]
async fn update_retains_disabled_state_and_reports_permission_expansion() {
    let harness = PluginTestHarness::new();
    let first = package(&harness, "1.0.0", &["host.read"]);
    harness.install_zip(&first).await;
    harness
        .registry
        .set_enabled("runory.review", false)
        .await
        .expect("disable");
    let next = package(&harness, "1.1.0", &["host.read", "ssh.exec"]);
    let review = harness
        .registry
        .prepare_install(&next, true, None)
        .await
        .expect("review");
    assert_eq!(review.installed_version.as_deref(), Some("1.0.0"));
    assert_eq!(review.permission_additions, vec!["ssh.exec"]);
    let installed = harness
        .registry
        .confirm_install(confirm(&review.review_id, &["host.read"]), true)
        .await
        .expect("update");
    assert!(!installed.enabled);
    assert_eq!(
        harness
            .registry
            .repository()
            .list_installed_versions("runory.review")
            .expect("versions")
            .len(),
        2
    );
}

#[tokio::test]
async fn state_change_invalidates_review_without_changing_installed_version() {
    let harness = PluginTestHarness::new();
    harness
        .install_zip(&package(&harness, "1.0.0", &["host.read"]))
        .await;
    let review = harness
        .registry
        .prepare_install(&package(&harness, "1.1.0", &["host.read"]), true, None)
        .await
        .expect("review");
    harness
        .registry
        .set_enabled("runory.review", false)
        .await
        .expect("disable");
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &["host.read"]), true)
            .await,
        Err(PluginError::PlanStale)
    ));
    assert_eq!(
        harness
            .registry
            .get_plugin("runory.review")
            .await
            .expect("get")
            .expect("installed")
            .version,
        "1.0.0"
    );
}

#[tokio::test]
async fn toggling_back_to_original_state_does_not_restore_consent() {
    let harness = PluginTestHarness::new();
    harness.install_zip(&package(&harness, "1.0.0", &[])).await;
    let review = harness
        .registry
        .prepare_install(&package(&harness, "1.1.0", &[]), true, None)
        .await
        .expect("review");
    harness
        .registry
        .set_enabled("runory.review", false)
        .await
        .expect("disable");
    harness
        .registry
        .set_enabled("runory.review", true)
        .await
        .expect("enable");
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &[]), true)
            .await,
        Err(PluginError::PlanStale)
    ));
}

#[tokio::test]
async fn revoked_package_cannot_be_confirmed_or_enabled() {
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &[]);
    let review = harness
        .registry
        .prepare_install(&path, true, None)
        .await
        .expect("review");
    harness
        .registry
        .repository()
        .upsert_revocation("plugin", "runory.review", None, None)
        .expect("revoke");
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &[]), true)
            .await,
        Err(PluginError::PluginRevoked)
    ));
}

#[tokio::test]
async fn concurrent_confirmation_consumes_only_once() {
    let harness = PluginTestHarness::new();
    let review = harness
        .registry
        .prepare_install(&package(&harness, "1.0.0", &[]), true, None)
        .await
        .expect("review");
    let (left, right) = tokio::join!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &[]), true),
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &[]), true),
    );
    assert!(left.is_ok());
    assert!(matches!(right, Err(PluginError::PlanStale)));
}

#[tokio::test]
async fn undeclared_grants_fail_closed() {
    let harness = PluginTestHarness::new();
    let review = harness
        .registry
        .prepare_install(&package(&harness, "1.0.0", &["host.read"]), true, None)
        .await
        .expect("review");
    assert!(matches!(
        harness
            .registry
            .confirm_install(confirm(&review.review_id, &["ssh.exec"]), true)
            .await,
        Err(PluginError::PermissionDenied(_))
    ));
    assert!(harness
        .registry
        .list_installed()
        .await
        .expect("list")
        .is_empty());
}

#[test]
fn frontend_cannot_supply_developer_mode_or_omit_explicit_grants() {
    for request in [
        serde_json::json!({"reviewId":"x"}),
        serde_json::json!({"reviewId":"x", "grantedPermissions":[], "allowUnsigned":true}),
    ] {
        assert!(serde_json::from_value::<ConfirmPluginInstall>(request).is_err());
    }
}

#[tokio::test]
async fn trusted_signer_cannot_impersonate_another_manifest_publisher() {
    use crate::plugins::signing::keys::{generate_keypair, load_private_key, save_keypair};
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &[]);
    let key = generate_keypair("community", "test-key").expect("key");
    let private_path = harness._root.path().join("test-key.json");
    save_keypair(&key, &private_path, None, None).expect("save");
    let loaded = load_private_key(&private_path, None).expect("load");
    crate::plugins::signing::verify::sign_package(&path, &loaded).expect("sign");
    let repo = harness.registry.repository();
    repo.upsert_publisher("community", "Community", "community")
        .expect("publisher");
    repo.upsert_publisher_key("test-key", "community", &key.public_key, "active")
        .expect("trust");
    assert!(matches!(
        harness.registry.prepare_install(&path, false, None).await,
        Err(PluginError::PublisherMismatch)
    ));
}

#[tokio::test]
async fn key_revocation_blocks_installed_workflows_and_reenable() {
    use crate::plugins::signing::keys::{generate_keypair, load_private_key, save_keypair};
    let harness = PluginTestHarness::new();
    let path = package(&harness, "1.0.0", &[]);
    let key = generate_keypair("runory", "review-key").expect("key");
    let private_path = harness._root.path().join("review-key.json");
    save_keypair(&key, &private_path, None, None).expect("save");
    let loaded = load_private_key(&private_path, None).expect("load");
    crate::plugins::signing::verify::sign_package(&path, &loaded).expect("sign");
    let repo = harness.registry.repository();
    repo.upsert_publisher_key("review-key", "runory", &key.public_key, "active")
        .expect("trust");
    harness.install_zip(&path).await;
    repo.upsert_publisher_key("review-key", "runory", &key.public_key, "revoked")
        .expect("revoke");
    assert!(matches!(
        harness.registry.require_enabled("runory.review"),
        Err(PluginError::SigningKeyRevoked)
    ));
    harness
        .registry
        .set_enabled("runory.review", false)
        .await
        .expect("disable");
    assert!(matches!(
        harness.registry.set_enabled("runory.review", true).await,
        Err(PluginError::SigningKeyRevoked)
    ));
}
