//! Durable storage for non-sensitive SSH catalog metadata.
//!
//! Profiles, groups, and locally trusted host fingerprints share this one
//! database so callers do not need to coordinate separate JSON files. Secrets
//! remain in the credential vault and Agent runtime data remains isolated in
//! `runory-agent.db`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::domain::{
    AppError, AppResult, AuthMethod, HostGroup, KeySource, KnownHost, OsDistribution, ServerProfile,
};

const SCHEMA_VERSION: i64 = 1;

/// SQLite-backed local catalog for SSH metadata only.
#[derive(Clone)]
pub struct CatalogDatabase {
    connection: Arc<Mutex<Connection>>,
}

impl CatalogDatabase {
    /// Opens the catalog database without importing any legacy JSON files.
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
                 CREATE TABLE IF NOT EXISTS catalog_schema_version (
                    version INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS host_groups (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    sort_order INTEGER NOT NULL,
                    collapsed INTEGER NOT NULL CHECK (collapsed IN (0, 1)),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS server_profiles (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    username TEXT NOT NULL,
                    group_id TEXT,
                    auth_method_json TEXT NOT NULL,
                    key_source_json TEXT,
                    sort_order INTEGER NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    last_connected_at TEXT,
                    os_distribution_json TEXT
                 );
                 CREATE INDEX IF NOT EXISTS server_profiles_group_sort_idx
                    ON server_profiles (group_id, sort_order);
                 CREATE TABLE IF NOT EXISTS known_hosts (
                    host TEXT NOT NULL,
                    port INTEGER NOT NULL,
                    key_type TEXT NOT NULL,
                    fingerprint TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY (host, port)
                 );",
            )
            .map_err(|_| AppError::Storage)?;

        let version: Option<i64> = connection
            .query_row(
                "SELECT version FROM catalog_schema_version ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| AppError::Storage)?;
        match version {
            None => {
                connection
                    .execute(
                        "INSERT INTO catalog_schema_version (version) VALUES (?1)",
                        params![SCHEMA_VERSION],
                    )
                    .map_err(|_| AppError::Storage)?;
            }
            Some(SCHEMA_VERSION) => {}
            Some(_) => return Err(AppError::Storage),
        }

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub(crate) fn list_groups(&self) -> AppResult<Vec<HostGroup>> {
        let connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, sort_order, collapsed, created_at, updated_at
                 FROM host_groups ORDER BY sort_order, id",
            )
            .map_err(|_| AppError::Storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sort_order: row.get(2)?,
                    collapsed: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|_| AppError::Storage)?;

        rows.map(|row| {
            row.map_err(|_| AppError::Storage)
                .and_then(HostGroup::try_from)
        })
        .collect()
    }

    pub(crate) fn replace_groups(&self, groups: &[HostGroup]) -> AppResult<()> {
        let groups = groups
            .iter()
            .cloned()
            .map(StoredGroup::from)
            .collect::<Vec<_>>();
        let mut connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        let transaction = connection.transaction().map_err(|_| AppError::Storage)?;
        transaction
            .execute("DELETE FROM host_groups", [])
            .map_err(|_| AppError::Storage)?;
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO host_groups (id, name, sort_order, collapsed, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|_| AppError::Storage)?;
            for group in groups {
                insert
                    .execute(params![
                        group.id,
                        group.name,
                        group.sort_order,
                        group.collapsed,
                        group.created_at,
                        group.updated_at,
                    ])
                    .map_err(|_| AppError::Storage)?;
            }
        }
        transaction.commit().map_err(|_| AppError::Storage)
    }

