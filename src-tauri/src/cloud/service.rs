use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::{
    AppError, AppResult, AuthMethod, CloudApplyResult, CloudConflictDecision, CloudConflictItem,
    CloudConflictResolution, CloudEncryptedPayload, CloudGroup, CloudImportPreview,
    CloudKeyEnvelope, CloudObjectKind, CloudProfile, CloudSyncKeyStatus, HostGroup, ServerProfile,
};
use crate::groups::timestamp;
use crate::groups::GroupRepository;
use crate::profiles::ProfileRepository;

use super::state::{CloudSyncStateRepository, CloudTombstone, OrganizationSyncState};
use super::CloudSyncKeyStore;

const SYNC_VERSION: u8 = 4;
const LEGACY_SYNC_VERSION: u8 = 3;
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const DATA_KEY_LENGTH: usize = 32;
const WRAPPED_KEY_LENGTH: usize = DATA_KEY_LENGTH + 16;
const STORED_KEY_MATERIAL_VERSION: u8 = 1;
const STORED_KEY_MATERIAL_LENGTH: usize =
    1 + DATA_KEY_LENGTH + SALT_LENGTH + NONCE_LENGTH + WRAPPED_KEY_LENGTH;
const MAX_PLAINTEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_OBJECTS: usize = 20_000;
const MAX_PENDING_IMPORTS: usize = 8;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudSnapshot {
    version: u8,
    groups: Vec<CloudGroup>,
    profiles: Vec<CloudProfile>,
    #[serde(default)]
    tombstones: Vec<CloudTombstone>,
}

struct PendingImport {
    organization_id: Uuid,
    snapshot: CloudSnapshot,
    migration_key: Option<CloudKeyMaterial>,
}

struct CloudKeyMaterial {
    key: Zeroizing<[u8; DATA_KEY_LENGTH]>,
    envelope: CloudKeyEnvelope,
}

impl Clone for CloudKeyMaterial {
    fn clone(&self) -> Self {
        Self {
            key: Zeroizing::new(*self.key),
            envelope: self.envelope.clone(),
        }
    }
}

pub struct CloudSyncService {
    profiles: ProfileRepository,
    groups: GroupRepository,
    write_lock: Arc<Mutex<()>>,
    state: CloudSyncStateRepository,
    pending: Mutex<HashMap<Uuid, PendingImport>>,
    key_store: Arc<dyn CloudSyncKeyStore>,
    active_keys: Mutex<HashMap<Uuid, (CloudKeyMaterial, bool)>>,
}

impl CloudSyncService {
    pub fn with_key_store(
        profiles: ProfileRepository,
        groups: GroupRepository,
        write_lock: Arc<Mutex<()>>,
        state: CloudSyncStateRepository,
        key_store: Arc<dyn CloudSyncKeyStore>,
    ) -> Self {
        Self {
            profiles,
            groups,
            write_lock,
            state,
            pending: Mutex::new(HashMap::new()),
            key_store,
            active_keys: Mutex::new(HashMap::new()),
        }
    }

    async fn secure_storage_available(&self) -> bool {
        if !self.key_store.is_supported() {
            return false;
        }
        let key_store = Arc::clone(&self.key_store);
        tokio::task::spawn_blocking(move || key_store.probe().is_ok())
            .await
            .unwrap_or(false)
    }

