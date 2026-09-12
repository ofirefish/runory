use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::domain::{
    AppError, AppResult, AuthMethod, ConnectionRoute, CreateProfileRequest, OsDistribution,
    ReorderProfilesRequest, ServerProfile, UpdateProfileRequest,
};
use crate::groups::{timestamp, GroupRepository};

use super::ProfileRepository;

pub struct ProfileService {
    profiles: ProfileRepository,
    groups: GroupRepository,
    write_lock: Arc<Mutex<()>>,
}

impl ProfileService {
    pub fn new(
        profiles: ProfileRepository,
        groups: GroupRepository,
        write_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            profiles,
            groups,
            write_lock,
        }
    }

    pub async fn list(&self) -> AppResult<Vec<ServerProfile>> {
        let _guard = self.write_lock.lock().await;
        let mut profiles = self.profiles.list().await?;
        profiles.sort_by_key(|profile| profile.sort_order);
        Ok(profiles)
    }

    pub async fn get(&self, id: uuid::Uuid) -> AppResult<ServerProfile> {
        let _guard = self.write_lock.lock().await;
        self.profiles
            .list()
            .await?
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or(AppError::ProfileNotFound)
    }

    pub async fn create(&self, request: CreateProfileRequest) -> AppResult<ServerProfile> {
        let fields = validate_fields(request.name, request.host, request.port, request.username)?;
        let key_source = validate_auth(&request.auth_method, request.key_source)?;
        let _guard = self.write_lock.lock().await;
        self.validate_group(request.group_id).await?;
        let mut profiles = self.profiles.list().await?;
        validate_route(&request.connection_route, None, &profiles)?;
        let now = timestamp();
        let profile = ServerProfile {
            id: uuid::Uuid::new_v4(),
            name: fields.0,
            host: fields.1,
            port: request.port,
            username: fields.2,
            group_id: request.group_id,
            auth_method: request.auth_method,
            key_source,
            connection_route: request.connection_route,
            sort_order: next_sort_order(&profiles, request.group_id),
            created_at: now.clone(),
            updated_at: now,
            last_connected_at: None,
            os_distribution: None,
        };
        profiles.push(profile.clone());
        self.profiles.save(&profiles).await?;
        Ok(profile)
    }

    pub async fn update(&self, request: UpdateProfileRequest) -> AppResult<ServerProfile> {
        let fields = validate_fields(request.name, request.host, request.port, request.username)?;
        let key_source = validate_auth(&request.auth_method, request.key_source)?;
        if request.sort_order < 0 {
            return Err(AppError::InvalidProfile);
        }
        let _guard = self.write_lock.lock().await;
        self.validate_group(request.group_id).await?;
        let mut profiles = self.profiles.list().await?;
        validate_route(&request.connection_route, Some(request.id), &profiles)?;
        if request.connection_route != ConnectionRoute::Direct
            && profiles.iter().any(|profile| {
                matches!(profile.connection_route, ConnectionRoute::JumpHost { profile_id } if profile_id == request.id)
            })
        {
            return Err(AppError::ProfileInUseAsJumpHost);
        }
        let existing = profiles
            .iter()
            .find(|profile| profile.id == request.id)
            .ok_or(AppError::ProfileNotFound)?;
        let existing_group_id = existing.group_id;
        let endpoint_changed = existing.host != fields.1 || existing.port != request.port;
        let sort_order = if existing_group_id == request.group_id {
            request.sort_order
        } else {
            next_sort_order(&profiles, request.group_id)
        };
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == request.id)
            .ok_or(AppError::ProfileNotFound)?;
        profile.name = fields.0;
        profile.host = fields.1;
        profile.port = request.port;
        profile.username = fields.2;
        profile.group_id = request.group_id;
        profile.auth_method = request.auth_method;
        profile.key_source = key_source;
        profile.connection_route = request.connection_route;
        profile.sort_order = sort_order;
        if endpoint_changed {
            profile.os_distribution = None;
        }
        profile.updated_at = timestamp();
        let updated = profile.clone();
        self.profiles.save(&profiles).await?;
        Ok(updated)
    }

    pub async fn delete(&self, id: uuid::Uuid) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut profiles = self.profiles.list().await?;
        if profiles.iter().any(|profile| {
            matches!(profile.connection_route, ConnectionRoute::JumpHost { profile_id } if profile_id == id)
        }) {
            return Err(AppError::ProfileInUseAsJumpHost);
        }
        let original_len = profiles.len();
        profiles.retain(|profile| profile.id != id);
        if profiles.len() == original_len {
            return Err(AppError::ProfileNotFound);
        }
        self.profiles.save(&profiles).await
    }

    pub async fn mark_connected(
        &self,
        id: uuid::Uuid,
        detected_os: Option<OsDistribution>,
    ) -> AppResult<ServerProfile> {
        let _guard = self.write_lock.lock().await;
        let mut profiles = self.profiles.list().await?;
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or(AppError::ProfileNotFound)?;
        let now = timestamp();
        profile.last_connected_at = Some(now.clone());
        if profile.os_distribution.is_none() {
            profile.os_distribution = detected_os;
        }
        profile.updated_at = now;
        let updated = profile.clone();
        self.profiles.save(&profiles).await?;
        Ok(updated)
    }

    pub async fn reorder(&self, request: ReorderProfilesRequest) -> AppResult<Vec<ServerProfile>> {
        let _guard = self.write_lock.lock().await;
        self.validate_group(request.group_id).await?;
        let mut profiles = self.profiles.list().await?;
        let expected: HashSet<_> = profiles
            .iter()
            .filter(|profile| profile.group_id == request.group_id)
            .map(|profile| profile.id)
            .collect();
        let ordered: HashSet<_> = request.ordered_ids.iter().copied().collect();
        if request.ordered_ids.len() != expected.len() || ordered != expected {
            return Err(AppError::InvalidProfile);
        }
        let now = timestamp();
        for (sort_order, id) in request.ordered_ids.into_iter().enumerate() {
            let profile = profiles
                .iter_mut()
                .find(|profile| profile.id == id && profile.group_id == request.group_id)
                .ok_or(AppError::InvalidProfile)?;
            profile.sort_order = i32::try_from(sort_order).map_err(|_| AppError::InvalidProfile)?;
            profile.updated_at = now.clone();
        }
        profiles.sort_by_key(|profile| profile.sort_order);
        self.profiles.save(&profiles).await?;
        Ok(profiles)
    }

    async fn validate_group(&self, group_id: Option<uuid::Uuid>) -> AppResult<()> {
        if let Some(group_id) = group_id {
            if !self
                .groups
                .list()
                .await?
                .iter()
                .any(|group| group.id == group_id)
            {
                return Err(AppError::InvalidGroup);
            }
        }
        Ok(())
    }
}

