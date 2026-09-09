use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;

use crate::domain::{
    AppError, AppResult, HostKeyInfo, HostVerification, HostVerificationStatus, KnownHost,
};
use crate::groups::timestamp;

use super::KnownHostRepository;

const ATTEMPT_LIFETIME: Duration = Duration::from_secs(120);

struct VerificationAttempt {
    route_scope: String,
    host: String,
    port: u16,
    key_type: String,
    fingerprint: String,
    approved: bool,
    expires_at: Instant,
}

pub struct KnownHostService {
    repository: KnownHostRepository,
    attempts: Mutex<HashMap<Uuid, VerificationAttempt>>,
    write_lock: Arc<Mutex<()>>,
}

impl KnownHostService {
    pub fn new(repository: KnownHostRepository, write_lock: Arc<Mutex<()>>) -> Self {
        Self {
            repository,
            attempts: Mutex::new(HashMap::new()),
            write_lock,
        }
    }

    pub async fn prepare(
        &self,
        host: &str,
        port: u16,
        observed: HostKeyInfo,
    ) -> AppResult<HostVerification> {
        self.prepare_scoped("direct", host, port, observed).await
    }

    pub async fn prepare_scoped(
        &self,
        route_scope: &str,
        host: &str,
        port: u16,
        observed: HostKeyInfo,
    ) -> AppResult<HostVerification> {
        let route_scope = normalize_scope(route_scope)?;
        let host = normalize_endpoint(host, port)?;
        let known_hosts = self.repository.list().await?;
        let status = match known_hosts.iter().find(|known| {
            known.route_scope == route_scope && known.host == host && known.port == port
        }) {
            Some(known)
                if known.key_type == observed.key_type
                    && known.fingerprint == observed.fingerprint =>
            {
                HostVerificationStatus::Trusted
            }
            Some(_) => return Err(AppError::HostKeyChanged),
            None => HostVerificationStatus::Unknown,
        };
        let attempt_id = Uuid::new_v4();
        self.attempts.lock().await.insert(
            attempt_id,
            VerificationAttempt {
                route_scope: route_scope.clone(),
                host: host.clone(),
                port,
                key_type: observed.key_type.clone(),
                fingerprint: observed.fingerprint.clone(),
                approved: status == HostVerificationStatus::Trusted,
                expires_at: Instant::now() + ATTEMPT_LIFETIME,
            },
        );
        Ok(HostVerification {
            attempt_id,
            host,
            port,
            key_type: observed.key_type,
            fingerprint: observed.fingerprint,
            status,
            route_scope,
        })
    }

    pub async fn trust(&self, attempt_id: Uuid, remember: bool) -> AppResult<()> {
        let mut attempts = self.attempts.lock().await;
        let attempt = attempts
            .get_mut(&attempt_id)
            .filter(|attempt| attempt.expires_at > Instant::now())
            .ok_or(AppError::HostVerificationExpired)?;
        if remember {
            let now = timestamp();
            let known_host = KnownHost {
                route_scope: attempt.route_scope.clone(),
                host: attempt.host.clone(),
                port: attempt.port,
                key_type: attempt.key_type.clone(),
                fingerprint: attempt.fingerprint.clone(),
                created_at: now.clone(),
                updated_at: now,
            };
            let _guard = self.write_lock.lock().await;
            let mut known_hosts = self.repository.list().await?;
            known_hosts.retain(|known| {
                known.route_scope != known_host.route_scope
                    || known.host != known_host.host
                    || known.port != known_host.port
            });
            known_hosts.push(known_host);
            known_hosts.sort_by(|left, right| {
                (&left.route_scope, &left.host, left.port).cmp(&(
                    &right.route_scope,
                    &right.host,
                    right.port,
                ))
            });
            self.repository.save(&known_hosts).await?;
        }
        attempt.approved = true;
        Ok(())
    }

    pub async fn cancel(&self, attempt_id: Uuid) {
        self.attempts.lock().await.remove(&attempt_id);
    }

    // Direct-route compatibility wrapper; production connection setup uses
    // the scope-aware method so jump-host trust cannot cross route scopes.
    #[allow(dead_code)]
    pub async fn consume(&self, attempt_id: Uuid, host: &str, port: u16) -> AppResult<String> {
        self.consume_scoped(attempt_id, "direct", host, port).await
    }

    pub async fn consume_scoped(
        &self,
        attempt_id: Uuid,
        route_scope: &str,
        host: &str,
        port: u16,
    ) -> AppResult<String> {
        let route_scope = normalize_scope(route_scope)?;
        let host = normalize_endpoint(host, port)?;
        let attempt = self
            .attempts
            .lock()
            .await
            .remove(&attempt_id)
            .filter(|attempt| attempt.expires_at > Instant::now())
            .filter(|attempt| attempt.approved)
            .filter(|attempt| {
                attempt.route_scope == route_scope && attempt.host == host && attempt.port == port
            })
            .ok_or(AppError::HostVerificationExpired)?;
        Ok(attempt.fingerprint)
    }

    pub async fn list(&self) -> AppResult<Vec<KnownHost>> {
        let mut hosts = self.repository.list().await?;
        hosts.sort_by(|left, right| {
            (&left.route_scope, &left.host, left.port).cmp(&(
                &right.route_scope,
                &right.host,
                right.port,
            ))
        });
        Ok(hosts)
    }

