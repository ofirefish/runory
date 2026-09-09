use super::*;
use crate::{
    domain::{
        AuthMethod, ConnectRequest, ConnectionRoute, ServerProfile, SshAuthentication,
        SshConnectionRequest,
    },
    ssh::{ServerSessionManager, SshService},
    tunnels::SaveTunnelRequest,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

fn profile() -> ServerProfile {
    ServerProfile {
        id: Uuid::new_v4(),
        name: "Forwarding fixture".into(),
        host: "127.0.0.1".into(),
        port: 2226,
        username: "runory".into(),
        auth_method: AuthMethod::Password,
        key_source: None,
        connection_route: ConnectionRoute::Direct,
        group_id: None,
        sort_order: 0,
        created_at: String::new(),
        updated_at: String::new(),
        last_connected_at: None,
        os_distribution: None,
    }
}

async fn rule(service: &TunnelService, profile: &ServerProfile, port: u16) -> TunnelRule {
    service
        .save(SaveTunnelRequest {
            id: None,
            name: "Private HTTP".into(),
            profile_id: profile.id,
            target_host: "tunnel-target".into(),
            target_port: 8080,
            local_port: port,
        })
        .await
        .expect("save")
}

fn credentials(port: u16, password: &str) -> SshConnectionRequest {
    SshConnectionRequest {
        host: "127.0.0.1".into(),
        port,
        username: "runory".into(),
        authentication: SshAuthentication::Password {
            password: zeroize::Zeroizing::new(password.into()),
        },
    }
}

async fn connect() -> AppResult<(BackgroundConnection, bool)> {
    let key = SshService::scan_host_key("127.0.0.1", 2226).await?;
    BackgroundConnection::connect(credentials(2226, "runory-spike"), key.fingerprint)
        .await
        .map(|c| (c, true))
}

async fn port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind")
        .local_addr()
        .expect("address")
        .port()
}

async fn fetch(port: u16) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect local");
        stream
            .write_all(b"GET / HTTP/1.0\r\nHost: tunnel-target\r\n\r\n")
            .await
            .expect("write");
        stream.shutdown().await.expect("half close");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        assert!(String::from_utf8(bytes)
            .expect("HTTP")
            .contains("Runory private-network tunnel target"));
    })
    .await
    .expect("forward timeout");
}

#[tokio::test]
async fn requires_auth_without_a_background_connection_and_rechecks_profile() {
    let directory = tempfile::tempdir().expect("temp");
    let service = TunnelService::at_path(directory.path().join("tunnels.json"));
    let profile = profile();
    let rule = rule(&service, &profile, 15432).await;
    assert!(matches!(
        service
            .start_background(rule.id, ConnectionIdentity::from(&profile), || async {
                Err(AppError::TunnelConnectionRequired)
            })
            .await,
        Err(AppError::TunnelConnectionRequired)
    ));
    assert_eq!(
        service.list().await.expect("list")[0].status.state,
        TunnelState::Stopped
    );
    let mut different = profile.clone();
    different.id = Uuid::new_v4();
    assert!(matches!(
        service
            .start_background(rule.id, ConnectionIdentity::from(&different), || async {
                panic!("must not authenticate wrong profile")
            })
            .await,
        Err(AppError::TunnelSessionMismatch)
    ));
}

#[tokio::test]
async fn stop_cancels_pending_auth_without_blocking_list_and_leaves_no_reservation() {
    let directory = tempfile::tempdir().expect("temp");
    let service = Arc::new(TunnelService::at_path(
        directory.path().join("tunnels.json"),
    ));
    let profile = profile();
    let rule = rule(&service, &profile, 15432).await;
    let (entered, ready) = tokio::sync::oneshot::channel();
    let start = {
        let service = service.clone();
        let identity = ConnectionIdentity::from(&profile);
        tokio::spawn(async move {
            service
                .start_background(rule.id, identity, || async {
                    entered.send(()).expect("notify");
                    std::future::pending().await
                })
                .await
        })
    };
    ready.await.expect("entered");
    tokio::time::timeout(Duration::from_secs(1), service.list())
        .await
        .expect("list not blocked")
        .expect("list");
    assert!(matches!(
        service.delete(rule.id).await,
        Err(AppError::TunnelRunning)
    ));
    service.stop(rule.id).await.expect("cancel");
    assert!(matches!(
        start.await.expect("join"),
        Err(AppError::TunnelStopped)
    ));
    service.delete(rule.id).await.expect("reservation released");
}

#[tokio::test]
async fn authentication_failure_is_stable_and_restart_never_restores_transport() {
    let directory = tempfile::tempdir().expect("temp");
    let path = directory.path().join("tunnels.json");
    let service = TunnelService::at_path(path.clone());
    let profile = profile();
    let rule = rule(&service, &profile, 15432).await;
    assert!(matches!(
        service
            .start_background(rule.id, ConnectionIdentity::from(&profile), || async {
                Err(AppError::AuthFailed)
            })
            .await,
        Err(AppError::AuthFailed)
    ));
    let status = service.list().await.expect("list")[0].status.clone();
    assert_eq!(status.error_code.as_deref(), Some("AUTH_FAILED"));
    assert!(status.session_id.is_none());
    let restarted = TunnelService::at_path(path);
    assert_eq!(
        restarted.list().await.expect("list")[0].status.state,
        TunnelState::Stopped
    );
}