fn validate_route(
    route: &ConnectionRoute,
    current_id: Option<uuid::Uuid>,
    profiles: &[ServerProfile],
) -> AppResult<()> {
    match route {
        ConnectionRoute::Direct => Ok(()),
        ConnectionRoute::Bastion {
            provider,
            asset_id: _,
            ..
        } => {
            if provider.trim().is_empty() {
                return Err(AppError::InvalidProfile);
            }
            Ok(())
        }
        ConnectionRoute::JumpHost { profile_id } => {
            if Some(*profile_id) == current_id {
                return Err(AppError::InvalidJumpHost);
            }
            let jump = profiles
                .iter()
                .find(|profile| profile.id == *profile_id)
                .ok_or(AppError::InvalidJumpHost)?;
            if jump.connection_route != ConnectionRoute::Direct {
                return Err(AppError::InvalidJumpHost);
            }
            Ok(())
        }
    }
}

fn validate_fields(
    name: String,
    host: String,
    port: u16,
    username: String,
) -> AppResult<(String, String, String)> {
    let name = name.trim();
    let host = host.trim();
    let username = username.trim();
    if name.is_empty()
        || name.chars().count() > 100
        || host.is_empty()
        || host.chars().count() > 255
        || host.chars().any(char::is_whitespace)
        || port == 0
        || username.is_empty()
        || username.chars().count() > 255
    {
        return Err(AppError::InvalidProfile);
    }
    Ok((name.to_owned(), host.to_owned(), username.to_owned()))
}

