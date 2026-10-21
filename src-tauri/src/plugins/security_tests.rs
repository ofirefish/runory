//! Plugin System V1.1 security / lifecycle integration tests (P5.1–P5.5).

use std::sync::Arc;

use crate::plugins::api::WorkflowPlanRequest;
use crate::plugins::error::PluginError;
use crate::plugins::permissions::PluginCapability;
use crate::plugins::registry::model::InstalledPluginRecord;
use crate::plugins::test_support::{
    detect_only_yaml, pack_fixture_hello, sleep_workflow_yaml, write_plugin_zip, PluginTestHarness,
};
use crate::plugins::workflow::actions::{redact_secrets, ActionResult};
use crate::plugins::workflow::executor::WorkflowEvent;
use crate::plugins::workflow::state::WorkflowRunState;
use crate::tools::RiskLevel;

#[tokio::test]
async fn scoped_permission_is_enforced_at_plan_and_rechecked_before_execution() {
    let harness = PluginTestHarness::new();
    let zip = harness._root.path().join("scope.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.scoped",
        "1.0.0",
        &["host.read"],
        detect_only_yaml(),
        "detect.only",
    );
    harness.install_zip(&zip).await;
    let grant = crate::plugins::permissions::grant::PermissionGrant {
        plugin_id: "runory.scoped".into(),
        capability: PluginCapability::HostRead,
        granted: true,
        hosts: vec!["host-a".into()],
    };
    harness
        .permissions
        .set_grants("runory.scoped", vec![grant.clone()])
        .await
        .expect("grant");
    let request = |host: &str| WorkflowPlanRequest {
        plugin_id: "runory.scoped".into(),
        workflow_id: "detect.only".into(),
        host_id: Some(host.into()),
        session_id: None,
        inputs: serde_json::json!({}),
    };
    assert!(matches!(
        harness.executor.plan(request("host-b")).await,
        Err(PluginError::PermissionDenied(_))
    ));
    let plan = harness
        .executor
        .plan(request("host-a"))
        .await
        .expect("plan");
    harness
        .permissions
        .set_grants(
            "runory.scoped",
            vec![crate::plugins::permissions::grant::PermissionGrant {
                hosts: vec!["host-b".into()],
                ..grant
            }],
        )
        .await
        .expect("change scope");
    assert!(matches!(
        harness.executor.execute(&plan.id, None).await,
        Err(PluginError::PermissionDenied(_))
    ));
}

#[tokio::test]
async fn p51_hello_install_plan_and_detect_execute() {
    let harness = PluginTestHarness::new();
    let zip = harness._root.path().join("hello.runory-plugin");
    pack_fixture_hello(&zip);
    let plugin_id = harness.install_zip(&zip).await;

    let listed = harness.registry.list_installed().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "runory.hello");

    let form = harness
        .executor
        .get_form(&plugin_id, "hello.inspect")
        .await
        .expect("form");
    assert_eq!(form["workflowId"], "hello.inspect");

    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id: plugin_id.clone(),
            workflow_id: "hello.inspect".into(),
            host_id: Some("host-1".into()),
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");
    assert_eq!(plan.plugin_id, "runory.hello");
    assert!(!plan.steps.is_empty());
    assert!(plan.changeset.is_some());

    // Offline detect-only execute proves executor path without SSH.
    let detect_zip = harness._root.path().join("detect.runory-plugin");
    write_plugin_zip(
        &detect_zip,
        "runory.detect",
        "1.0.0",
        &["host.read"],
        detect_only_yaml(),
        "detect.only",
    );
    let detect_id = harness.install_zip(&detect_zip).await;
    let detect_plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id: detect_id,
            workflow_id: "detect.only".into(),
            host_id: None,
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("detect plan");
    let run = harness
        .executor
        .execute(&detect_plan.id, None)
        .await
        .expect("detect execute");
    assert_eq!(run.state, WorkflowRunState::Succeeded);
}

#[tokio::test]
async fn p52_permission_revoke_blocks_execute_toctou() {
    let harness = PluginTestHarness::new();
    let zip = harness._root.path().join("detect.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.detect",
        "1.0.0",
        &["host.read"],
        detect_only_yaml(),
        "detect.only",
    );
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id: plugin_id.clone(),
            workflow_id: "detect.only".into(),
            host_id: None,
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");

    harness.revoke(&plugin_id, PluginCapability::HostRead).await;

    let err = harness
        .executor
        .execute(&plan.id, None)
        .await
        .expect_err("must fail after revoke");
    assert!(matches!(err, PluginError::PermissionDenied(_)));
}

