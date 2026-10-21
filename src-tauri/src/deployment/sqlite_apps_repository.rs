//! SQLite-backed Deployment App inventory (`deployment.db`).
//!
//! Separated from `runory.db` so catalog migrations stay independent.
//! One-shot import from legacy `deployment-apps.json` on first open.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

use crate::deployment::go_validation::{
    go_service_name, validate_go_config, validate_go_config_partial, validate_go_path,
};
use crate::deployment::html_validation::{
    html_nginx_conf_name, validate_html_config, validate_html_config_partial, validate_html_path,
};
use crate::deployment::java_validation::{
    java_service_name, validate_java_config, validate_java_config_partial, validate_java_path,
};
use crate::deployment::node_validation::{
    node_service_name, validate_node_config, validate_node_config_partial, validate_node_path,
};
use crate::deployment::php_validation::{
    php_service_name, validate_php_config, validate_php_config_partial, validate_php_path,
};
use crate::deployment::proxy_validation::{
    proxy_nginx_conf_name, proxy_site_path, validate_proxy_config, validate_proxy_config_partial,
};
use crate::deployment::validation::{
    python_service_name, validate_app_name, validate_branch, validate_python_config,
    validate_python_config_partial, validate_remote_path, validate_remote_url, validate_restart,
};
use crate::domain::{
    AppError, AppResult, BuildPreset, DeploymentApp, DeploymentSetupStatus, DeploymentSourceKind,
    GoProjectConfig, HtmlProjectConfig, JavaProjectConfig, NodeProjectConfig, PhpProjectConfig,
    ProjectKind, PythonProjectConfig, RestartTarget, ReverseProxyConfig,
};

const SCHEMA_VERSION: i64 = 1;
const MAX_APPS_PER_PROFILE: usize = 50;

#[derive(Clone)]
pub struct DeploymentDatabase {
    connection: Arc<Mutex<Connection>>,
}

impl DeploymentDatabase {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AppError::Storage)?;
        }
        let connection = Connection::open(path).map_err(|_| AppError::Storage)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = FULL;
                 CREATE TABLE IF NOT EXISTS deployment_schema_version (
                    version INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS deployment_apps (
                    id TEXT PRIMARY KEY NOT NULL,
                    profile_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    repository_path TEXT NOT NULL,
                    remote_url TEXT NOT NULL,
                    branch TEXT NOT NULL,
                    build_json TEXT NOT NULL,
                    restart_json TEXT NOT NULL,
                    project_kind TEXT NOT NULL,
                    python_json TEXT,
                    setup_step INTEGER NOT NULL,
                    setup_status TEXT NOT NULL,
                    source_kind TEXT NOT NULL,
                    venv_path TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS deployment_apps_profile_name_idx
                    ON deployment_apps (profile_id, name);
                 CREATE UNIQUE INDEX IF NOT EXISTS deployment_apps_profile_path_idx
                    ON deployment_apps (profile_id, repository_path);
                 CREATE INDEX IF NOT EXISTS deployment_apps_profile_updated_idx
                    ON deployment_apps (profile_id, updated_at DESC);",
            )
            .map_err(|_| AppError::Storage)?;
        add_column_if_missing(&connection, "php_json", "TEXT")?;
        add_column_if_missing(&connection, "java_json", "TEXT")?;
        add_column_if_missing(&connection, "node_json", "TEXT")?;
        add_column_if_missing(&connection, "go_json", "TEXT")?;
        add_column_if_missing(&connection, "html_json", "TEXT")?;
        add_column_if_missing(&connection, "proxy_json", "TEXT")?;

        let version: Option<i64> = connection
            .query_row(
                "SELECT version FROM deployment_schema_version LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AppError::Storage)?;
        if version.is_none() {
            connection
                .execute(
                    "INSERT INTO deployment_schema_version (version) VALUES (?1)",
                    params![SCHEMA_VERSION],
                )
                .map_err(|_| AppError::Storage)?;
        }

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    fn with_connection<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        f(&connection)
    }

    fn list_all(&self) -> AppResult<Vec<DeploymentApp>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, profile_id, name, repository_path, remote_url, branch,
                            build_json, restart_json, project_kind, python_json,
                            setup_step, setup_status, source_kind, venv_path, updated_at, php_json, java_json, node_json, go_json, html_json, proxy_json
                     FROM deployment_apps
                     ORDER BY updated_at DESC, name ASC",
                )
                .map_err(|_| AppError::Storage)?;
            let rows = statement
                .query_map([], map_app_row)
                .map_err(|_| AppError::Storage)?;
            let mut apps = Vec::new();
            for row in rows {
                apps.push(row.map_err(|_| AppError::Storage)?);
            }
            Ok(apps)
        })
    }

    fn list_for_profile(&self, profile_id: Uuid) -> AppResult<Vec<DeploymentApp>> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, profile_id, name, repository_path, remote_url, branch,
                            build_json, restart_json, project_kind, python_json,
                            setup_step, setup_status, source_kind, venv_path, updated_at, php_json, java_json, node_json, go_json, html_json, proxy_json
                     FROM deployment_apps
                     WHERE profile_id = ?1
                     ORDER BY updated_at DESC, name ASC",
                )
                .map_err(|_| AppError::Storage)?;
            let rows = statement
                .query_map(params![profile_id.to_string()], map_app_row)
                .map_err(|_| AppError::Storage)?;
            let mut apps = Vec::new();
            for row in rows {
                apps.push(row.map_err(|_| AppError::Storage)?);
            }
            Ok(apps)
        })
    }

    fn replace_all(&self, apps: &[DeploymentApp]) -> AppResult<()> {
        self.with_connection(|connection| {
            let tx = connection
                .unchecked_transaction()
                .map_err(|_| AppError::Storage)?;
            tx.execute("DELETE FROM deployment_apps", [])
                .map_err(|_| AppError::Storage)?;
            for app in apps {
                insert_app(&tx, app)?;
            }
            tx.commit().map_err(|_| AppError::Storage)?;
            Ok(())
        })
    }

    fn upsert_row(&self, app: &DeploymentApp) -> AppResult<()> {
        self.with_connection(|connection| {
            let tx = connection
                .unchecked_transaction()
                .map_err(|_| AppError::Storage)?;
            tx.execute(
                "DELETE FROM deployment_apps WHERE id = ?1",
                params![app.id.to_string()],
            )
            .map_err(|_| AppError::Storage)?;
            insert_app(&tx, app)?;
            tx.commit().map_err(|_| AppError::Storage)?;
            Ok(())
        })
    }

    fn delete_row(&self, profile_id: Uuid, id: Uuid) -> AppResult<bool> {
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "DELETE FROM deployment_apps WHERE id = ?1 AND profile_id = ?2",
                    params![id.to_string(), profile_id.to_string()],
                )
                .map_err(|_| AppError::Storage)?;
            Ok(changed > 0)
        })
    }

    fn count_for_profile(&self, profile_id: Uuid) -> AppResult<usize> {
        self.with_connection(|connection| {
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM deployment_apps WHERE profile_id = ?1",
                    params![profile_id.to_string()],
                    |row| row.get(0),
                )
                .map_err(|_| AppError::Storage)?;
            Ok(count as usize)
        })
    }

    fn get(&self, profile_id: Uuid, id: Uuid) -> AppResult<Option<DeploymentApp>> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, profile_id, name, repository_path, remote_url, branch,
                            build_json, restart_json, project_kind, python_json,
                            setup_step, setup_status, source_kind, venv_path, updated_at, php_json, java_json, node_json, go_json, html_json, proxy_json
                     FROM deployment_apps
                     WHERE id = ?1 AND profile_id = ?2",
                    params![id.to_string(), profile_id.to_string()],
                    map_app_row,
                )
                .optional()
                .map_err(|_| AppError::Storage)
        })
    }
}

