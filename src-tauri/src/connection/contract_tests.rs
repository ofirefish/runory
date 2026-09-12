use std::sync::Arc;
use uuid::Uuid;

use crate::connection::{
    default_bastion_registry, AssetQuery, AuthChallengeResponse, AuthStepResult,
    BastionConnectOptions, BastionConnectRequest, BastionContext, BastionCredential,
    BastionEndpoint, BastionError, BastionPorts, BastionProtocol, BastionProvider, BastionTimeouts,
    ConnectionContext, ConnectionRequest, ConnectionResolver, MockBastionProvider,
    ResolvedConnection, SessionIntent,
};
use crate::domain::ConnectionRoute;

fn mock_endpoint() -> BastionEndpoint {
    BastionEndpoint {
        id: Uuid::new_v4(),
        provider: "mock".into(),
        name: "Mock Corp Bastion".into(),
        host: "bastion.mock.local".into(),
        ports: BastionPorts {
            api: Some(443),
            ssh: Some(2222),
            web: Some(443),
        },
        tls: None,
        provider_config: serde_json::json!({}),
    }
}

fn password_credential() -> BastionCredential {
    BastionCredential::password("alice", "secret")
}

async fn authenticate(provider: &MockBastionProvider) -> crate::connection::AuthSession {
    let ctx = BastionContext {
        endpoint: mock_endpoint(),
        timeouts: BastionTimeouts::default(),
    };
    let step = provider
        .start_auth(&ctx, &password_credential())
        .await
        .expect("start_auth");
    let AuthStepResult::Challenge {
        pending,
        challenge,
    } = step
    else {
        panic!("expected totp challenge");
    };
    let challenge_id = challenge.id().to_string();
    let step = provider
        .continue_auth(
            &pending,
            AuthChallengeResponse::Totp {
                id: challenge_id,
                code: "123456".into(),
            },
        )
        .await
        .expect("continue_auth");
    match step {
        AuthStepResult::Authenticated(session) => session,
        other => panic!("expected authenticated session, got {other:?}"),
    }
}

#[tokio::test]
async fn mock_provider_contract_probe_auth_assets_connect_disconnect() {
    let provider = MockBastionProvider::new();
    assert_eq!(provider.id(), "mock");
    assert!(provider
        .capabilities()
        .contains(crate::connection::BastionCapabilities::MFA));

    let endpoint = mock_endpoint();
    let probe = provider.probe(&endpoint).await.expect("probe");
    assert!(probe.reachable);
    assert_eq!(probe.api_version.as_deref(), Some("1.0.0"));

    let session = authenticate(&provider).await;
    assert_eq!(session.principal.username, "alice");

    let page = provider
        .list_assets(
            &session,
            AssetQuery {
                search: Some("prod-db".into()),
                node: None,
                protocol: Some(BastionProtocol::Ssh),
                page: 0,
                page_size: 10,
            },
        )
        .await
        .expect("list_assets");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].remote_id, "asset-prod-db-01");
    assert!(!page.has_more);

    let accounts = provider
        .list_accounts(&session, &page.items[0])
        .await
        .expect("list_accounts");
    assert!(accounts
        .iter()
        .any(|account| account.username == "root" && account.secret_managed_by_bastion));

    let connection = provider
        .connect(
            &session,
            BastionConnectRequest {
                asset: page.items[0].clone(),
                account: accounts[0].clone(),
                protocol: BastionProtocol::Ssh,
                terminal: Some(crate::connection::TerminalOptions {
                    term: "xterm-256color".into(),
                    cols: 120,
                    rows: 40,
                }),
                options: BastionConnectOptions::default(),
            },
        )
        .await
        .expect("connect");
    assert_eq!(connection.metadata().provider, "mock");
    assert_eq!(connection.metadata().asset_id, "asset-prod-db-01");
    assert!(connection.metadata().recording);

    provider.disconnect(&connection).await.expect("disconnect");
}

#[tokio::test]
async fn mock_provider_rejects_invalid_totp_without_retryable_flag() {
    let provider = MockBastionProvider::new();
    let ctx = BastionContext {
        endpoint: mock_endpoint(),
        timeouts: BastionTimeouts::default(),
    };
    let step = provider
        .start_auth(&ctx, &password_credential())
        .await
        .expect("start_auth");
    let AuthStepResult::Challenge {
        pending,
        challenge,
    } = step
    else {
        panic!("expected challenge");
    };
    let err = provider
        .continue_auth(
            &pending,
            AuthChallengeResponse::Totp {
                id: challenge.id().into(),
                code: "000000".into(),
            },
        )
        .await
        .expect_err("bad totp");
    assert_eq!(err, BastionError::AuthenticationFailed);
    assert!(!err.is_retryable());
}

#[tokio::test]
async fn mock_provider_paginates_assets() {
    let provider = MockBastionProvider::new();
    let session = authenticate(&provider).await;
    let page0 = provider
        .list_assets(
            &session,
            AssetQuery {
                search: None,
                node: None,
                protocol: None,
                page: 0,
                page_size: 2,
            },
        )
        .await
        .expect("page0");
    assert_eq!(page0.items.len(), 2);
    assert!(page0.has_more);
    let page1 = provider
        .list_assets(
            &session,
            AssetQuery {
                search: None,
                node: None,
                protocol: None,
                page: 1,
                page_size: 2,
            },
        )
        .await
        .expect("page1");
    assert_eq!(page1.items.len(), 1);
    assert!(!page1.has_more);
}