    async fn load_key_material(
        &self,
        organization_id: Uuid,
    ) -> AppResult<(Option<CloudKeyMaterial>, bool)> {
        if let Some((material, persisted)) =
            self.active_keys.lock().await.get(&organization_id).cloned()
        {
            return Ok((Some(material), persisted));
        }
        if !self.secure_storage_available().await {
            return Ok((None, false));
        }
        let key_store = Arc::clone(&self.key_store);
        let stored = tokio::task::spawn_blocking(move || key_store.load(organization_id))
            .await
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)??;
        let Some(stored) = stored else {
            return Ok((None, true));
        };
        let material = decode_key_material(stored.as_slice())?;
        self.active_keys
            .lock()
            .await
            .insert(organization_id, (material.clone(), true));
        Ok((Some(material), true))
    }

    async fn remember_key_material(
        &self,
        organization_id: Uuid,
        material: CloudKeyMaterial,
    ) -> AppResult<bool> {
        let storage_available = self.secure_storage_available().await;
        let persisted = if storage_available {
            let encoded = encode_key_material(&material);
            let key_store = Arc::clone(&self.key_store);
            match tokio::task::spawn_blocking(move || {
                key_store.store(organization_id, encoded.as_slice())
            })
            .await
            {
                Ok(Ok(())) => true,
                Ok(Err(error)) => {
                    tracing::warn!(error_code = error.code(), %organization_id, "cloud sync key remains session-only");
                    false
                }
                Err(_) => {
                    tracing::warn!(%organization_id, "cloud sync key store task failed; key remains session-only");
                    false
                }
            }
        } else {
            false
        };
        self.active_keys
            .lock()
            .await
            .insert(organization_id, (material, persisted));
        Ok(persisted)
    }

    async fn export_key(
        &self,
        organization_id: Uuid,
        recovery_passphrase: Option<Zeroizing<String>>,
    ) -> AppResult<CloudKeyMaterial> {
        if let Some(material) = self.load_key_material(organization_id).await?.0 {
            return Ok(material);
        }
        let recovery_passphrase = recovery_passphrase.ok_or(AppError::CloudRecoveryRequired)?;
        validate_passphrase(&recovery_passphrase)?;
        let material = tokio::task::spawn_blocking(move || {
            create_key_material(organization_id, recovery_passphrase)
        })
        .await
        .map_err(|_| AppError::CloudCrypto)??;
        self.remember_key_material(organization_id, material.clone())
            .await?;
        Ok(material)
    }

    pub async fn key_status(&self, organization_id: Uuid) -> AppResult<CloudSyncKeyStatus> {
        let (material, persisted_on_device) = self.load_key_material(organization_id).await?;
        Ok(CloudSyncKeyStatus {
            configured: material.is_some(),
            persisted_on_device: material.is_some() && persisted_on_device,
            secure_storage_available: self.secure_storage_available().await,
        })
    }

    pub async fn forget_key(&self, organization_id: Uuid) -> AppResult<()> {
        if self.key_store.is_supported() {
            if !self.secure_storage_available().await {
                return Err(AppError::PlatformKeyStoreUnavailable);
            }
            let key_store = Arc::clone(&self.key_store);
            tokio::task::spawn_blocking(move || key_store.delete(organization_id))
                .await
                .map_err(|_| AppError::PlatformKeyStoreUnavailable)??;
        }
        self.active_keys.lock().await.remove(&organization_id);
        Ok(())
    }

    pub async fn rotate_recovery_passphrase(
        &self,
        organization_id: Uuid,
        new_recovery_passphrase: Zeroizing<String>,
        payload: CloudEncryptedPayload,
    ) -> AppResult<CloudEncryptedPayload> {
        validate_passphrase(&new_recovery_passphrase)?;
        if payload.version != SYNC_VERSION {
            return Err(AppError::CloudInvalid);
        }
        let (material, persisted_on_device) = self.load_key_material(organization_id).await?;
        let material = material.ok_or(AppError::CloudRecoveryRequired)?;
        let (rotated_material, rotated_payload) = tokio::task::spawn_blocking(move || {
            decrypt_snapshot_with_key(organization_id, material.key.as_ref(), &payload)?;
            let rotated_material = wrap_key_material(
                organization_id,
                Zeroizing::new(*material.key),
                new_recovery_passphrase,
            )?;
            let mut rotated_payload = payload;
            rotated_payload.key_envelope = Some(rotated_material.envelope.clone());
            Ok::<_, AppError>((rotated_material, rotated_payload))
        })
        .await
        .map_err(|_| AppError::CloudCrypto)??;
        if persisted_on_device {
            let encoded = encode_key_material(&rotated_material);
            let key_store = Arc::clone(&self.key_store);
            tokio::task::spawn_blocking(move || {
                key_store.store(organization_id, encoded.as_slice())
            })
            .await
            .map_err(|_| AppError::PlatformKeyStoreUnavailable)??;
            self.active_keys
                .lock()
                .await
                .insert(organization_id, (rotated_material, true));
        } else {
            self.remember_key_material(organization_id, rotated_material)
                .await?;
        }
        Ok(rotated_payload)
    }

    pub async fn export(
        &self,
        organization_id: Uuid,
        recovery_passphrase: Option<Zeroizing<String>>,
    ) -> AppResult<CloudEncryptedPayload> {
        let key_material = self
            .export_key(organization_id, recovery_passphrase)
            .await?;
        let _guard = self.write_lock.lock().await;
        let groups: Vec<_> = self
            .groups
            .list()
            .await?
            .into_iter()
            .map(CloudGroup::from)
            .collect();
        let profiles: Vec<_> = self
            .profiles
            .list()
            .await?
            .into_iter()
            .map(CloudProfile::from)
            .collect();
        let mut state = self.state.load().await?;
        let organization_state = state.organizations.entry(organization_id).or_default();
        record_local_deletions(organization_state, &groups, &profiles)?;
        let snapshot = CloudSnapshot {
            version: SYNC_VERSION,
            groups,
            profiles,
            tombstones: organization_state.tombstones.clone(),
        };
        self.state.save(&state).await?;
        drop(_guard);
        tokio::task::spawn_blocking(move || {
            encrypt_snapshot_with_key(organization_id, &key_material, &snapshot)
        })
        .await
        .map_err(|_| AppError::CloudCrypto)?
    }

    pub async fn preview(
        &self,
        organization_id: Uuid,
        recovery_passphrase: Option<Zeroizing<String>>,
        payload: CloudEncryptedPayload,
    ) -> AppResult<CloudImportPreview> {
        let (snapshot, migration_key) = if payload.version == SYNC_VERSION {
            let local_material = self.load_key_material(organization_id).await?.0;
            let local_attempt = if let Some(material) = local_material {
                let local_payload = payload.clone();
                Some(
                    tokio::task::spawn_blocking(move || {
                        decrypt_snapshot_with_key(
                            organization_id,
                            material.key.as_ref(),
                            &local_payload,
                        )
                    })
                    .await
                    .map_err(|_| AppError::CloudCrypto)?,
                )
            } else {
                None
            };
            let snapshot = match local_attempt {
                Some(Ok(snapshot)) => snapshot,
                Some(Err(error)) if recovery_passphrase.is_none() => return Err(error),
                _ => {
                    let recovery_passphrase =
                        recovery_passphrase.ok_or(AppError::CloudRecoveryRequired)?;
                    validate_passphrase(&recovery_passphrase)?;
                    let envelope = payload.key_envelope.clone().ok_or(AppError::CloudInvalid)?;
                    let (material, snapshot) = tokio::task::spawn_blocking(move || {
                        let material =
                            unwrap_key_material(organization_id, recovery_passphrase, envelope)?;
                        let snapshot = decrypt_snapshot_with_key(
                            organization_id,
                            material.key.as_ref(),
                            &payload,
                        )?;
                        Ok::<_, AppError>((material, snapshot))
                    })
                    .await
                    .map_err(|_| AppError::CloudCrypto)??;
                    self.remember_key_material(organization_id, material)
                        .await?;
                    snapshot
                }
            };
            (snapshot, None)
        } else {
            let recovery_passphrase = recovery_passphrase.ok_or(AppError::CloudRecoveryRequired)?;
            validate_passphrase(&recovery_passphrase)?;
            let migration_passphrase = Zeroizing::new(recovery_passphrase.to_string());
            let (snapshot, material) = tokio::task::spawn_blocking(move || {
                let snapshot =
                    decrypt_legacy_snapshot(organization_id, recovery_passphrase, payload)?;
                let material = create_key_material(organization_id, migration_passphrase)?;
                Ok::<_, AppError>((snapshot, material))
            })
            .await
            .map_err(|_| AppError::CloudCrypto)??;
            (snapshot, Some(material))
        };
        validate_snapshot(&snapshot)?;

        let _guard = self.write_lock.lock().await;
        let local_groups = self.groups.list().await?;
        let local_profiles = self.profiles.list().await?;
        let state = self.state.load().await?;
        let organization_state = state
            .organizations
            .get(&organization_id)
            .cloned()
            .unwrap_or_default();
        let mut preview = compare_snapshot(
            &snapshot,
            &local_groups,
            &local_profiles,
            &organization_state,
        );
        drop(_guard);
        preview.import_id = Uuid::new_v4();
        let mut pending = self.pending.lock().await;
        if pending.len() >= MAX_PENDING_IMPORTS {
            return Err(AppError::CloudInvalid);
        }
        pending.insert(
            preview.import_id,
            PendingImport {
                organization_id,
                snapshot,
                migration_key,
            },
        );
        Ok(preview)
    }

    pub async fn apply(
        &self,
        import_id: Uuid,
        decisions: Vec<CloudConflictDecision>,
    ) -> AppResult<CloudApplyResult> {
        let (organization_id, snapshot, migration_key) = {
            let pending = self.pending.lock().await;
            let import = pending
                .get(&import_id)
                .ok_or(AppError::CloudImportNotFound)?;
            (
                import.organization_id,
                import.snapshot.clone(),
                import.migration_key.clone(),
            )
        };
        let _guard = self.write_lock.lock().await;
        let original_groups = self.groups.list().await?;
        let original_profiles = self.profiles.list().await?;
        let original_state = self.state.load().await?;
        let mut state = original_state.clone();
        let organization_state = state.organizations.entry(organization_id).or_default();
        let current_preview = compare_snapshot(
            &snapshot,
            &original_groups,
            &original_profiles,
            organization_state,
        );
        let overrides = validate_decisions(&decisions, &current_preview.conflict_items)?;

        let (mut groups, groups_applied, group_skipped) = merge_groups(
            &original_groups,
            &snapshot.groups,
            organization_state,
            &overrides,
        );
        let valid_groups: HashSet<_> = groups.iter().map(|group| group.id).collect();
        let (mut profiles, profiles_applied, profile_skipped) = merge_profiles(
            &original_profiles,
            &snapshot.profiles,
            &valid_groups,
            organization_state,
            &overrides,
        );
        let (profiles_deleted, profile_delete_skipped) = apply_profile_tombstones(
            &mut profiles,
            &snapshot.tombstones,
            organization_state,
            &overrides,
        );
        let (groups_deleted, group_delete_skipped) = apply_group_tombstones(
            &mut groups,
            &mut profiles,
            &snapshot.tombstones,
            organization_state,
            &overrides,
        );
        organization_state.known_groups = groups.iter().map(|item| item.id).collect();
        organization_state.known_profiles = profiles.iter().map(|item| item.id).collect();

        self.groups.save(&groups).await?;
        if self.profiles.save(&profiles).await.is_err() {
            self.groups.save(&original_groups).await?;
            return Err(AppError::Storage);
        }
        if self.state.save(&state).await.is_err() {
            self.groups.save(&original_groups).await?;
            self.profiles.save(&original_profiles).await?;
            return Err(AppError::Storage);
        }
        drop(_guard);
        if let Some(material) = migration_key {
            self.remember_key_material(organization_id, material)
                .await?;
        }
        self.pending.lock().await.remove(&import_id);
        tracing::info!(%organization_id, %import_id, groups_applied, profiles_applied, groups_deleted, profiles_deleted, "cloud sync import applied");
        Ok(CloudApplyResult {
            groups_applied,
            profiles_applied,
            groups_deleted,
            profiles_deleted,
            skipped: group_skipped
                + profile_skipped
                + group_delete_skipped
                + profile_delete_skipped,
        })
    }

    pub async fn discard(&self, import_id: Uuid) -> AppResult<()> {
        if self.pending.lock().await.remove(&import_id).is_none() {
            return Err(AppError::CloudImportNotFound);
        }
        Ok(())
    }
}