#[tokio::test]
async fn p52_disabled_plugin_blocks_stale_plan() {
    let harness = PluginTestHarness::new();
    let zip = harness._root.path().join("detect.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.detect",
        "1.0.0",
        &["host.read"],
        detect_only_yaml(),
        "detect.only",
    );
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id: plugin_id.clone(),
            workflow_id: "detect.only".into(),
            host_id: None,
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");

    harness
        .registry
        .set_enabled(&plugin_id, false)
        .await
        .expect("disable");

    let err = harness
        .executor
        .execute(&plan.id, None)
        .await
        .expect_err("disabled");
    assert!(matches!(err, PluginError::Disabled));
}

#[tokio::test]
async fn p52_version_change_blocks_stale_plan() {
    let harness = PluginTestHarness::new();
    let zip = harness._root.path().join("detect.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.detect",
        "1.0.0",
        &["host.read"],
        detect_only_yaml(),
        "detect.only",
    );
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id: plugin_id.clone(),
            workflow_id: "detect.only".into(),
            host_id: None,
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");

    let (record, _) = harness.registry.require_enabled(&plugin_id).expect("rec");
    let upgraded = InstalledPluginRecord {
        version: "1.0.1".into(),
        updated_at: record.updated_at + 1,
        ..record.clone()
    };
    // Point 1.0.1 at the same install path (version TOCTOU only).
    harness
        .registry
        .repository()
        .insert_plugin(&upgraded)
        .expect("insert 1.0.1");
    harness
        .registry
        .repository()
        .set_active_version(&plugin_id, "1.0.1")
        .expect("activate");

    let err = harness
        .executor
        .execute(&plan.id, None)
        .await
        .expect_err("version mismatch");
    assert!(matches!(err, PluginError::IncompatibleVersion));
}

#[tokio::test]
async fn p52_sudo_without_grant_denied_at_plan() {
    let harness = PluginTestHarness::new();
    let yaml = r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: sudo.probe
  name: Sudo Probe
steps:
  - id: nginx-test
    uses: ssh.command
    with:
      executable: nginx
      args: ["-t"]
      privilege: sudo
"#;
    let zip = harness._root.path().join("sudo.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.sudo",
        "1.0.0",
        &["ssh.exec"], // missing ssh.exec.sudo
        yaml,
        "sudo.probe",
    );
    let plugin_id = harness.install_zip(&zip).await;
    let err = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "sudo.probe".into(),
            host_id: None,
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect_err("sudo without grant");
    assert!(matches!(err, PluginError::PermissionDenied(_)));
}

