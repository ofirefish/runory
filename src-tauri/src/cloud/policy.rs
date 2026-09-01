use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::Url;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::{AppError, AppResult};
use crate::storage::JsonRepository;

const POLICY_STATE_VERSION: u32 = 2;
const MAX_MANAGED_PROFILES: usize = 1_000;
const MAX_CACHED_DECISIONS: usize = MAX_MANAGED_PROFILES * 6;
const MAX_DECISION_TTL_SECONDS: u64 = 300;
const MAX_CLOCK_SKEW_SECONDS: u64 = 30;
const MAX_VERIFYING_KEYS: usize = 4;
const POLICY_VERIFYING_KEY: Option<&str> = option_env!("RUNORY_POLICY_VERIFYING_KEY_BASE64");
const POLICY_VERIFYING_KEYS: Option<&str> = option_env!("RUNORY_POLICY_VERIFYING_KEYS_JSON");

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudPolicyAction {
    Connect,
    ReadFiles,
    WriteFiles,
    Operate,
    Deploy,
    AiExecute,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPolicyBindingRequest {
    pub organization_id: Uuid,
    pub profile_ids: Vec<Uuid>,
    pub supabase_url: String,
    pub publishable_key: Zeroizing<String>,
    pub access_token: Zeroizing<String>,
    pub expires_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPolicyCredentialRequest {
    pub supabase_url: String,
    pub publishable_key: Zeroizing<String>,
    pub access_token: Zeroizing<String>,
    pub expires_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPolicyOrganizationRequest {
    pub organization_id: Uuid,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPolicyStatus {
    pub organization_id: Uuid,
    pub enabled: bool,
    pub authenticated: bool,
    pub profile_count: usize,
}

#[derive(Clone)]
struct CloudPolicyCredential {
    endpoint: Url,
    publishable_key: Zeroizing<String>,
    access_token: Zeroizing<String>,
    expires_at: u64,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct PersistedPolicyState {
    version: u32,
    assignments: Vec<PersistedPolicyAssignment>,
    #[serde(default)]
    cached_decisions: Vec<SignedPolicyDecision>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedPolicyAssignment {
    organization_id: Uuid,
    profile_ids: Vec<Uuid>,
    #[serde(default = "default_manage_all_profiles")]
    manage_all_profiles: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignedPolicyDecision {
    version: u32,
    #[serde(default)]
    key_id: Option<String>,
    organization_id: Uuid,
    profile_id: Uuid,
    action: CloudPolicyAction,
    allowed: bool,
    issued_at: u64,
    expires_at: u64,
    signature: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DecisionCacheKey {
    organization_id: Uuid,
    profile_id: Uuid,
    action: CloudPolicyAction,
}

impl From<&SignedPolicyDecision> for DecisionCacheKey {
    fn from(value: &SignedPolicyDecision) -> Self {
        Self {
            organization_id: value.organization_id,
            profile_id: value.profile_id,
            action: value.action,
        }
    }
}

pub struct CloudPolicyService {
    assignments: RwLock<HashMap<Uuid, Uuid>>,
    managed_organization: RwLock<Option<Uuid>>,
    credentials: RwLock<HashMap<Uuid, CloudPolicyCredential>>,
    cached_decisions: RwLock<HashMap<DecisionCacheKey, SignedPolicyDecision>>,
    repository: JsonRepository<PersistedPolicyState>,
    write_lock: Mutex<()>,
    client: reqwest::Client,
    legacy_verifying_key: Option<[u8; 32]>,
    verifying_keys: HashMap<String, [u8; 32]>,
}

#[derive(Serialize)]
struct DecisionRequest {
    target_organization_id: Uuid,
    target_action: CloudPolicyAction,
    target_resource_type: &'static str,
    target_resource_id: Uuid,
}

impl CloudPolicyService {
    pub fn at_path(path: impl Into<PathBuf>) -> AppResult<Self> {
        Ok(Self::with_verifying_keys(
            JsonRepository::new(path),
            POLICY_VERIFYING_KEY.map(parse_verifying_key).transpose()?,
            POLICY_VERIFYING_KEYS
                .map(parse_verifying_keys)
                .transpose()?
                .unwrap_or_default(),
        ))
    }

    #[cfg(test)]
    fn new(
        repository: JsonRepository<PersistedPolicyState>,
        verifying_key: Option<[u8; 32]>,
    ) -> Self {
        Self::with_verifying_keys(repository, verifying_key, HashMap::new())
    }

    fn with_verifying_keys(
        repository: JsonRepository<PersistedPolicyState>,
        legacy_verifying_key: Option<[u8; 32]>,
        verifying_keys: HashMap<String, [u8; 32]>,
    ) -> Self {
        Self {
            assignments: RwLock::new(HashMap::new()),
            managed_organization: RwLock::new(None),
            credentials: RwLock::new(HashMap::new()),
            cached_decisions: RwLock::new(HashMap::new()),
            repository,
            write_lock: Mutex::new(()),
            client: reqwest::Client::new(),
            legacy_verifying_key,
            verifying_keys,
        }
    }

    fn signed_decisions_enabled(&self) -> bool {
        self.legacy_verifying_key.is_some() || !self.verifying_keys.is_empty()
    }

    pub async fn load(&self) -> AppResult<()> {
        let state = self.repository.load_or_default().await?;
        let (assignments, managed_organization) = validate_state(&state)?;
        let cached_decisions = self.valid_cached_decisions(&state.cached_decisions)?;
        *self.assignments.write().await = assignments;
        *self.managed_organization.write().await = managed_organization;
        *self.cached_decisions.write().await = cached_decisions;
        Ok(())
    }

    pub async fn bind(&self, request: CloudPolicyBindingRequest) -> AppResult<CloudPolicyStatus> {
        validate_profile_ids(&request.profile_ids)?;
        let credential = credential(
            &request.supabase_url,
            &request.publishable_key,
            &request.access_token,
            request.expires_at,
            self.signed_decisions_enabled(),
        )?;
        let _write = self.write_lock.lock().await;
        let mut assignments = HashMap::new();
        for profile_id in request.profile_ids {
            assignments.insert(profile_id, request.organization_id);
        }
        self.repository
            .save_atomic(&persisted_state(
                &assignments,
                Some(request.organization_id),
                &HashMap::new(),
            ))
            .await?;
        *self.assignments.write().await = assignments;
        *self.managed_organization.write().await = Some(request.organization_id);
        self.cached_decisions.write().await.clear();
        let mut credentials = self.credentials.write().await;
        credentials.clear();
        credentials.insert(request.organization_id, credential);
        drop(credentials);
        self.status(request.organization_id).await
    }

    pub async fn refresh(&self, request: CloudPolicyCredentialRequest) -> AppResult<()> {
        let value = credential(
            &request.supabase_url,
            &request.publishable_key,
            &request.access_token,
            request.expires_at,
            self.signed_decisions_enabled(),
        )?;
        let organization = *self.managed_organization.read().await;
        let mut credentials = self.credentials.write().await;
        credentials.clear();
        if let Some(organization_id) = organization {
            credentials.insert(organization_id, value.clone());
        }
        Ok(())
    }

    pub async fn lock(&self) {
        self.credentials.write().await.clear();
    }

    pub async fn unbind(&self, organization_id: Uuid) -> AppResult<CloudPolicyStatus> {
        let _write = self.write_lock.lock().await;
        let mut assignments = self.assignments.read().await.clone();
        assignments.retain(|_, assigned_organization| *assigned_organization != organization_id);
        let managed_organization = self
            .managed_organization
            .read()
            .await
            .filter(|value| *value != organization_id);
        let mut cached_decisions = self.cached_decisions.read().await.clone();
        cached_decisions.retain(|key, _| key.organization_id != organization_id);
        self.repository
            .save_atomic(&persisted_state(
                &assignments,
                managed_organization,
                &cached_decisions,
            ))
            .await?;
        *self.assignments.write().await = assignments;
        *self.managed_organization.write().await = managed_organization;
        *self.cached_decisions.write().await = cached_decisions;
        self.credentials.write().await.remove(&organization_id);
        self.status(organization_id).await
    }

    pub async fn status(&self, organization_id: Uuid) -> AppResult<CloudPolicyStatus> {
        let now = now_unix_seconds()?;
        let profile_count = self
            .assignments
            .read()
            .await
            .values()
            .filter(|assigned| **assigned == organization_id)
            .count();
        let enabled = *self.managed_organization.read().await == Some(organization_id);
        let authenticated = self
            .credentials
            .read()
            .await
            .get(&organization_id)
            .is_some_and(|value| value.expires_at > now);
        Ok(CloudPolicyStatus {
            organization_id,
            enabled,
            authenticated,
            profile_count,
        })
    }

    pub async fn authorize(&self, profile_id: Uuid, action: CloudPolicyAction) -> AppResult<()> {
        let organization_id = self
            .assignments
            .read()
            .await
            .get(&profile_id)
            .copied()
            .or(*self.managed_organization.read().await);
        let Some(organization_id) = organization_id else {
            return Ok(());
        };
        let now = now_unix_seconds()?;
        let binding = self.credentials.read().await.get(&organization_id).cloned();
        if let Some(binding) = binding.filter(|value| value.expires_at > now) {
            if let Some(allowed) = self
                .authorize_online(&binding, organization_id, profile_id, action, now)
                .await
            {
                return decision_result(allowed);
            }
        }
        self.cached_result(organization_id, profile_id, action, now)
            .await
    }

    pub async fn reconcile_profiles(&self, profile_ids: Vec<Uuid>) -> AppResult<()> {
        validate_profile_ids_allow_empty(&profile_ids)?;
        let _write = self.write_lock.lock().await;
        let managed_organization = *self.managed_organization.read().await;
        let Some(organization_id) = managed_organization else {
            return Ok(());
        };
        let assignments = profile_ids
            .into_iter()
            .map(|profile_id| (profile_id, organization_id))
            .collect::<HashMap<_, _>>();
        let mut cached_decisions = self.cached_decisions.read().await.clone();
        cached_decisions.retain(|key, _| assignments.contains_key(&key.profile_id));
        self.repository
            .save_atomic(&persisted_state(
                &assignments,
                managed_organization,
                &cached_decisions,
            ))
            .await?;
        *self.assignments.write().await = assignments;
        *self.cached_decisions.write().await = cached_decisions;
        Ok(())
    }

    async fn authorize_online(
        &self,
        binding: &CloudPolicyCredential,
        organization_id: Uuid,
        profile_id: Uuid,
        action: CloudPolicyAction,
        now: u64,
    ) -> Option<bool> {
        let response = self
            .client
            .post(binding.endpoint.clone())
            .header("apikey", binding.publishable_key.as_str())
            .bearer_auth(binding.access_token.as_str())
            .timeout(Duration::from_secs(10))
            .json(&DecisionRequest {
                target_organization_id: organization_id,
                target_action: action,
                target_resource_type: "server-profile",
                target_resource_id: profile_id,
            })
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        if !self.signed_decisions_enabled() {
            return response.json::<bool>().await.ok();
        }
        let decision = response.json::<SignedPolicyDecision>().await.ok()?;
        if verify_decision(
            &decision,
            self.legacy_verifying_key.as_ref(),
            &self.verifying_keys,
            organization_id,
            profile_id,
            action,
            now,
        )
        .is_err()
        {
            return None;
        }
        let allowed = decision.allowed;
        if self.persist_cached_decision(decision).await.is_err() {
            tracing::warn!("verified cloud policy decision could not be cached");
        }
        Some(allowed)
    }

    async fn cached_result(
        &self,
        organization_id: Uuid,
        profile_id: Uuid,
        action: CloudPolicyAction,
        now: u64,
    ) -> AppResult<()> {
        if !self.signed_decisions_enabled() {
            return Err(AppError::CloudPolicyUnavailable);
        }
        let key = DecisionCacheKey {
            organization_id,
            profile_id,
            action,
        };
        let decisions = self.cached_decisions.read().await;
        let decision = decisions
            .get(&key)
            .ok_or(AppError::CloudPolicyUnavailable)?;
        verify_decision(
            decision,
            self.legacy_verifying_key.as_ref(),
            &self.verifying_keys,
            organization_id,
            profile_id,
            action,
            now,
        )?;
        decision_result(decision.allowed)
    }

    async fn persist_cached_decision(&self, decision: SignedPolicyDecision) -> AppResult<()> {
        let _write = self.write_lock.lock().await;
        let assignments = self.assignments.read().await.clone();
        let managed_organization = *self.managed_organization.read().await;
        let mut cached_decisions = self.cached_decisions.read().await.clone();
        cached_decisions.insert(DecisionCacheKey::from(&decision), decision);
        if cached_decisions.len() > MAX_CACHED_DECISIONS {
            return Err(AppError::CloudPolicyUnavailable);
        }
        self.repository
            .save_atomic(&persisted_state(
                &assignments,
                managed_organization,
                &cached_decisions,
            ))
            .await?;
        *self.cached_decisions.write().await = cached_decisions;
        Ok(())
    }

    fn valid_cached_decisions(
        &self,
        values: &[SignedPolicyDecision],
    ) -> AppResult<HashMap<DecisionCacheKey, SignedPolicyDecision>> {
        if values.len() > MAX_CACHED_DECISIONS {
            return Err(AppError::Storage);
        }
        if !self.signed_decisions_enabled() {
            return Ok(HashMap::new());
        }
        let now = now_unix_seconds()?;
        Ok(values
            .iter()
            .filter(|decision| {
                verify_decision(
                    decision,
                    self.legacy_verifying_key.as_ref(),
                    &self.verifying_keys,
                    decision.organization_id,
                    decision.profile_id,
                    decision.action,
                    now,
                )
                .is_ok()
            })
            .map(|decision| (DecisionCacheKey::from(decision), decision.clone()))
            .collect())
    }
}

fn credential(
    supabase_url: &str,
    publishable_key: &str,
    access_token: &str,
    expires_at: u64,
    signed: bool,
) -> AppResult<CloudPolicyCredential> {
    if publishable_key.is_empty() || access_token.is_empty() || expires_at <= now_unix_seconds()? {
        return Err(AppError::CloudInvalid);
    }
    Ok(CloudPolicyCredential {
        endpoint: decision_endpoint(supabase_url, signed)?,
        publishable_key: Zeroizing::new(publishable_key.to_owned()),
        access_token: Zeroizing::new(access_token.to_owned()),
        expires_at,
    })
}

fn validate_profile_ids(profile_ids: &[Uuid]) -> AppResult<()> {
    if profile_ids.is_empty() {
        return Err(AppError::CloudInvalid);
    }
    validate_profile_ids_allow_empty(profile_ids)
}

fn validate_profile_ids_allow_empty(profile_ids: &[Uuid]) -> AppResult<()> {
    if profile_ids.len() > MAX_MANAGED_PROFILES {
        return Err(AppError::CloudInvalid);
    }
    let unique = profile_ids.iter().copied().collect::<HashSet<_>>();
    (unique.len() == profile_ids.len())
        .then_some(())
        .ok_or(AppError::CloudInvalid)
}

fn persisted_state(
    assignments: &HashMap<Uuid, Uuid>,
    managed_organization: Option<Uuid>,
    cached_decisions: &HashMap<DecisionCacheKey, SignedPolicyDecision>,
) -> PersistedPolicyState {
    let mut by_organization = HashMap::<Uuid, Vec<Uuid>>::new();
    for (profile_id, organization_id) in assignments {
        by_organization
            .entry(*organization_id)
            .or_default()
            .push(*profile_id);
    }
    let mut values = by_organization
        .into_iter()
        .map(|(organization_id, mut profile_ids)| {
            profile_ids.sort_unstable();
            PersistedPolicyAssignment {
                organization_id,
                profile_ids,
                manage_all_profiles: managed_organization == Some(organization_id),
            }
        })
        .collect::<Vec<_>>();
    values.sort_unstable_by_key(|value| value.organization_id);
    PersistedPolicyState {
        version: POLICY_STATE_VERSION,
        assignments: values,
        cached_decisions: {
            let mut values = cached_decisions.values().cloned().collect::<Vec<_>>();
            values.sort_unstable_by_key(|value| {
                (
                    value.organization_id,
                    value.profile_id,
                    value.action.as_str(),
                )
            });
            values
        },
    }
}

fn validate_state(state: &PersistedPolicyState) -> AppResult<(HashMap<Uuid, Uuid>, Option<Uuid>)> {
    if state.version != 0 && state.version != 1 && state.version != POLICY_STATE_VERSION {
        return Err(AppError::Storage);
    }
    let total = state
        .assignments
        .iter()
        .map(|value| value.profile_ids.len())
        .sum::<usize>();
    if total > MAX_MANAGED_PROFILES {
        return Err(AppError::Storage);
    }
    let mut values = HashMap::with_capacity(total);
    let mut managed_organization = None;
    for assignment in &state.assignments {
        if assignment.manage_all_profiles
            && managed_organization
                .replace(assignment.organization_id)
                .is_some()
        {
            return Err(AppError::Storage);
        }
        for profile_id in &assignment.profile_ids {
            if values
                .insert(*profile_id, assignment.organization_id)
                .is_some()
            {
                return Err(AppError::Storage);
            }
        }
    }
    Ok((values, managed_organization))
}

const fn default_manage_all_profiles() -> bool {
    true
}

fn decision_endpoint(value: &str, signed: bool) -> AppResult<Url> {
    let mut url = Url::parse(value).map_err(|_| AppError::CloudInvalid)?;
    let local = url
        .host_str()
        .is_some_and(|host| host == "localhost" || host == "127.0.0.1");
    if url.scheme() != "https" && !(local && url.scheme() == "http") {
        return Err(AppError::CloudInvalid);
    }
    url.set_path(if signed {
        "/functions/v1/evaluate-access-policy"
    } else {
        "/rest/v1/rpc/evaluate_access_policy"
    });
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

impl CloudPolicyAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::ReadFiles => "read-files",
            Self::WriteFiles => "write-files",
            Self::Operate => "operate",
            Self::Deploy => "deploy",
            Self::AiExecute => "ai-execute",
        }
    }
}

fn parse_verifying_key(value: &str) -> AppResult<[u8; 32]> {
    let decoded = BASE64.decode(value).map_err(|_| AppError::CloudInvalid)?;
    decoded.try_into().map_err(|_| AppError::CloudInvalid)
}

fn parse_verifying_keys(value: &str) -> AppResult<HashMap<String, [u8; 32]>> {
    let encoded = serde_json::from_str::<HashMap<String, String>>(value)
        .map_err(|_| AppError::CloudInvalid)?;
    if encoded.is_empty() || encoded.len() > MAX_VERIFYING_KEYS {
        return Err(AppError::CloudInvalid);
    }
    encoded
        .into_iter()
        .map(|(key_id, value)| {
            validate_key_id(&key_id)?;
            Ok((key_id, parse_verifying_key(&value)?))
        })
        .collect()
}

fn validate_key_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 32
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(AppError::CloudInvalid);
    }
    Ok(())
}

fn canonical_decision(decision: &SignedPolicyDecision) -> String {
    match (decision.version, decision.key_id.as_deref()) {
        (2, Some(key_id)) => format!(
            "runory-policy-decision-v2\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            key_id,
            decision.organization_id,
            decision.profile_id,
            decision.action.as_str(),
            decision.allowed,
            decision.issued_at,
            decision.expires_at
        ),
        _ => format!(
            "runory-policy-decision-v1\n{}\n{}\n{}\n{}\n{}\n{}",
            decision.organization_id,
            decision.profile_id,
            decision.action.as_str(),
            decision.allowed,
            decision.issued_at,
            decision.expires_at
        ),
    }
}

fn verify_decision(
    decision: &SignedPolicyDecision,
    legacy_verifying_key: Option<&[u8; 32]>,
    verifying_keys: &HashMap<String, [u8; 32]>,
    organization_id: Uuid,
    profile_id: Uuid,
    action: CloudPolicyAction,
    now: u64,
) -> AppResult<()> {
    let verifying_key = match (decision.version, decision.key_id.as_deref()) {
        (1, None) => legacy_verifying_key.ok_or(AppError::CloudPolicyUnavailable)?,
        (2, Some(key_id)) => {
            validate_key_id(key_id).map_err(|_| AppError::CloudPolicyUnavailable)?;
            verifying_keys
                .get(key_id)
                .ok_or(AppError::CloudPolicyUnavailable)?
        }
        _ => return Err(AppError::CloudPolicyUnavailable),
    };
    if decision.organization_id != organization_id
        || decision.profile_id != profile_id
        || decision.action != action
        || decision.expires_at <= now
        || decision.expires_at < decision.issued_at
        || decision.expires_at - decision.issued_at > MAX_DECISION_TTL_SECONDS
        || decision.issued_at > now.saturating_add(MAX_CLOCK_SKEW_SECONDS)
    {
        return Err(AppError::CloudPolicyUnavailable);
    }
    let signature = BASE64
        .decode(&decision.signature)
        .map_err(|_| AppError::CloudPolicyUnavailable)?;
    UnparsedPublicKey::new(&ED25519, verifying_key)
        .verify(canonical_decision(decision).as_bytes(), &signature)
        .map_err(|_| AppError::CloudPolicyUnavailable)
}

fn decision_result(allowed: bool) -> AppResult<()> {
    if allowed {
        Ok(())
    } else {
        Err(AppError::CloudPolicyDenied)
    }
}

fn now_unix_seconds() -> AppResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AppError::CloudPolicyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    fn repository(directory: &tempfile::TempDir) -> JsonRepository<PersistedPolicyState> {
        JsonRepository::new(directory.path().join("cloud-policy-bindings.json"))
    }

    #[test]
    fn decision_url_is_business_specific() {
        let value =
            decision_endpoint("https://example.supabase.co/ignored?value=1", false).unwrap();
        assert_eq!(
            value.as_str(),
            "https://example.supabase.co/rest/v1/rpc/evaluate_access_policy"
        );
        assert_eq!(
            decision_endpoint("https://example.supabase.co", true)
                .unwrap()
                .as_str(),
            "https://example.supabase.co/functions/v1/evaluate-access-policy"
        );
        assert!(decision_endpoint("http://example.supabase.co", false).is_err());
        assert!(decision_endpoint("http://127.0.0.1:54321", false).is_ok());
    }

    #[tokio::test]
    async fn unbound_local_profile_remains_available() {
        let directory = tempfile::tempdir().unwrap();
        let service = CloudPolicyService::new(repository(&directory), None);
        assert!(service
            .authorize(Uuid::new_v4(), CloudPolicyAction::Connect)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn persisted_assignment_is_fail_closed_until_credentials_are_refreshed() {
        let directory = tempfile::tempdir().unwrap();
        let organization_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let service = CloudPolicyService::new(repository(&directory), None);
        service
            .bind(CloudPolicyBindingRequest {
                organization_id,
                profile_ids: vec![profile_id],
                supabase_url: "https://example.supabase.co".into(),
                publishable_key: Zeroizing::new("publishable".into()),
                access_token: Zeroizing::new("access-token".into()),
                expires_at: u64::MAX,
            })
            .await
            .unwrap();

        let restarted = CloudPolicyService::new(repository(&directory), None);
        restarted.load().await.unwrap();
        assert!(matches!(
            restarted
                .authorize(Uuid::new_v4(), CloudPolicyAction::Connect)
                .await,
            Err(AppError::CloudPolicyUnavailable)
        ));
        let status = restarted.status(organization_id).await.unwrap();
        assert!(status.enabled);
        assert!(!status.authenticated);
        assert_eq!(status.profile_count, 1);
    }

    #[tokio::test]
    async fn reconciliation_updates_the_persisted_profile_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let organization_id = Uuid::new_v4();
        let old_profile_id = Uuid::new_v4();
        let new_profile_id = Uuid::new_v4();
        let service = CloudPolicyService::new(repository(&directory), None);
        service
            .bind(CloudPolicyBindingRequest {
                organization_id,
                profile_ids: vec![old_profile_id],
                supabase_url: "https://example.supabase.co".into(),
                publishable_key: Zeroizing::new("publishable".into()),
                access_token: Zeroizing::new("access-token".into()),
                expires_at: u64::MAX,
            })
            .await
            .unwrap();

        service
            .reconcile_profiles(vec![new_profile_id])
            .await
            .unwrap();
        let state = repository(&directory).load_or_default().await.unwrap();
        assert_eq!(state.assignments.len(), 1);
        assert_eq!(state.assignments[0].profile_ids, vec![new_profile_id]);
        assert!(state.assignments[0].manage_all_profiles);
        assert_eq!(
            service.status(organization_id).await.unwrap().profile_count,
            1
        );
    }

    #[tokio::test]
    async fn unbind_removes_assignment_without_persisting_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let organization_id = Uuid::new_v4();
        let service = CloudPolicyService::new(repository(&directory), None);
        service
            .bind(CloudPolicyBindingRequest {
                organization_id,
                profile_ids: vec![Uuid::new_v4()],
                supabase_url: "https://example.supabase.co".into(),
                publishable_key: Zeroizing::new("publishable".into()),
                access_token: Zeroizing::new("access-token".into()),
                expires_at: u64::MAX,
            })
            .await
            .unwrap();
        assert!(!service.unbind(organization_id).await.unwrap().enabled);
        let state = repository(&directory).load_or_default().await.unwrap();
        assert!(state.assignments.is_empty());
        let persisted =
            std::fs::read_to_string(directory.path().join("cloud-policy-bindings.json")).unwrap();
        assert!(!persisted.contains("access-token"));
        assert!(!persisted.contains("publishable"));
    }

    fn signing_pair() -> Ed25519KeyPair {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap()
    }

    fn signed_decision(
        pair: &Ed25519KeyPair,
        organization_id: Uuid,
        profile_id: Uuid,
        action: CloudPolicyAction,
        allowed: bool,
        issued_at: u64,
        expires_at: u64,
    ) -> SignedPolicyDecision {
        let mut decision = SignedPolicyDecision {
            version: 1,
            key_id: None,
            organization_id,
            profile_id,
            action,
            allowed,
            issued_at,
            expires_at,
            signature: String::new(),
        };
        decision.signature = BASE64.encode(pair.sign(canonical_decision(&decision).as_bytes()));
        decision
    }

    fn signed_decision_v2(
        pair: &Ed25519KeyPair,
        key_id: &str,
        organization_id: Uuid,
        profile_id: Uuid,
        action: CloudPolicyAction,
        issued_at: u64,
    ) -> SignedPolicyDecision {
        let mut decision = SignedPolicyDecision {
            version: 2,
            key_id: Some(key_id.to_owned()),
            organization_id,
            profile_id,
            action,
            allowed: true,
            issued_at,
            expires_at: issued_at + MAX_DECISION_TTL_SECONDS,
            signature: String::new(),
        };
        decision.signature = BASE64.encode(pair.sign(canonical_decision(&decision).as_bytes()));
        decision
    }

    #[test]
    fn version_two_decision_uses_pinned_key_id() {
        let pair = signing_pair();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        let keys = HashMap::from([("policy-2026-a".to_owned(), key)]);
        let organization_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let decision = signed_decision_v2(
            &pair,
            "policy-2026-a",
            organization_id,
            profile_id,
            CloudPolicyAction::Operate,
            1_000,
        );
        assert!(verify_decision(
            &decision,
            None,
            &keys,
            organization_id,
            profile_id,
            CloudPolicyAction::Operate,
            1_000,
        )
        .is_ok());

        let mut unknown_key = decision;
        unknown_key.key_id = Some("policy-unknown".into());
        assert!(verify_decision(
            &unknown_key,
            None,
            &keys,
            organization_id,
            profile_id,
            CloudPolicyAction::Operate,
            1_000,
        )
        .is_err());
    }

    #[test]
    fn verifying_key_set_is_strict_and_bounded() {
        let pair = signing_pair();
        let encoded = BASE64.encode(pair.public_key().as_ref());
        let keys = parse_verifying_keys(&format!(r#"{{"policy-a":"{encoded}"}}"#)).unwrap();
        assert_eq!(keys.len(), 1);
        assert!(parse_verifying_keys("{}").is_err());
        assert!(parse_verifying_keys(r#"{"bad key":"AAAA"}"#).is_err());
    }

    #[test]
    fn signed_decision_binds_every_security_field() {
        let pair = signing_pair();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        let organization_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let now = 1_000;
        let decision = signed_decision(
            &pair,
            organization_id,
            profile_id,
            CloudPolicyAction::Connect,
            true,
            now,
            now + MAX_DECISION_TTL_SECONDS,
        );
        assert!(verify_decision(
            &decision,
            Some(&key),
            &HashMap::new(),
            organization_id,
            profile_id,
            CloudPolicyAction::Connect,
            now
        )
        .is_ok());

        let mut tampered = decision.clone();
        tampered.allowed = false;
        assert!(verify_decision(
            &tampered,
            Some(&key),
            &HashMap::new(),
            organization_id,
            profile_id,
            CloudPolicyAction::Connect,
            now
        )
        .is_err());
        assert!(verify_decision(
            &decision,
            Some(&key),
            &HashMap::new(),
            organization_id,
            Uuid::new_v4(),
            CloudPolicyAction::Connect,
            now
        )
        .is_err());
    }

    #[test]
    fn signed_decision_rejects_expiry_future_and_excessive_ttl() {
        let pair = signing_pair();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        let organization_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        for decision in [
            signed_decision(
                &pair,
                organization_id,
                profile_id,
                CloudPolicyAction::Deploy,
                true,
                900,
                1_000,
            ),
            signed_decision(
                &pair,
                organization_id,
                profile_id,
                CloudPolicyAction::Deploy,
                true,
                1_031,
                1_100,
            ),
            signed_decision(
                &pair,
                organization_id,
                profile_id,
                CloudPolicyAction::Deploy,
                true,
                1_000,
                1_301,
            ),
        ] {
            assert!(verify_decision(
                &decision,
                Some(&key),
                &HashMap::new(),
                organization_id,
                profile_id,
                CloudPolicyAction::Deploy,
                1_000
            )
            .is_err());
        }
    }

    #[tokio::test]
    async fn verified_cache_survives_restart_without_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let pair = signing_pair();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        let organization_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let now = now_unix_seconds().unwrap();
        let service = CloudPolicyService::new(repository(&directory), Some(key));
        service
            .bind(CloudPolicyBindingRequest {
                organization_id,
                profile_ids: vec![profile_id],
                supabase_url: "https://example.supabase.co".into(),
                publishable_key: Zeroizing::new("publishable".into()),
                access_token: Zeroizing::new("access-token".into()),
                expires_at: u64::MAX,
            })
            .await
            .unwrap();
        service
            .persist_cached_decision(signed_decision(
                &pair,
                organization_id,
                profile_id,
                CloudPolicyAction::Connect,
                true,
                now,
                now + MAX_DECISION_TTL_SECONDS,
            ))
            .await
            .unwrap();
        service
            .persist_cached_decision(signed_decision(
                &pair,
                organization_id,
                profile_id,
                CloudPolicyAction::WriteFiles,
                false,
                now,
                now + MAX_DECISION_TTL_SECONDS,
            ))
            .await
            .unwrap();

        let restarted = CloudPolicyService::new(repository(&directory), Some(key));
        restarted.load().await.unwrap();
        assert!(restarted
            .cached_result(organization_id, profile_id, CloudPolicyAction::Connect, now)
            .await
            .is_ok());
        assert!(matches!(
            restarted
                .cached_result(
                    organization_id,
                    profile_id,
                    CloudPolicyAction::WriteFiles,
                    now
                )
                .await,
            Err(AppError::CloudPolicyDenied)
        ));
        assert!(restarted.credentials.read().await.is_empty());
    }
}
