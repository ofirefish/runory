use super::*;
use crate::credentials::{CredentialVault, PortableCredentialVault};
use std::{path::Path, sync::Arc};

pub(super) fn credentials(directory: &Path) -> CredentialService {
    let vault = Arc::new(PortableCredentialVault::new(
        directory.join("test-vault.bin"),
    ));
    vault
        .unlock(Zeroizing::new("test-vault-master-password".into()))
        .expect("unlock test vault");
    CredentialService::new(vault)
}

fn request(name: &str, key: Option<&str>) -> ModelConfigureRequest {
    ModelConfigureRequest {
        kind: ModelProviderKind::OpenAiCompatible,
        name: name.into(),
        base_url: "https://api.example.com/v1".into(),
        model: "test-model".into(),
        max_context_tokens: 8192,
        api_key: key.map(str::to_owned),
    }
}

#[tokio::test]
async fn additional_providers_round_trip_through_ipc_and_saved_profiles() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("models.json");
    let credentials = credentials(directory.path());
    let gateway = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials.clone());
    for kind in ["glm", "deep-seek", "qwen", "kimi", "minimax"] {
        let request: ModelConfigureRequest = serde_json::from_value(json!({
            "kind": kind, "name": kind, "baseUrl": "", "model": "test-model",
            "maxContextTokens": 8192, "apiKey": "test-provider-key"
        }))
        .expect("IPC request");
        gateway
            .save_profile(None, request)
            .await
            .expect("save provider");
    }
    let reloaded = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials);
    reloaded.load().await.expect("load");
    let profiles = reloaded.profiles().await.expect("profiles");
    assert_eq!(profiles.len(), 5);
    assert_eq!(profiles.iter().filter(|p| p.active).count(), 1);
    for profile in profiles {
        reloaded
            .activate_profile(profile.id)
            .await
            .expect("activate provider");
        assert!(profile.status.base_url.starts_with("https://"));
        let ipc = serde_json::to_value(&profile).expect("IPC status");
        assert_eq!(ipc["kind"], profile.status.name);
    }
}

#[tokio::test]
async fn failed_metadata_write_preserves_the_previous_profile_and_key() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("models.json");
    let backup = directory.path().join("previous.json");
    let gateway = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials(directory.path()));
    let id = gateway
        .save_profile(None, request("Original", Some("original-test-key")))
        .await
        .expect("save")[0]
        .id;
    tokio::fs::rename(&path, &backup)
        .await
        .expect("backup fixture");
    tokio::fs::create_dir(&path)
        .await
        .expect("block atomic replacement");
    assert!(gateway
        .save_profile(
            Some(id),
            request("Replacement", Some("replacement-test-key"))
        )
        .await
        .is_err());
    let profiles = gateway.profiles().await.expect("profiles after failure");
    assert_eq!(profiles[0].status.name, "Original");
    assert!(profiles[0].active);
    assert_eq!(
        gateway
            .repository
            .get(id)
            .await
            .expect("original credential")
            .api_key
            .as_ref()
            .expect("key")
            .as_str(),
        "original-test-key"
    );
}

#[tokio::test]
async fn profiles_keep_independent_keys_and_exactly_one_active_across_reload() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("models.json");
    let credentials = credentials(directory.path());
    let gateway = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials.clone());
    let first = gateway
        .save_profile(None, request("First", Some("first-test-key")))
        .await
        .expect("first")[0]
        .id;
    let profiles = gateway
        .save_profile(None, request("Second", Some("second-test-key")))
        .await
        .expect("second");
    let second = profiles[1].id;
    assert_eq!(profiles.len(), 2);
    assert!(profiles[0].active);
    assert!(!profiles[1].active);
    assert!(gateway
        .save_profile(None, request("Missing key", None))
        .await
        .is_err());
    gateway.activate_profile(second).await.expect("activate");
    assert_eq!(
        gateway
            .repository
            .load_or_default()
            .await
            .expect("active")
            .api_key
            .as_ref()
            .expect("key")
            .as_str(),
        "second-test-key"
    );
    gateway
        .save_profile(Some(first), request("Renamed", None))
        .await
        .expect("edit inactive");
    assert!(gateway.profiles().await.expect("profiles")[1].active);
    let reloaded = ModelGateway::at_path(&path)
        .expect("reload")
        .with_credentials(credentials);
    reloaded.load().await.expect("load");
    let profiles = reloaded.profiles().await.expect("profiles");
    assert_eq!(profiles.iter().filter(|p| p.active).count(), 1);
    assert!(profiles[1].active);
    assert_eq!(profiles[0].status.name, "Renamed");
    reloaded.activate_profile(first).await.expect("switch back");
    assert_eq!(
        reloaded
            .repository
            .load_or_default()
            .await
            .expect("active")
            .api_key
            .as_ref()
            .expect("key")
            .as_str(),
        "first-test-key"
    );
    let json = tokio::fs::read_to_string(&path).await.expect("JSON");
    assert!(!json.contains("first-test-key"));
    assert!(!json.contains("second-test-key"));
    let ipc = serde_json::to_string(&profiles).expect("IPC");
    assert!(!ipc.contains("test-key"));
    assert!(!ipc.contains("credentialId"));
}