/// Additive schema change for databases created before the column existed.
fn add_column_if_missing(connection: &Connection, column: &str, definition: &str) -> AppResult<()> {
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('deployment_apps') WHERE name = ?1",
            params![column],
            |row| row.get(0),
        )
        .map_err(|_| AppError::Storage)?;
    if exists == 0 {
        connection
            .execute_batch(&format!(
                "ALTER TABLE deployment_apps ADD COLUMN {column} {definition}"
            ))
            .map_err(|_| AppError::Storage)?;
    }
    Ok(())
}

fn insert_app(connection: &Connection, app: &DeploymentApp) -> AppResult<()> {
    let build_json = serde_json::to_string(&app.build).map_err(|_| AppError::Storage)?;
    let restart_json = serde_json::to_string(&app.restart).map_err(|_| AppError::Storage)?;
    let project_kind = serde_json::to_string(&app.project_kind).map_err(|_| AppError::Storage)?;
    let python_json = match &app.python {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let php_json = match &app.php {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let java_json = match &app.java {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let node_json = match &app.node {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let go_json = match &app.go {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let html_json = match &app.html {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let proxy_json = match &app.proxy {
        Some(config) => Some(serde_json::to_string(config).map_err(|_| AppError::Storage)?),
        None => None,
    };
    let setup_status = serde_json::to_string(&app.setup_status).map_err(|_| AppError::Storage)?;
    let source_kind = serde_json::to_string(&app.source_kind).map_err(|_| AppError::Storage)?;
    connection
        .execute(
            "INSERT INTO deployment_apps (
                id, profile_id, name, repository_path, remote_url, branch,
                build_json, restart_json, project_kind, python_json,
                setup_step, setup_status, source_kind, venv_path, updated_at, php_json, java_json, node_json, go_json, html_json, proxy_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            params![
                app.id.to_string(),
                app.profile_id.to_string(),
                app.name,
                app.repository_path,
                app.remote_url,
                app.branch,
                build_json,
                restart_json,
                project_kind.trim_matches('"'),
                python_json,
                app.setup_step as i64,
                setup_status.trim_matches('"'),
                source_kind.trim_matches('"'),
                app.venv_path,
                app.updated_at_epoch_seconds as i64,
                php_json,
                java_json,
                node_json,
                go_json,
                html_json,
                proxy_json,
            ],
        )
        .map_err(|_| AppError::Storage)?;
    Ok(())
}

fn map_app_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeploymentApp> {
    let id: String = row.get(0)?;
    let profile_id: String = row.get(1)?;
    let build_json: String = row.get(6)?;
    let restart_json: String = row.get(7)?;
    let project_kind: String = row.get(8)?;
    let python_json: Option<String> = row.get(9)?;
    let setup_step: i64 = row.get(10)?;
    let setup_status: String = row.get(11)?;
    let source_kind: String = row.get(12)?;
    let updated_at: i64 = row.get(14)?;
    let php_json: Option<String> = row.get(15)?;
    let java_json: Option<String> = row.get(16)?;
    let node_json: Option<String> = row.get(17)?;
    let go_json: Option<String> = row.get(18)?;
    let html_json: Option<String> = row.get(19)?;
    let proxy_json: Option<String> = row.get(20)?;

    Ok(DeploymentApp {
        id: Uuid::parse_str(&id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        profile_id: Uuid::parse_str(&profile_id).map_err(|_| rusqlite::Error::InvalidQuery)?,
        name: row.get(2)?,
        repository_path: row.get(3)?,
        remote_url: row.get(4)?,
        branch: row.get(5)?,
        build: serde_json::from_str(&build_json).map_err(|_| rusqlite::Error::InvalidQuery)?,
        restart: serde_json::from_str(&restart_json).map_err(|_| rusqlite::Error::InvalidQuery)?,
        project_kind: serde_json::from_str(&format!("\"{project_kind}\""))
            .or_else(|_| serde_json::from_str(&project_kind))
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
        python: match python_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        php: match php_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        java: match java_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        node: match node_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        go: match go_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        html: match html_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        proxy: match proxy_json {
            Some(raw) if !raw.is_empty() => {
                Some(serde_json::from_str(&raw).map_err(|_| rusqlite::Error::InvalidQuery)?)
            }
            _ => None,
        },
        setup_step: setup_step.clamp(1, 4) as u8,
        setup_status: serde_json::from_str(&format!("\"{setup_status}\""))
            .or_else(|_| serde_json::from_str(&setup_status))
            .unwrap_or(DeploymentSetupStatus::Draft),
        source_kind: serde_json::from_str(&format!("\"{source_kind}\""))
            .or_else(|_| serde_json::from_str(&source_kind))
            .unwrap_or(DeploymentSourceKind::ExistingPath),
        venv_path: row.get(13)?,
        updated_at_epoch_seconds: updated_at.max(0) as u64,
    })
}

#[derive(Clone)]
pub struct DeploymentAppsRepository {
    database: DeploymentDatabase,
    write_lock: Arc<AsyncMutex<()>>,
}

impl DeploymentAppsRepository {
    pub fn open(db_path: impl AsRef<Path>, legacy_json: PathBuf) -> AppResult<Self> {
        let database = DeploymentDatabase::open(db_path)?;
        let repository = Self {
            database,
            write_lock: Arc::new(AsyncMutex::new(())),
        };
        // Migration is sync and idempotent; run before serving requests.
        repository.migrate_legacy_json_if_needed(&legacy_json)?;
        Ok(repository)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let database = DeploymentDatabase::open(":memory:")?;
        Ok(Self {
            database,
            write_lock: Arc::new(AsyncMutex::new(())),
        })
    }

    fn migrate_legacy_json_if_needed(&self, legacy_json: &Path) -> AppResult<()> {
        if !legacy_json.exists() {
            return Ok(());
        }
        let existing = self.database.list_all()?;
        if !existing.is_empty() {
            return Ok(());
        }
        let raw = std::fs::read_to_string(legacy_json).map_err(|_| AppError::Storage)?;
        let apps: Vec<DeploymentApp> = serde_json::from_str(&raw).map_err(|_| AppError::Storage)?;
        if apps.is_empty() {
            let backup = legacy_json.with_extension("json.bak");
            let _ = std::fs::rename(legacy_json, &backup);
            return Ok(());
        }
        let migrated: Vec<DeploymentApp> = apps
            .into_iter()
            .map(|mut app| {
                if app.setup_step == 0 {
                    app.setup_step = 4;
                }
                if app.python.is_some() && matches!(app.setup_status, DeploymentSetupStatus::Draft)
                {
                    app.setup_status = DeploymentSetupStatus::Ready;
                    app.setup_step = 4;
                }
                app
            })
            .collect();
        self.database.replace_all(&migrated)?;
        let backup = legacy_json.with_extension("json.bak");
        let _ = std::fs::rename(legacy_json, &backup);
        Ok(())
    }

    pub async fn list(&self, profile_id: Uuid) -> AppResult<Vec<DeploymentApp>> {
        self.database.list_for_profile(profile_id)
    }

    pub async fn list_all(&self) -> AppResult<Vec<DeploymentApp>> {
        self.database.list_all()
    }

    pub async fn save_all(&self, apps: &[DeploymentApp]) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        self.database.replace_all(apps)
    }

    /// Python-only convenience wrapper kept for the repository tests.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        &self,
        id: Option<Uuid>,
        profile_id: Uuid,
        name: String,
        repository_path: String,
        remote_url: String,
        branch: String,
        build: BuildPreset,
        restart: RestartTarget,
        project_kind: ProjectKind,
        python: Option<PythonProjectConfig>,
        setup_step: u8,
        setup_status: DeploymentSetupStatus,
        source_kind: DeploymentSourceKind,
        venv_path: String,
    ) -> AppResult<DeploymentApp> {
        self.upsert_with_runtimes(
            id,
            profile_id,
            name,
            repository_path,
            remote_url,
            branch,
            build,
            restart,
            project_kind,
            python,
            None,
            None,
            None,
            None,
            None,
            None,
            setup_step,
            setup_status,
            source_kind,
            venv_path,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_with_runtimes(
        &self,
        id: Option<Uuid>,
        profile_id: Uuid,
        name: String,
        repository_path: String,
        remote_url: String,
        branch: String,
        build: BuildPreset,
        restart: RestartTarget,
        project_kind: ProjectKind,
        python: Option<PythonProjectConfig>,
        php: Option<PhpProjectConfig>,
        java: Option<JavaProjectConfig>,
        node: Option<NodeProjectConfig>,
        go: Option<GoProjectConfig>,
        html: Option<HtmlProjectConfig>,
        proxy: Option<ReverseProxyConfig>,
        setup_step: u8,
        setup_status: DeploymentSetupStatus,
        source_kind: DeploymentSourceKind,
        venv_path: String,
    ) -> AppResult<DeploymentApp> {
        let name = name.trim().to_string();
        let repository_path = repository_path.trim().to_string();
        let repository_path = if project_kind == ProjectKind::ReverseProxy {
            proxy_site_path(&name)?
        } else {
            repository_path
        };
        let remote_url = remote_url.trim().to_string();
        let mut branch = branch.trim().to_string();
        let mut restart = trim_restart(restart);
        let mut build = build;
        let venv_path = venv_path.trim().to_string();
        let setup_step = setup_step.clamp(1, 4);
        let mut python = python.map(|config| PythonProjectConfig {
            port: config.port,
            python_bin: config.python_bin.trim().to_string(),
            startup: config.startup,
            entry_file: config.entry_file.trim().to_string(),
            protocol: config.protocol,
            application: config.application.trim().to_string(),
            run_user: config.run_user.trim().to_string(),
            requirements_path: config.requirements_path.trim().to_string(),
            startup_command: config.startup_command.trim().to_string(),
            processes: config.processes,
            threads: config.threads,
            auto_start: config.auto_start,
        });

        validate_app_name(&name)?;
        if !repository_path.is_empty() {
            validate_remote_path(&repository_path)?;
        } else if setup_status != DeploymentSetupStatus::Draft || setup_step > 1 {
            return Err(AppError::InvalidOperation);
        }
        validate_remote_url(&remote_url)?;

        match source_kind {
            DeploymentSourceKind::GitClone => {
                let require_remote =
                    setup_step >= 2 || !matches!(setup_status, DeploymentSetupStatus::Draft);
                if require_remote && remote_url.is_empty() {
                    return Err(AppError::InvalidOperation);
                }
                if branch.is_empty() {
                    if matches!(setup_status, DeploymentSetupStatus::Draft) && setup_step <= 2 {
                        branch = "main".into();
                    } else {
                        return Err(AppError::InvalidOperation);
                    }
                } else {
                    validate_branch(&branch)?;
                }
            }
            DeploymentSourceKind::ExistingPath => {
                if branch.is_empty() {
                    branch = "main".into();
                } else {
                    validate_branch(&branch)?;
                }
            }
        }

        if !venv_path.is_empty() {
            validate_remote_path(&venv_path)?;
        }

        if project_kind == ProjectKind::Python {
            build = BuildPreset::Pip;
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = python {
                        validate_python_config_partial(config, setup_step)?;
                    } else if setup_step >= 3 {
                        return Err(AppError::InvalidOperation);
                    }
                    if setup_step >= 4 {
                        let service = python_service_name(&name)?;
                        restart = RestartTarget::Systemd { service };
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = python.take().ok_or(AppError::InvalidOperation)?;
                    validate_python_config(&config)?;
                    let service = python_service_name(&name)?;
                    restart = RestartTarget::Systemd { service };
                    python = Some(config);
                }
            }
        } else if python.is_some() {
            return Err(AppError::InvalidOperation);
        }

        let mut php = php.map(normalize_php_config);
        if project_kind == ProjectKind::Php {
            build = BuildPreset::None;
            if !repository_path.is_empty() {
                validate_php_path(&repository_path)?;
            }
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = php {
                        validate_php_config_partial(config, setup_step)?;
                    } else if setup_step >= 3 {
                        return Err(AppError::InvalidOperation);
                    }
                    if setup_step >= 4 {
                        restart = RestartTarget::Systemd {
                            service: php_service_name(&name)?,
                        };
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = php.take().ok_or(AppError::InvalidOperation)?;
                    validate_php_config(&config)?;
                    restart = RestartTarget::Systemd {
                        service: php_service_name(&name)?,
                    };
                    php = Some(config);
                }
            }
        } else if php.is_some() {
            return Err(AppError::InvalidOperation);
        }

        let mut java = java.map(normalize_java_config);
        if project_kind == ProjectKind::Java {
            build = BuildPreset::None;
            if !repository_path.is_empty() {
                validate_java_path(&repository_path)?;
            }
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = java {
                        validate_java_config_partial(config, setup_step)?;
                    } else if setup_step >= 3 {
                        return Err(AppError::InvalidOperation);
                    }
                    if setup_step >= 4 {
                        restart = RestartTarget::Systemd {
                            service: java_service_name(&name)?,
                        };
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = java.take().ok_or(AppError::InvalidOperation)?;
                    validate_java_config(&config)?;
                    restart = RestartTarget::Systemd {
                        service: java_service_name(&name)?,
                    };
                    java = Some(config);
                }
            }
        } else if java.is_some() {
            return Err(AppError::InvalidOperation);
        }

        let mut node = node.map(normalize_node_config);
        if project_kind == ProjectKind::Node {
            build = BuildPreset::None;
            if !repository_path.is_empty() {
                validate_node_path(&repository_path)?;
            }
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = node {
                        validate_node_config_partial(config, setup_step)?;
                    } else if setup_step >= 3 {
                        return Err(AppError::InvalidOperation);
                    }
                    if setup_step >= 4 {
                        restart = RestartTarget::Systemd {
                            service: node_service_name(&name)?,
                        };
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = node.take().ok_or(AppError::InvalidOperation)?;
                    validate_node_config(&config)?;
                    restart = RestartTarget::Systemd {
                        service: node_service_name(&name)?,
                    };
                    node = Some(config);
                }
            }
        } else if node.is_some() {
            return Err(AppError::InvalidOperation);
        }

        let mut go = go.map(normalize_go_config);
        if project_kind == ProjectKind::Go {
            build = BuildPreset::None;
            if !repository_path.is_empty() {
                validate_go_path(&repository_path)?;
            }
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = go {
                        validate_go_config_partial(config, setup_step)?;
                    } else if setup_step >= 3 {
                        return Err(AppError::InvalidOperation);
                    }
                    if setup_step >= 4 {
                        restart = RestartTarget::Systemd {
                            service: go_service_name(&name)?,
                        };
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = go.take().ok_or(AppError::InvalidOperation)?;
                    validate_go_config(&config)?;
                    restart = RestartTarget::Systemd {
                        service: go_service_name(&name)?,
                    };
                    go = Some(config);
                }
            }
        } else if go.is_some() {
            return Err(AppError::InvalidOperation);
        }

        let mut html = html.map(normalize_html_config);
        if project_kind == ProjectKind::Html {
            // Static sites have no process to supervise; only the nginx site is managed.
            build = BuildPreset::None;
            restart = RestartTarget::None;
            if !repository_path.is_empty() {
                validate_html_path(&repository_path)?;
            }
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = html {
                        validate_html_config_partial(config, setup_step)?;
                    } else if setup_step >= 3 {
                        return Err(AppError::InvalidOperation);
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = html.take().ok_or(AppError::InvalidOperation)?;
                    validate_html_config(&config)?;
                    html_nginx_conf_name(&name)?;
                    html = Some(config);
                }
            }
        } else if html.is_some() {
            return Err(AppError::InvalidOperation);
        }

        let mut proxy = proxy;
        if project_kind == ProjectKind::ReverseProxy {
            build = BuildPreset::None;
            restart = RestartTarget::None;
            proxy = proxy.map(normalize_proxy_config);
            match setup_status {
                DeploymentSetupStatus::Draft => {
                    if let Some(ref config) = proxy {
                        validate_proxy_config_partial(config, setup_step)?;
                    } else if setup_step >= 2 {
                        return Err(AppError::InvalidOperation);
                    }
                }
                DeploymentSetupStatus::Ready
                | DeploymentSetupStatus::Provisioning
                | DeploymentSetupStatus::Provisioned
                | DeploymentSetupStatus::Failed => {
                    let config = proxy.take().ok_or(AppError::InvalidOperation)?;
                    validate_proxy_config(&config)?;
                    proxy = Some(config);
                }
            }
        } else if proxy.is_some() {
            return Err(AppError::InvalidOperation);
        }

        if !matches!(restart, RestartTarget::None) {
            validate_restart(&restart)?;
        }

        let _guard = self.write_lock.lock().await;
        let now = epoch_seconds()?;
        let app_id = id.unwrap_or_else(Uuid::new_v4);

        if let Some(existing_id) = id {
            if self.database.get(profile_id, existing_id)?.is_none() {
                return Err(AppError::InvalidOperation);
            }
        } else if self.database.count_for_profile(profile_id)? >= MAX_APPS_PER_PROFILE {
            return Err(AppError::InvalidOperation);
        }

        let conflicts = self.database.list_for_profile(profile_id)?;
        for app in conflicts.iter().filter(|app| app.id != app_id) {
            if !name.is_empty() && app.name == name {
                return Err(AppError::DeploymentNameConflict);
            }
            if project_kind == ProjectKind::ReverseProxy
                && app.project_kind == ProjectKind::ReverseProxy
            {
                // Same site file or same nginx listener would silently overwrite the other proxy.
                let same_site = proxy_nginx_conf_name(&name).ok().is_some_and(|conf| {
                    proxy_nginx_conf_name(&app.name).ok().as_ref() == Some(&conf)
                });
                let same_listener = matches!((&proxy, &app.proxy), (Some(mine), Some(other))
                    if mine.port == other.port && mine.server_name == other.server_name);
                if same_site || same_listener {
                    return Err(AppError::DeploymentServiceConflict);
                }
            }
            if !repository_path.is_empty() && app.repository_path == repository_path {
                return Err(AppError::DeploymentPathConflict);
            }
            if project_kind == ProjectKind::Python
                && app.project_kind == ProjectKind::Python
                && python_service_name(&name).ok().is_some_and(|service| {
                    python_service_name(&app.name).ok().as_ref() == Some(&service)
                })
            {
                return Err(AppError::DeploymentServiceConflict);
            }
            if project_kind == ProjectKind::Php && app.project_kind == ProjectKind::Php {
                // Same unit name or same nginx listener would silently overwrite the other site.
                let same_service = php_service_name(&name).ok().is_some_and(|service| {
                    php_service_name(&app.name).ok().as_ref() == Some(&service)
                });
                let same_listener = matches!((&php, &app.php), (Some(mine), Some(other))
                    if mine.port == other.port && mine.server_name == other.server_name);
                if same_service || same_listener {
                    return Err(AppError::DeploymentServiceConflict);
                }
            }
            if project_kind == ProjectKind::Html && app.project_kind == ProjectKind::Html {
                // Same site file or same nginx listener would silently overwrite the other site.
                let same_site = html_nginx_conf_name(&name).ok().is_some_and(|conf| {
                    html_nginx_conf_name(&app.name).ok().as_ref() == Some(&conf)
                });
                let same_listener = matches!((&html, &app.html), (Some(mine), Some(other))
                    if mine.port == other.port && mine.server_name == other.server_name);
                if same_site || same_listener {
                    return Err(AppError::DeploymentServiceConflict);
                }
            }
            if project_kind == ProjectKind::Go && app.project_kind == ProjectKind::Go {
                // Two units on one port would race for the listener after a restart.
                let same_service = go_service_name(&name).ok().is_some_and(|service| {
                    go_service_name(&app.name).ok().as_ref() == Some(&service)
                });
                let same_port = matches!((&go, &app.go), (Some(mine), Some(other))
                    if mine.port == other.port);
                if same_service || same_port {
                    return Err(AppError::DeploymentServiceConflict);
                }
            }
            if project_kind == ProjectKind::Node && app.project_kind == ProjectKind::Node {
                // Two units on one port would race for the listener after a restart.
                let same_service = node_service_name(&name).ok().is_some_and(|service| {
                    node_service_name(&app.name).ok().as_ref() == Some(&service)
                });
                let same_port = matches!((&node, &app.node), (Some(mine), Some(other))
                    if mine.port == other.port);
                if same_service || same_port {
                    return Err(AppError::DeploymentServiceConflict);
                }
            }
            if project_kind == ProjectKind::Java && app.project_kind == ProjectKind::Java {
                // Two units on one port would race for the listener after a restart.
                let same_service = java_service_name(&name).ok().is_some_and(|service| {
                    java_service_name(&app.name).ok().as_ref() == Some(&service)
                });
                let same_port = matches!((&java, &app.java), (Some(mine), Some(other))
                    if mine.port == other.port);
                if same_service || same_port {
                    return Err(AppError::DeploymentServiceConflict);
                }
            }
        }

        let app = DeploymentApp {
            id: app_id,
            profile_id,
            name,
            repository_path,
            remote_url,
            branch,
            build,
            restart,
            project_kind,
            python,
            php,
            java,
            node,
            go,
            html,
            proxy,
            setup_step,
            setup_status,
            source_kind,
            venv_path,
            updated_at_epoch_seconds: now,
        };
        self.database.upsert_row(&app)?;
        Ok(app)
    }

    pub async fn delete(&self, profile_id: Uuid, id: Uuid) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        if !self.database.delete_row(profile_id, id)? {
            return Err(AppError::InvalidOperation);
        }
        Ok(())
    }
}

