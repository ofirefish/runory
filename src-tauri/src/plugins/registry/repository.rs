use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use super::model::InstalledPluginRecord;
use crate::plugins::error::{PluginError, PluginResult};
use crate::plugins::permissions::capability::PluginCapability;
use crate::plugins::permissions::grant::PermissionGrant;

const SCHEMA_VERSION: i64 = 2;

pub struct PluginRegistryRepository {
    path: PathBuf,
    conn: Mutex<Connection>,
}

impl PluginRegistryRepository {
    pub fn open(path: impl Into<PathBuf>) -> PluginResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| PluginError::Storage)?;
        }
        let conn = Connection::open(&path).map_err(|_| PluginError::Storage)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )
        .map_err(|_| PluginError::Storage)?;
        let repo = Self {
            path,
            conn: Mutex::new(conn),
        };
        repo.migrate()?;
        Ok(repo)
    }

    pub fn open_in_memory() -> PluginResult<Self> {
        let conn = Connection::open_in_memory().map_err(|_| PluginError::Storage)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|_| PluginError::Storage)?;
        let repo = Self {
            path: PathBuf::from(":memory:"),
            conn: Mutex::new(conn),
        };
        repo.migrate()?;
        Ok(repo)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
             );",
        )
        .map_err(|_| PluginError::Storage)?;
        let current: i64 = conn
            .query_row(
                "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| PluginError::Storage)?
            .unwrap_or(0);
        if current < 1 {
            conn.execute_batch(
                "CREATE TABLE plugins (
                    id TEXT NOT NULL,
                    version TEXT NOT NULL,
                    name TEXT NOT NULL,
                    publisher_id TEXT NOT NULL,
                    install_path TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    installed_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY (id, version)
                );
                CREATE TABLE plugin_active_versions (
                    plugin_id TEXT PRIMARY KEY,
                    version TEXT NOT NULL,
                    FOREIGN KEY (plugin_id, version)
                        REFERENCES plugins(id, version)
                );
                CREATE TABLE plugin_permissions (
                    plugin_id TEXT NOT NULL,
                    permission TEXT NOT NULL,
                    granted INTEGER NOT NULL,
                    granted_at INTEGER,
                    hosts_json TEXT NOT NULL DEFAULT '[]',
                    PRIMARY KEY (plugin_id, permission)
                );
                CREATE TABLE plugin_execution_runs (
                    id TEXT PRIMARY KEY,
                    plugin_id TEXT NOT NULL,
                    plugin_version TEXT NOT NULL,
                    workflow_id TEXT NOT NULL,
                    host_id TEXT,
                    session_id TEXT,
                    state TEXT NOT NULL,
                    started_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    error_code TEXT,
                    error_message TEXT,
                    plan_json TEXT,
                    inputs_json TEXT NOT NULL DEFAULT '{}'
                );
                CREATE TABLE plugin_execution_steps (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    step_id TEXT NOT NULL,
                    action TEXT NOT NULL,
                    state TEXT NOT NULL,
                    risk TEXT NOT NULL,
                    started_at INTEGER,
                    completed_at INTEGER,
                    exit_code INTEGER,
                    outputs_json TEXT,
                    stdout TEXT,
                    stderr TEXT,
                    FOREIGN KEY (run_id)
                        REFERENCES plugin_execution_runs(id)
                );
                INSERT INTO schema_version(version) VALUES (1);",
            )
            .map_err(|_| PluginError::Storage)?;
        }
        if current < 2 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS plugin_publishers (
                    publisher_id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    verification_status TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS publisher_keys (
                    id TEXT NOT NULL,
                    publisher_id TEXT NOT NULL,
                    algorithm TEXT NOT NULL,
                    public_key TEXT NOT NULL,
                    status TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    revoked_at INTEGER,
                    PRIMARY KEY (id, publisher_id),
                    FOREIGN KEY (publisher_id) REFERENCES plugin_publishers(publisher_id)
                );
                CREATE TABLE IF NOT EXISTS plugin_versions_security (
                    plugin_id TEXT NOT NULL,
                    version TEXT NOT NULL,
                    security_status TEXT NOT NULL,
                    advisory_id TEXT,
                    checked_at INTEGER NOT NULL,
                    PRIMARY KEY (plugin_id, version)
                );
                CREATE TABLE IF NOT EXISTS plugin_marketplace_cache (
                    plugin_id TEXT PRIMARY KEY,
                    metadata_json TEXT NOT NULL,
                    cached_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS plugin_revocations (
                    kind TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    version TEXT,
                    reason TEXT,
                    revoked_at INTEGER NOT NULL,
                    PRIMARY KEY (kind, target_id, version)
                );
                INSERT INTO schema_version(version) VALUES (2);",
            )
            .map_err(|_| PluginError::Storage)?;
        }
        Ok(())
    }

    pub fn insert_plugin(&self, record: &InstalledPluginRecord) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugins(id, version, name, publisher_id, install_path, enabled, installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.version,
                record.name,
                record.publisher_id,
                record.install_path,
                if record.enabled { 1 } else { 0 },
                record.installed_at,
                record.updated_at
            ],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    /// Commit the installed version, active pointer and reviewed grants together.
    pub fn install_version(
        &self,
        record: &InstalledPluginRecord,
        grants: &[PermissionGrant],
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|_| PluginError::Storage)?;
        tx.execute(
            "INSERT INTO plugins(id, version, name, publisher_id, install_path, enabled, installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![record.id, record.version, record.name, record.publisher_id,
                record.install_path, record.enabled, record.installed_at, record.updated_at],
        ).map_err(|_| PluginError::Storage)?;
        tx.execute(
            "INSERT INTO plugin_active_versions(plugin_id, version) VALUES (?1, ?2)
            ON CONFLICT(plugin_id) DO UPDATE SET version = excluded.version",
            params![record.id, record.version],
        )
        .map_err(|_| PluginError::Storage)?;
        tx.execute(
            "DELETE FROM plugin_permissions WHERE plugin_id = ?1",
            params![record.id],
        )
        .map_err(|_| PluginError::Storage)?;
        for grant in grants {
            let hosts = serde_json::to_string(&grant.hosts).map_err(|_| PluginError::Storage)?;
            tx.execute("INSERT INTO plugin_permissions(plugin_id, permission, granted, granted_at, hosts_json)
                VALUES (?1, ?2, ?3, ?4, ?5)",
                params![record.id, grant.capability.as_str(), grant.granted, now_secs(), hosts])
                .map_err(|_| PluginError::Storage)?;
        }
        tx.commit().map_err(|_| PluginError::Storage)
    }

    pub fn set_active_version(&self, plugin_id: &str, version: &str) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_active_versions(plugin_id, version) VALUES (?1, ?2)
             ON CONFLICT(plugin_id) DO UPDATE SET version = excluded.version",
            params![plugin_id, version],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn list_active(&self) -> PluginResult<Vec<InstalledPluginRecord>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let mut stmt = conn
            .prepare(
                "SELECT p.id, p.version, p.name, p.publisher_id, p.install_path, p.enabled, p.installed_at, p.updated_at
                 FROM plugins p
                 INNER JOIN plugin_active_versions a
                   ON a.plugin_id = p.id AND a.version = p.version
                 ORDER BY p.id",
            )
            .map_err(|_| PluginError::Storage)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(InstalledPluginRecord {
                    id: row.get(0)?,
                    version: row.get(1)?,
                    name: row.get(2)?,
                    publisher_id: row.get(3)?,
                    install_path: row.get(4)?,
                    enabled: row.get::<_, i64>(5)? != 0,
                    installed_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .map_err(|_| PluginError::Storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|_| PluginError::Storage)?);
        }
        Ok(out)
    }

    pub fn get_active(&self, plugin_id: &str) -> PluginResult<Option<InstalledPluginRecord>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.query_row(
            "SELECT p.id, p.version, p.name, p.publisher_id, p.install_path, p.enabled, p.installed_at, p.updated_at
             FROM plugins p
             INNER JOIN plugin_active_versions a
               ON a.plugin_id = p.id AND a.version = p.version
             WHERE p.id = ?1",
            params![plugin_id],
            |row| {
                Ok(InstalledPluginRecord {
                    id: row.get(0)?,
                    version: row.get(1)?,
                    name: row.get(2)?,
                    publisher_id: row.get(3)?,
                    install_path: row.get(4)?,
                    enabled: row.get::<_, i64>(5)? != 0,
                    installed_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|_| PluginError::Storage)
    }

    pub fn set_enabled(&self, plugin_id: &str, enabled: bool) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let now = now_secs();
        let changed = conn
            .execute(
                "UPDATE plugins SET enabled = ?1, updated_at = ?2
                 WHERE id = ?3 AND version = (
                   SELECT version FROM plugin_active_versions WHERE plugin_id = ?3
                 )",
                params![if enabled { 1 } else { 0 }, now, plugin_id],
            )
            .map_err(|_| PluginError::Storage)?;
        if changed == 0 {
            return Err(PluginError::NotFound);
        }
        Ok(())
    }

    pub fn uninstall(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let mut stmt = conn
            .prepare("SELECT install_path FROM plugins WHERE id = ?1")
            .map_err(|_| PluginError::Storage)?;
        let paths: Vec<String> = stmt
            .query_map(params![plugin_id], |row| row.get(0))
            .map_err(|_| PluginError::Storage)?
            .collect::<Result<_, _>>()
            .map_err(|_| PluginError::Storage)?;
        if paths.is_empty() {
            return Err(PluginError::NotFound);
        }
        conn.execute(
            "DELETE FROM plugin_permissions WHERE plugin_id = ?1",
            params![plugin_id],
        )
        .map_err(|_| PluginError::Storage)?;
        conn.execute(
            "DELETE FROM plugin_active_versions WHERE plugin_id = ?1",
            params![plugin_id],
        )
        .map_err(|_| PluginError::Storage)?;
        conn.execute("DELETE FROM plugins WHERE id = ?1", params![plugin_id])
            .map_err(|_| PluginError::Storage)?;
        Ok(paths)
    }

    pub fn replace_permissions(
        &self,
        plugin_id: &str,
        grants: &[PermissionGrant],
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|_| PluginError::Storage)?;
        tx.execute(
            "DELETE FROM plugin_permissions WHERE plugin_id = ?1",
            params![plugin_id],
        )
        .map_err(|_| PluginError::Storage)?;
        let now = now_secs();
        for grant in grants {
            let hosts = serde_json::to_string(&grant.hosts).unwrap_or_else(|_| "[]".into());
            tx.execute(
                "INSERT INTO plugin_permissions(plugin_id, permission, granted, granted_at, hosts_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    plugin_id,
                    grant.capability.as_str(),
                    if grant.granted { 1 } else { 0 },
                    now,
                    hosts
                ],
            )
            .map_err(|_| PluginError::Storage)?;
        }
        tx.commit().map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn list_permissions(&self, plugin_id: &str) -> PluginResult<Vec<PermissionGrant>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let mut stmt = conn
            .prepare(
                "SELECT permission, granted, hosts_json FROM plugin_permissions WHERE plugin_id = ?1",
            )
            .map_err(|_| PluginError::Storage)?;
        let rows = stmt
            .query_map(params![plugin_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|_| PluginError::Storage)?;
        let mut grants = Vec::new();
        for row in rows {
            let (permission, granted, hosts_json) = row.map_err(|_| PluginError::Storage)?;
            let capability =
                PluginCapability::parse(&permission).map_err(|_| PluginError::Storage)?;
            let hosts: Vec<String> =
                serde_json::from_str(&hosts_json).map_err(|_| PluginError::Storage)?;
            grants.push(PermissionGrant {
                plugin_id: plugin_id.to_owned(),
                capability,
                granted: granted != 0,
                hosts,
            });
        }
        Ok(grants)
    }

    pub fn insert_run(
        &self,
        id: &str,
        plugin_id: &str,
        plugin_version: &str,
        workflow_id: &str,
        host_id: Option<&str>,
        session_id: Option<&str>,
        state: &str,
        plan_json: Option<&str>,
        inputs_json: &str,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_execution_runs(
                id, plugin_id, plugin_version, workflow_id, host_id, session_id, state,
                started_at, plan_json, inputs_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                plugin_id,
                plugin_version,
                workflow_id,
                host_id,
                session_id,
                state,
                now_secs(),
                plan_json,
                inputs_json
            ],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn update_run_state(
        &self,
        id: &str,
        state: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
        completed: bool,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let completed_at = if completed { Some(now_secs()) } else { None };
        conn.execute(
            "UPDATE plugin_execution_runs
             SET state = ?1, error_code = ?2, error_message = ?3, completed_at = COALESCE(?4, completed_at)
             WHERE id = ?5",
            params![state, error_code, error_message, completed_at, id],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn insert_step(
        &self,
        id: &str,
        run_id: &str,
        step_id: &str,
        action: &str,
        state: &str,
        risk: &str,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_execution_steps(id, run_id, step_id, action, state, risk)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, run_id, step_id, action, state, risk],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn update_step(
        &self,
        id: &str,
        state: &str,
        exit_code: Option<i32>,
        outputs_json: Option<&str>,
        stdout: Option<&str>,
        stderr: Option<&str>,
        started: bool,
        completed: bool,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let now = now_secs();
        conn.execute(
            "UPDATE plugin_execution_steps
             SET state = ?1,
                 exit_code = COALESCE(?2, exit_code),
                 outputs_json = COALESCE(?3, outputs_json),
                 stdout = COALESCE(?4, stdout),
                 stderr = COALESCE(?5, stderr),
                 started_at = CASE WHEN ?6 THEN COALESCE(started_at, ?7) ELSE started_at END,
                 completed_at = CASE WHEN ?8 THEN ?7 ELSE completed_at END
             WHERE id = ?9",
            params![
                state,
                exit_code,
                outputs_json,
                stdout,
                stderr,
                started,
                now,
                completed,
                id
            ],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn interrupt_running_runs(&self) -> PluginResult<usize> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let changed = conn
            .execute(
                "UPDATE plugin_execution_runs
                 SET state = 'Interrupted', completed_at = ?1
                 WHERE state IN ('Created', 'Planning', 'AwaitingApproval', 'Running', 'RollingBack')",
                params![now_secs()],
            )
            .map_err(|_| PluginError::Storage)?;
        Ok(changed)
    }

    pub fn get_run_state(&self, id: &str) -> PluginResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.query_row(
            "SELECT state FROM plugin_execution_runs WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| PluginError::Storage)
    }

    pub fn upsert_publisher(
        &self,
        publisher_id: &str,
        display_name: &str,
        verification_status: &str,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_publishers(publisher_id, display_name, verification_status, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(publisher_id) DO UPDATE SET
               display_name = excluded.display_name,
               verification_status = excluded.verification_status,
               updated_at = excluded.updated_at",
            params![publisher_id, display_name, verification_status, now_secs()],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn upsert_publisher_key(
        &self,
        key_id: &str,
        publisher_id: &str,
        public_key: &str,
        status: &str,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO publisher_keys(id, publisher_id, algorithm, public_key, status, created_at, revoked_at)
             VALUES (?1, ?2, 'ed25519', ?3, ?4, ?5, NULL)
             ON CONFLICT(id, publisher_id) DO UPDATE SET
               public_key = CASE WHEN publisher_keys.status = 'revoked' THEN publisher_keys.public_key ELSE excluded.public_key END,
               status = CASE WHEN publisher_keys.status = 'revoked' THEN 'revoked' ELSE excluded.status END",
            params![key_id, publisher_id, public_key, status, now_secs()],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn list_publisher_keys(&self) -> PluginResult<Vec<(String, String, String, bool)>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let mut stmt = conn
            .prepare(
                "SELECT publisher_id, id, public_key, status FROM publisher_keys ORDER BY publisher_id, id",
            )
            .map_err(|_| PluginError::Storage)?;
        let rows = stmt
            .query_map([], |row| {
                let status: String = row.get(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    status == "revoked",
                ))
            })
            .map_err(|_| PluginError::Storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|_| PluginError::Storage)?);
        }
        Ok(out)
    }

    pub fn set_plugin_security_status(
        &self,
        plugin_id: &str,
        version: &str,
        status: &str,
        advisory_id: Option<&str>,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_versions_security(plugin_id, version, security_status, advisory_id, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(plugin_id, version) DO UPDATE SET
               security_status = excluded.security_status,
               advisory_id = excluded.advisory_id,
               checked_at = excluded.checked_at",
            params![plugin_id, version, status, advisory_id, now_secs()],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn get_plugin_security_status(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> PluginResult<Option<String>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.query_row(
            "SELECT security_status FROM plugin_versions_security
             WHERE plugin_id = ?1 AND version = ?2",
            params![plugin_id, version],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| PluginError::Storage)
    }

    pub fn upsert_revocation(
        &self,
        kind: &str,
        target_id: &str,
        version: Option<&str>,
        reason: Option<&str>,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_revocations(kind, target_id, version, reason, revoked_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(kind, target_id, version) DO UPDATE SET
               reason = excluded.reason,
               revoked_at = excluded.revoked_at",
            params![kind, target_id, version.unwrap_or(""), reason, now_secs()],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn is_revoked(
        &self,
        kind: &str,
        target_id: &str,
        version: Option<&str>,
    ) -> PluginResult<bool> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM plugin_revocations
                 WHERE kind = ?1 AND target_id = ?2 AND version = ?3",
                params![kind, target_id, version.unwrap_or("")],
                |row| row.get(0),
            )
            .map_err(|_| PluginError::Storage)?;
        Ok(count > 0)
    }

    pub fn cache_marketplace_plugin(
        &self,
        plugin_id: &str,
        metadata_json: &str,
    ) -> PluginResult<()> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.execute(
            "INSERT INTO plugin_marketplace_cache(plugin_id, metadata_json, cached_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(plugin_id) DO UPDATE SET
               metadata_json = excluded.metadata_json,
               cached_at = excluded.cached_at",
            params![plugin_id, metadata_json, now_secs()],
        )
        .map_err(|_| PluginError::Storage)?;
        Ok(())
    }

    pub fn list_marketplace_cache(&self) -> PluginResult<Vec<(String, String)>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let mut stmt = conn
            .prepare(
                "SELECT plugin_id, metadata_json FROM plugin_marketplace_cache ORDER BY plugin_id",
            )
            .map_err(|_| PluginError::Storage)?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|_| PluginError::Storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|_| PluginError::Storage)?);
        }
        Ok(out)
    }

    pub fn list_installed_versions(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        let mut stmt = conn
            .prepare("SELECT version FROM plugins WHERE id = ?1 ORDER BY installed_at")
            .map_err(|_| PluginError::Storage)?;
        let rows = stmt
            .query_map(params![plugin_id], |row| row.get(0))
            .map_err(|_| PluginError::Storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|_| PluginError::Storage)?);
        }
        Ok(out)
    }

    pub fn get_plugin_version(
        &self,
        plugin_id: &str,
        version: &str,
    ) -> PluginResult<Option<InstalledPluginRecord>> {
        let conn = self.conn.lock().map_err(|_| PluginError::Storage)?;
        conn.query_row(
            "SELECT id, version, name, publisher_id, install_path, enabled, installed_at, updated_at
             FROM plugins WHERE id = ?1 AND version = ?2",
            params![plugin_id, version],
            |row| {
                Ok(InstalledPluginRecord {
                    id: row.get(0)?,
                    version: row.get(1)?,
                    name: row.get(2)?,
                    publisher_id: row.get(3)?,
                    install_path: row.get(4)?,
                    enabled: row.get::<_, i64>(5)? != 0,
                    installed_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|_| PluginError::Storage)
    }
}

pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_lists_plugins() {
        let repo = PluginRegistryRepository::open_in_memory().expect("open");
        let record = InstalledPluginRecord {
            id: "runory.hello".into(),
            version: "1.0.0".into(),
            name: "Hello".into(),
            publisher_id: "runory".into(),
            install_path: "/tmp/x".into(),
            enabled: true,
            installed_at: 1,
            updated_at: 1,
        };
        repo.insert_plugin(&record).expect("insert");
        repo.set_active_version("runory.hello", "1.0.0")
            .expect("active");
        let listed = repo.list_active().expect("list");
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn failed_install_transaction_preserves_version_and_permissions() {
        let repo = PluginRegistryRepository::open_in_memory().expect("open");
        let mut record = InstalledPluginRecord {
            id: "runory.atomic".into(),
            version: "1.0.0".into(),
            name: "Atomic".into(),
            publisher_id: "runory".into(),
            install_path: "/tmp/atomic".into(),
            enabled: true,
            installed_at: 1,
            updated_at: 1,
        };
        let grant = PermissionGrant {
            plugin_id: record.id.clone(),
            capability: PluginCapability::HostRead,
            granted: true,
            hosts: vec![],
        };
        repo.install_version(&record, std::slice::from_ref(&grant))
            .expect("initial");
        record.version = "2.0.0".into();
        // Duplicate primary keys simulate failure after the active pointer changed inside the transaction.
        assert!(repo
            .install_version(&record, &[grant.clone(), grant])
            .is_err());
        assert_eq!(
            repo.get_active(&record.id)
                .expect("get")
                .expect("active")
                .version,
            "1.0.0"
        );
        assert_eq!(repo.list_permissions(&record.id).expect("grants").len(), 1);
        assert!(repo
            .get_plugin_version(&record.id, "2.0.0")
            .expect("version")
            .is_none());
    }

    #[test]
    fn publisher_key_revocation_cannot_be_undone_by_catalog_sync() {
        let repo = PluginRegistryRepository::open_in_memory().expect("open");
        repo.upsert_publisher("runory", "Runory", "official")
            .expect("publisher");
        repo.upsert_publisher_key("key", "runory", "old", "revoked")
            .expect("revoke");
        repo.upsert_publisher_key("key", "runory", "new", "active")
            .expect("sync");
        let keys = repo.list_publisher_keys().expect("keys");
        assert!(keys[0].3);
        assert_eq!(keys[0].2, "old");
    }

    #[test]
    fn corrupt_host_scope_does_not_become_unrestricted() {
        let repo = PluginRegistryRepository::open_in_memory().expect("open");
        repo.replace_permissions(
            "runory.scope",
            &[PermissionGrant {
                plugin_id: "runory.scope".into(),
                capability: PluginCapability::HostRead,
                granted: true,
                hosts: vec!["host-a".into()],
            }],
        )
        .expect("grant");
        repo.conn
            .lock()
            .expect("lock")
            .execute("UPDATE plugin_permissions SET hosts_json = 'invalid'", [])
            .expect("corrupt fixture");
        assert!(matches!(
            repo.list_permissions("runory.scope"),
            Err(PluginError::Storage)
        ));
    }
}
