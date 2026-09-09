use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult, DockerRegistry, OperationResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};
use crate::storage::JsonRepository;

const MAX_REGISTRIES_PER_PROFILE: usize = 50;
const MAX_URL_LEN: usize = 256;
const MAX_NAME_LEN: usize = 128;
const MAX_USER_LEN: usize = 128;
const MAX_PASSWORD_LEN: usize = 512;
const MAX_NAMESPACE_LEN: usize = 128;
const MAX_REMARKS_LEN: usize = 512;

/// Password via stdin only — never interpolated into the remote command line.
const DOCKER_LOGIN: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
user="$1"
server="$2"
docker login -u "$user" --password-stdin "$server"
"#;

const DOCKER_LOGOUT: &str = "command -v docker >/dev/null 2>&1 || exit 90; docker logout \"$1\"";

pub struct DockerRegistriesRepository {
    repository: JsonRepository<Vec<DockerRegistry>>,
    write_lock: Mutex<()>,
}

impl DockerRegistriesRepository {
    pub fn new(repository: JsonRepository<Vec<DockerRegistry>>) -> Self {
        Self {
            repository,
            write_lock: Mutex::new(()),
        }
    }

    pub async fn list(&self, profile_id: Uuid) -> AppResult<Vec<DockerRegistry>> {
        let mut items = self
            .repository
            .load_or_default()
            .await?
            .into_iter()
            .filter(|item| item.profile_id == profile_id)
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .updated_at_epoch_seconds
                .cmp(&left.updated_at_epoch_seconds)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(items)
    }

    pub async fn upsert(
        &self,
        id: Option<Uuid>,
        profile_id: Uuid,
        url: String,
        name: String,
        username: String,
        namespace: String,
        remarks: String,
    ) -> AppResult<DockerRegistry> {
        let url = normalize_registry_url(&url)?;
        let name = validate_required(&name, MAX_NAME_LEN)?;
        let username = validate_required(&username, MAX_USER_LEN)?;
        let namespace = validate_required(&namespace, MAX_NAMESPACE_LEN)?;
        let remarks = validate_optional(&remarks, MAX_REMARKS_LEN)?;

        let _guard = self.write_lock.lock().await;
        let mut items = self.repository.load_or_default().await?;
        let now = epoch_seconds()?;
        let registry_id = id.unwrap_or_else(Uuid::new_v4);

        if let Some(existing_id) = id {
            if !items
                .iter()
                .any(|item| item.id == existing_id && item.profile_id == profile_id)
            {
                return Err(AppError::InvalidOperation);
            }
        } else {
            let profile_count = items
                .iter()
                .filter(|item| item.profile_id == profile_id)
                .count();
            if profile_count >= MAX_REGISTRIES_PER_PROFILE {
                return Err(AppError::InvalidOperation);
            }
        }

        if items.iter().any(|item| {
            item.profile_id == profile_id
                && item.id != registry_id
                && (item.name.eq_ignore_ascii_case(&name) || item.url.eq_ignore_ascii_case(&url))
        }) {
            return Err(AppError::InvalidOperation);
        }

        let registry = DockerRegistry {
            id: registry_id,
            profile_id,
            url,
            name,
            username,
            namespace,
            remarks,
            updated_at_epoch_seconds: now,
        };

        if let Some(index) = items.iter().position(|entry| entry.id == registry_id) {
            items[index] = registry.clone();
        } else {
            items.push(registry.clone());
        }
        self.repository.save_atomic(&items).await?;
        Ok(registry)
    }

    pub async fn delete_many(
        &self,
        profile_id: Uuid,
        ids: &[Uuid],
    ) -> AppResult<Vec<DockerRegistry>> {
        if ids.is_empty() || ids.len() > MAX_REGISTRIES_PER_PROFILE {
            return Err(AppError::InvalidOperation);
        }
        let id_set: std::collections::HashSet<Uuid> = ids.iter().copied().collect();
        if id_set.len() != ids.len() {
            return Err(AppError::InvalidOperation);
        }

        let _guard = self.write_lock.lock().await;
        let mut items = self.repository.load_or_default().await?;
        let mut removed = Vec::new();
        items.retain(|item| {
            if item.profile_id == profile_id && id_set.contains(&item.id) {
                removed.push(item.clone());
                false
            } else {
                true
            }
        });
        if removed.len() != id_set.len() {
            return Err(AppError::InvalidOperation);
        }
        self.repository.save_atomic(&items).await?;
        Ok(removed)
    }
}

pub struct DockerRegistriesService;