fn normalize_php_config(config: PhpProjectConfig) -> PhpProjectConfig {
    PhpProjectConfig {
        php_bin: config.php_bin.trim().to_string(),
        server_name: config
            .server_name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        run_dir: config.run_dir.trim().trim_matches('/').to_string(),
        run_user: config.run_user.trim().to_string(),
        ..config
    }
}

fn normalize_proxy_config(config: ReverseProxyConfig) -> ReverseProxyConfig {
    ReverseProxyConfig {
        server_name: config
            .server_name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        target_url: config.target_url.trim().to_string(),
        ..config
    }
}

fn normalize_html_config(config: HtmlProjectConfig) -> HtmlProjectConfig {
    HtmlProjectConfig {
        server_name: config
            .server_name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        root_dir: config.root_dir.trim().trim_matches('/').to_string(),
        index_files: config
            .index_files
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        ..config
    }
}

fn normalize_go_config(config: GoProjectConfig) -> GoProjectConfig {
    GoProjectConfig {
        executable: config.executable.trim().to_string(),
        go_bin: config.go_bin.trim().to_string(),
        build_package: config.build_package.trim().to_string(),
        app_args: config
            .app_args
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        env_vars: config
            .env_vars
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        run_user: config.run_user.trim().to_string(),
        ..config
    }
}

