use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelReadHalf, ChannelWriteHalf, Disconnect};

use crate::domain::{
    AppError, AppResult, ConnectRequest, HostKeyInfo, SshAuthentication, SshConnectionRequest,
};

#[derive(Clone)]
pub(crate) struct HostKeyHandler {
    expected_fingerprint: Option<String>,
    observed: Arc<Mutex<Option<HostKeyInfo>>>,
}

impl client::Handler for HostKeyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let public_key = key.public_key();
        let info = HostKeyInfo {
            key_type: public_key.algorithm().to_string(),
            fingerprint: public_key.fingerprint(HashAlg::Sha256).to_string(),
        };
        let trusted = self
            .expected_fingerprint
            .as_deref()
            .is_none_or(|expected| expected == info.fingerprint);
        if let Ok(mut observed) = self.observed.lock() {
            *observed = Some(info);
        }
        // A probe performs no authentication. A real connection only passes KEX when the exact user-confirmed fingerprint matches.
        Ok(trusted)
    }
}

pub struct OpenedSession {
    pub client: client::Handle<HostKeyHandler>,
    pub reader: ChannelReadHalf,
    pub writer: ChannelWriteHalf<client::Msg>,
}

pub struct SshService;

impl SshService {
    pub async fn scan_host_key(host: &str, port: u16) -> AppResult<HostKeyInfo> {
        validate_endpoint(host, port)?;
        let observed = Arc::new(Mutex::new(None));
        let handler = HostKeyHandler {
            expected_fingerprint: None,
            observed: Arc::clone(&observed),
        };
        let client = connect_transport(host, port, handler).await?;
        let _ = client
            .disconnect(Disconnect::ByApplication, "host key probe complete", "en")
            .await;
        observed
            .lock()
            .ok()
            .and_then(|mut value| value.take())
            .ok_or(AppError::HostKeyUnknown)
    }

    pub async fn connect(
        request: ConnectRequest,
        expected_fingerprint: String,
    ) -> AppResult<OpenedSession> {
        if request.cols == 0 || request.rows == 0 {
            return Err(AppError::InvalidProfile);
        }
        let client = authenticate(request.connection, expected_fingerprint).await?;
        let channel = client
            .channel_open_session()
            .await
            .map_err(map_russh_error)?;
        channel
            .request_pty(
                false,
                "xterm-256color",
                request.cols,
                request.rows,
                0,
                0,
                &[],
            )
            .await
            .map_err(map_russh_error)?;
        channel.request_shell(true).await.map_err(map_russh_error)?;
        let (reader, writer) = channel.split();
        Ok(OpenedSession {
            client,
            reader,
            writer,
        })
    }

    pub async fn test_connection(
        request: SshConnectionRequest,
        expected_fingerprint: String,
    ) -> AppResult<()> {
        let client = authenticate(request, expected_fingerprint).await?;
        let _ = client
            .disconnect(Disconnect::ByApplication, "connection test complete", "en")
            .await;
        Ok(())
    }
}

async fn authenticate(
    request: SshConnectionRequest,
    expected_fingerprint: String,
) -> AppResult<client::Handle<HostKeyHandler>> {
    validate_endpoint(&request.host, request.port)?;
    if request.username.trim().is_empty() || expected_fingerprint.trim().is_empty() {
        return Err(AppError::InvalidProfile);
    }
    let authentication = prepare_authentication(request.authentication).await?;
    let observed = Arc::new(Mutex::new(None));
    let handler = HostKeyHandler {
        expected_fingerprint: Some(expected_fingerprint.clone()),
        observed: Arc::clone(&observed),
    };
    let mut client = match connect_transport(&request.host, request.port, handler).await {
        Ok(client) => client,
        Err(error) => {
            if let Some(actual) = observed
                .lock()
                .ok()
                .and_then(|value| value.as_ref().map(|info| info.fingerprint.clone()))
            {
                verify_exact_fingerprint(&expected_fingerprint, &actual)?;
            }
            return Err(error);
        }
    };
    let actual = observed
        .lock()
        .ok()
        .and_then(|value| value.as_ref().map(|info| info.fingerprint.clone()))
        .ok_or(AppError::HostKeyUnknown)?;
    verify_exact_fingerprint(&expected_fingerprint, &actual)?;
    let auth = match authentication {
        PreparedAuthentication::Password(password) => client
            .authenticate_password(&request.username, password.as_str())
            .await
            .map_err(map_russh_error)?,
        PreparedAuthentication::PrivateKey(key) => {
            let hash = client
                .best_supported_rsa_hash()
                .await
                .map_err(map_russh_error)?
                .flatten();
            client
                .authenticate_publickey(
                    &request.username,
                    PrivateKeyWithHashAlg::new(Arc::new(*key), hash),
                )
                .await
                .map_err(map_russh_error)?
        }
    };
    if !auth.success() {
        let _ = client
            .disconnect(Disconnect::ByApplication, "authentication failed", "en")
            .await;
        return Err(AppError::AuthFailed);
    }
    Ok(client)
}

