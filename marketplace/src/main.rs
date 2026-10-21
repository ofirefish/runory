//! Runory Marketplace backend (PP5) — filesystem + SQLite catalog.
//!
//! Run: `cargo run --manifest-path marketplace/Cargo.toml -- --root ./marketplace-data --bind 127.0.0.1:8787`
//!
//! Publish: `POST /api/v1/admin/publish` with JSON body including `packageBase64`.

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 70 * 1024 * 1024;

#[derive(Clone)]
struct AppState {
    root: PathBuf,
    db: Arc<Mutex<Connection>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginRow {
    id: String,
    publisher_id: String,
    name: String,
    description: Option<String>,
    category: Option<String>,
    latest_version: String,
    api_version: String,
    verified: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionRow {
    plugin_id: String,
    version: String,
    api_version: String,
    min_runory_version: Option<String>,
    package_sha256: String,
    download_url: String,
    status: String,
    changelog: Option<String>,
    permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevocationRow {
    kind: String,
    target_id: String,
    version: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherKeyRow {
    publisher_id: String,
    key_id: String,
    algorithm: String,
    public_key: String,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishRequest {
    plugin_id: String,
    publisher_id: String,
    name: String,
    description: Option<String>,
    category: Option<String>,
    version: String,
    #[serde(default = "default_api_version")]
    api_version: String,
    min_runory_version: Option<String>,
    changelog: Option<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    verified: bool,
    package_base64: String,
}

fn default_api_version() -> String {
    "1".into()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublisherKeyUpsert {
    publisher_id: String,
    key_id: String,
    public_key: String,
    #[serde(default = "default_algorithm")]
    algorithm: String,
    #[serde(default = "default_key_status")]
    status: String,
}

fn default_algorithm() -> String {
    "ed25519".into()
}

fn default_key_status() -> String {
    "active".into()
}

/// Official Runory publisher public key (matches plugin-sdk/keys/runory-2026-main.pub.json).
const OFFICIAL_RUNORY_KEY_ID: &str = "runory-2026-main";
const OFFICIAL_RUNORY_PUBLIC_KEY: &str = "YfFuwISbG9zyHwlChH6PPyQ6rueVJp7ofqFC1VGndiI=";

fn main() {
    let mut root = PathBuf::from("./marketplace-data");
    let mut bind = "127.0.0.1:8787".to_string();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                root = PathBuf::from(&args[i]);
            }
            "--bind" => {
                i += 1;
                bind = args[i].clone();
            }
            _ => {}
        }
        i += 1;
    }
    fs::create_dir_all(root.join("packages")).expect("packages dir");
    let db_path = root.join("marketplace.db");
    let conn = Connection::open(&db_path).expect("db");
    migrate(&conn);
    seed_if_empty(&conn, &root);
    seed_official_keys(&conn);
    let state = AppState {
        root,
        db: Arc::new(Mutex::new(conn)),
    };
    let listener = TcpListener::bind(&bind).expect("bind");
    eprintln!("runory-marketplace listening on http://{bind}");
    for stream in listener.incoming().flatten() {
        let state = state.clone();
        let _ = std::thread::spawn(move || {
            let _ = handle_client(stream, &state);
        });
    }
}

fn migrate(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY,
            publisher_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            category TEXT,
            latest_version TEXT NOT NULL,
            api_version TEXT NOT NULL,
            verified INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS plugin_versions (
            plugin_id TEXT NOT NULL,
            version TEXT NOT NULL,
            api_version TEXT NOT NULL,
            min_runory_version TEXT,
            package_sha256 TEXT NOT NULL,
            package_path TEXT NOT NULL,
            status TEXT NOT NULL,
            changelog TEXT,
            permissions_json TEXT NOT NULL DEFAULT '[]',
            PRIMARY KEY (plugin_id, version)
        );
        CREATE TABLE IF NOT EXISTS revocations (
            kind TEXT NOT NULL,
            target_id TEXT NOT NULL,
            version TEXT NOT NULL DEFAULT '',
            reason TEXT,
            PRIMARY KEY (kind, target_id, version)
        );
        CREATE TABLE IF NOT EXISTS publisher_keys (
            publisher_id TEXT NOT NULL,
            key_id TEXT NOT NULL,
            algorithm TEXT NOT NULL,
            public_key TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'active',
            PRIMARY KEY (publisher_id, key_id)
        );",
    )
    .expect("migrate");
}

fn seed_official_keys(conn: &Connection) {
    let _ = conn.execute(
        "INSERT OR IGNORE INTO publisher_keys(publisher_id, key_id, algorithm, public_key, status)
         VALUES ('runory', ?1, 'ed25519', ?2, 'active')",
        params![OFFICIAL_RUNORY_KEY_ID, OFFICIAL_RUNORY_PUBLIC_KEY],
    );
}

fn seed_if_empty(conn: &Connection, root: &Path) {
    let count: i64 = conn
        .query_row("SELECT COUNT(1) FROM plugins", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        return;
    }
    let seeds = [
        (
            "runory.hello",
            "Runory Hello",
            "Minimal hello plugin",
            "Development",
            "1.0.0",
        ),
        (
            "runory.django-deployer",
            "Django Deployment",
            "Deploy Django apps",
            "Deployment",
            "0.2.0",
        ),
        (
            "runory.docker-manager",
            "Docker Manager",
            "Operate Docker containers",
            "Containers",
            "1.0.0",
        ),
    ];
    for (id, name, desc, category, version) in seeds {
        conn.execute(
            "INSERT INTO plugins(id, publisher_id, name, description, category, latest_version, api_version, verified)
             VALUES (?1, 'runory', ?2, ?3, ?4, ?5, '1', 1)",
            params![id, name, desc, category, version],
        )
        .ok();
        let package_path = format!("packages/{id}/{version}/plugin.runory-plugin");
        fs::create_dir_all(root.join(format!("packages/{id}/{version}"))).ok();
        conn.execute(
            "INSERT INTO plugin_versions(plugin_id, version, api_version, min_runory_version, package_sha256, package_path, status, changelog, permissions_json)
             VALUES (?1, ?2, '1', '0.1.0', '', ?3, 'published', 'Initial marketplace listing', '[]')",
            params![id, version, package_path],
        )
        .ok();
    }
}

fn handle_client(mut stream: TcpStream, state: &AppState) -> std::io::Result<()> {
    let (method, path, body) = match read_http_request(&mut stream) {
        Ok(v) => v,
        Err(_) => {
            return write_response(&mut stream, 400, "application/json", b"{\"error\":\"bad_request\"}");
        }
    };

    if method == "OPTIONS" {
        return write_cors_preflight(&mut stream);
    }
    if method != "GET" && method != "POST" {
        return write_response(&mut stream, 405, "application/json", b"{\"error\":\"method\"}");
    }

    let (status, body) = match (method.as_str(), path.as_str()) {
        ("GET", "/api/v1/plugins") => {
            let plugins = list_plugins(state);
            (200, serde_json::to_vec(&plugins).unwrap_or_default())
        }
        (m, p) if m == "GET" && p.starts_with("/api/v1/plugins/") => {
            let rest = &p["/api/v1/plugins/".len()..];
            if let Some((id, "versions")) = rest.split_once('/') {
                let versions = list_versions(state, id);
                (200, serde_json::to_vec(&versions).unwrap_or_default())
            } else if let Some((id, rest2)) = rest.split_once('/') {
                if rest2 == "download" {
                    return serve_download(&mut stream, state, id, None);
                }
                if let Some((ver, "download")) = rest2.split_once('/') {
                    return serve_download(&mut stream, state, id, Some(ver));
                }
                if rest2.starts_with("versions/") {
                    let after = &rest2["versions/".len()..];
                    if let Some((ver, "download")) = after.split_once('/') {
                        return serve_download(&mut stream, state, id, Some(ver));
                    }
                    let version = after;
                    let versions = list_versions(state, id);
                    let found = versions.into_iter().find(|v| v.version == version);
                    (
                        if found.is_some() { 200 } else { 404 },
                        serde_json::to_vec(&found).unwrap_or_default(),
                    )
                } else {
                    (404, b"{}".to_vec())
                }
            } else {
                let plugins = list_plugins(state);
                let found = plugins.into_iter().find(|p| p.id == rest);
                (
                    if found.is_some() { 200 } else { 404 },
                    serde_json::to_vec(&found).unwrap_or_default(),
                )
            }
        }
        ("GET", "/api/v1/security/revocations") => {
            let rows = list_revocations(state);
            (200, serde_json::to_vec(&rows).unwrap_or_default())
        }
        ("GET", p) if p.starts_with("/api/v1/publishers/") => {
            let rest = &p["/api/v1/publishers/".len()..];
            if let Some((id, "keys")) = rest.split_once('/') {
                let keys = list_publisher_keys(state, id);
                (200, serde_json::to_vec(&keys).unwrap_or_default())
            } else if rest.ends_with("/keys") {
                let id = &rest[..rest.len() - "/keys".len()];
                let keys = list_publisher_keys(state, id);
                (200, serde_json::to_vec(&keys).unwrap_or_default())
            } else {
                (
                    200,
                    serde_json::to_vec(&json!({
                        "id": rest,
                        "name": rest,
                        "verified": rest == "runory"
                    }))
                    .unwrap_or_default(),
                )
            }
        }
        ("POST", "/api/v1/admin/publish") => match publish_package(state, &body) {
            Ok(resp) => (200, resp),
            Err(code) => (
                code,
                serde_json::to_vec(&json!({ "error": status_label(code) })).unwrap_or_default(),
            ),
        },
        ("POST", "/api/v1/admin/publisher-keys") => {
            match upsert_publisher_key_api(state, &body) {
                Ok(resp) => (200, resp),
                Err(code) => (
                    code,
                    serde_json::to_vec(&json!({ "error": status_label(code) })).unwrap_or_default(),
                ),
            }
        }
        ("POST", "/api/v1/admin/revoke") => {
            if let Ok(entry) = serde_json::from_slice::<RevocationRow>(&body) {
                let db = state.db.lock().unwrap();
                let _ = db.execute(
                    "INSERT OR REPLACE INTO revocations(kind, target_id, version, reason) VALUES (?1,?2,?3,?4)",
                    params![
                        entry.kind,
                        entry.target_id,
                        entry.version.clone().unwrap_or_default(),
                        entry.reason
                    ],
                );
                (200, b"{\"ok\":true}".to_vec())
            } else {
                (400, b"{\"error\":\"invalid\"}".to_vec())
            }
        }
        _ => (404, b"{\"error\":\"not_found\"}".to_vec()),
    };
    write_response(&mut stream, status, "application/json", &body)
}

fn status_label(code: u16) -> &'static str {
    match code {
        400 => "invalid",
        409 => "version_immutable",
        413 => "too_large",
        _ => "error",
    }
}

fn read_http_request(stream: &mut TcpStream) -> std::io::Result<(String, String, Vec<u8>)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_header_end(&buf) {
            break Some(pos);
        }
        if buf.len() > MAX_HEADER_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "headers too large",
            ));
        }
    };
    let Some(pos) = header_end else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "incomplete headers",
        ));
    };
    let header = String::from_utf8_lossy(&buf[..pos]).into_owned();
    let mut lines = header.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let path = parts.next().unwrap_or("/").to_owned();
    let content_length = parse_content_length(&header).unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "body too large",
        ));
    }
    let mut body = buf[pos + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
        if body.len() > MAX_BODY_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "body too large",
            ));
        }
    }
    body.truncate(content_length);
    Ok((method, path, body))
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_content_length(header: &str) -> Option<usize> {
    for line in header.lines() {
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            return rest.trim().parse().ok();
        }
    }
    None
}