fn normalize_node_config(config: NodeProjectConfig) -> NodeProjectConfig {
    NodeProjectConfig {
        node_bin: config.node_bin.trim().to_string(),
        script: config.script.trim().to_string(),
        entry_file: config.entry_file.trim().to_string(),
        app_args: config
            .app_args
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        node_env: config.node_env.trim().to_string(),
        build_script: config.build_script.trim().to_string(),
        run_user: config.run_user.trim().to_string(),
        ..config
    }
}

fn normalize_java_config(config: JavaProjectConfig) -> JavaProjectConfig {
    JavaProjectConfig {
        java_bin: config.java_bin.trim().to_string(),
        jar_path: config.jar_path.trim().to_string(),
        jvm_args: config
            .jvm_args
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        app_args: config
            .app_args
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        run_user: config.run_user.trim().to_string(),
        ..config
    }
}

fn trim_restart(target: RestartTarget) -> RestartTarget {
    match target {
        RestartTarget::None => RestartTarget::None,
        RestartTarget::Systemd { service } => RestartTarget::Systemd {
            service: service.trim().to_string(),
        },
        RestartTarget::Pm2 { process } => RestartTarget::Pm2 {
            process: process.trim().to_string(),
        },
        RestartTarget::DockerCompose { service } => RestartTarget::DockerCompose {
            service: service.trim().to_string(),
        },
    }
}

