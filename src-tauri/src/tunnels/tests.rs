use super::{
    model::{TunnelHealth, TunnelState},
    *,
};
use crate::{
    domain::{AppError, ConnectRequest, SshAuthentication, SshConnectionRequest},
    ssh::{ServerSessionManager, SshService},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{timeout, Duration},
};
use uuid::Uuid;

fn request(profile_id: Uuid, local_port: u16) -> SaveTunnelRequest {
    SaveTunnelRequest {
        id: None,
        name: "Private service".into(),
        profile_id,
        target_host: "tunnel-target".into(),
        target_port: 8080,
        local_port,
    }
}

#[test]
fn rejects_invalid_rules_and_unexpected_bind_or_secret_fields() {
    let profile_id = Uuid::new_v4();
    for host in [
        "",
        "host/path",
        "user@host",
        "host;id",
        "host\n",
        "-host",
        "a..b",
        "db..",
        "0.0.0.0/0",
    ] {
        let rule = TunnelRule {
            id: Uuid::new_v4(),
            name: "test".into(),
            profile_id,
            target_host: host.into(),
            target_port: 80,
            local_port: 8080,
        };
        assert!(rule.validate().is_err(), "accepted {host:?}");
    }
    for host in [
        "127.0.0.1",
        "::1",
        "db.internal",
        "tunnel-target",
        "db.internal.",
    ] {
        let rule = TunnelRule {
            id: Uuid::new_v4(),
            name: "数据库".into(),
            profile_id,
            target_host: host.into(),
            target_port: 80,
            local_port: 8080,
        };
        assert!(rule.validate().is_ok());
    }
    for extra in ["bindAddress", "password", "command"] {
        let value = serde_json::json!({"name":"x", "profileId":profile_id,"targetHost":"db","targetPort":80,"localPort":8080,extra:"unexpected"});
        assert!(serde_json::from_value::<SaveTunnelRequest>(value).is_err());
    }
}

#[tokio::test]
async fn saved_rules_restart_stopped_and_delete_is_persisted() {
    let dir = tempfile::tempdir().expect("directory");
    let path = dir.path().join("tunnels.json");
    let service = TunnelService::at_path(path.clone());
    let rule = service
        .save(request(Uuid::new_v4(), 15432))
        .await
        .expect("save");
    let loaded = TunnelService::at_path(path.clone());
    let views = loaded.list().await.expect("load");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].status.state, TunnelState::Stopped);
    assert!(views[0].status.session_id.is_none());
    let json = std::fs::read_to_string(&path).expect("read");
    assert!(!json.contains("sessionId"));
    assert!(!json.contains("bytesSent"));
    assert!(!json.contains("password"));
    loaded.delete(rule.id).await.expect("delete");
    assert!(TunnelService::at_path(path)
        .list()
        .await
        .expect("load")
        .is_empty());
}

#[tokio::test]
async fn corrupt_catalog_and_duplicate_ids_fail_closed_without_overwrite() {
    let dir = tempfile::tempdir().expect("directory");
    let path = dir.path().join("tunnels.json");
    let id = Uuid::new_v4();
    let rule = TunnelRule {
        id,
        name: "test".into(),
        profile_id: Uuid::new_v4(),
        target_host: "db".into(),
        target_port: 80,
        local_port: 8080,
    };
    let duplicate =
        serde_json::to_string(&serde_json::json!({"schemaVersion":1,"rules":[rule,rule]}))
            .expect("json");
    for invalid in [
        "{broken".to_owned(),
        duplicate,
        "{\"schemaVersion\":2,\"rules\":[]}".into(),
    ] {
        std::fs::write(&path, &invalid).expect("fixture");
        let service = TunnelService::at_path(path.clone());
        assert!(matches!(service.list().await, Err(AppError::Storage)));
        assert!(matches!(
            service.save(request(Uuid::new_v4(), 12345)).await,
            Err(AppError::Storage)
        ));
        assert_eq!(std::fs::read_to_string(&path).expect("unchanged"), invalid);
        assert!(service.session_impact(Uuid::new_v4()).await.is_empty());
    }
}

#[tokio::test]
async fn start_requires_matching_profile_and_existing_verified_session() {
    let dir = tempfile::tempdir().expect("directory");
    let service = TunnelService::at_path(dir.path().join("tunnels.json"));
    let rule = service
        .save(request(Uuid::new_v4(), 15432))
        .await
        .expect("save");
    let sessions = ServerSessionManager::default();
    assert!(matches!(
        service
            .start(rule.id, Uuid::new_v4(), Uuid::new_v4(), &sessions)
            .await,
        Err(AppError::TunnelSessionMismatch)
    ));
    assert!(matches!(
        service
            .start(rule.id, Uuid::new_v4(), rule.profile_id, &sessions)
            .await,
        Err(AppError::SessionNotFound)
    ));
    assert!(matches!(
        service.check(rule.id, rule.profile_id).await,
        Err(AppError::TunnelStopped)
    ));
}

async fn fixture_session(manager: &ServerSessionManager, profile: Uuid, port: u16) -> Uuid {
    let key = SshService::scan_host_key("127.0.0.1", port)
        .await
        .expect("scan fixture host key");
    manager
        .connect(
            profile,
            ConnectRequest {
                connection: SshConnectionRequest {
                    host: "127.0.0.1".into(),
                    port,
                    username: "runory".into(),
                    authentication: SshAuthentication::Password {
                        password: zeroize::Zeroizing::new("runory-spike".into()),
                    },
                },
                cols: 100,
                rows: 30,
            },
            key.fingerprint,
            tauri::ipc::Channel::new(|_| Ok(())),
        )
        .await
        .expect("connect using exact scanned host key")
}