fn write_cors_preflight(stream: &mut TcpStream) -> std::io::Result<()> {
    let header = "HTTP/1.1 204 No Content\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type\r\n\
Content-Length: 0\r\n\
Connection: close\r\n\r\n";
    stream.write_all(header.as_bytes())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

fn list_plugins(state: &AppState) -> Vec<PluginRow> {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare(
            "SELECT id, publisher_id, name, description, category, latest_version, api_version, verified FROM plugins ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| {
        Ok(PluginRow {
            id: row.get(0)?,
            publisher_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            category: row.get(4)?,
            latest_version: row.get(5)?,
            api_version: row.get(6)?,
            verified: row.get::<_, i64>(7)? != 0,
        })
    })
    .unwrap()
    .filter_map(Result::ok)
    .collect()
}

fn list_versions(state: &AppState, plugin_id: &str) -> Vec<VersionRow> {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare(
            "SELECT plugin_id, version, api_version, min_runory_version, package_sha256, package_path, status, changelog, permissions_json
             FROM plugin_versions WHERE plugin_id = ?1 ORDER BY version",
        )
        .unwrap();
    stmt.query_map(params![plugin_id], |row| {
        let version: String = row.get(1)?;
        let permissions_json: String = row.get(8)?;
        let permissions: Vec<String> =
            serde_json::from_str(&permissions_json).unwrap_or_default();
        Ok(VersionRow {
            plugin_id: row.get(0)?,
            version: version.clone(),
            api_version: row.get(2)?,
            min_runory_version: row.get(3)?,
            package_sha256: row.get(4)?,
            download_url: format!(
                "/api/v1/plugins/{plugin_id}/versions/{version}/download"
            ),
            status: row.get(6)?,
            changelog: row.get(7)?,
            permissions,
        })
    })
    .unwrap()
    .filter_map(Result::ok)
    .collect()
}