    pub(crate) fn list_profiles(&self) -> AppResult<Vec<ServerProfile>> {
        let connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, host, port, username, group_id, auth_method_json,
                        key_source_json, sort_order, created_at, updated_at, last_connected_at,
                        os_distribution_json
                 FROM server_profiles ORDER BY sort_order, id",
            )
            .map_err(|_| AppError::Storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredProfile {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    host: row.get(2)?,
                    port: row.get(3)?,
                    username: row.get(4)?,
                    group_id: row.get(5)?,
                    auth_method_json: row.get(6)?,
                    key_source_json: row.get(7)?,
                    sort_order: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                    last_connected_at: row.get(11)?,
                    os_distribution_json: row.get(12)?,
                })
            })
            .map_err(|_| AppError::Storage)?;

        rows.map(|row| {
            row.map_err(|_| AppError::Storage)
                .and_then(ServerProfile::try_from)
        })
        .collect()
    }

    pub(crate) fn replace_profiles(&self, profiles: &[ServerProfile]) -> AppResult<()> {
        let profiles = profiles
            .iter()
            .map(StoredProfile::try_from)
            .collect::<AppResult<Vec<_>>>()?;
        let mut connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        let transaction = connection.transaction().map_err(|_| AppError::Storage)?;
        transaction
            .execute("DELETE FROM server_profiles", [])
            .map_err(|_| AppError::Storage)?;
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO server_profiles (
                        id, name, host, port, username, group_id, auth_method_json,
                        key_source_json, sort_order, created_at, updated_at, last_connected_at,
                        os_distribution_json
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                     )",
                )
                .map_err(|_| AppError::Storage)?;
            for profile in profiles {
                insert
                    .execute(params![
                        profile.id,
                        profile.name,
                        profile.host,
                        profile.port,
                        profile.username,
                        profile.group_id,
                        profile.auth_method_json,
                        profile.key_source_json,
                        profile.sort_order,
                        profile.created_at,
                        profile.updated_at,
                        profile.last_connected_at,
                        profile.os_distribution_json,
                    ])
                    .map_err(|_| AppError::Storage)?;
            }
        }
        transaction.commit().map_err(|_| AppError::Storage)
    }

    pub(crate) fn list_known_hosts(&self) -> AppResult<Vec<KnownHost>> {
        let connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        let mut statement = connection
            .prepare(
                "SELECT host, port, key_type, fingerprint, created_at, updated_at
                 FROM known_hosts ORDER BY host, port",
            )
            .map_err(|_| AppError::Storage)?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredKnownHost {
                    host: row.get(0)?,
                    port: row.get(1)?,
                    key_type: row.get(2)?,
                    fingerprint: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|_| AppError::Storage)?;

        rows.map(|row| {
            row.map_err(|_| AppError::Storage)
                .and_then(KnownHost::try_from)
        })
        .collect()
    }

    pub(crate) fn replace_known_hosts(&self, hosts: &[KnownHost]) -> AppResult<()> {
        let hosts = hosts
            .iter()
            .cloned()
            .map(StoredKnownHost::from)
            .collect::<Vec<_>>();
        let mut connection = self.connection.lock().map_err(|_| AppError::Storage)?;
        let transaction = connection.transaction().map_err(|_| AppError::Storage)?;
        transaction
            .execute("DELETE FROM known_hosts", [])
            .map_err(|_| AppError::Storage)?;
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO known_hosts (host, port, key_type, fingerprint, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|_| AppError::Storage)?;
            for host in hosts {
                insert
                    .execute(params![
                        host.host,
                        host.port,
                        host.key_type,
                        host.fingerprint,
                        host.created_at,
                        host.updated_at,
                    ])
                    .map_err(|_| AppError::Storage)?;
            }
        }
        transaction.commit().map_err(|_| AppError::Storage)
    }
}

struct StoredGroup {
    id: String,
    name: String,
    sort_order: i32,
    collapsed: i64,
    created_at: String,
    updated_at: String,
}