impl From<HostGroup> for CloudGroup {
    fn from(group: HostGroup) -> Self {
        Self {
            id: group.id,
            name: group.name,
            sort_order: group.sort_order,
            collapsed: group.collapsed,
            created_at: group.created_at,
            updated_at: group.updated_at,
        }
    }
}

impl From<ServerProfile> for CloudProfile {
    fn from(profile: ServerProfile) -> Self {
        Self {
            id: profile.id,
            name: profile.name,
            host: profile.host,
            port: profile.port,
            username: profile.username,
            group_id: profile.group_id,
            auth_method: profile.auth_method,
            connection_route: profile.connection_route,
            sort_order: profile.sort_order,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        }
    }
}

fn validate_passphrase(passphrase: &str) -> AppResult<()> {
    if (12..=1024).contains(&passphrase.chars().count()) {
        Ok(())
    } else {
        Err(AppError::CloudInvalid)
    }
}

fn aad(version: u8, organization_id: Uuid) -> String {
    format!("runory-cloud-sync-v{version}:{organization_id}")
}

fn key_aad(organization_id: Uuid) -> String {
    format!("runory-cloud-sync-key-v{SYNC_VERSION}:{organization_id}")
}

fn derive_key(passphrase: &str, salt: &[u8; SALT_LENGTH]) -> AppResult<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0_u8; 32]);
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|_| AppError::CloudCrypto)?;
    Ok(key)
}

fn create_key_material(
    organization_id: Uuid,
    recovery_passphrase: Zeroizing<String>,
) -> AppResult<CloudKeyMaterial> {
    let mut key = Zeroizing::new([0_u8; DATA_KEY_LENGTH]);
    OsRng.fill_bytes(key.as_mut());
    wrap_key_material(organization_id, key, recovery_passphrase)
}

fn wrap_key_material(
    organization_id: Uuid,
    key: Zeroizing<[u8; DATA_KEY_LENGTH]>,
    recovery_passphrase: Zeroizing<String>,
) -> AppResult<CloudKeyMaterial> {
    let mut salt = [0_u8; SALT_LENGTH];
    let mut nonce = [0_u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let wrapping_key = derive_key(recovery_passphrase.as_str(), &salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(wrapping_key.as_ref()).map_err(|_| AppError::CloudCrypto)?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: key.as_ref(),
                aad: key_aad(organization_id).as_bytes(),
            },
        )
        .map_err(|_| AppError::CloudCrypto)?;
    Ok(CloudKeyMaterial {
        key,
        envelope: CloudKeyEnvelope {
            salt: salt.to_vec(),
            nonce: nonce.to_vec(),
            ciphertext,
        },
    })
}

fn unwrap_key_material(
    organization_id: Uuid,
    recovery_passphrase: Zeroizing<String>,
    envelope: CloudKeyEnvelope,
) -> AppResult<CloudKeyMaterial> {
    if envelope.salt.len() != SALT_LENGTH
        || envelope.nonce.len() != NONCE_LENGTH
        || envelope.ciphertext.len() != WRAPPED_KEY_LENGTH
    {
        return Err(AppError::CloudInvalid);
    }
    let salt: [u8; SALT_LENGTH] = envelope
        .salt
        .as_slice()
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let nonce: [u8; NONCE_LENGTH] = envelope
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let wrapping_key = derive_key(recovery_passphrase.as_str(), &salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(wrapping_key.as_ref()).map_err(|_| AppError::CloudCrypto)?;
    let decrypted = Zeroizing::new(
        cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &envelope.ciphertext,
                    aad: key_aad(organization_id).as_bytes(),
                },
            )
            .map_err(|_| AppError::CloudDecrypt)?,
    );
    let key: [u8; DATA_KEY_LENGTH] = decrypted
        .as_slice()
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    Ok(CloudKeyMaterial {
        key: Zeroizing::new(key),
        envelope,
    })
}