#[tokio::test]
async fn editing_endpoint_never_reuses_saved_key_and_active_deletion_is_blocked() {
    let directory = tempfile::tempdir().expect("directory");
    let gateway = ModelGateway::at_path(directory.path().join("models.json"))
        .expect("gateway")
        .with_credentials(credentials(directory.path()));
    let id = gateway
        .save_profile(None, request("First", Some("first-test-key")))
        .await
        .expect("add")[0]
        .id;
    let mut changed = request("Changed", None);
    changed.base_url = "https://other.example.com/v1".into();
    assert!(gateway.save_profile(Some(id), changed).await.is_err());
    assert!(gateway.remove_profile(id).await.is_err());
    assert!(gateway
        .activate_profile(uuid::Uuid::new_v4())
        .await
        .is_err());
    assert_eq!(
        gateway.profiles().await.expect("profiles")[0].status.name,
        "First"
    );
    let second = gateway
        .save_profile(None, request("Second", Some("second-test-key")))
        .await
        .expect("second")[1]
        .id;
    gateway
        .remove_profile(second)
        .await
        .expect("remove inactive");
    assert_eq!(gateway.profiles().await.expect("profiles").len(), 1);
}

#[tokio::test]
async fn legacy_configuration_migrates_on_save_without_exposing_or_losing_credentials() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("models.json");
    let config = configured_model(
        request("Legacy", Some("legacy-secret-key")),
        &ModelProviderConfig::default(),
    )
    .expect("config");
    JsonRepository::new(&path)
        .save_atomic(&config)
        .await
        .expect("legacy fixture");
    let gateway = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials(directory.path()));
    gateway.load().await.expect("load legacy");
    let old_id = gateway.profiles().await.expect("profiles")[0].id;
    gateway
        .save_profile(None, request("New", Some("new-secret-key")))
        .await
        .expect("save");
    assert_eq!(
        gateway
            .repository
            .get(old_id)
            .await
            .expect("legacy key")
            .api_key
            .as_ref()
            .expect("key")
            .as_str(),
        "legacy-secret-key"
    );
    let json = tokio::fs::read_to_string(&path).await.expect("JSON");
    assert!(!json.contains("legacy-secret-key"));
    assert!(!json.contains("new-secret-key"));
}

#[tokio::test]
async fn locked_vault_keeps_list_available_and_never_overwrites_saved_credentials() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("models.json");
    let credentials = credentials(directory.path());
    let gateway = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials.clone());
    let id = gateway
        .save_profile(None, request("Saved", Some("stored-test-key")))
        .await
        .expect("save")[0]
        .id;
    let before = tokio::fs::read(&path).await.expect("before");
    credentials.lock().await.expect("lock");
    let reloaded = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials.clone());
    reloaded.load().await.expect("startup with locked vault");
    assert!(
        reloaded.profiles().await.expect("list")[0]
            .status
            .api_key_configured
    );
    assert!(reloaded.activate_profile(id).await.is_err());
    assert!(reloaded
        .save_profile(Some(id), request("Changed", None))
        .await
        .is_err());
    assert_eq!(tokio::fs::read(&path).await.expect("after"), before);
    credentials
        .unlock("test-vault-master-password".into())
        .await
        .expect("unlock");
    reloaded
        .activate_profile(id)
        .await
        .expect("activate after unlock");
}

#[tokio::test]
async fn oauth_addition_preserves_active_profile_and_tokens_are_only_in_vault() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("models.json");
    let gateway = ModelGateway::at_path(&path)
        .expect("gateway")
        .with_credentials(credentials(directory.path()));
    gateway
        .save_profile(None, request("API", Some("stored-test-key")))
        .await
        .expect("save");
    gateway
        .persist_chatgpt_tokens(ChatGptTokenSet {
            access_token: "access-test-secret".into(),
            refresh_token: "refresh-test-secret".into(),
            expires_at_epoch_ms: None,
        })
        .await
        .expect("oauth");
    let profiles = gateway.profiles().await.expect("profiles");
    assert_eq!(profiles.len(), 2);
    assert!(profiles[0].active);
    assert_eq!(profiles[1].status.auth_mode, ModelAuthMode::Oauth);
    gateway
        .activate_profile(profiles[1].id)
        .await
        .expect("switch");
    let json = tokio::fs::read_to_string(&path).await.expect("JSON");
    assert!(!json.contains("access-test-secret"));
    assert!(!json.contains("refresh-test-secret"));
}