enum PreparedAuthentication {
    Password(zeroize::Zeroizing<String>),
    PrivateKey(Box<russh::keys::PrivateKey>),
}

async fn prepare_authentication(
    authentication: SshAuthentication,
) -> AppResult<PreparedAuthentication> {
    match authentication {
        SshAuthentication::Password { password } => {
            if password.is_empty() {
                Err(AppError::InvalidProfile)
            } else {
                Ok(PreparedAuthentication::Password(password))
            }
        }
        SshAuthentication::PrivateKeyFile { path, passphrase } => {
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|_| AppError::PrivateKeyInvalid)?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 {
                return Err(AppError::PrivateKeyInvalid);
            }
            let bytes = zeroize::Zeroizing::new(
                tokio::fs::read(&path)
                    .await
                    .map_err(|_| AppError::PrivateKeyInvalid)?,
            );
            decode_private_key(bytes.as_slice(), passphrase.as_ref())
        }
        SshAuthentication::PrivateKeyData {
            content,
            passphrase,
        } => decode_private_key(content.as_slice(), passphrase.as_ref()),
    }
}

fn decode_private_key(
    bytes: &[u8],
    passphrase: Option<&zeroize::Zeroizing<String>>,
) -> AppResult<PreparedAuthentication> {
    let encoded = std::str::from_utf8(bytes).map_err(|_| AppError::PrivateKeyInvalid)?;
    let passphrase_ref = passphrase.and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(value.as_str())
        }
    });
    let key = russh::keys::decode_secret_key(encoded, passphrase_ref).map_err(|_| {
        if passphrase_ref.is_none() {
            AppError::PassphraseRequired
        } else {
            AppError::PrivateKeyInvalid
        }
    })?;
    Ok(PreparedAuthentication::PrivateKey(Box::new(key)))
}

fn verify_exact_fingerprint(expected: &str, actual: &str) -> AppResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(AppError::HostKeyChanged)
    }
}

async fn connect_transport(
    host: &str,
    port: u16,
    handler: HostKeyHandler,
) -> AppResult<client::Handle<HostKeyHandler>> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        ..Default::default()
    });
    tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (host, port), handler),
    )
    .await
    .map_err(|_| AppError::ConnectionTimeout)?
    .map_err(map_russh_error)
}

fn validate_endpoint(host: &str, port: u16) -> AppResult<()> {
    if host.trim().is_empty() || host.chars().any(char::is_whitespace) || port == 0 {
        Err(AppError::InvalidProfile)
    } else {
        Ok(())
    }
}