fn encrypt_snapshot_with_key(
    organization_id: Uuid,
    material: &CloudKeyMaterial,
    snapshot: &CloudSnapshot,
) -> AppResult<CloudEncryptedPayload> {
    let plaintext =
        Zeroizing::new(serde_json::to_vec(snapshot).map_err(|_| AppError::CloudInvalid)?);
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(AppError::CloudInvalid);
    }
    let mut salt = [0_u8; SALT_LENGTH];
    let mut nonce = [0_u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let cipher =
        Aes256Gcm::new_from_slice(material.key.as_ref()).map_err(|_| AppError::CloudCrypto)?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext.as_slice(),
                aad: aad(SYNC_VERSION, organization_id).as_bytes(),
            },
        )
        .map_err(|_| AppError::CloudCrypto)?;
    Ok(CloudEncryptedPayload {
        version: SYNC_VERSION,
        salt: salt.to_vec(),
        nonce: nonce.to_vec(),
        ciphertext,
        key_envelope: Some(material.envelope.clone()),
    })
}

fn decrypt_snapshot_with_key(
    organization_id: Uuid,
    key: &[u8],
    payload: &CloudEncryptedPayload,
) -> AppResult<CloudSnapshot> {
    if payload.version != SYNC_VERSION
        || payload.salt.len() != SALT_LENGTH
        || payload.nonce.len() != NONCE_LENGTH
        || payload.ciphertext.len() > MAX_PLAINTEXT_BYTES + 32
        || payload.key_envelope.is_none()
    {
        return Err(AppError::CloudInvalid);
    }
    let nonce: [u8; NONCE_LENGTH] = payload
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| AppError::CloudCrypto)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &payload.ciphertext,
                    aad: aad(payload.version, organization_id).as_bytes(),
                },
            )
            .map_err(|_| AppError::CloudDecrypt)?,
    );
    let snapshot: CloudSnapshot =
        serde_json::from_slice(plaintext.as_slice()).map_err(|_| AppError::CloudInvalid)?;
    if snapshot.version != payload.version {
        return Err(AppError::CloudInvalid);
    }
    Ok(snapshot)
}

fn decrypt_legacy_snapshot(
    organization_id: Uuid,
    passphrase: Zeroizing<String>,
    payload: CloudEncryptedPayload,
) -> AppResult<CloudSnapshot> {
    if !matches!(payload.version, 1 | 2 | LEGACY_SYNC_VERSION)
        || payload.salt.len() != SALT_LENGTH
        || payload.nonce.len() != NONCE_LENGTH
        || payload.ciphertext.len() > MAX_PLAINTEXT_BYTES + 32
    {
        return Err(AppError::CloudInvalid);
    }
    let version = payload.version;
    let salt: [u8; SALT_LENGTH] = payload
        .salt
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let nonce: [u8; NONCE_LENGTH] = payload
        .nonce
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let key = derive_key(passphrase.as_str(), &salt)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| AppError::CloudCrypto)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: &payload.ciphertext,
                    aad: aad(version, organization_id).as_bytes(),
                },
            )
            .map_err(|_| AppError::CloudDecrypt)?,
    );
    let snapshot: CloudSnapshot =
        serde_json::from_slice(plaintext.as_slice()).map_err(|_| AppError::CloudInvalid)?;
    if snapshot.version != version {
        return Err(AppError::CloudInvalid);
    }
    Ok(snapshot)
}

fn encode_key_material(material: &CloudKeyMaterial) -> Zeroizing<Vec<u8>> {
    let mut encoded = Zeroizing::new(Vec::with_capacity(STORED_KEY_MATERIAL_LENGTH));
    encoded.push(STORED_KEY_MATERIAL_VERSION);
    encoded.extend_from_slice(material.key.as_ref());
    encoded.extend_from_slice(&material.envelope.salt);
    encoded.extend_from_slice(&material.envelope.nonce);
    encoded.extend_from_slice(&material.envelope.ciphertext);
    encoded
}

fn decode_key_material(encoded: &[u8]) -> AppResult<CloudKeyMaterial> {
    if encoded.len() != STORED_KEY_MATERIAL_LENGTH
        || encoded.first().copied() != Some(STORED_KEY_MATERIAL_VERSION)
    {
        return Err(AppError::CloudCrypto);
    }
    let mut offset = 1;
    let key = encoded[offset..offset + DATA_KEY_LENGTH]
        .try_into()
        .map_err(|_| AppError::CloudCrypto)?;
    offset += DATA_KEY_LENGTH;
    let salt = encoded[offset..offset + SALT_LENGTH].to_vec();
    offset += SALT_LENGTH;
    let nonce = encoded[offset..offset + NONCE_LENGTH].to_vec();
    offset += NONCE_LENGTH;
    let ciphertext = encoded[offset..].to_vec();
    Ok(CloudKeyMaterial {
        key: Zeroizing::new(key),
        envelope: CloudKeyEnvelope {
            salt,
            nonce,
            ciphertext,
        },
    })
}

fn validate_snapshot(snapshot: &CloudSnapshot) -> AppResult<()> {
    if !matches!(snapshot.version, 1 | 2 | LEGACY_SYNC_VERSION | SYNC_VERSION)
        || snapshot
            .groups
            .len()
            .saturating_add(snapshot.profiles.len())
            .saturating_add(snapshot.tombstones.len())
            > MAX_OBJECTS
    {
        return Err(AppError::CloudInvalid);
    }
    let group_ids: HashSet<_> = snapshot.groups.iter().map(|group| group.id).collect();
    let profile_ids: HashSet<_> = snapshot.profiles.iter().map(|profile| profile.id).collect();
    if group_ids.len() != snapshot.groups.len() || profile_ids.len() != snapshot.profiles.len() {
        return Err(AppError::CloudInvalid);
    }
    let tombstone_ids: HashSet<_> = snapshot
        .tombstones
        .iter()
        .map(|item| (item.kind, item.id))
        .collect();
    if tombstone_ids.len() != snapshot.tombstones.len()
        || snapshot.tombstones.iter().any(|item| {
            !valid_timestamp(&item.deleted_at)
                || match item.kind {
                    CloudObjectKind::Group => group_ids.contains(&item.id),
                    CloudObjectKind::Profile => profile_ids.contains(&item.id),
                }
        })
    {
        return Err(AppError::CloudInvalid);
    }
    let valid_group = |group: &CloudGroup| {
        !group.name.trim().is_empty()
            && group.name.chars().count() <= 50
            && group.sort_order >= 0
            && valid_timestamp(&group.created_at)
            && valid_timestamp(&group.updated_at)
    };
    let valid_profile = |profile: &CloudProfile| {
        !profile.name.trim().is_empty()
            && profile.name.chars().count() <= 100
            && !profile.host.trim().is_empty()
            && profile.host.chars().count() <= 255
            && !profile.host.chars().any(char::is_whitespace)
            && profile.port > 0
            && !profile.username.trim().is_empty()
            && profile.username.chars().count() <= 255
            && profile.sort_order >= 0
            && profile.group_id.is_none_or(|id| group_ids.contains(&id))
            && valid_timestamp(&profile.created_at)
            && valid_timestamp(&profile.updated_at)
    };
    if snapshot.groups.iter().all(valid_group) && snapshot.profiles.iter().all(valid_profile) {
        Ok(())
    } else {
        Err(AppError::CloudInvalid)
    }
}