#[tokio::test]
#[ignore = "requires docker compose --profile tunnels up -d --build"]
async fn real_openssh_background_reuses_transport_without_pty_and_outlives_terminal() {
    let directory = tempfile::tempdir().expect("temp");
    let service = TunnelService::at_path(directory.path().join("tunnels.json"));
    let profile = profile();
    let identity = ConnectionIdentity::from(&profile);
    // This fixture rejects ALL SSH session channels: shell/PTY/SFTP cannot accidentally work.
    let key = SshService::scan_host_key("127.0.0.1", 2226)
        .await
        .expect("key");
    assert!(SshService::connect(
        ConnectRequest {
            connection: credentials(2226, "runory-spike"),
            cols: 80,
            rows: 24
        },
        key.fingerprint
    )
    .await
    .is_err());
    let first = rule(&service, &profile, port().await).await;
    let second = rule(&service, &profile, port().await).await;
    service
        .start_background(first.id, identity.clone(), connect)
        .await
        .expect("start");
    service
        .start_background(second.id, identity.clone(), || async {
            panic!("must reuse background transport")
        })
        .await
        .expect("reuse");
    let views = service.list().await.expect("list");
    assert_eq!(views[0].status.session_id, views[1].status.session_id);
    let weak = service
        .background
        .connections
        .lock()
        .await
        .get(&profile.id)
        .expect("pool")
        .1
        .clone();
    let transport = weak.upgrade().expect("owner").transport();
    let manager = ServerSessionManager::default();
    let key = SshService::scan_host_key("127.0.0.1", 2222)
        .await
        .expect("key");
    let terminal = manager
        .connect(
            profile.id,
            ConnectRequest {
                connection: credentials(2222, "runory-spike"),
                cols: 80,
                rows: 24,
            },
            key.fingerprint,
            tauri::ipc::Channel::new(|_| Ok(())),
        )
        .await
        .expect("independent terminal");
    assert!(service.session_impact(terminal).await.is_empty());
    manager.disconnect(terminal).await.expect("close terminal");
    service.stop_session(terminal).await;
    fetch(first.local_port).await;
    fetch(second.local_port).await;
    service.check(second.id, profile.id).await.expect("probe");
    service.stop(first.id).await.expect("stop first");
    assert!(weak.upgrade().is_some());
    assert!(TcpStream::connect(("127.0.0.1", first.local_port))
        .await
        .is_err());
    fetch(second.local_port).await;
    // Editing a profile must not reuse its old authenticated endpoint.
    let mut changed = profile.clone();
    changed.port = 2225;
    assert!(matches!(
        service
            .start_background(first.id, ConnectionIdentity::from(&changed), || async {
                Err(AppError::TunnelConnectionRequired)
            })
            .await,
        Err(AppError::TunnelConnectionRequired)
    ));
    service.stop(second.id).await.expect("stop last");
    assert!(
        weak.upgrade().is_none(),
        "no owner remains after last listener stops"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while !transport.is_closed() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("transport released");
    assert!(matches!(
        service
            .start_background(first.id, identity, || async {
                Err(AppError::TunnelConnectionRequired)
            })
            .await,
        Err(AppError::TunnelConnectionRequired)
    ));
}

#[tokio::test]
#[ignore = "requires docker compose --profile tunnels up -d --build"]
async fn real_openssh_background_failures_release_transport_and_reject_wrong_credentials_or_key() {
    let directory = tempfile::tempdir().expect("temp");
    let service = TunnelService::at_path(directory.path().join("tunnels.json"));
    let profile = profile();
    let occupied = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let rule = rule(
        &service,
        &profile,
        occupied.local_addr().expect("address").port(),
    )
    .await;
    assert!(matches!(
        service
            .start_background(rule.id, ConnectionIdentity::from(&profile), connect)
            .await,
        Err(AppError::TunnelPortInUse)
    ));
    let weak = service
        .background
        .connections
        .lock()
        .await
        .get(&profile.id)
        .expect("pool")
        .1
        .clone();
    assert!(weak.upgrade().is_none(), "failed bind must not retain SSH");
    let key = SshService::scan_host_key("127.0.0.1", 2226)
        .await
        .expect("key");
    assert!(matches!(
        BackgroundConnection::connect(credentials(2226, "incorrect"), key.fingerprint).await,
        Err(AppError::AuthFailed)
    ));
    assert!(matches!(
        BackgroundConnection::connect(credentials(2226, "runory-spike"), "SHA256:wrong".into())
            .await,
        Err(AppError::HostKeyChanged)
    ));
}
