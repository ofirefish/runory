//! Real Docker OpenSSH E2E for plugin workflows (opt-in / ignored).

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::domain::{ConnectRequest, SshAuthentication, SshConnectionRequest};
use crate::plugins::api::WorkflowPlanRequest;
use crate::plugins::test_support::{pack_fixture_hello, PluginTestHarness};
use crate::plugins::workflow::state::WorkflowRunState;
use crate::ssh::{ServerSessionManager, SshService};

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

async fn current_fingerprint() -> String {
    SshService::scan_host_key("127.0.0.1", 2222)
        .await
        .expect("scan host key")
        .fingerprint
}

fn password_connection() -> ConnectRequest {
    ConnectRequest {
        connection: SshConnectionRequest {
            host: "127.0.0.1".into(),
            port: 2222,
            username: "runory".into(),
            authentication: SshAuthentication::Password {
                password: zeroize::Zeroizing::new("runory-spike".into()),
            },
        },
        cols: 80,
        rows: 24,
    }
}

#[tokio::test]
#[ignore = "requires docker compose up -d --build openssh"]
async fn p51_hello_e2e_with_real_openssh_session() {
    wait_for_fixture().await;
    let sessions = Arc::new(ServerSessionManager::default());
    let fingerprint = current_fingerprint().await;
    let session_id = sessions
        .connect(
            Uuid::new_v4(),
            password_connection(),
            fingerprint,
            tauri::ipc::Channel::new(|_| Ok(())),
        )
        .await
        .expect("connect");

    let harness = PluginTestHarness::new();
    // Replace executor sessions by installing into a harness that shares manager —
    // re-plan using registry from harness but executor wired to real sessions.
    let zip = harness._root.path().join("hello.runory-plugin");
    pack_fixture_hello(&zip);
    let plugin_id = harness.install_zip(&zip).await;

    let executor = Arc::new(crate::plugins::workflow::WorkflowExecutor::with_services(
        Arc::clone(&harness.registry),
        Arc::clone(&sessions),
        None,
        None,
    ));

    let mismatch = executor
        .plan(WorkflowPlanRequest {
            plugin_id: plugin_id.clone(),
            workflow_id: "hello.inspect".into(),
            host_id: Some(Uuid::new_v4().to_string()),
            session_id: Some(session_id.to_string()),
            inputs: serde_json::json!({}),
        })
        .await;
    assert!(matches!(
        mismatch,
        Err(crate::plugins::error::PluginError::PolicyDenied)
    ));

    let plan = executor
        .plan(WorkflowPlanRequest {
            plugin_id: plugin_id.clone(),
            workflow_id: "hello.inspect".into(),
            host_id: Some(
                sessions
                    .profile_id(session_id)
                    .await
                    .expect("profile")
                    .to_string(),
            ),
            session_id: Some(session_id.to_string()),
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");

    let run = executor
        .execute(&plan.id, None)
        .await
        .expect("execute hello over real SSH");
    assert_eq!(run.state, WorkflowRunState::Succeeded);
    assert!(run.steps.iter().any(|s| s.action_id == "ssh.command"));
    let uname = run
        .steps
        .iter()
        .find(|s| s.step_id == "uname")
        .expect("uname step");
    assert!(
        uname
            .stdout
            .as_deref()
            .is_some_and(|s| s.to_ascii_lowercase().contains("linux")),
        "expected linux uname, got {:?}",
        uname.stdout
    );
}

#[tokio::test]
#[ignore = "requires docker compose up -d --build openssh with nginx"]
async fn p65_nginx_manager_plan_binds_changeset_when_session_present() {
    wait_for_fixture().await;
    let sessions = Arc::new(ServerSessionManager::default());
    let fingerprint = current_fingerprint().await;
    let session_id = sessions
        .connect(
            Uuid::new_v4(),
            password_connection(),
            fingerprint,
            tauri::ipc::Channel::new(|_| Ok(())),
        )
        .await
        .expect("connect");

    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.nginx-manager");
    let zip = harness._root.path().join("nginx-manager.runory-plugin");
    {
        use std::fs;
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;
        let file = fs::File::create(&zip).expect("create");
        let mut archive = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        fn walk(
            dir: &std::path::Path,
            root: &std::path::Path,
            zip: &mut ZipWriter<fs::File>,
            options: SimpleFileOptions,
        ) {
            for entry in fs::read_dir(dir).expect("rd").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, zip, options);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .expect("rel")
                        .to_string_lossy()
                        .replace('\\', "/");
                    zip.start_file(&rel, options).expect("start");
                    zip.write_all(&fs::read(&path).expect("read"))
                        .expect("bytes");
                }
            }
        }
        walk(&fixture, &fixture, &mut archive, options);
        archive.finish().expect("finish");
    }

    let plugin_id = harness.install_zip(&zip).await;
    let changes = crate::agentic::ChangeSetService::default();
    let policies = crate::policy::AgentPolicyService::default();
    let executor = Arc::new(crate::plugins::workflow::WorkflowExecutor::with_services(
        Arc::clone(&harness.registry),
        Arc::clone(&sessions),
        Some(changes),
        Some(policies),
    ));

    let plan = executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "nginx.reload".into(),
            host_id: Some(
                sessions
                    .profile_id(session_id)
                    .await
                    .expect("profile")
                    .to_string(),
            ),
            session_id: Some(session_id.to_string()),
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan nginx.reload");
    let cs = plan.changeset.expect("changeset");
    assert!(cs.requires_changeset);
    assert!(cs.change_set_id.is_some());
    assert_eq!(cs.change_set_version, Some(1));
}

#[tokio::test]
#[ignore = "requires docker OpenSSH + Ubuntu Django sample host"]
async fn django_preflight_e2e_plans_r0_over_real_session() {
    wait_for_fixture().await;
    let sessions = Arc::new(ServerSessionManager::default());
    let fingerprint = current_fingerprint().await;
    let session_id = sessions
        .connect(
            Uuid::new_v4(),
            password_connection(),
            fingerprint,
            tauri::ipc::Channel::new(|_| Ok(())),
        )
        .await
        .expect("connect");

    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.django-deployer");
    let zip = harness
        ._root
        .path()
        .join("django-preflight-e2e.runory-plugin");
    {
        use std::fs;
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;
        let file = fs::File::create(&zip).expect("create");
        let mut archive = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        fn walk(
            dir: &std::path::Path,
            root: &std::path::Path,
            zip: &mut ZipWriter<fs::File>,
            options: SimpleFileOptions,
        ) {
            for entry in fs::read_dir(dir).expect("rd").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, zip, options);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .expect("rel")
                        .to_string_lossy()
                        .replace('\\', "/");
                    zip.start_file(&rel, options).expect("start");
                    zip.write_all(&fs::read(&path).expect("read"))
                        .expect("bytes");
                }
            }
        }
        walk(&fixture, &fixture, &mut archive, options);
        archive.finish().expect("finish");
    }

    let plugin_id = harness.install_zip(&zip).await;
    let executor = Arc::new(crate::plugins::workflow::WorkflowExecutor::with_services(
        Arc::clone(&harness.registry),
        Arc::clone(&sessions),
        None,
        None,
    ));

    let plan = executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "django.preflight".into(),
            host_id: Some(
                sessions
                    .profile_id(session_id)
                    .await
                    .expect("profile")
                    .to_string(),
            ),
            session_id: Some(session_id.to_string()),
            inputs: serde_json::json!({ "port": 8000 }),
        })
        .await
        .expect("django preflight plan");
    assert_eq!(plan.risk, crate::tools::RiskLevel::R0);
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action_id == "system.preflight_report"));
}