fn valid_timestamp(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.parse::<u128>().is_ok()
}

fn record_local_deletions(
    state: &mut OrganizationSyncState,
    groups: &[CloudGroup],
    profiles: &[CloudProfile],
) -> AppResult<()> {
    let current_groups: HashSet<_> = groups.iter().map(|item| item.id).collect();
    let current_profiles: HashSet<_> = profiles.iter().map(|item| item.id).collect();
    let deleted_at = timestamp();
    for id in state
        .known_groups
        .difference(&current_groups)
        .copied()
        .collect::<Vec<_>>()
    {
        if state.tombstone(CloudObjectKind::Group, id).is_none() {
            state.upsert_tombstone(CloudTombstone {
                kind: CloudObjectKind::Group,
                id,
                deleted_at: deleted_at.clone(),
            });
        }
    }
    for id in state
        .known_profiles
        .difference(&current_profiles)
        .copied()
        .collect::<Vec<_>>()
    {
        if state.tombstone(CloudObjectKind::Profile, id).is_none() {
            state.upsert_tombstone(CloudTombstone {
                kind: CloudObjectKind::Profile,
                id,
                deleted_at: deleted_at.clone(),
            });
        }
    }
    for id in &current_groups {
        state.remove_tombstone(CloudObjectKind::Group, *id);
    }
    for id in &current_profiles {
        state.remove_tombstone(CloudObjectKind::Profile, *id);
    }
    state.known_groups = current_groups;
    state.known_profiles = current_profiles;
    if state
        .known_groups
        .len()
        .saturating_add(state.known_profiles.len())
        .saturating_add(state.tombstones.len())
        > MAX_OBJECTS
    {
        return Err(AppError::CloudInvalid);
    }
    Ok(())
}

fn timestamp_order(remote: &str, local: &str) -> std::cmp::Ordering {
    remote
        .parse::<u128>()
        .unwrap_or_default()
        .cmp(&local.parse::<u128>().unwrap_or_default())
}