fn validate_auth(
    auth_method: &AuthMethod,
    key_source: Option<crate::domain::KeySource>,
) -> AppResult<Option<crate::domain::KeySource>> {
    match (auth_method, key_source) {
        (AuthMethod::Password, None) => Ok(None),
        (AuthMethod::PrivateKey, Some(crate::domain::KeySource::File { path })) => {
            let path = path.trim();
            if path.is_empty() || path.len() > 4096 || path.contains('\0') {
                Err(AppError::InvalidProfile)
            } else {
                Ok(Some(crate::domain::KeySource::File {
                    path: path.to_owned(),
                }))
            }
        }
        (AuthMethod::PrivateKey, Some(source @ crate::domain::KeySource::Vault { .. })) => {
            Ok(Some(source))
        }
        _ => Err(AppError::InvalidProfile),
    }
}

fn next_sort_order(profiles: &[ServerProfile], group_id: Option<uuid::Uuid>) -> i32 {
    profiles
        .iter()
        .filter(|profile| profile.group_id == group_id)
        .map(|profile| profile.sort_order)
        .max()
        .unwrap_or(-1)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{CreateProfileRequest, HostGroup, UpdateProfileRequest},
        storage::CatalogDatabase,
    };

    fn profile_request(name: &str, connection_route: ConnectionRoute) -> CreateProfileRequest {
        CreateProfileRequest {
            name: name.into(),
            host: format!("{}.example.com", name.to_lowercase()),
            port: 22,
            username: "root".into(),
            group_id: None,
            auth_method: AuthMethod::Password,
            key_source: None,
            connection_route,
        }
    }

    #[tokio::test]
    async fn creates_updates_and_deletes_profile_metadata() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("open");
        let group_repository = GroupRepository::new(database.clone());
        let profile_repository = ProfileRepository::new(database);
        let group = HostGroup {
            id: uuid::Uuid::new_v4(),
            name: "Production".into(),
            sort_order: 0,
            collapsed: false,
            created_at: timestamp(),
            updated_at: timestamp(),
        };
        group_repository
            .save(std::slice::from_ref(&group))
            .await
            .expect("save group");
        let service = ProfileService::new(
            profile_repository.clone(),
            group_repository,
            Arc::new(Mutex::new(())),
        );

        let created = service
            .create(CreateProfileRequest {
                name: " API ".into(),
                host: "api.example.com".into(),
                port: 22,
                username: "root".into(),
                group_id: Some(group.id),
                auth_method: AuthMethod::Password,
                key_source: None,
                connection_route: ConnectionRoute::Direct,
            })
            .await
            .expect("create profile");
        assert_eq!(created.name, "API");
        assert_eq!(created.auth_method, AuthMethod::Password);
        service
            .mark_connected(created.id, Some(OsDistribution::Ubuntu))
            .await
            .expect("mark initial endpoint connected");

        let updated = service
            .update(UpdateProfileRequest {
                id: created.id,
                name: "API 2".into(),
                host: created.host,
                port: 2222,
                username: created.username,
                group_id: None,
                auth_method: AuthMethod::PrivateKey,
                key_source: Some(crate::domain::KeySource::File {
                    path: "C:/keys/id_ed25519".into(),
                }),
                connection_route: ConnectionRoute::Direct,
                sort_order: 0,
            })
            .await
            .expect("update profile");
        assert_eq!(updated.group_id, None);
        assert_eq!(updated.port, 2222);
        assert_eq!(updated.auth_method, AuthMethod::PrivateKey);
        assert_eq!(updated.os_distribution, None);

        let connected = service
            .mark_connected(created.id, Some(OsDistribution::Debian))
            .await
            .expect("mark connected");
        assert!(connected.last_connected_at.is_some());
        assert_eq!(connected.os_distribution, Some(OsDistribution::Debian));

        service.delete(created.id).await.expect("delete profile");
        assert!(profile_repository
            .list()
            .await
            .expect("profiles")
            .is_empty());
    }

    #[tokio::test]
    async fn reorders_only_the_profiles_in_the_requested_group() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("open");
        let group_repository = GroupRepository::new(database.clone());
        let profile_repository = ProfileRepository::new(database);
        let group = HostGroup {
            id: uuid::Uuid::new_v4(),
            name: "Production".into(),
            sort_order: 0,
            collapsed: false,
            created_at: timestamp(),
            updated_at: timestamp(),
        };
        group_repository
            .save(std::slice::from_ref(&group))
            .await
            .expect("save group");
        let service = ProfileService::new(
            profile_repository,
            group_repository,
            Arc::new(Mutex::new(())),
        );
        let mut ids = Vec::new();
        for name in ["A", "B", "C"] {
            ids.push(
                service
                    .create(CreateProfileRequest {
                        name: name.into(),
                        host: format!("{name}.example.com"),
                        port: 22,
                        username: "root".into(),
                        group_id: Some(group.id),
                        auth_method: AuthMethod::Password,
                        key_source: None,
                        connection_route: ConnectionRoute::Direct,
                    })
                    .await
                    .expect("create profile")
                    .id,
            );
        }

        let reordered = service
            .reorder(ReorderProfilesRequest {
                group_id: Some(group.id),
                ordered_ids: vec![ids[2], ids[0], ids[1]],
            })
            .await
            .expect("reorder profiles");
        let group_ids = reordered
            .iter()
            .filter(|profile| profile.group_id == Some(group.id))
            .map(|profile| profile.id)
            .collect::<Vec<_>>();
        assert_eq!(group_ids, vec![ids[2], ids[0], ids[1]]);

        let invalid = service
            .reorder(ReorderProfilesRequest {
                group_id: Some(group.id),
                ordered_ids: vec![ids[0], ids[0], ids[1]],
            })
            .await;
        assert!(matches!(invalid, Err(AppError::InvalidProfile)));

        let outside = service
            .create(CreateProfileRequest {
                name: "Outside".into(),
                host: "outside.example.com".into(),
                port: 22,
                username: "root".into(),
                group_id: None,
                auth_method: AuthMethod::Password,
                key_source: None,
                connection_route: ConnectionRoute::Direct,
            })
            .await
            .expect("create outside profile");
        let moved = service
            .update(UpdateProfileRequest {
                id: outside.id,
                name: outside.name,
                host: outside.host,
                port: outside.port,
                username: outside.username,
                group_id: Some(group.id),
                auth_method: outside.auth_method,
                key_source: outside.key_source,
                connection_route: ConnectionRoute::Direct,
                sort_order: outside.sort_order,
            })
            .await
            .expect("move profile");
        assert_eq!(moved.sort_order, 3);
    }

    #[tokio::test]
    async fn enforces_single_hop_routes_and_preserves_referenced_jump_hosts() {
        let directory = tempfile::tempdir().expect("temp directory");
        let database = CatalogDatabase::open(directory.path().join("runory.db")).expect("open");
        let service = ProfileService::new(
            ProfileRepository::new(database.clone()),
            GroupRepository::new(database),
            Arc::new(Mutex::new(())),
        );

        let jump = service
            .create(profile_request("Jump", ConnectionRoute::Direct))
            .await
            .expect("create jump host");
        let target = service
            .create(profile_request(
                "Target",
                ConnectionRoute::JumpHost {
                    profile_id: jump.id,
                },
            ))
            .await
            .expect("create target");

        assert!(matches!(
            service
                .create(profile_request(
                    "Nested",
                    ConnectionRoute::JumpHost {
                        profile_id: target.id,
                    },
                ))
                .await,
            Err(AppError::InvalidJumpHost)
        ));
        assert!(matches!(
            service.delete(jump.id).await,
            Err(AppError::ProfileInUseAsJumpHost)
        ));

        let update_jump = UpdateProfileRequest {
            id: jump.id,
            name: jump.name,
            host: jump.host,
            port: jump.port,
            username: jump.username,
            group_id: jump.group_id,
            auth_method: jump.auth_method,
            key_source: jump.key_source,
            connection_route: ConnectionRoute::JumpHost {
                profile_id: target.id,
            },
            sort_order: jump.sort_order,
        };
        assert!(matches!(
            service.update(update_jump).await,
            Err(AppError::InvalidJumpHost) | Err(AppError::ProfileInUseAsJumpHost)
        ));
    }
}