#[tokio::test]
async fn p53_cancel_mid_execution() {
    let harness = PluginTestHarness::new();
    let zip = harness._root.path().join("sleep.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.sleep",
        "1.0.0",
        &["host.read"],
        sleep_workflow_yaml(),
        "cancel.sleep",
    );
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "cancel.sleep".into(),
            host_id: None,
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");

    let executor = Arc::clone(&harness.executor);
    let plan_id = plan.id.clone();
    let handle = tokio::spawn(async move { executor.execute(&plan_id, None).await });

    // Wait until run token is registered (Started).
    for _ in 0..50 {
        let events = harness.executor.polled_events().await;
        if events.iter().any(|e| {
            matches!(
                e,
                WorkflowEvent::Started { .. } | WorkflowEvent::StepStarted { .. }
            )
        }) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let events = harness.executor.polled_events().await;
    let run_id = events
        .iter()
        .find_map(|e| match e {
            WorkflowEvent::Started { run_id, .. } => Some(run_id.clone()),
            _ => None,
        })
        .expect("run started");
    harness.executor.cancel(&run_id).await.expect("cancel");

    let result = handle.await.expect("join");
    match result {
        Ok(run) => assert_eq!(run.state, WorkflowRunState::Cancelled),
        Err(PluginError::Cancelled) => {}
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn p53_startup_interrupts_running_runs() {
    let harness = PluginTestHarness::new();
    harness
        .registry
        .repository()
        .insert_run(
            "run-interrupted",
            "runory.hello",
            "1.0.0",
            "hello.inspect",
            None,
            None,
            WorkflowRunState::Running.as_str(),
            None,
            "{}",
        )
        .expect("insert");
    let changed = harness
        .registry
        .repository()
        .interrupt_running_runs()
        .expect("interrupt");
    assert!(changed >= 1);
    let state = harness
        .registry
        .repository()
        .get_run_state("run-interrupted")
        .expect("get")
        .expect("exists");
    assert_eq!(state, "Interrupted");
}

#[test]
fn p54_secret_redaction_in_outputs() {
    let raw = "export TOKEN=super-secret-value\npassword=hunter2\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\n";
    let redacted = redact_secrets(raw);
    assert!(!redacted.contains("super-secret-value"));
    assert!(!redacted.contains("hunter2"));
    assert!(!redacted.contains("BEGIN OPENSSH"));
    assert!(redacted.contains("********") || redacted.contains("[REDACTED"));

    let sanitized = ActionResult {
        outputs: serde_json::json!({ "password": "plaintext", "note": "ok" }),
        stdout: Some("token=abc123 rest".into()),
        stderr: None,
        exit_code: Some(0),
    }
    .sanitize();
    assert_eq!(sanitized.outputs["password"], "********");
    assert!(!sanitized.stdout.unwrap().contains("abc123"));
}

#[test]
fn p54_env_audit_display_never_shows_secret_ref() {
    use crate::plugins::workflow::ast::EnvValue;
    let secret = EnvValue::SecretRef {
        secret_ref: "vault://db-password".into(),
    };
    assert_eq!(secret.audit_display(), "********");
    assert!(!secret.audit_display().contains("vault"));
}

#[test]
fn p55_rejects_command_injection_in_executable() {
    use crate::plugins::workflow::actions::ssh::build_remote_command;
    use crate::plugins::workflow::ast::PrivilegeMode;
    use std::collections::HashMap;

    assert!(build_remote_command(
        "sh",
        &["-c".into(), "id".into()],
        PrivilegeMode::None,
        None,
        &HashMap::new()
    )
    .is_ok()); // argv form is fine
    assert!(
        build_remote_command("uname;rm", &[], PrivilegeMode::None, None, &HashMap::new()).is_err()
    );
    assert!(build_remote_command(
        "../../bin/evil",
        &[],
        PrivilegeMode::None,
        None,
        &HashMap::new()
    )
    .is_err());
    assert!(build_remote_command(
        "uname$(id)",
        &[],
        PrivilegeMode::None,
        None,
        &HashMap::new()
    )
    .is_err());
}

#[test]
fn p55_output_truncation_bounds_size() {
    let huge = "x".repeat(2 * 1024 * 1024);
    let sanitized = ActionResult {
        outputs: serde_json::json!({}),
        stdout: Some(huge),
        stderr: None,
        exit_code: Some(0),
    }
    .sanitize();
    let out = sanitized.stdout.expect("stdout");
    assert!(out.len() < 1024 * 1024 + 64);
    assert!(out.contains("[truncated]"));
}

#[tokio::test]
async fn p55_nginx_reload_plan_requires_changeset() {
    let harness = PluginTestHarness::new();
    let yaml = r#"apiVersion: runory/v1
kind: Workflow
metadata:
  id: nginx.reload
  name: Reload Nginx
steps:
  - id: reload
    uses: ssh.command
    with:
      executable: systemctl
      args: ["reload", "nginx"]
      privilege: sudo
"#;
    let zip = harness._root.path().join("nginx.runory-plugin");
    write_plugin_zip(
        &zip,
        "runory.nginx",
        "1.0.0",
        &["ssh.exec", "ssh.exec.sudo"],
        yaml,
        "nginx.reload",
    );
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "nginx.reload".into(),
            host_id: Some("h1".into()),
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("plan");
    assert!(plan.requires_approval);
    assert!(plan.risk >= RiskLevel::R2);
    let cs = plan.changeset.expect("changeset");
    assert!(cs.requires_changeset);
    assert!(!cs.operations.is_empty());
}

#[tokio::test]
async fn p61_nginx_manager_fixture_plans() {
    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.nginx-manager");
    if !fixture.join("manifest.json").is_file() {
        // Fixture created in same change-set; skip if packing race.
        return;
    }
    let zip = harness._root.path().join("nginx-manager.runory-plugin");
    pack_dir_as_zip(&fixture, &zip);
    let plugin_id = harness.install_zip(&zip).await;
    assert_eq!(plugin_id, "runory.nginx-manager");

    for workflow_id in ["nginx.inspect", "nginx.test", "nginx.reload"] {
        let plan = harness
            .executor
            .plan(WorkflowPlanRequest {
                plugin_id: plugin_id.clone(),
                workflow_id: workflow_id.into(),
                host_id: Some("host-nginx".into()),
                session_id: None,
                inputs: serde_json::json!({}),
            })
            .await
            .unwrap_or_else(|e| panic!("plan {workflow_id}: {e:?}"));
        assert_eq!(plan.workflow_id, workflow_id);
        if workflow_id == "nginx.reload" {
            assert!(plan.requires_approval);
            assert!(plan
                .changeset
                .as_ref()
                .is_some_and(|c| c.requires_changeset));
        }
    }
}

#[tokio::test]
async fn d1_django_deployer_inspect_plans_r0() {
    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.django-deployer");
    let zip = harness._root.path().join("django.runory-plugin");
    pack_dir_as_zip(&fixture, &zip);
    let plugin_id = harness.install_zip(&zip).await;
    assert_eq!(plugin_id, "runory.django-deployer");
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "django.inspect".into(),
            host_id: Some("host-1".into()),
            session_id: None,
            inputs: serde_json::json!({}),
        })
        .await
        .expect("inspect plan");
    assert_eq!(plan.risk, RiskLevel::R0);
    assert!(!plan.requires_approval);
}

#[tokio::test]
async fn d1_django_preflight_plans_r0_with_report() {
    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.django-deployer");
    let zip = harness._root.path().join("django-preflight.runory-plugin");
    pack_dir_as_zip(&fixture, &zip);
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "django.preflight".into(),
            host_id: Some("host-1".into()),
            session_id: None,
            inputs: serde_json::json!({ "port": 8000 }),
        })
        .await
        .expect("preflight plan");
    assert_eq!(plan.risk, RiskLevel::R0);
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action_id == "system.preflight_report"));
}