fn compare_snapshot(
    snapshot: &CloudSnapshot,
    local_groups: &[HostGroup],
    local_profiles: &[ServerProfile],
    local_state: &OrganizationSyncState,
) -> CloudImportPreview {
    let mut result = CloudImportPreview {
        import_id: Uuid::nil(),
        group_additions: 0,
        group_updates: 0,
        profile_additions: 0,
        profile_updates: 0,
        group_deletions: 0,
        profile_deletions: 0,
        local_newer: 0,
        conflicts: 0,
        conflict_items: Vec::new(),
    };
    for remote in &snapshot.groups {
        match local_groups.iter().find(|local| local.id == remote.id) {
            None => match local_state.tombstone(CloudObjectKind::Group, remote.id) {
                None => result.group_additions += 1,
                Some(local) => match timestamp_order(&remote.updated_at, &local.deleted_at) {
                    std::cmp::Ordering::Greater => result.group_additions += 1,
                    ordering => add_conflict(
                        &mut result,
                        CloudObjectKind::Group,
                        remote.id,
                        remote.name.clone(),
                        local.deleted_at.clone(),
                        remote.updated_at.clone(),
                        false,
                        ordering,
                    ),
                },
            },
            Some(local) => match classify(
                timestamp_order(&remote.updated_at, &local.updated_at),
                remote != &CloudGroup::from(local.clone()),
            ) {
                Difference::RemoteNewer => result.group_updates += 1,
                Difference::LocalNewer => add_conflict(
                    &mut result,
                    CloudObjectKind::Group,
                    remote.id,
                    local.name.clone(),
                    local.updated_at.clone(),
                    remote.updated_at.clone(),
                    false,
                    std::cmp::Ordering::Less,
                ),
                Difference::Conflict => add_conflict(
                    &mut result,
                    CloudObjectKind::Group,
                    remote.id,
                    local.name.clone(),
                    local.updated_at.clone(),
                    remote.updated_at.clone(),
                    false,
                    std::cmp::Ordering::Equal,
                ),
                Difference::None => {}
            },
        }
    }
    for remote in &snapshot.profiles {
        match local_profiles.iter().find(|local| local.id == remote.id) {
            None => match local_state.tombstone(CloudObjectKind::Profile, remote.id) {
                None => result.profile_additions += 1,
                Some(local) => match timestamp_order(&remote.updated_at, &local.deleted_at) {
                    std::cmp::Ordering::Greater => result.profile_additions += 1,
                    ordering => add_conflict(
                        &mut result,
                        CloudObjectKind::Profile,
                        remote.id,
                        remote.name.clone(),
                        local.deleted_at.clone(),
                        remote.updated_at.clone(),
                        false,
                        ordering,
                    ),
                },
            },
            Some(local) => match classify(
                timestamp_order(&remote.updated_at, &local.updated_at),
                remote != &CloudProfile::from(local.clone()),
            ) {
                Difference::RemoteNewer => result.profile_updates += 1,
                Difference::LocalNewer => add_conflict(
                    &mut result,
                    CloudObjectKind::Profile,
                    remote.id,
                    local.name.clone(),
                    local.updated_at.clone(),
                    remote.updated_at.clone(),
                    false,
                    std::cmp::Ordering::Less,
                ),
                Difference::Conflict => add_conflict(
                    &mut result,
                    CloudObjectKind::Profile,
                    remote.id,
                    local.name.clone(),
                    local.updated_at.clone(),
                    remote.updated_at.clone(),
                    false,
                    std::cmp::Ordering::Equal,
                ),
                Difference::None => {}
            },
        }
    }
    for tombstone in &snapshot.tombstones {
        match tombstone.kind {
            CloudObjectKind::Group => {
                if let Some(local) = local_groups.iter().find(|item| item.id == tombstone.id) {
                    match timestamp_order(&tombstone.deleted_at, &local.updated_at) {
                        std::cmp::Ordering::Greater => result.group_deletions += 1,
                        ordering => add_conflict(
                            &mut result,
                            CloudObjectKind::Group,
                            local.id,
                            local.name.clone(),
                            local.updated_at.clone(),
                            tombstone.deleted_at.clone(),
                            true,
                            ordering,
                        ),
                    }
                }
            }
            CloudObjectKind::Profile => {
                if let Some(local) = local_profiles.iter().find(|item| item.id == tombstone.id) {
                    match timestamp_order(&tombstone.deleted_at, &local.updated_at) {
                        std::cmp::Ordering::Greater => result.profile_deletions += 1,
                        ordering => add_conflict(
                            &mut result,
                            CloudObjectKind::Profile,
                            local.id,
                            local.name.clone(),
                            local.updated_at.clone(),
                            tombstone.deleted_at.clone(),
                            true,
                            ordering,
                        ),
                    }
                }
            }
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn add_conflict(
    result: &mut CloudImportPreview,
    kind: CloudObjectKind,
    id: Uuid,
    label: String,
    local_updated_at: String,
    remote_updated_at: String,
    remote_deleted: bool,
    ordering: std::cmp::Ordering,
) {
    if ordering == std::cmp::Ordering::Less {
        result.local_newer += 1;
    } else {
        result.conflicts += 1;
    }
    result.conflict_items.push(CloudConflictItem {
        kind,
        id,
        label,
        local_updated_at,
        remote_updated_at,
        remote_deleted,
    });
}

enum Difference {
    RemoteNewer,
    LocalNewer,
    Conflict,
    None,
}

fn classify(ordering: std::cmp::Ordering, differs: bool) -> Difference {
    use std::cmp::Ordering;
    match (ordering, differs) {
        (Ordering::Greater, true) => Difference::RemoteNewer,
        (Ordering::Less, true) => Difference::LocalNewer,
        (Ordering::Equal, true) => Difference::Conflict,
        _ => Difference::None,
    }
}

type OverrideSet = HashSet<(CloudObjectKind, Uuid)>;

fn validate_decisions(
    decisions: &[CloudConflictDecision],
    conflicts: &[CloudConflictItem],
) -> AppResult<OverrideSet> {
    let mut seen = HashSet::new();
    let mut overrides = HashSet::new();
    for decision in decisions {
        let key = (decision.kind, decision.id);
        if !seen.insert(key) {
            return Err(AppError::CloudInvalid);
        }
        let valid = conflicts.iter().any(|item| {
            item.kind == decision.kind
                && item.id == decision.id
                && item.local_updated_at == decision.expected_local_updated_at
        });
        if !valid {
            return Err(AppError::CloudInvalid);
        }
        if decision.resolution == CloudConflictResolution::UseRemote {
            overrides.insert(key);
        }
    }
    Ok(overrides)
}

fn merge_groups(
    local: &[HostGroup],
    remote: &[CloudGroup],
    state: &mut OrganizationSyncState,
    overrides: &OverrideSet,
) -> (Vec<HostGroup>, usize, usize) {
    let mut merged = local.to_vec();
    let mut applied = 0;
    let mut skipped = 0;
    for item in remote {
        let converted = HostGroup {
            id: item.id,
            name: item.name.clone(),
            sort_order: item.sort_order,
            collapsed: item.collapsed,
            created_at: item.created_at.clone(),
            updated_at: item.updated_at.clone(),
        };
        match merged.iter_mut().find(|current| current.id == item.id) {
            None if state
                .tombstone(CloudObjectKind::Group, item.id)
                .is_none_or(|deleted| {
                    timestamp_order(&item.updated_at, &deleted.deleted_at).is_gt()
                        || overrides.contains(&(CloudObjectKind::Group, item.id))
                }) =>
            {
                merged.push(converted);
                state.remove_tombstone(CloudObjectKind::Group, item.id);
                applied += 1;
            }
            None => skipped += 1,
            Some(current)
                if timestamp_order(&item.updated_at, &current.updated_at).is_gt()
                    || overrides.contains(&(CloudObjectKind::Group, item.id)) =>
            {
                *current = converted;
                applied += 1;
            }
            Some(_) => skipped += 1,
        }
    }
    (merged, applied, skipped)
}

fn merge_profiles(
    local: &[ServerProfile],
    remote: &[CloudProfile],
    valid_groups: &HashSet<Uuid>,
    state: &mut OrganizationSyncState,
    overrides: &OverrideSet,
) -> (Vec<ServerProfile>, usize, usize) {
    let mut merged = local.to_vec();
    let mut applied = 0;
    let mut skipped = 0;
    for item in remote {
        let group_id = item.group_id.filter(|id| valid_groups.contains(id));
        match merged.iter_mut().find(|current| current.id == item.id) {
            None if state
                .tombstone(CloudObjectKind::Profile, item.id)
                .is_none_or(|deleted| {
                    timestamp_order(&item.updated_at, &deleted.deleted_at).is_gt()
                        || overrides.contains(&(CloudObjectKind::Profile, item.id))
                }) =>
            {
                merged.push(ServerProfile {
                    id: item.id,
                    name: item.name.clone(),
                    host: item.host.clone(),
                    port: item.port,
                    username: item.username.clone(),
                    group_id,
                    auth_method: item.auth_method,
                    key_source: None,
                    connection_route: item.connection_route.clone(),
                    sort_order: item.sort_order,
                    created_at: item.created_at.clone(),
                    updated_at: item.updated_at.clone(),
                    last_connected_at: None,
                    os_distribution: None,
                });
                state.remove_tombstone(CloudObjectKind::Profile, item.id);
                applied += 1;
            }
            None => skipped += 1,
            Some(current)
                if timestamp_order(&item.updated_at, &current.updated_at).is_gt()
                    || overrides.contains(&(CloudObjectKind::Profile, item.id)) =>
            {
                let key_source = if current.auth_method == AuthMethod::PrivateKey
                    && item.auth_method == AuthMethod::PrivateKey
                {
                    current.key_source.clone()
                } else {
                    None
                };
                let last_connected_at = current.last_connected_at.clone();
                let os_distribution = (current.host == item.host && current.port == item.port)
                    .then_some(current.os_distribution)
                    .flatten();
                *current = ServerProfile {
                    id: item.id,
                    name: item.name.clone(),
                    host: item.host.clone(),
                    port: item.port,
                    username: item.username.clone(),
                    group_id,
                    auth_method: item.auth_method,
                    key_source,
                    connection_route: item.connection_route.clone(),
                    sort_order: item.sort_order,
                    created_at: item.created_at.clone(),
                    updated_at: item.updated_at.clone(),
                    last_connected_at,
                    os_distribution,
                };
                applied += 1;
            }
            Some(_) => skipped += 1,
        }
    }
    (merged, applied, skipped)
}

fn apply_profile_tombstones(
    profiles: &mut Vec<ServerProfile>,
    tombstones: &[CloudTombstone],
    state: &mut OrganizationSyncState,
    overrides: &OverrideSet,
) -> (usize, usize) {
    let mut deleted = 0;
    let mut skipped = 0;
    for tombstone in tombstones
        .iter()
        .filter(|item| item.kind == CloudObjectKind::Profile)
    {
        if let Some(local) = profiles.iter().find(|item| item.id == tombstone.id) {
            if timestamp_order(&tombstone.deleted_at, &local.updated_at).is_gt()
                || overrides.contains(&(CloudObjectKind::Profile, tombstone.id))
            {
                profiles.retain(|item| item.id != tombstone.id);
                state.upsert_tombstone(tombstone.clone());
                deleted += 1;
            } else {
                skipped += 1;
            }
        } else if state
            .tombstone(CloudObjectKind::Profile, tombstone.id)
            .is_none_or(|local| timestamp_order(&tombstone.deleted_at, &local.deleted_at).is_gt())
        {
            state.upsert_tombstone(tombstone.clone());
        }
    }
    (deleted, skipped)
}

fn apply_group_tombstones(
    groups: &mut Vec<HostGroup>,
    profiles: &mut [ServerProfile],
    tombstones: &[CloudTombstone],
    state: &mut OrganizationSyncState,
    overrides: &OverrideSet,
) -> (usize, usize) {
    let mut deleted = 0;
    let mut skipped = 0;
    for tombstone in tombstones
        .iter()
        .filter(|item| item.kind == CloudObjectKind::Group)
    {
        if let Some(local) = groups.iter().find(|item| item.id == tombstone.id) {
            if timestamp_order(&tombstone.deleted_at, &local.updated_at).is_gt()
                || overrides.contains(&(CloudObjectKind::Group, tombstone.id))
            {
                groups.retain(|item| item.id != tombstone.id);
                for profile in profiles
                    .iter_mut()
                    .filter(|item| item.group_id == Some(tombstone.id))
                {
                    profile.group_id = None;
                    profile.updated_at = tombstone.deleted_at.clone();
                }
                state.upsert_tombstone(tombstone.clone());
                deleted += 1;
            } else {
                skipped += 1;
            }
        } else if state
            .tombstone(CloudObjectKind::Group, tombstone.id)
            .is_none_or(|local| timestamp_order(&tombstone.deleted_at, &local.deleted_at).is_gt())
        {
            state.upsert_tombstone(tombstone.clone());
        }
    }
    (deleted, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> CloudSnapshot {
        CloudSnapshot {
            version: SYNC_VERSION,
            groups: vec![CloudGroup {
                id: Uuid::new_v4(),
                name: "Production".into(),
                sort_order: 0,
                collapsed: false,
                created_at: "1".into(),
                updated_at: "2".into(),
            }],
            profiles: vec![],
            tombstones: vec![],
        }
    }

    #[test]
    fn encrypted_snapshot_uses_a_random_key_wrapped_by_the_recovery_passphrase() {
        let organization_id = Uuid::new_v4();
        let material = create_key_material(
            organization_id,
            Zeroizing::new("a sufficiently long passphrase".into()),
        )
        .expect("create key material");
        let encrypted =
            encrypt_snapshot_with_key(organization_id, &material, &snapshot()).expect("encrypt");
        assert!(!encrypted
            .ciphertext
            .windows("Production".len())
            .any(|bytes| bytes == b"Production"));
        let recovered = unwrap_key_material(
            organization_id,
            Zeroizing::new("a sufficiently long passphrase".into()),
            encrypted.key_envelope.clone().expect("key envelope"),
        )
        .expect("recover data key");
        assert_eq!(recovered.key.as_ref(), material.key.as_ref());
        assert!(matches!(
            unwrap_key_material(
                Uuid::new_v4(),
                Zeroizing::new("a sufficiently long passphrase".into()),
                encrypted.key_envelope.clone().expect("key envelope")
            ),
            Err(AppError::CloudDecrypt)
        ));
        assert!(matches!(
            unwrap_key_material(
                organization_id,
                Zeroizing::new("the wrong long passphrase".into()),
                encrypted.key_envelope.expect("key envelope")
            ),
            Err(AppError::CloudDecrypt)
        ));
    }

    #[test]
    fn stored_key_material_round_trips_without_the_recovery_passphrase() {
        let material = create_key_material(
            Uuid::new_v4(),
            Zeroizing::new("a sufficiently long passphrase".into()),
        )
        .expect("create key material");
        let encoded = encode_key_material(&material);
        let decoded = decode_key_material(encoded.as_slice()).expect("decode key material");
        assert_eq!(decoded.key.as_ref(), material.key.as_ref());
        assert_eq!(decoded.envelope, material.envelope);
    }

    #[test]
    fn recovery_passphrase_rotation_keeps_the_data_key_and_snapshot_ciphertext() {
        let organization_id = Uuid::new_v4();
        let material = create_key_material(
            organization_id,
            Zeroizing::new("the original recovery phrase".into()),
        )
        .expect("create key material");
        let encrypted =
            encrypt_snapshot_with_key(organization_id, &material, &snapshot()).expect("encrypt");
        let rotated = wrap_key_material(
            organization_id,
            Zeroizing::new(*material.key),
            Zeroizing::new("the replacement recovery phrase".into()),
        )
        .expect("rotate recovery envelope");
        let mut rotated_payload = encrypted.clone();
        rotated_payload.key_envelope = Some(rotated.envelope.clone());

        assert_eq!(rotated.key.as_ref(), material.key.as_ref());
        assert_eq!(rotated_payload.ciphertext, encrypted.ciphertext);
        assert!(
            decrypt_snapshot_with_key(organization_id, rotated.key.as_ref(), &rotated_payload)
                .is_ok()
        );
        assert!(unwrap_key_material(
            organization_id,
            Zeroizing::new("the original recovery phrase".into()),
            rotated.envelope.clone(),
        )
        .is_err());
        assert!(unwrap_key_material(
            organization_id,
            Zeroizing::new("the replacement recovery phrase".into()),
            rotated.envelope,
        )
        .is_ok());
    }

    #[test]
    fn legacy_snapshot_remains_recoverable_for_migration() {
        let organization_id = Uuid::new_v4();
        let mut legacy_snapshot = snapshot();
        legacy_snapshot.version = LEGACY_SYNC_VERSION;
        let plaintext =
            Zeroizing::new(serde_json::to_vec(&legacy_snapshot).expect("serialize snapshot"));
        let mut salt = [0_u8; SALT_LENGTH];
        let mut nonce = [0_u8; NONCE_LENGTH];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);
        let passphrase = "a sufficiently long passphrase";
        let key = derive_key(passphrase, &salt).expect("derive legacy key");
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).expect("legacy cipher");
        let ciphertext = cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: aad(LEGACY_SYNC_VERSION, organization_id).as_bytes(),
                },
            )
            .expect("legacy encrypt");
        let payload = CloudEncryptedPayload {
            version: LEGACY_SYNC_VERSION,
            salt: salt.to_vec(),
            nonce: nonce.to_vec(),
            ciphertext,
            key_envelope: None,
        };

        let recovered =
            decrypt_legacy_snapshot(organization_id, Zeroizing::new(passphrase.into()), payload)
                .expect("legacy decrypt");
        assert_eq!(recovered.version, legacy_snapshot.version);
        assert_eq!(recovered.groups, legacy_snapshot.groups);
        assert_eq!(recovered.profiles, legacy_snapshot.profiles);
    }

    #[test]
    fn cloud_profile_never_contains_device_key_source_or_connection_history() {
        let profile = ServerProfile {
            id: Uuid::new_v4(),
            name: "API".into(),
            host: "api.example.com".into(),
            port: 22,
            username: "root".into(),
            group_id: None,
            auth_method: AuthMethod::PrivateKey,
            key_source: Some(crate::domain::KeySource::File {
                path: "C:/secret/id_ed25519".into(),
            }),
            connection_route: crate::domain::ConnectionRoute::Direct,
            sort_order: 0,
            created_at: "1".into(),
            updated_at: "2".into(),
            last_connected_at: Some("3".into()),
            os_distribution: Some(crate::domain::OsDistribution::Ubuntu),
        };
        let serialized = serde_json::to_string(&CloudProfile::from(profile)).expect("serialize");
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("keySource"));
        assert!(!serialized.contains("lastConnectedAt"));
        assert!(!serialized.contains("osDistribution"));
    }

    #[test]
    fn merge_applies_only_remote_newer_items_and_preserves_local_key_binding() {
        let profile_id = Uuid::new_v4();
        let local = ServerProfile {
            id: profile_id,
            name: "Old name".into(),
            host: "api.example.com".into(),
            port: 22,
            username: "root".into(),
            group_id: None,
            auth_method: AuthMethod::PrivateKey,
            key_source: Some(crate::domain::KeySource::File {
                path: "C:/keys/id_ed25519".into(),
            }),
            connection_route: crate::domain::ConnectionRoute::Direct,
            sort_order: 0,
            created_at: "1".into(),
            updated_at: "2".into(),
            last_connected_at: Some("2".into()),
            os_distribution: Some(crate::domain::OsDistribution::Debian),
        };
        let mut remote = CloudProfile::from(local.clone());
        remote.name = "New name".into();
        remote.updated_at = "3".into();
        let (merged, applied, skipped) = merge_profiles(
            &[local],
            &[remote],
            &HashSet::new(),
            &mut OrganizationSyncState::default(),
            &HashSet::new(),
        );
        assert_eq!((applied, skipped), (1, 0));
        assert_eq!(merged[0].name, "New name");
        assert!(matches!(
            merged[0].key_source,
            Some(crate::domain::KeySource::File { .. })
        ));
        assert_eq!(merged[0].last_connected_at.as_deref(), Some("2"));
        assert_eq!(
            merged[0].os_distribution,
            Some(crate::domain::OsDistribution::Debian)
        );
    }

    #[test]
    fn local_deletion_becomes_tombstone_and_resurrection_clears_it() {
        let group_id = Uuid::new_v4();
        let mut state = OrganizationSyncState::default();
        state.known_groups.insert(group_id);
        record_local_deletions(&mut state, &[], &[]).expect("record deletion");
        assert!(state.tombstone(CloudObjectKind::Group, group_id).is_some());
        let group = CloudGroup {
            id: group_id,
            name: "Restored".into(),
            sort_order: 0,
            collapsed: false,
            created_at: "1".into(),
            updated_at: "2".into(),
        };
        record_local_deletions(&mut state, &[group], &[]).expect("record resurrection");
        assert!(state.tombstone(CloudObjectKind::Group, group_id).is_none());
    }

    #[test]
    fn remote_delete_requires_review_when_local_item_is_newer() {
        let group = HostGroup {
            id: Uuid::new_v4(),
            name: "Production".into(),
            sort_order: 0,
            collapsed: false,
            created_at: "1".into(),
            updated_at: "5".into(),
        };
        let remote = CloudSnapshot {
            version: SYNC_VERSION,
            groups: vec![],
            profiles: vec![],
            tombstones: vec![CloudTombstone {
                kind: CloudObjectKind::Group,
                id: group.id,
                deleted_at: "4".into(),
            }],
        };
        let preview = compare_snapshot(&remote, &[group], &[], &OrganizationSyncState::default());
        assert_eq!(preview.local_newer, 1);
        assert_eq!(preview.group_deletions, 0);
        assert!(preview.conflict_items[0].remote_deleted);
    }

    #[test]
    fn remote_group_delete_ungroups_retained_local_profiles() {
        let group_id = Uuid::new_v4();
        let mut groups = vec![HostGroup {
            id: group_id,
            name: "Production".into(),
            sort_order: 0,
            collapsed: false,
            created_at: "1".into(),
            updated_at: "2".into(),
        }];
        let mut profiles = vec![ServerProfile {
            id: Uuid::new_v4(),
            name: "API".into(),
            host: "api.example.com".into(),
            port: 22,
            username: "root".into(),
            group_id: Some(group_id),
            auth_method: AuthMethod::Password,
            key_source: None,
            connection_route: crate::domain::ConnectionRoute::Direct,
            sort_order: 0,
            created_at: "1".into(),
            updated_at: "2".into(),
            last_connected_at: None,
            os_distribution: None,
        }];
        let tombstone = CloudTombstone {
            kind: CloudObjectKind::Group,
            id: group_id,
            deleted_at: "3".into(),
        };
        let result = apply_group_tombstones(
            &mut groups,
            &mut profiles,
            &[tombstone],
            &mut OrganizationSyncState::default(),
            &HashSet::new(),
        );
        assert_eq!(result, (1, 0));
        assert!(groups.is_empty());
        assert_eq!(profiles[0].group_id, None);
        assert_eq!(profiles[0].updated_at, "3");
    }
}