async fn unused_port() -> u16 {
    TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("port")
        .local_addr()
        .expect("address")
        .port()
}

async fn fetch(port: u16) -> String {
    timeout(Duration::from_secs(10), async {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("local connection");
        stream
            .write_all(b"GET / HTTP/1.0\r\nHost: tunnel-target\r\n\r\n")
            .await
            .expect("request");
        stream.shutdown().await.expect("half close");
        let mut body = Vec::new();
        stream.read_to_end(&mut body).await.expect("response");
        String::from_utf8(body).expect("http fixture")
    })
    .await
    .expect("forwarding timeout")
}

#[tokio::test]
#[ignore = "requires docker compose --profile tunnels up -d --build"]
async fn real_openssh_private_target_forward_probe_stop_restart_and_session_lifetime() {
    let dir = tempfile::tempdir().expect("directory");
    let service = TunnelService::at_path(dir.path().join("tunnels.json"));
    let manager = ServerSessionManager::default();
    let profile = Uuid::new_v4();
    let session = fixture_session(&manager, profile, 2222).await;
    let port = unused_port().await;
    let rule = service.save(request(profile, port)).await.expect("save");
    assert!(matches!(
        manager.forward_transport(session, Uuid::new_v4()).await,
        Err(AppError::TunnelSessionMismatch)
    ));
    service
        .start(rule.id, session, profile, &manager)
        .await
        .expect("start");
    assert_eq!(
        service.list().await.expect("list")[0].status.health,
        TunnelHealth::Unchecked
    );
    assert!(matches!(
        service.delete(rule.id).await,
        Err(AppError::TunnelRunning)
    ));
    let mut edit = request(profile, port);
    edit.id = Some(rule.id);
    assert!(matches!(
        service.save(edit).await,
        Err(AppError::TunnelRunning)
    ));
    assert!(matches!(
        service.start(rule.id, session, profile, &manager).await,
        Err(AppError::TunnelRunning)
    ));
    let (a, b, c) = tokio::join!(fetch(port), fetch(port), fetch(port));
    for result in [a, b, c] {
        assert!(result.contains("Runory private-network tunnel target"));
    }
    service.check(rule.id, profile).await.expect("probe");
    let status = &service.list().await.expect("list")[0].status;
    assert_eq!(status.health, TunnelHealth::Reachable);
    assert!(status.bytes_received > 0 && status.bytes_sent > 0);
    assert_eq!(service.session_impact(session).await.len(), 1);
    let mut pending = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("pending stream");
    pending.write_all(b"GET /").await.expect("partial request");
    tokio::time::sleep(Duration::from_millis(100)).await;
    service.stop(rule.id).await.expect("stop");
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    let mut buf = [0u8; 1];
    let pending_result = timeout(Duration::from_secs(2), pending.read(&mut buf))
        .await
        .expect("stream closed");
    assert!(matches!(pending_result, Ok(0) | Err(_)));
    assert_eq!(
        service.list().await.expect("list")[0]
            .status
            .active_connections,
        0
    );
    manager
        .write(session, b"echo tunnel-session-still-alive\r".to_vec())
        .await
        .expect("terminal unaffected");
    manager.sftp_open(session).await.expect("SFTP unaffected");
    service
        .start(rule.id, session, profile, &manager)
        .await
        .expect("restart same port");
    assert!(fetch(port)
        .await
        .contains("Runory private-network tunnel target"));
    manager.disconnect(session).await.expect("disconnect SSH");
    timeout(Duration::from_secs(3), async {
        loop {
            if service.list().await.expect("list")[0].status.state == TunnelState::Interrupted {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("SSH lifetime stops forwarding");
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    let next_session = fixture_session(&manager, profile, 2222).await;
    service
        .start(rule.id, next_session, profile, &manager)
        .await
        .expect("explicit rebind after reconnect");
    service.stop_session(next_session).await;
    manager.disconnect(next_session).await.expect("cleanup");
}

#[tokio::test]
#[ignore = "requires docker compose --profile tunnels up -d --build"]
async fn real_openssh_port_conflict_unreachable_and_server_denied_are_distinct() {
    let dir = tempfile::tempdir().expect("directory");
    let service = TunnelService::at_path(dir.path().join("tunnels.json"));
    let manager = ServerSessionManager::default();
    let profile = Uuid::new_v4();
    let session = fixture_session(&manager, profile, 2222).await;
    let occupied = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("occupy port");
    let port = occupied.local_addr().expect("address").port();
    let mut draft = request(profile, port);
    draft.target_port = 1;
    let rule = service.save(draft).await.expect("save");
    assert!(matches!(
        service.start(rule.id, session, profile, &manager).await,
        Err(AppError::TunnelPortInUse)
    ));
    drop(occupied);
    service
        .start(rule.id, session, profile, &manager)
        .await
        .expect("start listener only");
    service.check(rule.id, profile).await.expect("probe result");
    let status = service.list().await.expect("list")[0].status.clone();
    assert_eq!(status.state, TunnelState::Running);
    assert_eq!(status.health, TunnelHealth::Unreachable);
    assert_eq!(status.error_code.as_deref(), Some("TUNNEL_TARGET_FAILED"));
    service.stop(rule.id).await.expect("stop");
    manager.disconnect(session).await.expect("cleanup");
    let denied_session = fixture_session(&manager, profile, 2225).await;
    service
        .start(rule.id, denied_session, profile, &manager)
        .await
        .expect("listener still possible");
    service
        .check(rule.id, profile)
        .await
        .expect("denied probe result");
    assert_eq!(
        service.list().await.expect("list")[0]
            .status
            .error_code
            .as_deref(),
        Some("TUNNEL_DENIED")
    );
    service.stop(rule.id).await.expect("stop");
    manager.disconnect(denied_session).await.expect("cleanup");
}