    // Direct-route compatibility wrapper matching `prepare` and `consume`.
    #[allow(dead_code)]
    pub async fn remove(&self, host: &str, port: u16) -> AppResult<()> {
        self.remove_scoped("direct", host, port).await
    }

    pub async fn remove_scoped(&self, route_scope: &str, host: &str, port: u16) -> AppResult<()> {
        let route_scope = normalize_scope(route_scope)?;
        let host = normalize_endpoint(host, port)?;
        let _guard = self.write_lock.lock().await;
        let mut hosts = self.repository.list().await?;
        let original_len = hosts.len();
        hosts.retain(|known| {
            known.route_scope != route_scope || known.host != host || known.port != port
        });
        if hosts.len() == original_len {
            return Err(AppError::HostKeyUnknown);
        }
        self.repository.save(&hosts).await
    }
}

fn normalize_scope(scope: &str) -> AppResult<String> {
    let scope = scope.trim().to_lowercase();
    if scope.is_empty() || scope.len() > 512 || scope.chars().any(char::is_whitespace) {
        Err(AppError::InvalidProfile)
    } else {
        Ok(scope)
    }
}

fn normalize_endpoint(host: &str, port: u16) -> AppResult<String> {
    let host = host.trim().to_lowercase();
    if host.is_empty() || host.chars().any(char::is_whitespace) || port == 0 {
        Err(AppError::InvalidProfile)
    } else {
        Ok(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::CatalogDatabase;

    fn service(directory: &tempfile::TempDir) -> KnownHostService {
        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("open");
        KnownHostService::new(KnownHostRepository::new(database), Arc::new(Mutex::new(())))
    }

    fn key(fingerprint: &str) -> HostKeyInfo {
        HostKeyInfo {
            key_type: "ssh-ed25519".into(),
            fingerprint: fingerprint.into(),
        }
    }

    #[tokio::test]
    async fn trust_once_is_single_use_and_not_persisted() {
        let directory = tempfile::tempdir().expect("temp directory");
        let service = service(&directory);
        let verification = service
            .prepare("Example.COM", 22, key("SHA256:new"))
            .await
            .expect("prepare");
        assert_eq!(verification.status, HostVerificationStatus::Unknown);
        service
            .trust(verification.attempt_id, false)
            .await
            .expect("trust once");
        assert_eq!(
            service
                .consume(verification.attempt_id, "example.com", 22)
                .await
                .expect("consume"),
            "SHA256:new"
        );
        assert!(matches!(
            service
                .consume(verification.attempt_id, "example.com", 22)
                .await,
            Err(AppError::HostVerificationExpired)
        ));
        assert!(service.list().await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn remembered_match_is_approved_and_changed_key_is_blocked() {
        let directory = tempfile::tempdir().expect("temp directory");
        let service = service(&directory);
        let unknown = service
            .prepare("example.com", 22, key("SHA256:first"))
            .await
            .expect("prepare");
        service
            .trust(unknown.attempt_id, true)
            .await
            .expect("remember");
        let trusted = service
            .prepare("example.com", 22, key("SHA256:first"))
            .await
            .expect("trusted");
        assert_eq!(trusted.status, HostVerificationStatus::Trusted);
        assert!(matches!(
            service
                .prepare("example.com", 22, key("SHA256:changed"))
                .await,
            Err(AppError::HostKeyChanged)
        ));
    }

    #[tokio::test]
    async fn attempt_cannot_be_reused_for_another_endpoint() {
        let directory = tempfile::tempdir().expect("temp directory");
        let service = service(&directory);
        let verification = service
            .prepare("one.example.com", 22, key("SHA256:one"))
            .await
            .expect("prepare");
        service
            .trust(verification.attempt_id, false)
            .await
            .expect("trust");
        assert!(matches!(
            service
                .consume(verification.attempt_id, "two.example.com", 22)
                .await,
            Err(AppError::HostVerificationExpired)
        ));
    }

    #[tokio::test]
    async fn identical_endpoints_are_isolated_by_route_scope() {
        let directory = tempfile::tempdir().expect("temp directory");
        let service = service(&directory);
        let direct = service
            .prepare_scoped("direct", "target.internal", 22, key("SHA256:direct"))
            .await
            .expect("prepare direct");
        service
            .trust(direct.attempt_id, true)
            .await
            .expect("remember direct");

        let jumped = service
            .prepare_scoped("jump:route-a", "target.internal", 22, key("SHA256:jumped"))
            .await
            .expect("prepare jumped");
        assert_eq!(jumped.status, HostVerificationStatus::Unknown);
        service
            .trust(jumped.attempt_id, true)
            .await
            .expect("remember jumped");

        let hosts = service.list().await.expect("list known hosts");
        assert_eq!(hosts.len(), 2);
        assert!(hosts.iter().any(|host| host.route_scope == "direct"));
        assert!(hosts.iter().any(|host| host.route_scope == "jump:route-a"));
        assert!(matches!(
            service
                .prepare_scoped("jump:route-a", "target.internal", 22, key("SHA256:direct"),)
                .await,
            Err(AppError::HostKeyChanged)
        ));
    }
}
