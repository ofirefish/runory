use std::sync::Arc;
use uuid::Uuid;

use crate::connection::{
    default_bastion_registry, ConnectionContext, ConnectionRequest, ConnectionResolver,
    ResolvedConnection, SessionIntent,
};
use crate::domain::ConnectionRoute;

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
                    provider: "jumpserver".into(),
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
            provider, asset_id, ..
        } => {
            assert_eq!(provider, "jumpserver");
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
