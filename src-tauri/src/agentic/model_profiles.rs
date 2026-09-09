use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::model_gateway::{
    status_from_config, validate_config, ModelProviderConfig, ModelProviderKind,
    ModelProviderStatus,
};
use crate::credentials::CredentialService;
use crate::domain::{AppError, AppResult, CredentialInput, CredentialKind};
use crate::storage::JsonRepository;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedProfile {
    id: Uuid,
    config: ModelProviderConfig,
    credential_id: Option<Uuid>,
    auth_mode: super::model_gateway::ModelAuthMode,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileCollection {
    active_id: Option<Uuid>,
    profiles: Vec<SavedProfile>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum Document {
    Collection(ProfileCollection),
    Legacy(ModelProviderConfig),
}

impl Default for Document {
    fn default() -> Self {
        Self::Collection(ProfileCollection::default())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelProfile {
    pub id: Uuid,
    pub active: bool,
    #[serde(flatten)]
    pub status: ModelProviderStatus,
}

pub(super) struct ModelProfileRepository {
    repository: JsonRepository<Document>,
    credentials: Option<CredentialService>,
    state: Mutex<Option<ProfileCollection>>,
}

impl ModelProfileRepository {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            repository: JsonRepository::new(path),
            credentials: None,
            state: Mutex::new(None),
        }
    }

    pub fn with_credentials(mut self, credentials: CredentialService) -> Self {
        self.credentials = Some(credentials);
        self
    }

    pub async fn migrate_legacy(&self) -> AppResult<bool> {
        let mut state = self.state.lock().await;
        if state.is_some() {
            return Ok(false);
        }
        let Document::Legacy(mut config) = self.repository.load_or_default().await? else {
            return Ok(false);
        };
        validate_config(&config)?;
        if config.kind == ModelProviderKind::Local {
            let collection = ProfileCollection::default();
            self.repository
                .save_atomic(&Document::Collection(collection.clone()))
                .await?;
            *state = Some(collection);
            return Ok(true);
        }

        let profile_id = Uuid::new_v4();
        let auth_mode = status_from_config(&config, false).auth_mode;
        let credential_id = if config.api_key.is_some() || config.oauth.is_some() {
            let secret = Zeroizing::new(
                serde_json::to_string(&(&config.api_key, &config.oauth))
                    .map_err(|_| AppError::VaultInvalid)?,
            );
            let credential_id = Uuid::new_v4();
            self.credentials
                .as_ref()
                .ok_or(AppError::VaultLocked)?
                .remember(credential_id, CredentialKind::LlmApiKey, secret)
                .await?;
            config.api_key = None;
            config.oauth = None;
            Some(credential_id)
        } else {
            None
        };
        let collection = ProfileCollection {
            active_id: Some(profile_id),
            profiles: vec![SavedProfile {
                id: profile_id,
                config,
                credential_id,
                auth_mode,
            }],
        };
        if let Err(error) = self
            .repository
            .save_atomic(&Document::Collection(collection.clone()))
            .await
        {
            if let (Some(credentials), Some(credential_id)) = (&self.credentials, credential_id) {
                let _ = credentials
                    .forget(credential_id, CredentialKind::LlmApiKey)
                    .await;
            }
            return Err(error);
        }
        *state = Some(collection);
        Ok(true)
    }

    async fn collection(
        &self,
        state: &mut Option<ProfileCollection>,
    ) -> AppResult<ProfileCollection> {
        if let Some(collection) = state {
            return Ok(collection.clone());
        }
        let collection = match self.repository.load_or_default().await? {
            Document::Collection(collection) => collection,
            Document::Legacy(config) => {
                validate_config(&config)?;
                if config.kind == ModelProviderKind::Local {
                    ProfileCollection::default()
                } else {
                    let id = Uuid::new_v4();
                    let auth_mode = status_from_config(&config, false).auth_mode;
                    ProfileCollection {
                        active_id: Some(id),
                        profiles: vec![SavedProfile {
                            id,
                            config,
                            credential_id: None,
                            auth_mode,
                        }],
                    }
                }
            }
        };
        let mut ids = std::collections::HashSet::new();
        for profile in &collection.profiles {
            validate_config(&profile.config)?;
            if !ids.insert(profile.id) {
                return Err(AppError::Storage);
            }
        }
        if collection.active_id.is_some_and(|id| !ids.contains(&id)) {
            return Err(AppError::Storage);
        }
        *state = Some(collection.clone());
        Ok(collection)
    }

    async fn hydrate(&self, profile: &SavedProfile) -> AppResult<ModelProviderConfig> {
        let mut config = profile.config.clone();
        if let Some(id) = profile.credential_id {
            let secret = self
                .credentials
                .as_ref()
                .ok_or(AppError::VaultLocked)?
                .resolve_for_profile(
                    id,
                    CredentialKind::LlmApiKey,
                    CredentialInput::Stored,
                    false,
                )
                .await?;
            let (api_key, oauth) =
                serde_json::from_str(secret.secret.as_str()).map_err(|_| AppError::VaultInvalid)?;
            config.api_key = api_key;
            config.oauth = oauth;
        }
        Ok(config)
    }

    pub async fn load_or_default(&self) -> AppResult<ModelProviderConfig> {
        let mut state = self.state.lock().await;
        let collection = self.collection(&mut state).await?;
        match collection
            .profiles
            .iter()
            .find(|p| Some(p.id) == collection.active_id)
        {
            Some(profile) => self.hydrate(profile).await,
            None => Ok(ModelProviderConfig::default()),
        }
    }

    pub async fn active_metadata(&self) -> AppResult<ModelProviderConfig> {
        let mut state = self.state.lock().await;
        let collection = self.collection(&mut state).await?;
        Ok(collection
            .profiles
            .iter()
            .find(|p| Some(p.id) == collection.active_id)
            .map(|p| p.config.clone())
            .unwrap_or_default())
    }

    pub async fn list(&self) -> AppResult<Vec<ModelProfile>> {
        let mut state = self.state.lock().await;
        let collection = self.collection(&mut state).await?;
        Ok(collection
            .profiles
            .iter()
            .map(|profile| {
                let mut status = status_from_config(&profile.config, false);
                status.auth_mode = profile.auth_mode;
                status.api_key_configured =
                    profile.credential_id.is_some() || status.api_key_configured;
                ModelProfile {
                    id: profile.id,
                    active: collection.active_id == Some(profile.id),
                    status,
                }
            })
            .collect())
    }

    pub async fn get(&self, id: Uuid) -> AppResult<ModelProviderConfig> {
        let mut state = self.state.lock().await;
        let collection = self.collection(&mut state).await?;
        self.hydrate(
            collection
                .profiles
                .iter()
                .find(|p| p.id == id)
                .ok_or(AppError::ModelInvalid)?,
        )
        .await
    }

    async fn commit(
        &self,
        state: &mut Option<ProfileCollection>,
        mut collection: ProfileCollection,
    ) -> AppResult<()> {
        // Fresh IDs preserve the old credentials if the atomic metadata save fails.
        let mut fresh_ids = Vec::new();
        let result: AppResult<()> = async {
            for profile in &mut collection.profiles {
                if profile.config.api_key.is_some() || profile.config.oauth.is_some() {
                    let secret = Zeroizing::new(
                        serde_json::to_string(&(&profile.config.api_key, &profile.config.oauth))
                            .map_err(|_| AppError::VaultInvalid)?,
                    );
                    let id = Uuid::new_v4();
                    self.credentials
                        .as_ref()
                        .ok_or(AppError::VaultLocked)?
                        .remember(id, CredentialKind::LlmApiKey, secret)
                        .await?;
                    fresh_ids.push(id);
                    profile.credential_id = Some(id);
                    profile.config.api_key = None;
                    profile.config.oauth = None;
                }
            }
            self.repository
                .save_atomic(&Document::Collection(collection.clone()))
                .await?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            if let Some(credentials) = &self.credentials {
                for id in fresh_ids {
                    let _ = credentials.forget(id, CredentialKind::LlmApiKey).await;
                }
            }
            return Err(error);
        }
        let previous = state.replace(collection.clone());
        if let (Some(credentials), Some(previous)) = (&self.credentials, previous) {
            for id in previous.profiles.iter().filter_map(|p| p.credential_id) {
                if !collection
                    .profiles
                    .iter()
                    .any(|p| p.credential_id == Some(id))
                {
                    let _ = credentials.forget(id, CredentialKind::LlmApiKey).await;
                }
            }
        }
        Ok(())
    }

    pub async fn save_profile(
        &self,
        id: Option<Uuid>,
        config: &ModelProviderConfig,
        activate: bool,
        auth_mode: Option<super::model_gateway::ModelAuthMode>,
    ) -> AppResult<Uuid> {
        validate_config(config)?;
        if config.kind == ModelProviderKind::Local {
            return Err(AppError::ModelInvalid);
        }
        let mut state = self.state.lock().await;
        let mut collection = self.collection(&mut state).await?;
        let id = match id {
            Some(id) if collection.profiles.iter().any(|p| p.id == id) => id,
            Some(_) => return Err(AppError::ModelInvalid),
            None => Uuid::new_v4(),
        };
        let profile = SavedProfile {
            id,
            config: config.clone(),
            credential_id: None,
            auth_mode: auth_mode.unwrap_or_else(|| status_from_config(config, false).auth_mode),
        };
        if let Some(existing) = collection.profiles.iter_mut().find(|p| p.id == id) {
            *existing = profile;
        } else {
            collection.profiles.push(profile);
        }
        if activate || collection.active_id.is_none() {
            collection.active_id = Some(id);
        }
        self.commit(&mut state, collection).await?;
        Ok(id)
    }

    pub async fn save_atomic(&self, config: &ModelProviderConfig) -> AppResult<()> {
        if config.kind == ModelProviderKind::Local {
            let mut state = self.state.lock().await;
            let mut collection = self.collection(&mut state).await?;
            collection.active_id = None;
            return self.commit(&mut state, collection).await;
        }
        let active = self
            .list()
            .await?
            .into_iter()
            .find(|p| p.active && p.status.kind == config.kind);
        self.save_profile(
            active.as_ref().map(|p| p.id),
            config,
            true,
            active.map(|p| p.status.auth_mode),
        )
        .await?;
        Ok(())
    }

    pub async fn activate(&self, id: Uuid) -> AppResult<ModelProviderConfig> {
        let mut state = self.state.lock().await;
        let mut collection = self.collection(&mut state).await?;
        let config = self
            .hydrate(
                collection
                    .profiles
                    .iter()
                    .find(|p| p.id == id)
                    .ok_or(AppError::ModelInvalid)?,
            )
            .await?;
        if !status_from_config(&config, false).api_key_configured {
            return Err(AppError::ModelAuthFailed);
        }
        collection.active_id = Some(id);
        self.commit(&mut state, collection).await?;
        Ok(config)
    }

    pub async fn remove(&self, id: Uuid) -> AppResult<()> {
        let mut state = self.state.lock().await;
        let mut collection = self.collection(&mut state).await?;
        // An enabled model must be explicitly switched before it can be removed.
        if collection.active_id == Some(id) {
            return Err(AppError::ModelInvalid);
        }
        if !collection.profiles.iter().any(|p| p.id == id) {
            return Err(AppError::ModelInvalid);
        }
        if collection
            .profiles
            .iter()
            .any(|p| p.id == id && p.credential_id.is_some())
        {
            let credentials = self.credentials.as_ref().ok_or(AppError::VaultLocked)?;
            if !credentials.status(None, None).await?.vault_unlocked {
                return Err(AppError::VaultLocked);
            }
        }
        collection.profiles.retain(|p| p.id != id);
        self.commit(&mut state, collection).await
    }
}