#[tokio::test]
async fn mock_provider_blocks_port_forward_capability() {
    let provider = MockBastionProvider::new();
    let session = authenticate(&provider).await;
    let assets = provider
        .list_assets(
            &session,
            AssetQuery {
                search: None,
                node: None,
                protocol: None,
                page: 0,
                page_size: 1,
            },
        )
        .await
        .expect("assets");
    let accounts = provider
        .list_accounts(&session, &assets.items[0])
        .await
        .expect("accounts");
    let err = provider
        .connect(
            &session,
            BastionConnectRequest {
                asset: assets.items[0].clone(),
                account: accounts[0].clone(),
                protocol: BastionProtocol::Ssh,
                terminal: None,
                options: BastionConnectOptions {
                    request_port_forward: true,
                    ..BastionConnectOptions::default()
                },
            },
        )
        .await
        .expect_err("port forward");
    assert_eq!(err, BastionError::CapabilityUnavailable);
}

#[tokio::test]
async fn connection_resolver_routes_bastion_through_registry() {
    let registry = Arc::new(default_bastion_registry());
    let resolver = ConnectionResolver::new(registry);
    let bastion_id = Uuid::new_v4();
    let ctx = ConnectionContext {
        profile_id: Uuid::new_v4(),
        intent: SessionIntent::Terminal,
    };
    let resolved = resolver
        .resolve(
            &ctx,
            ConnectionRequest {
                route: ConnectionRoute::Bastion {
                    bastion_id,
                    provider: "mock".into(),
                    asset_id: "asset-prod-db-01".into(),
                    account_id: Some("root".into()),
                    api_base_url: None,
                    org_id: None,
                    cli_path: None,
                    cluster_name: None,
                    insecure: false,
                },
                intent: SessionIntent::Terminal,
            },
        )
        .await
        .expect("resolve bastion");
    match resolved {
        ResolvedConnection::Bastion {
            provider,
            asset_id,
            ..
        } => {
            assert_eq!(provider, "mock");
            assert_eq!(asset_id, "asset-prod-db-01");
        }
        other => panic!("unexpected resolution: {other:?}"),
    }
}

#[tokio::test]
async fn connection_resolver_rejects_unknown_provider() {
    let registry = Arc::new(default_bastion_registry());
    let resolver = ConnectionResolver::new(registry);
    let err = resolver
        .resolve(
            &ConnectionContext {
                profile_id: Uuid::new_v4(),
                intent: SessionIntent::Terminal,
            },
            ConnectionRequest {
                route: ConnectionRoute::Bastion {
                    bastion_id: Uuid::new_v4(),
                    provider: "unknown-vendor".into(),
                    asset_id: "x".into(),
                    account_id: None,
                    api_base_url: None,
                    org_id: None,
                    cli_path: None,
                    cluster_name: None,
                    insecure: false,
                },
                intent: SessionIntent::Terminal,
            },
        )
        .await
        .expect_err("unknown provider");
    assert_eq!(err.code(), "BASTION_PROVIDER_NOT_FOUND");
}

#[tokio::test]
async fn connection_resolver_keeps_direct_on_legacy_ssh_path() {
    let registry = Arc::new(default_bastion_registry());
    let resolver = ConnectionResolver::new(registry);
    let resolved = resolver
        .resolve(
            &ConnectionContext {
                profile_id: Uuid::new_v4(),
                intent: SessionIntent::AgentTool,
            },
            ConnectionRequest {
                route: ConnectionRoute::Direct,
                intent: SessionIntent::AgentTool,
            },
        )
        .await
        .expect("direct");
    match resolved {
        ResolvedConnection::LegacySsh {
            route: ConnectionRoute::Direct,
            transport: crate::connection::TransportPlan::Tcp { .. },
        } => {}
        other => panic!("unexpected resolution: {other:?}"),
    }
}

#[tokio::test]
async fn connection_resolver_jump_host_carries_ssh_jump_transport_plan() {
    let registry = Arc::new(default_bastion_registry());
    let resolver = ConnectionResolver::new(registry);
    let jump_id = Uuid::new_v4();
    let resolved = resolver
        .resolve(
            &ConnectionContext {
                profile_id: Uuid::new_v4(),
                intent: SessionIntent::Terminal,
            },
            ConnectionRequest {
                route: ConnectionRoute::JumpHost {
                    profile_id: jump_id,
                },
                intent: SessionIntent::Terminal,
            },
        )
        .await
        .expect("jump");
    match resolved {
        ResolvedConnection::LegacySsh {
            route: ConnectionRoute::JumpHost { profile_id },
            transport: crate::connection::TransportPlan::SshJump { .. },
        } => assert_eq!(profile_id, jump_id),
        other => panic!("unexpected resolution: {other:?}"),
    }
}

#[tokio::test]
async fn transport_factory_opens_tcp_against_local_listener() {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let accept = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        use tokio::io::AsyncWriteExt;
        let _ = socket.write_all(b"ok").await;
    });

    let plan = crate::connection::TransportPlan::Tcp {
        host: "127.0.0.1".into(),
        port,
    };
    let opened = crate::connection::TransportFactory::open(
        &plan,
        &crate::connection::TransportContext::with_timeout(5),
    )
    .await
    .expect("open tcp");
    assert!(opened.cleanup.is_none());
    accept.await.expect("accept task");
}