fn list_revocations(state: &AppState) -> Vec<RevocationRow> {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT kind, target_id, version, reason FROM revocations")
        .unwrap();
    stmt.query_map([], |row| {
        let version: String = row.get(2)?;
        Ok(RevocationRow {
            kind: row.get(0)?,
            target_id: row.get(1)?,
            version: if version.is_empty() {
                None
            } else {
                Some(version)
            },
            reason: row.get(3)?,
        })
    })
    .unwrap()
    .filter_map(Result::ok)
    .collect()
}

fn list_publisher_keys(state: &AppState, publisher_id: &str) -> Vec<PublisherKeyRow> {
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .prepare(
            "SELECT publisher_id, key_id, algorithm, public_key, status FROM publisher_keys
             WHERE publisher_id = ?1 ORDER BY key_id",
        )
        .unwrap();
    stmt.query_map(params![publisher_id], |row| {
        Ok(PublisherKeyRow {
            publisher_id: row.get(0)?,
            key_id: row.get(1)?,
            algorithm: row.get(2)?,
            public_key: row.get(3)?,
            status: row.get(4)?,
        })
    })
    .unwrap()
    .filter_map(Result::ok)
    .collect()
}

fn publish_package(state: &AppState, body: &[u8]) -> Result<Vec<u8>, u16> {
    let req: PublishRequest = serde_json::from_slice(body).map_err(|_| 400u16)?;
    if req.plugin_id.trim().is_empty()
        || req.version.trim().is_empty()
        || req.publisher_id.trim().is_empty()
        || req.name.trim().is_empty()
    {
        return Err(400);
    }
    let package_bytes = B64.decode(req.package_base64.as_bytes()).map_err(|_| 400u16)?;
    if package_bytes.is_empty() || package_bytes.len() > 50 * 1024 * 1024 {
        return Err(413);
    }
    let digest = Sha256::digest(&package_bytes);
    let sha256 = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let rel = format!(
        "packages/{}/{}/plugin.runory-plugin",
        req.plugin_id, req.version
    );
    let abs = state.root.join(&rel);
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent).map_err(|_| 500u16)?;
    }

    {
        let db = state.db.lock().map_err(|_| 500u16)?;
        let exists: i64 = db
            .query_row(
                "SELECT COUNT(1) FROM plugin_versions WHERE plugin_id = ?1 AND version = ?2",
                params![req.plugin_id, req.version],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            return Err(409);
        }
        fs::write(&abs, &package_bytes).map_err(|_| 500u16)?;
        let permissions_json =
            serde_json::to_string(&req.permissions).unwrap_or_else(|_| "[]".into());
        db.execute(
            "INSERT INTO plugin_versions(plugin_id, version, api_version, min_runory_version, package_sha256, package_path, status, changelog, permissions_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'published', ?7, ?8)",
            params![
                req.plugin_id,
                req.version,
                req.api_version,
                req.min_runory_version,
                sha256,
                rel,
                req.changelog,
                permissions_json
            ],
        )
        .map_err(|_| 500u16)?;
        db.execute(
            "INSERT INTO plugins(id, publisher_id, name, description, category, latest_version, api_version, verified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               publisher_id = excluded.publisher_id,
               name = excluded.name,
               description = excluded.description,
               category = excluded.category,
               latest_version = excluded.latest_version,
               api_version = excluded.api_version,
               verified = excluded.verified",
            params![
                req.plugin_id,
                req.publisher_id,
                req.name,
                req.description,
                req.category,
                req.version,
                req.api_version,
                if req.verified { 1 } else { 0 }
            ],
        )
        .map_err(|_| 500u16)?;
    }

    Ok(serde_json::to_vec(&json!({
        "ok": true,
        "pluginId": req.plugin_id,
        "version": req.version,
        "packageSha256": sha256,
        "downloadUrl": format!(
            "/api/v1/plugins/{}/versions/{}/download",
            req.plugin_id, req.version
        )
    }))
    .unwrap_or_default())
}