fn map_russh_error(error: russh::Error) -> AppError {
    match error {
        russh::Error::ConnectionTimeout => AppError::ConnectionTimeout,
        russh::Error::Disconnect => AppError::ConnectionLost,
        _ => AppError::ConnectionRefused,
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::agentic::{
        ChangeSetDraftRequest, ChangeSetService, ChangeStepDraft, IncidentRequest, IncidentService,
        IncidentSeverity, ObservationCache, OperationsPack,
    };
    use crate::dashboard::DashboardService;
    use crate::deployment::{DeploymentHistoryRepository, DeploymentService};
    use crate::domain::{CronSchedule, CronTask, EnvironmentEntry};
    use crate::known_hosts::{KnownHostRepository, KnownHostService};
    use crate::ssh::SftpChannel;
    use crate::storage::JsonRepository;
    use crate::tools::{
        NativeToolExecutionService, NativeToolInvocation, NativeToolRequest, ToolAuditRepository,
        ToolAuditStatus, ToolCancellationStatus, ToolData,
    };
    use crate::transfers::{LocalFileGrant, LocalFileGrantKind, TransferQueue};
    use russh::{ChannelMsg, Disconnect};
    use tokio::sync::Mutex as TokioMutex;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ProductionFaultLabFixture {
        schema_version: u8,
        ports: Vec<u16>,
        service: String,
        expected_root_cause: String,
        expected_comparison: String,
        expected_states: Vec<String>,
    }

    fn production_fault_lab_fixture() -> ProductionFaultLabFixture {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/agentic/production-fault-lab.json"
        ))
        .expect("valid production fault-lab fixture")
    }

    fn integration_lock() -> &'static TokioMutex<()> {
        static LOCK: std::sync::OnceLock<TokioMutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| TokioMutex::new(()))
    }

    fn fixture_key(name: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/ssh-server/generated")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    async fn current_fingerprint() -> String {
        current_fingerprint_on_port(2222).await
    }

    async fn current_fingerprint_on_port(port: u16) -> String {
        SshService::scan_host_key("127.0.0.1", port)
            .await
            .expect("scan host key")
            .fingerprint
    }

    fn docker_compose(arguments: &[&str]) {
        let status = std::process::Command::new("docker")
            .args(["compose"])
            .args(arguments)
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
            .status()
            .expect("run docker compose");
        assert!(status.success(), "docker compose command failed");
    }

    async fn wait_for_fixture() {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if SshService::scan_host_key("127.0.0.1", 2222).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        })
        .await
        .expect("OpenSSH fixture startup timeout");
    }

    fn connection(authentication: SshAuthentication) -> SshConnectionRequest {
        connection_on_port(2222, authentication)
    }

    fn connection_on_port(port: u16, authentication: SshAuthentication) -> SshConnectionRequest {
        SshConnectionRequest {
            host: "127.0.0.1".into(),
            port,
            username: "runory".into(),
            authentication,
        }
    }

    fn terminal_request(authentication: SshAuthentication) -> ConnectRequest {
        ConnectRequest {
            connection: connection(authentication),
            cols: 100,
            rows: 30,
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn test_connection_authenticates_without_terminal_session() {
        let _guard = integration_lock().lock().await;
        SshService::test_connection(
            connection(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            current_fingerprint().await,
        )
        .await
        .expect("test connection");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn password_pty_shell_echo_resize_and_disconnect() {
        let _guard = integration_lock().lock().await;
        let directory = tempfile::tempdir().expect("temp directory");
        let known_hosts = KnownHostService::new(
            KnownHostRepository::new(JsonRepository::new(
                directory.path().join("known-hosts.json"),
            )),
            Arc::new(TokioMutex::new(())),
        );
        let key = SshService::scan_host_key("127.0.0.1", 2222)
            .await
            .expect("scan host key");
        let verification = known_hosts
            .prepare("127.0.0.1", 2222, key)
            .await
            .expect("prepare verification");
        known_hosts
            .trust(verification.attempt_id, true)
            .await
            .expect("remember host");
        let expected_fingerprint = known_hosts
            .consume(verification.attempt_id, "127.0.0.1", 2222)
            .await
            .expect("consume verification");
        let mut session = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            expected_fingerprint,
        )
        .await
        .expect("connect");
        session
            .writer
            .window_change(120, 34, 0, 0)
            .await
            .expect("resize");
        session
            .writer
            .data(&b"echo integration-ok\r"[..])
            .await
            .expect("write");
        let output = tokio::time::timeout(Duration::from_secs(10), async {
            let mut bytes = Vec::new();
            while let Some(message) = session.reader.wait().await {
                if let ChannelMsg::Data { data } = message {
                    bytes.extend_from_slice(&data);
                    if String::from_utf8_lossy(&bytes).contains("integration-ok") {
                        return true;
                    }
                }
            }
            false
        })
        .await
        .expect("output timeout");
        assert!(output);
        session
            .client
            .disconnect(Disconnect::ByApplication, "test complete", "en")
            .await
            .expect("disconnect");

        let reconnected = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            current_fingerprint().await,
        )
        .await
        .expect("reconnect");
        reconnected
            .client
            .disconnect(Disconnect::ByApplication, "reconnect test complete", "en")
            .await
            .expect("disconnect reconnected session");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn private_key_file_vault_data_and_encrypted_key_authenticate() {
        let _guard = integration_lock().lock().await;
        for (name, passphrase) in [
            ("id_ed25519", None),
            ("id_ed25519_encrypted", Some("runory-key-passphrase")),
        ] {
            let path = fixture_key(name);
            let content = std::fs::read(&path).expect("read private key fixture");
            let authentications = [
                SshAuthentication::PrivateKeyFile {
                    path,
                    passphrase: passphrase.map(|value| zeroize::Zeroizing::new(value.into())),
                },
                SshAuthentication::PrivateKeyData {
                    content: zeroize::Zeroizing::new(content),
                    passphrase: passphrase.map(|value| zeroize::Zeroizing::new(value.into())),
                },
            ];
            for authentication in authentications {
                let session = SshService::connect(
                    terminal_request(authentication),
                    current_fingerprint().await,
                )
                .await
                .expect("private key connection");
                session
                    .client
                    .disconnect(Disconnect::ByApplication, "test complete", "en")
                    .await
                    .expect("disconnect");
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn ten_sessions_connect_concurrently_and_disconnect_independently() {
        let _guard = integration_lock().lock().await;
        let fingerprint = current_fingerprint().await;
        let mut connections = tokio::task::JoinSet::new();
        for _ in 0..10 {
            let expected_fingerprint = fingerprint.clone();
            connections.spawn(async move {
                SshService::connect(
                    terminal_request(SshAuthentication::Password {
                        password: zeroize::Zeroizing::new("runory-spike".into()),
                    }),
                    expected_fingerprint,
                )
                .await
            });
        }

        let mut sessions = Vec::new();
        while let Some(result) = connections.join_next().await {
            sessions.push(
                result
                    .expect("connection task")
                    .expect("concurrent connection"),
            );
        }
        assert_eq!(sessions.len(), 10);
        for session in sessions {
            session
                .client
                .disconnect(Disconnect::ByApplication, "concurrency test complete", "en")
                .await
                .expect("disconnect concurrent session");
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn wrong_password_key_and_passphrase_are_rejected() {
        let _guard = integration_lock().lock().await;
        let wrong_password = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("wrong-password".into()),
            }),
            current_fingerprint().await,
        )
        .await;
        assert!(matches!(wrong_password, Err(AppError::AuthFailed)));

        let wrong_key = SshService::connect(
            terminal_request(SshAuthentication::PrivateKeyFile {
                path: fixture_key("id_ed25519_wrong"),
                passphrase: None,
            }),
            current_fingerprint().await,
        )
        .await;
        assert!(matches!(wrong_key, Err(AppError::AuthFailed)));

        let wrong_passphrase = SshService::connect(
            terminal_request(SshAuthentication::PrivateKeyFile {
                path: fixture_key("id_ed25519_encrypted"),
                passphrase: Some(zeroize::Zeroizing::new("wrong-passphrase".into())),
            }),
            current_fingerprint().await,
        )
        .await;
        assert!(matches!(wrong_passphrase, Err(AppError::PrivateKeyInvalid)));
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn large_terminal_output_is_streamed_without_truncation() {
        let _guard = integration_lock().lock().await;
        let mut session = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            current_fingerprint().await,
        )
        .await
        .expect("connect");
        session
            .writer
            .data(&b"yes 0123456789abcdef | head -c 2097152; printf '\nRUNORY-LARGE-END\n'\r"[..])
            .await
            .expect("request large output");
        let received = tokio::time::timeout(Duration::from_secs(30), async {
            let mut total = 0usize;
            let mut tail = Vec::new();
            while let Some(message) = session.reader.wait().await {
                if let ChannelMsg::Data { data } = message {
                    total += data.len();
                    tail.extend_from_slice(&data);
                    if tail.len() > 128 {
                        tail.drain(..tail.len() - 128);
                    }
                    if total >= 2 * 1024 * 1024
                        && String::from_utf8_lossy(&tail).contains("RUNORY-LARGE-END")
                    {
                        return total;
                    }
                }
            }
            total
        })
        .await
        .expect("large output timeout");
        assert!(received >= 2 * 1024 * 1024);
        session
            .client
            .disconnect(
                Disconnect::ByApplication,
                "large output test complete",
                "en",
            )
            .await
            .expect("disconnect");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn terminal_and_sftp_share_one_authenticated_transport() {
        let _guard = integration_lock().lock().await;
        let session = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            current_fingerprint().await,
        )
        .await
        .expect("connect");
        let channel = session
            .client
            .channel_open_session()
            .await
            .expect("open SFTP SSH channel");
        channel
            .request_subsystem(true, "sftp")
            .await
            .expect("request SFTP subsystem");
        let raw = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .expect("initialize SFTP protocol");
        let sftp = SftpChannel::new(raw).await.expect("open SFTP channel");
        let home = sftp.refresh().await.expect("refresh home");
        assert!(home
            .entries
            .iter()
            .any(|entry| entry.name == "sftp-fixture"));
        let fixture = sftp
            .change_directory("sftp-fixture")
            .await
            .expect("change directory");
        assert!(fixture
            .entries
            .iter()
            .any(|entry| entry.name == "subdirectory"));
        let file = sftp.stat("example.txt").await.expect("stat file");
        assert!(matches!(file.kind, crate::domain::SftpEntryKind::File));
        assert!(file.size.is_some_and(|size| size > 0));
        sftp.close().await;
        session
            .client
            .disconnect(Disconnect::ByApplication, "SFTP test complete", "en")
            .await
            .expect("disconnect");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn sftp_mutations_and_transfer_queue_roundtrip() {
        let _guard = integration_lock().lock().await;
        let session = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            current_fingerprint().await,
        )
        .await
        .expect("connect");
        let channel = session
            .client
            .channel_open_session()
            .await
            .expect("open SFTP SSH channel");
        channel
            .request_subsystem(true, "sftp")
            .await
            .expect("request SFTP subsystem");
        let raw = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .expect("initialize SFTP protocol");
        let sftp = std::sync::Arc::new(SftpChannel::new(raw).await.expect("open SFTP channel"));
        let home = sftp.refresh().await.expect("refresh home").path;
        let marker = uuid::Uuid::new_v4().simple().to_string();
        let directory_name = format!("runory-{marker}");
        sftp.create_directory(&home, &directory_name)
            .await
            .expect("create remote directory");
        let renamed_directory = format!("runory-{marker}-renamed");
        sftp.rename(&format!("{home}/{directory_name}"), &renamed_directory)
            .await
            .expect("rename remote directory");

        let local = tempfile::tempdir().expect("local transfer directory");
        let source_path = local.path().join(format!("payload-{marker}.bin"));
        let payload = vec![0x5au8; 4 * 1024 * 1024];
        tokio::fs::write(&source_path, &payload)
            .await
            .expect("write upload fixture");
        let queue = TransferQueue::default();
        let session_id = uuid::Uuid::new_v4();
        let upload = queue
            .enqueue_upload(
                session_id,
                std::sync::Arc::clone(&sftp),
                LocalFileGrant {
                    path: source_path,
                    name: format!("payload-{marker}.bin"),
                    size: payload.len() as u64,
                    kind: LocalFileGrantKind::UploadSource,
                },
                format!("{home}/{renamed_directory}"),
                false,
            )
            .await
            .expect("enqueue upload");
        queue.cancel(upload.id).await.expect("cancel upload");
        wait_for_transfer(&queue, upload.id, crate::domain::TransferState::Cancelled).await;
        queue.retry(upload.id, false).await.expect("retry upload");
        wait_for_transfer(&queue, upload.id, crate::domain::TransferState::Completed).await;

        let download_path = local.path().join(format!("download-{marker}.bin"));
        let download = queue
            .enqueue_download(
                session_id,
                std::sync::Arc::clone(&sftp),
                LocalFileGrant {
                    path: download_path.clone(),
                    name: format!("download-{marker}.bin"),
                    size: 0,
                    kind: LocalFileGrantKind::DownloadTarget,
                },
                upload.remote_path.clone(),
            )
            .await
            .expect("enqueue download");
        wait_for_transfer(&queue, download.id, crate::domain::TransferState::Completed).await;
        assert_eq!(
            tokio::fs::read(download_path).await.expect("read download"),
            payload
        );

        sftp.delete(&format!("{home}/{renamed_directory}"), true)
            .await
            .expect("delete recursive remote directory");
        sftp.close().await;
        session
            .client
            .disconnect(
                Disconnect::ByApplication,
                "SFTP transfer test complete",
                "en",
            )
            .await
            .expect("disconnect");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn exec_dashboard_operations_and_deployment_share_authenticated_transport() {
        let _guard = integration_lock().lock().await;
        let manager = crate::ssh::ServerSessionManager::default();
        let profile_id = uuid::Uuid::new_v4();
        let output = tauri::ipc::Channel::new(|_| Ok(()));
        let session_id = manager
            .connect(
                profile_id,
                terminal_request(SshAuthentication::Password {
                    password: zeroize::Zeroizing::new("runory-spike".into()),
                }),
                current_fingerprint().await,
                output,
            )
            .await
            .expect("connect manager");

        let probe = manager
            .exec(
                session_id,
                crate::ssh::RemoteCommand::program("printf", vec!["runory-exec".into()]),
            )
            .await
            .expect("exec probe");
        assert_eq!(probe.stdout, "runory-exec");
        assert_eq!(
            manager
                .detect_os_distribution(session_id)
                .await
                .expect("detect operating system"),
            Some(crate::domain::OsDistribution::AlpineLinux)
        );

        let dashboard = DashboardService::overview(&manager, session_id)
            .await
            .expect("dashboard overview");
        assert!(dashboard.memory_total_bytes > 0);
        assert!(matches!(
            crate::operations::OperationsService::docker_list(&manager, session_id).await,
            Err(AppError::UnsupportedRemote)
        ));

        let local = tempfile::tempdir().expect("history directory");
        let deployment = DeploymentService::new(DeploymentHistoryRepository::new(
            JsonRepository::new(local.path().join("history.json")),
        ));
        deployment
            .write_environment(
                &manager,
                session_id,
                "/home/runory/sftp-fixture/.env".into(),
                vec![EnvironmentEntry {
                    key: "RUNORY_TEST".into(),
                    value: "safe-value".into(),
                }],
            )
            .await
            .expect("environment write");
        let environment = manager
            .exec(
                session_id,
                crate::ssh::RemoteCommand::program(
                    "cat",
                    vec!["/home/runory/sftp-fixture/.env".into()],
                ),
            )
            .await
            .expect("read environment fixture");
        assert_eq!(environment.stdout, "RUNORY_TEST=\"safe-value\"\n");
        deployment
            .backup(
                &manager,
                crate::domain::BackupRequest {
                    session_id,
                    source_path: "/home/runory/sftp-fixture".into(),
                    destination_directory: "/home/runory/backups".into(),
                },
            )
            .await
            .expect("backup");
        let cron = DeploymentService::cron_add(
            &manager,
            session_id,
            CronSchedule::Daily,
            CronTask::GitPull {
                repository_path: "/home/runory/sftp-fixture".into(),
                branch: "main".into(),
            },
        )
        .await;
        assert!(
            cron.is_err(),
            "fixture intentionally has no privileged crontab helper"
        );
        let history = deployment.history(Some(profile_id)).await.expect("history");
        assert!(history.len() >= 2);
        manager
            .disconnect(session_id)
            .await
            .expect("disconnect manager");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn native_tools_use_policy_audit_and_isolated_exec_channels_on_the_existing_session() {
        let _guard = integration_lock().lock().await;
        let manager = crate::ssh::ServerSessionManager::default();
        let output = tauri::ipc::Channel::new(|_| Ok(()));
        let session_id = manager
            .connect(
                uuid::Uuid::new_v4(),
                terminal_request(SshAuthentication::Password {
                    password: zeroize::Zeroizing::new("runory-spike".into()),
                }),
                current_fingerprint().await,
                output,
            )
            .await
            .expect("connect manager");
        let audit_directory = tempfile::tempdir().expect("tool audit directory");
        let tools = NativeToolExecutionService::foundation(ToolAuditRepository::new(
            JsonRepository::new(audit_directory.path().join("tool-audit.json")),
        ));

        let system = tools
            .execute(
                &manager,
                NativeToolRequest::new(session_id, NativeToolInvocation::SystemInfo),
            )
            .await
            .expect("system.info audit");
        assert!(matches!(system.data, Some(ToolData::SystemInfo(_))));

        let disk = tools
            .execute(
                &manager,
                NativeToolRequest::new(session_id, NativeToolInvocation::SystemDisk),
            )
            .await
            .expect("system.disk_usage audit");
        assert!(
            matches!(disk.data, Some(ToolData::SystemDisk(ref data)) if !data.disks.is_empty())
        );

        let status = tools
            .execute(
                &manager,
                NativeToolRequest::new(
                    session_id,
                    NativeToolInvocation::ServiceStatus {
                        service: "runory-fixture.service".into(),
                    },
                ),
            )
            .await
            .expect("service.status audit");
        assert!(matches!(status.data, Some(ToolData::ServiceStatus(_))));

        let logs = tools
            .execute(
                &manager,
                NativeToolRequest::new(
                    session_id,
                    NativeToolInvocation::ServiceLogs {
                        service: "runory-fixture.service".into(),
                        lines: 20,
                    },
                ),
            )
            .await
            .expect("service.logs audit");
        assert!(
            matches!(logs.data, Some(ToolData::ServiceLogs(ref data)) if data.entries.len() == 2)
        );

        let port = tools
            .execute(
                &manager,
                NativeToolRequest::new(
                    session_id,
                    NativeToolInvocation::NetworkPortCheck {
                        host: "127.0.0.1".into(),
                        port: 8080,
                    },
                ),
            )
            .await
            .expect("network.port_check audit");
        assert!(matches!(port.data, Some(ToolData::NetworkPortCheck(ref data)) if data.reachable));

        let http = tools
            .execute(
                &manager,
                NativeToolRequest::new(
                    session_id,
                    NativeToolInvocation::HttpRequest {
                        url: "http://127.0.0.1:8080/".into(),
                    },
                ),
            )
            .await
            .expect("http.request audit");
        assert!(matches!(
            http.data,
            Some(ToolData::HttpResponse(ref data))
                if (200..300).contains(&data.status_code)
                    && data.body_preview.contains("Runory HTTP integration fixture")
        ));

        let nginx = tools
            .execute(
                &manager,
                NativeToolRequest::new(session_id, NativeToolInvocation::NginxTest),
            )
            .await
            .expect("nginx.test audit");
        assert!(matches!(
            nginx.data,
            Some(ToolData::NginxTest(ref data))
                if data.valid
                    && data.config_file.as_deref() == Some("/etc/nginx/nginx.conf")
        ));

        let (request, cancellation) =
            NativeToolRequest::cancellable(session_id, NativeToolInvocation::NginxTest);
        let cancelled_invocation_id = cancellation.invocation_id();
        let execute_cancelled = tools.execute(&manager, request);
        let request_cancellation = async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancellation.cancel()
        };
        let (cancelled, cancellation_status) =
            tokio::join!(execute_cancelled, request_cancellation);
        assert_eq!(cancellation_status, ToolCancellationStatus::Requested);
        let cancelled = cancelled.expect("cancelled nginx.test audit");
        assert!(cancelled.cancelled);
        assert_eq!(cancelled.error_code, Some("TOOL_CANCELLED"));

        let audit = tools.audit().await.expect("tool audit");
        assert_eq!(audit.len(), 8);
        assert!(audit.iter().take(7).all(|record| {
            record.profile_id.is_some()
                && record.status == ToolAuditStatus::Succeeded
                && record.succeeded == Some(true)
        }));
        let cancelled_audit = audit
            .iter()
            .find(|record| record.invocation_id == cancelled_invocation_id)
            .expect("cancelled audit record");
        assert_eq!(cancelled_audit.status, ToolAuditStatus::Cancelled);
        assert!(cancelled_audit.cancellation_requested_at_epoch_ms.is_some());

        manager
            .disconnect(session_id)
            .await
            .expect("disconnect manager");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn production_operations_pack_runs_through_typed_tools_on_real_openssh() {
        let _guard = integration_lock().lock().await;
        let manager = crate::ssh::ServerSessionManager::default();
        let session_id = manager
            .connect(
                uuid::Uuid::new_v4(),
                terminal_request(SshAuthentication::Password {
                    password: zeroize::Zeroizing::new("runory-spike".into()),
                }),
                current_fingerprint().await,
                tauri::ipc::Channel::new(|_| Ok(())),
            )
            .await
            .expect("connect manager");
        let directory = tempfile::tempdir().expect("audit directory");
        let tools = NativeToolExecutionService::foundation(ToolAuditRepository::new(
            JsonRepository::new(directory.path().join("tool-audit.json")),
        ));
        let incidents = IncidentService::at_path(directory.path().join("incidents.json"));
        let incident = incidents
            .run(
                &manager,
                &tools,
                &ObservationCache::default(),
                IncidentRequest {
                    id: uuid::Uuid::new_v4(),
                    pack: OperationsPack::Service,
                    severity: IncidentSeverity::Sev3,
                    targets: vec![session_id],
                    symptoms: vec!["fixture service validation".into()],
                    host: Some("127.0.0.1".into()),
                    port: Some(8080),
                    url: None,
                    service: Some("runory-fixture.service".into()),
                    container: None,
                    config_path: None,
                    upstream_host: None,
                    upstream_port: None,
                    dependencies: Vec::new(),
                },
            )
            .await
            .expect("incident investigation");
        assert!(incident.evidence.len() >= 5);
        assert_eq!(incident.evidence.len(), incident.evidence_references.len());
        assert!(!incident.root_cause.evidence_ids.is_empty());
        assert!(incident
            .evidence
            .iter()
            .all(|item| item.source.starts_with("tool.")));
        manager
            .disconnect(session_id)
            .await
            .expect("disconnect manager");
    }

    #[tokio::test]
    #[ignore = "requires docker compose --profile fault-lab OpenSSH fixtures"]
    async fn production_fault_lab_correlates_three_real_openssh_targets() {
        let _guard = integration_lock().lock().await;
        let fixture = production_fault_lab_fixture();
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.ports.len(), fixture.expected_states.len());
        let manager = crate::ssh::ServerSessionManager::default();
        let mut targets = Vec::new();
        for port in fixture.ports {
            let session_id = manager
                .connect(
                    uuid::Uuid::new_v4(),
                    ConnectRequest {
                        connection: connection_on_port(
                            port,
                            SshAuthentication::Password {
                                password: zeroize::Zeroizing::new("runory-spike".into()),
                            },
                        ),
                        cols: 100,
                        rows: 30,
                    },
                    current_fingerprint_on_port(port).await,
                    tauri::ipc::Channel::new(|_| Ok(())),
                )
                .await
                .expect("connect fault-lab target");
            targets.push(session_id);
        }
        let directory = tempfile::tempdir().expect("qualification directory");
        let tools = NativeToolExecutionService::foundation(ToolAuditRepository::new(
            JsonRepository::new(directory.path().join("tool-audit.json")),
        ));
        let incidents = IncidentService::at_path(directory.path().join("incidents.json"));
        let incident = incidents
            .run(
                &manager,
                &tools,
                &ObservationCache::default(),
                IncidentRequest {
                    id: uuid::Uuid::new_v4(),
                    pack: OperationsPack::Service,
                    severity: IncidentSeverity::Sev2,
                    targets: targets.clone(),
                    symptoms: vec!["service state differs across production targets".into()],
                    host: None,
                    port: None,
                    url: None,
                    service: Some(fixture.service.clone()),
                    container: None,
                    config_path: None,
                    upstream_host: None,
                    upstream_port: None,
                    dependencies: Vec::new(),
                },
            )
            .await
            .expect("fault-lab investigation");
        assert_eq!(incident.targets, targets);
        assert_eq!(incident.root_cause.code, fixture.expected_root_cause);
        assert!(incident.comparisons.iter().any(|comparison| {
            comparison.dimension == fixture.expected_comparison
                && comparison.drift
                && comparison.values.len() == 3
        }));
        assert!(incident.root_cause.evidence_ids.iter().all(|id| incident
            .evidence_references
            .iter()
            .any(|item| item.id == *id)));
        for target in targets {
            manager.disconnect(target).await.expect("disconnect target");
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn approved_changeset_patches_verifies_and_audits_on_the_existing_session() {
        let _guard = integration_lock().lock().await;
        let manager = crate::ssh::ServerSessionManager::default();
        let session_id = manager
            .connect(
                uuid::Uuid::new_v4(),
                terminal_request(SshAuthentication::Password {
                    password: zeroize::Zeroizing::new("runory-spike".into()),
                }),
                current_fingerprint().await,
                tauri::ipc::Channel::new(|_| Ok(())),
            )
            .await
            .expect("connect manager");
        let path = "/home/runory/sftp-fixture/agentic-change.txt";
        manager
            .sftp_write_text(session_id, path.into(), "before\n".into())
            .await
            .expect("seed change fixture");
        let audit_directory = tempfile::tempdir().expect("tool audit directory");
        let tools = NativeToolExecutionService::approved_repair(ToolAuditRepository::new(
            JsonRepository::new(audit_directory.path().join("tool-audit.json")),
        ));
        let changes = ChangeSetService::default();
        let draft = changes
            .draft(ChangeSetDraftRequest {
                agent_run_id: uuid::Uuid::new_v4(),
                session_id,
                title: "integration patch".into(),
                steps: vec![ChangeStepDraft::FilePatch {
                    path: path.into(),
                    expected: "before".into(),
                    replacement: "after".into(),
                }],
            })
            .await
            .expect("draft");
        changes
            .approve(draft.id, draft.version)
            .await
            .expect("approve");
        let executed = changes
            .execute(draft.id, draft.version, &manager, &tools)
            .await
            .expect("execute");
        assert_eq!(
            executed.execution_state,
            crate::agentic::ExecutionState::Committed
        );
        let (_, content) = manager
            .sftp_read_text(session_id, path.into())
            .await
            .expect("verify");
        assert_eq!(content, "after\n");
        let audit = tools.audit().await.expect("audit");
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].change_set_id, Some(draft.id));
        assert_eq!(audit[0].change_set_version, Some(draft.version));
        assert_eq!(audit[0].approval, crate::tools::ToolApprovalAudit::Approved);
        assert_eq!(
            audit[0].verification,
            crate::tools::ToolVerificationAudit::Succeeded
        );
        manager.disconnect(session_id).await.expect("disconnect");
    }

    async fn wait_for_transfer(
        queue: &TransferQueue,
        job_id: uuid::Uuid,
        expected: crate::domain::TransferState,
    ) {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let state = queue
                    .list()
                    .await
                    .into_iter()
                    .find(|job| job.id == job_id)
                    .expect("transfer exists")
                    .state;
                if state == expected {
                    return;
                }
                if matches!(state, crate::domain::TransferState::Failed) {
                    panic!("transfer failed while waiting for {expected:?}");
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("transfer timeout");
    }

    #[tokio::test]
    #[ignore = "requires docker compose OpenSSH fixture"]
    async fn network_interruption_is_observed_and_changed_host_key_is_blocked() {
        let _guard = integration_lock().lock().await;
        let original_fingerprint = current_fingerprint().await;
        let mut session = SshService::connect(
            terminal_request(SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            }),
            original_fingerprint.clone(),
        )
        .await
        .expect("connect before interruption");

        docker_compose(&["stop", "openssh"]);
        let closed = tokio::time::timeout(Duration::from_secs(15), session.reader.wait())
            .await
            .expect("transport did not observe interruption");
        assert!(closed.is_none() || matches!(closed, Some(ChannelMsg::Close | ChannelMsg::Eof)));

        docker_compose(&["up", "-d", "--force-recreate", "openssh"]);
        wait_for_fixture().await;
        let changed_fingerprint = current_fingerprint().await;
        assert_ne!(original_fingerprint, changed_fingerprint);

        let directory = tempfile::tempdir().expect("temp directory");
        let known_hosts = KnownHostService::new(
            KnownHostRepository::new(JsonRepository::new(
                directory.path().join("known-hosts.json"),
            )),
            Arc::new(TokioMutex::new(())),
        );
        let remembered = known_hosts
            .prepare(
                "127.0.0.1",
                2222,
                HostKeyInfo {
                    key_type: "ssh-ed25519".into(),
                    fingerprint: original_fingerprint,
                },
            )
            .await
            .expect("prepare original host key");
        known_hosts
            .trust(remembered.attempt_id, true)
            .await
            .expect("remember original host key");
        let changed = SshService::scan_host_key("127.0.0.1", 2222)
            .await
            .expect("scan changed host key");
        assert!(matches!(
            known_hosts.prepare("127.0.0.1", 2222, changed).await,
            Err(AppError::HostKeyChanged)
        ));
    }
}