impl From<HostGroup> for StoredGroup {
    fn from(group: HostGroup) -> Self {
        Self {
            id: group.id.to_string(),
            name: group.name,
            sort_order: group.sort_order,
            collapsed: i64::from(group.collapsed),
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}

impl TryFrom<StoredGroup> for HostGroup {
    type Error = AppError;

    fn try_from(group: StoredGroup) -> AppResult<Self> {
        let collapsed = match group.collapsed {
            0 => false,
            1 => true,
            _ => return Err(AppError::Storage),
        };
        Ok(Self {
            id: parse_uuid(group.id)?,
            name: group.name,
            sort_order: group.sort_order,
            collapsed,
            created_at: group.created_at,
            updated_at: group.updated_at,
        })
    }
}

struct StoredProfile {
    id: String,
    name: String,
    host: String,
    port: i64,
    username: String,
    group_id: Option<String>,
    auth_method_json: String,
    key_source_json: Option<String>,
    sort_order: i32,
    created_at: String,
    updated_at: String,
    last_connected_at: Option<String>,
    os_distribution_json: Option<String>,
}

impl TryFrom<&ServerProfile> for StoredProfile {
    type Error = AppError;

    fn try_from(profile: &ServerProfile) -> AppResult<Self> {
        Ok(Self {
            id: profile.id.to_string(),
            name: profile.name.clone(),
            host: profile.host.clone(),
            port: i64::from(profile.port),
            username: profile.username.clone(),
            group_id: profile.group_id.map(|id| id.to_string()),
            auth_method_json: encode_json(&profile.auth_method)?,
            key_source_json: profile.key_source.as_ref().map(encode_json).transpose()?,
            sort_order: profile.sort_order,
            created_at: profile.created_at.clone(),
            updated_at: profile.updated_at.clone(),
            last_connected_at: profile.last_connected_at.clone(),
            os_distribution_json: profile
                .os_distribution
                .as_ref()
                .map(encode_json)
                .transpose()?,
        })
    }
}

impl TryFrom<StoredProfile> for ServerProfile {
    type Error = AppError;

    fn try_from(profile: StoredProfile) -> AppResult<Self> {
        Ok(Self {
            id: parse_uuid(profile.id)?,
            name: profile.name,
            host: profile.host,
            port: parse_port(profile.port)?,
            username: profile.username,
            group_id: profile.group_id.map(parse_uuid).transpose()?,
            auth_method: decode_json(&profile.auth_method_json)?,
            key_source: profile
                .key_source_json
                .as_deref()
                .map(decode_json)
                .transpose()?,
            sort_order: profile.sort_order,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
            last_connected_at: profile.last_connected_at,
            os_distribution: profile
                .os_distribution_json
                .as_deref()
                .map(decode_json)
                .transpose()?,
        })
    }
}

struct StoredKnownHost {
    host: String,
    port: i64,
    key_type: String,
    fingerprint: String,
    created_at: String,
    updated_at: String,
}

impl From<KnownHost> for StoredKnownHost {
    fn from(host: KnownHost) -> Self {
        Self {
            host: host.host,
            port: i64::from(host.port),
            key_type: host.key_type,
            fingerprint: host.fingerprint,
            created_at: host.created_at,
            updated_at: host.updated_at,
        }
    }
}

impl TryFrom<StoredKnownHost> for KnownHost {
    type Error = AppError;

    fn try_from(host: StoredKnownHost) -> AppResult<Self> {
        Ok(Self {
            host: host.host,
            port: parse_port(host.port)?,
            key_type: host.key_type,
            fingerprint: host.fingerprint,
            created_at: host.created_at,
            updated_at: host.updated_at,
        })
    }
}

fn parse_uuid(value: String) -> AppResult<Uuid> {
    value.parse().map_err(|_| AppError::Storage)
}

fn parse_port(value: i64) -> AppResult<u16> {
    let port = u16::try_from(value).map_err(|_| AppError::Storage)?;
    if port == 0 {
        return Err(AppError::Storage);
    }
    Ok(port)
}

fn encode_json<T: Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_string(value).map_err(|_| AppError::Storage)
}

fn decode_json<T: DeserializeOwned>(value: &str) -> AppResult<T> {
    serde_json::from_str(value).map_err(|_| AppError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_catalog_records_in_one_database() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("open");
        let group = HostGroup {
            id: Uuid::new_v4(),
            name: "Production".into(),
            sort_order: 0,
            collapsed: true,
            created_at: "1".into(),
            updated_at: "2".into(),
        };
        let profile = ServerProfile {
            id: Uuid::new_v4(),
            name: "API".into(),
            host: "api.example.com".into(),
            port: 22,
            username: "deploy".into(),
            group_id: Some(group.id),
            auth_method: AuthMethod::PrivateKey,
            key_source: Some(KeySource::Vault {
                key_id: Uuid::new_v4(),
            }),
            sort_order: 0,
            created_at: "3".into(),
            updated_at: "4".into(),
            last_connected_at: Some("5".into()),
            os_distribution: Some(OsDistribution::Ubuntu),
        };
        let known_host = KnownHost {
            host: "api.example.com".into(),
            port: 22,
            key_type: "ssh-ed25519".into(),
            fingerprint: "SHA256:example".into(),
            created_at: "6".into(),
            updated_at: "7".into(),
        };

        database
            .replace_groups(std::slice::from_ref(&group))
            .expect("save groups");
        database
            .replace_profiles(std::slice::from_ref(&profile))
            .expect("save profiles");
        database
            .replace_known_hosts(std::slice::from_ref(&known_host))
            .expect("save known hosts");

        drop(database);
        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("reopen");

        assert_eq!(database.list_groups().expect("groups"), vec![group]);
        assert_eq!(database.list_profiles().expect("profiles"), vec![profile]);
        assert_eq!(
            database.list_known_hosts().expect("known hosts"),
            vec![known_host]
        );
    }

    #[test]
    fn starts_empty_without_importing_legacy_json_catalogs() {
        let directory = tempfile::tempdir().expect("temp directory");
        std::fs::write(directory.path().join("profiles.json"), "not a profile list")
            .expect("write legacy fixture");
        std::fs::write(directory.path().join("groups.json"), "not a group list")
            .expect("write legacy fixture");
        std::fs::write(
            directory.path().join("known-hosts.json"),
            "not a known-host list",
        )
        .expect("write legacy fixture");

        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("open");

        assert!(database.list_groups().expect("groups").is_empty());
        assert!(database.list_profiles().expect("profiles").is_empty());
        assert!(database.list_known_hosts().expect("known hosts").is_empty());
    }
}