fn upsert_publisher_key_api(state: &AppState, body: &[u8]) -> Result<Vec<u8>, u16> {
    let req: PublisherKeyUpsert = serde_json::from_slice(body).map_err(|_| 400u16)?;
    if req.publisher_id.trim().is_empty()
        || req.key_id.trim().is_empty()
        || req.public_key.trim().is_empty()
    {
        return Err(400);
    }
    let db = state.db.lock().map_err(|_| 500u16)?;
    db.execute(
        "INSERT INTO publisher_keys(publisher_id, key_id, algorithm, public_key, status)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(publisher_id, key_id) DO UPDATE SET
           algorithm = excluded.algorithm,
           public_key = excluded.public_key,
           status = excluded.status",
        params![
            req.publisher_id,
            req.key_id,
            req.algorithm,
            req.public_key,
            req.status
        ],
    )
    .map_err(|_| 500u16)?;
    Ok(b"{\"ok\":true}".to_vec())
}

fn serve_download(
    stream: &mut TcpStream,
    state: &AppState,
    plugin_id: &str,
    version: Option<&str>,
) -> std::io::Result<()> {
    let db = state.db.lock().unwrap();
    let row: Result<(String, String), _> = if let Some(version) = version {
        db.query_row(
            "SELECT package_path, package_sha256 FROM plugin_versions WHERE plugin_id = ?1 AND version = ?2",
            params![plugin_id, version],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    } else {
        db.query_row(
            "SELECT v.package_path, v.package_sha256 FROM plugin_versions v
             INNER JOIN plugins p ON p.id = v.plugin_id AND p.latest_version = v.version
             WHERE p.id = ?1",
            params![plugin_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    };
    drop(db);
    let Ok((rel, _sha)) = row else {
        return write_response(stream, 404, "application/json", b"{\"error\":\"missing\"}");
    };
    let path = state.root.join(rel);
    let Ok(bytes) = fs::read(&path) else {
        return write_response(stream, 404, "application/json", b"{\"error\":\"file\"}");
    };
    write_response(stream, 200, "application/octet-stream", &bytes)
}