fn epoch_seconds() -> AppResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AppError::InvalidOperation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PythonProtocol, PythonStartupMethod};
    use tempfile::tempdir;

    fn repo(path: &Path) -> DeploymentAppsRepository {
        DeploymentAppsRepository::open(path.join("deployment.db"), path.join("missing.json"))
            .unwrap()
    }

    #[tokio::test]
    async fn upsert_enforces_unique_name_and_path_per_profile() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let other = Uuid::new_v4();

        let first = repository
            .upsert(
                None,
                profile,
                "api".into(),
                "/srv/api".into(),
                "https://example.com/api.git".into(),
                "main".into(),
                BuildPreset::Pnpm,
                RestartTarget::Systemd {
                    service: "api.service".into(),
                },
                ProjectKind::Other,
                None,
                3,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .unwrap();

        assert!(repository
            .upsert(
                None,
                profile,
                "api".into(),
                "/srv/other".into(),
                "https://example.com/other.git".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Other,
                None,
                3,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());

        let cross = repository
            .upsert(
                None,
                other,
                "api".into(),
                "/srv/api".into(),
                "https://example.com/api.git".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Other,
                None,
                3,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .unwrap();
        assert_ne!(first.id, cross.id);
        assert_eq!(repository.list(profile).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn draft_python_step1_allows_incomplete_python() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let saved = repository
            .upsert(
                None,
                profile,
                "demo".into(),
                "/srv/demo".into(),
                "".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Python,
                None,
                1,
                DeploymentSetupStatus::Draft,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .unwrap();
        assert_eq!(saved.setup_step, 1);
        assert_eq!(saved.setup_status, DeploymentSetupStatus::Draft);
        assert!(saved.python.is_none());
        assert!(matches!(saved.build, BuildPreset::Pip));
    }

    #[tokio::test]
    async fn saved_drafts_report_the_specific_name_path_or_service_conflict() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        for (name, path, expected) in [
            ("demo-app", "/srv/demo", None),
            ("demo-app", "/srv/another", Some("DEPLOYMENT_NAME_CONFLICT")),
            ("another", "/srv/demo", Some("DEPLOYMENT_PATH_CONFLICT")),
            (
                "demo_app",
                "/srv/another",
                Some("DEPLOYMENT_SERVICE_CONFLICT"),
            ),
        ] {
            let result = repository
                .upsert(
                    None,
                    profile,
                    name.into(),
                    path.into(),
                    String::new(),
                    "master".into(),
                    BuildPreset::Pip,
                    RestartTarget::None,
                    ProjectKind::Python,
                    None,
                    2,
                    DeploymentSetupStatus::Draft,
                    DeploymentSourceKind::ExistingPath,
                    String::new(),
                )
                .await;
            if let Some(code) = expected {
                assert_eq!(result.unwrap_err().code(), code);
            } else {
                assert!(result.is_ok());
            }
        }
        assert_eq!(repository.list(profile).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ready_python_requires_full_config() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        assert!(repository
            .upsert(
                None,
                profile,
                "demo".into(),
                "/srv/demo".into(),
                "".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Python,
                None,
                3,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());

        let saved = repository
            .upsert(
                None,
                profile,
                "demo".into(),
                "/srv/demo".into(),
                "".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Python,
                Some(PythonProjectConfig {
                    port: 8000,
                    python_bin: "python3".into(),
                    startup: PythonStartupMethod::Gunicorn,
                    entry_file: "main.py".into(),
                    protocol: PythonProtocol::Asgi,
                    application: "main:app".into(),
                    run_user: "www".into(),
                    requirements_path: "requirements.txt".into(),
                    startup_command: String::new(),
                    processes: 1,
                    threads: 1,
                    auto_start: true,
                }),
                3,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                "/srv/demo/.venv".into(),
            )
            .await
            .unwrap();
        assert!(matches!(
            saved.restart,
            RestartTarget::Systemd { service } if service == "runory-py-demo.service"
        ));
        assert_eq!(saved.venv_path, "/srv/demo/.venv");
    }

    #[tokio::test]
    async fn entering_startup_step_saves_incomplete_draft_but_ready_rejects_it() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let config: PythonProjectConfig = serde_json::from_str(
            r#"{"port":8000,"pythonBin":"python3","startup":"gunicorn","entryFile":"main.py","protocol":"asgi","application":"","runUser":"www","requirementsPath":"requirements.txt"}"#,
        ).unwrap();
        let draft = repository
            .upsert(
                None,
                profile,
                "draft-demo".into(),
                "/srv/draft-demo".into(),
                String::new(),
                "master".into(),
                BuildPreset::Pip,
                RestartTarget::None,
                ProjectKind::Python,
                Some(config.clone()),
                4,
                DeploymentSetupStatus::Draft,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .unwrap();
        assert_eq!(draft.setup_step, 4);
        assert_eq!(draft.setup_status, DeploymentSetupStatus::Draft);
        assert!(repository
            .upsert(
                Some(draft.id),
                profile,
                draft.name,
                draft.repository_path,
                String::new(),
                "master".into(),
                BuildPreset::Pip,
                RestartTarget::None,
                ProjectKind::Python,
                Some(config),
                4,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
        assert_eq!(
            repository.list(profile).await.unwrap()[0].setup_status,
            DeploymentSetupStatus::Draft
        );
    }

    #[tokio::test]
    async fn migrates_legacy_json_once() {
        let directory = tempdir().unwrap();
        let json_path = directory.path().join("deployment-apps.json");
        let apps = vec![DeploymentApp {
            id: Uuid::new_v4(),
            profile_id: Uuid::new_v4(),
            name: "legacy".into(),
            repository_path: "/srv/legacy".into(),
            remote_url: "https://example.com/legacy.git".into(),
            branch: "main".into(),
            build: BuildPreset::None,
            restart: RestartTarget::None,
            project_kind: ProjectKind::Other,
            python: None,
            php: None,
            java: None,
            node: None,
            go: None,
            html: None,
            proxy: None,
            setup_step: 3,
            setup_status: DeploymentSetupStatus::Ready,
            source_kind: DeploymentSourceKind::ExistingPath,
            venv_path: String::new(),
            updated_at_epoch_seconds: 100,
        }];
        std::fs::write(&json_path, serde_json::to_string(&apps).unwrap()).unwrap();
        let repository = DeploymentAppsRepository::open(
            directory.path().join("deployment.db"),
            json_path.clone(),
        )
        .unwrap();
        let listed = repository.list_all().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "legacy");
        assert!(!json_path.exists());
        assert!(directory.path().join("deployment-apps.json.bak").exists());
    }

    fn php_config(port: u16) -> PhpProjectConfig {
        serde_json::from_str(&format!(
            r#"{{"phpBin":"php8.2","port":{port},"serverName":"example.com","runDir":"/public/","rewrite":"frontController","runUser":"www-data"}}"#
        ))
        .unwrap()
    }

    async fn save_php(
        repository: &DeploymentAppsRepository,
        profile: Uuid,
        name: &str,
        path: &str,
        config: Option<PhpProjectConfig>,
        status: DeploymentSetupStatus,
    ) -> AppResult<DeploymentApp> {
        repository
            .upsert_with_runtimes(
                None,
                profile,
                name.into(),
                path.into(),
                String::new(),
                "main".into(),
                BuildPreset::Npm,
                RestartTarget::None,
                ProjectKind::Php,
                None,
                config,
                None,
                None,
                None,
                None,
                None,
                4,
                status,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
    }

    #[tokio::test]
    async fn php_project_roundtrips_with_normalized_config_and_service() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let saved = save_php(
            &repository,
            profile,
            "Shop",
            "/srv/shop",
            Some(php_config(8080)),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        assert_eq!(saved.build, BuildPreset::None);
        assert!(matches!(
            &saved.restart,
            RestartTarget::Systemd { service } if service == "runory-php-shop.service"
        ));
        let listed = repository.list(profile).await.unwrap();
        assert_eq!(listed[0].php.as_ref().unwrap().run_dir, "public");
        assert_eq!(listed[0].php, saved.php);
    }

    #[tokio::test]
    async fn php_ready_requires_valid_config_and_safe_path() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        assert!(save_php(
            &repository,
            profile,
            "a",
            "/srv/a",
            None,
            DeploymentSetupStatus::Ready
        )
        .await
        .is_err());
        let mut bad = php_config(8080);
        bad.max_children = 0;
        assert!(save_php(
            &repository,
            profile,
            "a",
            "/srv/a",
            Some(bad),
            DeploymentSetupStatus::Ready
        )
        .await
        .is_err());
        assert!(save_php(
            &repository,
            profile,
            "a",
            "/srv/my site",
            Some(php_config(8080)),
            DeploymentSetupStatus::Ready
        )
        .await
        .is_err());
        // PHP settings are rejected for other project kinds.
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "b".into(),
                "/srv/b".into(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Node,
                None,
                Some(php_config(8081)),
                None,
                None,
                None,
                None,
                None,
                4,
                DeploymentSetupStatus::Ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
    }

    fn java_config(port: u16) -> JavaProjectConfig {
        let mut config = crate::deployment::java_validation::tests::sample_config();
        config.port = port;
        config.jvm_args = " -XX:+UseG1GC   -Xss512k ".into();
        config
    }

    async fn save_java(
        repository: &DeploymentAppsRepository,
        profile: Uuid,
        name: &str,
        path: &str,
        config: Option<JavaProjectConfig>,
        status: DeploymentSetupStatus,
    ) -> AppResult<DeploymentApp> {
        repository
            .upsert_with_runtimes(
                None,
                profile,
                name.into(),
                path.into(),
                String::new(),
                "main".into(),
                BuildPreset::Npm,
                RestartTarget::None,
                ProjectKind::Java,
                None,
                None,
                config,
                None,
                None,
                None,
                None,
                4,
                status,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
    }

    #[tokio::test]
    async fn java_project_roundtrips_with_normalized_config_and_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let saved = save_java(
            &repository,
            profile,
            "Orders API",
            "/srv/orders",
            Some(java_config(8080)),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            &saved.restart,
            RestartTarget::Systemd { service } if service == "runory-java-orders-api.service"
        ));
        assert_eq!(saved.build, BuildPreset::None);
        let listed = repository.list(profile).await.unwrap();
        assert_eq!(
            listed[0].java.as_ref().unwrap().jvm_args,
            "-XX:+UseG1GC -Xss512k"
        );
        assert_eq!(listed[0].java, saved.java);
    }

    #[tokio::test]
    async fn java_ready_requires_valid_config_and_is_rejected_for_other_kinds() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        assert!(save_java(&repository, profile, "a", "/srv/a", None, ready)
            .await
            .is_err());
        let mut bad = java_config(8080);
        bad.jar_path = "app.war".into();
        assert!(
            save_java(&repository, profile, "a", "/srv/a", Some(bad), ready)
                .await
                .is_err()
        );
        assert!(save_java(
            &repository,
            profile,
            "a",
            "/srv/my app",
            Some(java_config(8080)),
            ready
        )
        .await
        .is_err());
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "b".into(),
                "/srv/b".into(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Node,
                None,
                None,
                Some(java_config(8081)),
                None,
                None,
                None,
                None,
                4,
                ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn java_projects_cannot_share_a_port_or_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        save_java(
            &repository,
            profile,
            "api",
            "/srv/a",
            Some(java_config(8080)),
            ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            save_java(
                &repository,
                profile,
                "other",
                "/srv/b",
                Some(java_config(8080)),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(matches!(
            save_java(
                &repository,
                profile,
                "API",
                "/srv/c",
                Some(java_config(8081)),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(save_java(
            &repository,
            profile,
            "other",
            "/srv/d",
            Some(java_config(8081)),
            ready
        )
        .await
        .is_ok());
    }

    fn node_config(port: u16) -> NodeProjectConfig {
        let mut config = crate::deployment::node_validation::tests::sample_config();
        config.port = port;
        config.app_args = " --a=1   --b ".into();
        config
    }

    async fn save_node(
        repository: &DeploymentAppsRepository,
        profile: Uuid,
        name: &str,
        path: &str,
        config: Option<NodeProjectConfig>,
        status: DeploymentSetupStatus,
    ) -> AppResult<DeploymentApp> {
        repository
            .upsert_with_runtimes(
                None,
                profile,
                name.into(),
                path.into(),
                String::new(),
                "main".into(),
                BuildPreset::Npm,
                RestartTarget::None,
                ProjectKind::Node,
                None,
                None,
                None,
                config,
                None,
                None,
                None,
                4,
                status,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
    }

    #[tokio::test]
    async fn node_project_roundtrips_with_normalized_config_and_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let saved = save_node(
            &repository,
            profile,
            "Web App",
            "/srv/web",
            Some(node_config(3000)),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            &saved.restart,
            RestartTarget::Systemd { service } if service == "runory-node-web-app.service"
        ));
        assert_eq!(saved.build, BuildPreset::None);
        let listed = repository.list(profile).await.unwrap();
        assert_eq!(listed[0].node.as_ref().unwrap().app_args, "--a=1 --b");
        assert_eq!(listed[0].node, saved.node);
    }

    #[tokio::test]
    async fn node_ready_requires_valid_config_and_is_rejected_for_other_kinds() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        assert!(save_node(&repository, profile, "a", "/srv/a", None, ready)
            .await
            .is_err());
        let mut bad = node_config(3000);
        bad.script = "a b".into();
        assert!(
            save_node(&repository, profile, "a", "/srv/a", Some(bad), ready)
                .await
                .is_err()
        );
        assert!(save_node(
            &repository,
            profile,
            "a",
            "/srv/my app",
            Some(node_config(3000)),
            ready
        )
        .await
        .is_err());
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "b".into(),
                "/srv/b".into(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Go,
                None,
                None,
                None,
                Some(node_config(3001)),
                None,
                None,
                None,
                4,
                ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn node_projects_cannot_share_a_port_or_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        save_node(
            &repository,
            profile,
            "web",
            "/srv/a",
            Some(node_config(3000)),
            ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            save_node(
                &repository,
                profile,
                "other",
                "/srv/b",
                Some(node_config(3000)),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(matches!(
            save_node(
                &repository,
                profile,
                "WEB",
                "/srv/c",
                Some(node_config(3001)),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(save_node(
            &repository,
            profile,
            "other",
            "/srv/d",
            Some(node_config(3001)),
            ready
        )
        .await
        .is_ok());
    }

    fn go_config(port: u16) -> GoProjectConfig {
        let mut config = crate::deployment::go_validation::tests::sample_config();
        config.port = port;
        config.env_vars = " A=1   B=2 ".into();
        config
    }

    async fn save_go(
        repository: &DeploymentAppsRepository,
        profile: Uuid,
        name: &str,
        path: &str,
        config: Option<GoProjectConfig>,
        status: DeploymentSetupStatus,
    ) -> AppResult<DeploymentApp> {
        repository
            .upsert_with_runtimes(
                None,
                profile,
                name.into(),
                path.into(),
                String::new(),
                "main".into(),
                BuildPreset::Npm,
                RestartTarget::None,
                ProjectKind::Go,
                None,
                None,
                None,
                None,
                config,
                None,
                None,
                4,
                status,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
    }

    #[tokio::test]
    async fn go_project_roundtrips_with_normalized_config_and_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let saved = save_go(
            &repository,
            profile,
            "Order Svc",
            "/srv/orders",
            Some(go_config(8080)),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            &saved.restart,
            RestartTarget::Systemd { service } if service == "runory-go-order-svc.service"
        ));
        assert_eq!(saved.build, BuildPreset::None);
        let listed = repository.list(profile).await.unwrap();
        assert_eq!(listed[0].go.as_ref().unwrap().env_vars, "A=1 B=2");
        assert_eq!(listed[0].go, saved.go);
    }

    #[tokio::test]
    async fn go_ready_requires_valid_config_and_is_rejected_for_other_kinds() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        assert!(save_go(&repository, profile, "a", "/srv/a", None, ready)
            .await
            .is_err());
        let mut bad = go_config(8080);
        bad.executable = "bin/".into();
        assert!(
            save_go(&repository, profile, "a", "/srv/a", Some(bad), ready)
                .await
                .is_err()
        );
        assert!(save_go(
            &repository,
            profile,
            "a",
            "/srv/my app",
            Some(go_config(8080)),
            ready
        )
        .await
        .is_err());
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "b".into(),
                "/srv/b".into(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Node,
                None,
                None,
                None,
                None,
                Some(go_config(8081)),
                None,
                None,
                4,
                ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn go_projects_cannot_share_a_port_or_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        save_go(
            &repository,
            profile,
            "svc",
            "/srv/a",
            Some(go_config(8080)),
            ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            save_go(
                &repository,
                profile,
                "other",
                "/srv/b",
                Some(go_config(8080)),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(matches!(
            save_go(
                &repository,
                profile,
                "SVC",
                "/srv/c",
                Some(go_config(8081)),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(save_go(
            &repository,
            profile,
            "other",
            "/srv/d",
            Some(go_config(8081)),
            ready
        )
        .await
        .is_ok());
    }

    fn html_config(port: u16, server_name: &str) -> HtmlProjectConfig {
        let mut config = crate::deployment::html_validation::tests::sample_config();
        config.port = port;
        config.server_name = server_name.into();
        config.root_dir = "/dist/".into();
        config.index_files = " index.html   index.htm ".into();
        config
    }

    async fn save_html(
        repository: &DeploymentAppsRepository,
        profile: Uuid,
        name: &str,
        path: &str,
        config: Option<HtmlProjectConfig>,
        status: DeploymentSetupStatus,
    ) -> AppResult<DeploymentApp> {
        repository
            .upsert_with_runtimes(
                None,
                profile,
                name.into(),
                path.into(),
                String::new(),
                "main".into(),
                BuildPreset::Npm,
                RestartTarget::Systemd {
                    service: "ignored.service".into(),
                },
                ProjectKind::Html,
                None,
                None,
                None,
                None,
                None,
                config,
                None,
                4,
                status,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
    }

    #[tokio::test]
    async fn html_project_roundtrips_with_normalized_config_and_no_service() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let saved = save_html(
            &repository,
            profile,
            "Docs Site",
            "/srv/docs",
            Some(html_config(8080, "a.com")),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        assert_eq!(saved.restart, RestartTarget::None);
        assert_eq!(saved.build, BuildPreset::None);
        let listed = repository.list(profile).await.unwrap();
        let html = listed[0].html.as_ref().unwrap();
        assert_eq!(html.root_dir, "dist");
        assert_eq!(html.index_files, "index.html index.htm");
        assert_eq!(listed[0].html, saved.html);
    }

    #[tokio::test]
    async fn html_ready_requires_valid_config_and_is_rejected_for_other_kinds() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        assert!(save_html(&repository, profile, "a", "/srv/a", None, ready)
            .await
            .is_err());
        let mut bad = html_config(8080, "a.com");
        bad.server_name = "a.com; root /;".into();
        assert!(
            save_html(&repository, profile, "a", "/srv/a", Some(bad), ready)
                .await
                .is_err()
        );
        assert!(save_html(
            &repository,
            profile,
            "a",
            "/srv/my site",
            Some(html_config(8080, "")),
            ready
        )
        .await
        .is_err());
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "b".into(),
                "/srv/b".into(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Go,
                None,
                None,
                None,
                None,
                None,
                Some(html_config(8081, "")),
                None,
                4,
                ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn html_projects_cannot_share_a_listener_or_site_file() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        save_html(
            &repository,
            profile,
            "docs",
            "/srv/a",
            Some(html_config(8080, "a.com")),
            ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            save_html(
                &repository,
                profile,
                "other",
                "/srv/b",
                Some(html_config(8080, "a.com")),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(matches!(
            save_html(
                &repository,
                profile,
                "DOCS",
                "/srv/c",
                Some(html_config(8081, "b.com")),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(save_html(
            &repository,
            profile,
            "other",
            "/srv/d",
            Some(html_config(8080, "b.com")),
            ready
        )
        .await
        .is_ok());
    }

    fn proxy_config(port: u16, server_name: &str) -> ReverseProxyConfig {
        let mut config = crate::deployment::proxy_validation::tests::sample_config();
        config.port = port;
        config.server_name = server_name.into();
        config.target_url = " http://127.0.0.1:3000 ".into();
        config
    }

    async fn save_proxy(
        repository: &DeploymentAppsRepository,
        profile: Uuid,
        name: &str,
        path: &str,
        config: Option<ReverseProxyConfig>,
        status: DeploymentSetupStatus,
    ) -> AppResult<DeploymentApp> {
        repository
            .upsert_with_runtimes(
                None,
                profile,
                name.into(),
                path.into(),
                String::new(),
                "main".into(),
                BuildPreset::Npm,
                RestartTarget::Systemd {
                    service: "ignored.service".into(),
                },
                ProjectKind::ReverseProxy,
                None,
                None,
                None,
                None,
                None,
                None,
                config,
                3,
                status,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
    }

    #[tokio::test]
    async fn proxy_project_derives_its_site_path_and_normalizes_config() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let saved = save_proxy(
            &repository,
            profile,
            "My API",
            "/ignored/path",
            Some(proxy_config(80, " a.com   b.com ")),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        assert_eq!(
            saved.repository_path,
            "/etc/nginx/conf.d/runory-proxy-my-api.conf"
        );
        assert_eq!(saved.restart, RestartTarget::None);
        assert_eq!(saved.build, BuildPreset::None);
        let listed = repository.list(profile).await.unwrap();
        let proxy = listed[0].proxy.as_ref().unwrap();
        assert_eq!(proxy.target_url, "http://127.0.0.1:3000");
        assert_eq!(proxy.server_name, "a.com b.com");
        assert_eq!(listed[0].proxy, saved.proxy);
    }

    #[tokio::test]
    async fn proxy_requires_valid_config_and_is_rejected_for_other_kinds() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        assert!(save_proxy(&repository, profile, "a", "", None, ready)
            .await
            .is_err());
        let mut bad = proxy_config(80, "");
        bad.target_url = "http://x; proxy_pass http://evil".into();
        assert!(save_proxy(&repository, profile, "a", "", Some(bad), ready)
            .await
            .is_err());
        // Drafts may be saved without a target on the first screen but not afterwards.
        let draft = DeploymentSetupStatus::Draft;
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "d".into(),
                String::new(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::ReverseProxy,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                1,
                draft,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_ok());
        assert!(save_proxy(&repository, profile, "e", "", None, draft)
            .await
            .is_err());
        assert!(repository
            .upsert_with_runtimes(
                None,
                profile,
                "b".into(),
                "/srv/b".into(),
                String::new(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
                ProjectKind::Go,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(proxy_config(81, "")),
                4,
                ready,
                DeploymentSourceKind::ExistingPath,
                String::new(),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn proxies_cannot_share_a_listener_or_site_file() {
        let repository = DeploymentAppsRepository::open_in_memory().unwrap();
        let profile = Uuid::new_v4();
        let ready = DeploymentSetupStatus::Ready;
        save_proxy(
            &repository,
            profile,
            "api",
            "",
            Some(proxy_config(80, "a.com")),
            ready,
        )
        .await
        .unwrap();
        assert!(matches!(
            save_proxy(
                &repository,
                profile,
                "other",
                "",
                Some(proxy_config(80, "a.com")),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(matches!(
            save_proxy(
                &repository,
                profile,
                "API",
                "",
                Some(proxy_config(81, "b.com")),
                ready
            )
            .await,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(save_proxy(
            &repository,
            profile,
            "other",
            "",
            Some(proxy_config(80, "b.com")),
            ready
        )
        .await
        .is_ok());
    }

    #[tokio::test]
    async fn php_projects_cannot_share_a_listener_or_service() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        save_php(
            &repository,
            profile,
            "Shop",
            "/srv/shop",
            Some(php_config(8080)),
            DeploymentSetupStatus::Ready,
        )
        .await
        .unwrap();
        let same_listener = save_php(
            &repository,
            profile,
            "Blog",
            "/srv/blog",
            Some(php_config(8080)),
            DeploymentSetupStatus::Ready,
        )
        .await;
        assert!(matches!(
            same_listener,
            Err(AppError::DeploymentServiceConflict)
        ));
        let same_service = save_php(
            &repository,
            profile,
            "shop!",
            "/srv/shop2",
            Some(php_config(8081)),
            DeploymentSetupStatus::Ready,
        )
        .await;
        assert!(matches!(
            same_service,
            Err(AppError::DeploymentServiceConflict)
        ));
        assert!(save_php(
            &repository,
            profile,
            "Blog",
            "/srv/blog",
            Some(php_config(8081)),
            DeploymentSetupStatus::Ready
        )
        .await
        .is_ok());
    }

    #[test]
    fn databases_without_php_column_are_migrated() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("deployment.db");
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE deployment_apps (
                        id TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL, name TEXT NOT NULL,
                        repository_path TEXT NOT NULL, remote_url TEXT NOT NULL, branch TEXT NOT NULL,
                        build_json TEXT NOT NULL, restart_json TEXT NOT NULL, project_kind TEXT NOT NULL,
                        python_json TEXT, setup_step INTEGER NOT NULL, setup_status TEXT NOT NULL,
                        source_kind TEXT NOT NULL, venv_path TEXT NOT NULL, updated_at INTEGER NOT NULL
                     );",
                )
                .unwrap();
        }
        let database = DeploymentDatabase::open(&path).unwrap();
        assert!(database.list_all().unwrap().is_empty());
        // Reopening must be idempotent.
        DeploymentDatabase::open(&path).unwrap();
    }
}