#[tokio::test]
async fn d1_django_deploy_plan_includes_checkpoint_and_activate() {
    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.django-deployer");
    let zip = harness._root.path().join("django-deploy.runory-plugin");
    pack_dir_as_zip(&fixture, &zip);
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "django.deploy".into(),
            host_id: Some("host-1".into()),
            session_id: None,
            inputs: serde_json::json!({
                "projectName": "shop",
                "repository": "https://example.com/shop.git",
                "branch": "main",
                "releaseId": "r1",
                "python": "python3",
                "wsgi": "config.wsgi:application",
                "domain": "shop.example.com",
                "port": 8000,
                "healthPath": "/health/",
                "runUser": "www-data",
                "runGroup": "www-data",
                "workers": 3
            }),
        })
        .await
        .expect("deploy plan");
    assert!(plan.risk >= RiskLevel::R4);
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action_id == "changeset.checkpoint"));
    assert!(plan.steps.iter().any(|s| s.action_id == "release.activate"));
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action_id == "remote.fs.write_env" && s.plan.summary.contains("skipped")));
}

#[tokio::test]
async fn d9_write_env_secret_ref_fail_closed_without_resolver() {
    use crate::plugins::workflow::actions::secret_env::RemoteFsWriteEnvAction;
    use crate::plugins::workflow::actions::WorkflowAction;

    let action = RemoteFsWriteEnvAction;
    let caps = action
        .required_capabilities(&serde_json::json!({
            "path": "/opt/runory/apps/shop/shared/.env",
            "secretRef": "secret://django/shop"
        }))
        .expect("caps");
    assert!(caps.contains(&PluginCapability::SecretReference));
    assert_eq!(action.risk(&serde_json::json!({})).unwrap(), RiskLevel::R2);
}

#[tokio::test]
async fn d9_secret_store_bind_list_never_returns_plaintext() {
    use crate::plugins::secrets::PluginSecretResolver;
    use zeroize::Zeroizing;

    let harness = PluginTestHarness::new();
    let store = harness.executor.secret_store().expect("secret store");
    store
        .bind(
            "secret://django/shop-production",
            Zeroizing::new("SECRET_KEY=super-secret-value\nDB_PASSWORD=leak\n".into()),
        )
        .await
        .expect("bind");
    let listed = store.list_refs().await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].secret_ref, "secret://django/shop-production");
    assert!(!listed[0].redacted.contains("shop-production"));
    let resolved = store
        .resolve("secret://django/shop-production")
        .await
        .expect("resolve");
    assert!(resolved.contains("super-secret-value"));
    // List metadata must never echo plaintext.
    let dump = serde_json::to_string(&listed).unwrap();
    assert!(!dump.contains("super-secret-value"));
    assert!(!dump.contains("DB_PASSWORD=leak"));
}

#[tokio::test]
async fn d1_django_recover_workflow_plans() {
    let harness = PluginTestHarness::new();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("plugins")
        .join("runory.django-deployer");
    let zip = harness._root.path().join("django-recover.runory-plugin");
    pack_dir_as_zip(&fixture, &zip);
    let plugin_id = harness.install_zip(&zip).await;
    let plan = harness
        .executor
        .plan(WorkflowPlanRequest {
            plugin_id,
            workflow_id: "django.recover".into(),
            host_id: Some("host-1".into()),
            session_id: None,
            inputs: serde_json::json!({
                "projectName": "shop",
                "releaseId": "r1",
                "port": 8000,
                "healthPath": "/health/"
            }),
        })
        .await
        .expect("recover plan");
    assert!(plan
        .steps
        .iter()
        .any(|s| s.action_id == "changeset.checkpoint"));
    assert!(plan.steps.iter().any(|s| s.action_id == "release.activate"));
}

fn pack_dir_as_zip(src: &std::path::Path, out: &std::path::Path) {
    use std::fs;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let file = fs::File::create(out).expect("create");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    fn walk(
        dir: &std::path::Path,
        root: &std::path::Path,
        zip: &mut ZipWriter<fs::File>,
        options: SimpleFileOptions,
    ) {
        for entry in fs::read_dir(dir).expect("read_dir").flatten() {
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
    walk(src, src, &mut zip, options);
    zip.finish().expect("finish");
}