impl DockerRegistriesService {
    pub async fn list(
        repository: &DockerRegistriesRepository,
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<DockerRegistry>> {
        let profile_id = sessions.profile_id(session_id).await?;
        repository.list(profile_id).await
    }

    pub async fn upsert(
        repository: &DockerRegistriesRepository,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        id: Option<Uuid>,
        url: String,
        name: String,
        username: String,
        password: String,
        namespace: String,
        remarks: String,
    ) -> AppResult<DockerRegistry> {
        let profile_id = sessions.profile_id(session_id).await?;
        let is_create = id.is_none();
        let mut password = Zeroizing::new(password.trim().to_owned());
        if is_create && password.is_empty() {
            return Err(AppError::InvalidOperation);
        }
        if password.len() > MAX_PASSWORD_LEN {
            return Err(AppError::InvalidOperation);
        }

        let url_for_login = normalize_registry_url(&url)?;
        let username_for_login = validate_required(&username, MAX_USER_LEN)?;

        if !password.is_empty() {
            let login = docker_login(
                sessions,
                session_id,
                &username_for_login,
                &url_for_login,
                password.as_bytes(),
            )
            .await;
            password.zeroize();
            let login = login?;
            if !login.success {
                return Err(AppError::ExecFailed);
            }
        } else {
            password.zeroize();
        }

        repository
            .upsert(id, profile_id, url, name, username, namespace, remarks)
            .await
    }

    pub async fn delete(
        repository: &DockerRegistriesRepository,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        ids: Vec<Uuid>,
    ) -> AppResult<OperationResult> {
        let profile_id = sessions.profile_id(session_id).await?;
        let removed = repository.delete_many(profile_id, &ids).await?;
        let mut outputs = Vec::new();
        let mut all_ok = true;
        for item in removed {
            let result = docker_logout(sessions, session_id, &item.url).await?;
            if !result.success {
                all_ok = false;
            }
            if !result.output.is_empty() {
                outputs.push(result.output);
            }
        }
        Ok(OperationResult {
            success: all_ok,
            output: outputs.join("\n"),
        })
    }
}

async fn docker_login(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    username: &str,
    url: &str,
    password: &[u8],
) -> AppResult<OperationResult> {
    let server = login_server(url);
    let command = RemoteCommand::script(DOCKER_LOGIN, vec![username.to_owned(), server])
        .with_stdin(password.to_vec())?;
    run_docker_auth(sessions, session_id, command).await
}

async fn docker_logout(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    url: &str,
) -> AppResult<OperationResult> {
    let server = login_server(url);
    run_docker_auth(
        sessions,
        session_id,
        RemoteCommand::script(DOCKER_LOGOUT, vec![server]),
    )
    .await
}

async fn run_docker_auth(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    command: RemoteCommand,
) -> AppResult<OperationResult> {
    let result = sessions.exec(session_id, command).await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    // Never echo remote stdout/stderr from login/logout — may contain hints about auth.
    Ok(OperationResult {
        success: result.exit_code == 0,
        output: String::new(),
    })
}

fn login_server(url: &str) -> String {
    let lower = url.to_ascii_lowercase();
    if lower == "docker.io" || lower == "index.docker.io" || lower == "registry-1.docker.io" {
        "https://index.docker.io/v1/".to_owned()
    } else {
        url.to_owned()
    }
}

fn normalize_registry_url(value: &str) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_URL_LEN {
        return Err(AppError::InvalidOperation);
    }
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    if without_scheme.is_empty() || without_scheme.len() > MAX_URL_LEN {
        return Err(AppError::InvalidOperation);
    }
    if !without_scheme.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'/' | b'_')
    }) {
        return Err(AppError::InvalidOperation);
    }
    if without_scheme.contains("..") || without_scheme.starts_with('/') {
        return Err(AppError::InvalidOperation);
    }
    Ok(without_scheme.to_owned())
}

fn validate_required(value: &str, max: usize) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max {
        return Err(AppError::InvalidOperation);
    }
    if trimmed.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(AppError::InvalidOperation);
    }
    Ok(trimmed.to_owned())
}

fn validate_optional(value: &str, max: usize) -> AppResult<String> {
    let trimmed = value.trim();
    if trimmed.len() > max {
        return Err(AppError::InvalidOperation);
    }
    if trimmed.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(AppError::InvalidOperation);
    }
    Ok(trimmed.to_owned())
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
    use tempfile::tempdir;

    fn repo(path: &std::path::Path) -> DockerRegistriesRepository {
        DockerRegistriesRepository::new(JsonRepository::new(path.join("docker-registries.json")))
    }

    #[tokio::test]
    async fn upsert_list_delete_scoped_by_profile() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let other = Uuid::new_v4();

        let first = repository
            .upsert(
                None,
                profile,
                "ccr.ccs.tencentyun.com".into(),
                "tencent".into(),
                "alice".into(),
                "ns".into(),
                "note".into(),
            )
            .await
            .unwrap();
        assert_eq!(first.url, "ccr.ccs.tencentyun.com");
        assert_eq!(first.namespace, "ns");

        repository
            .upsert(
                None,
                other,
                "docker.io".into(),
                "hub".into(),
                "bob".into(),
                "library".into(),
                String::new(),
            )
            .await
            .unwrap();

        assert_eq!(repository.list(profile).await.unwrap().len(), 1);
        assert_eq!(repository.list(other).await.unwrap().len(), 1);

        let duplicate = repository
            .upsert(
                None,
                profile,
                "ccr.ccs.tencentyun.com".into(),
                "other".into(),
                "alice".into(),
                "ns".into(),
                String::new(),
            )
            .await;
        assert!(duplicate.is_err());

        let updated = repository
            .upsert(
                Some(first.id),
                profile,
                "https://ccr.ccs.tencentyun.com/".into(),
                "tencent-cloud".into(),
                "alice".into(),
                "ns2".into(),
                "updated".into(),
            )
            .await
            .unwrap();
        assert_eq!(updated.name, "tencent-cloud");
        assert_eq!(updated.url, "ccr.ccs.tencentyun.com");
        assert_eq!(updated.namespace, "ns2");

        repository.delete_many(profile, &[first.id]).await.unwrap();
        assert!(repository.list(profile).await.unwrap().is_empty());
        assert_eq!(repository.list(other).await.unwrap().len(), 1);
    }

    #[test]
    fn normalize_strips_scheme_and_slash() {
        assert_eq!(
            normalize_registry_url("https://docker.io/").unwrap(),
            "docker.io"
        );
        assert!(normalize_registry_url("").is_err());
        assert!(normalize_registry_url("bad host").is_err());
        assert!(normalize_registry_url("../evil").is_err());
    }

    #[test]
    fn login_server_maps_docker_hub() {
        assert_eq!(login_server("docker.io"), "https://index.docker.io/v1/");
        assert_eq!(login_server("ccr.example.com"), "ccr.example.com");
    }
}
