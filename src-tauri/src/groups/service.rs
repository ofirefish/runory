use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::domain::{
    AppError, AppResult, CreateGroupRequest, HostGroup, ReorderGroupsRequest, UpdateGroupRequest,
};
use crate::profiles::ProfileRepository;

use super::GroupRepository;

pub struct GroupService {
    groups: GroupRepository,
    profiles: ProfileRepository,
    write_lock: Arc<Mutex<()>>,
}

impl GroupService {
    pub fn new(
        groups: GroupRepository,
        profiles: ProfileRepository,
        write_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            groups,
            profiles,
            write_lock,
        }
    }

    pub async fn list(&self) -> AppResult<Vec<HostGroup>> {
        let _guard = self.write_lock.lock().await;
        let mut groups = self.groups.list().await?;
        groups.sort_by_key(|group| group.sort_order);
        Ok(groups)
    }

    pub async fn create(&self, request: CreateGroupRequest) -> AppResult<HostGroup> {
        let name = validate_name(request.name)?;
        let _guard = self.write_lock.lock().await;
        let mut groups = self.groups.list().await?;
        let now = timestamp();
        let group = HostGroup {
            id: Uuid::new_v4(),
            name,
            sort_order: next_sort_order(&groups),
            collapsed: false,
            created_at: now.clone(),
            updated_at: now,
        };
        groups.push(group.clone());
        self.groups.save(&groups).await?;
        Ok(group)
    }

    pub async fn update(&self, request: UpdateGroupRequest) -> AppResult<HostGroup> {
        let name = validate_name(request.name)?;
        if request.sort_order < 0 {
            return Err(AppError::InvalidGroup);
        }
        let _guard = self.write_lock.lock().await;
        let mut groups = self.groups.list().await?;
        let group = groups
            .iter_mut()
            .find(|group| group.id == request.id)
            .ok_or(AppError::GroupNotFound)?;
        group.name = name;
        group.sort_order = request.sort_order;
        group.collapsed = request.collapsed;
        group.updated_at = timestamp();
        let updated = group.clone();
        self.groups.save(&groups).await?;
        Ok(updated)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut groups = self.groups.list().await?;
        let original_len = groups.len();
        groups.retain(|group| group.id != id);
        if groups.len() == original_len {
            return Err(AppError::GroupNotFound);
        }

        let mut profiles = self.profiles.list().await?;
        let now = timestamp();
        for profile in profiles
            .iter_mut()
            .filter(|profile| profile.group_id == Some(id))
        {
            profile.group_id = None;
            profile.updated_at = now.clone();
        }
        // Persist the safe ungrouping first so a partial failure cannot leave dangling group references.
        self.profiles.save(&profiles).await?;
        self.groups.save(&groups).await
    }

    pub async fn reorder(&self, request: ReorderGroupsRequest) -> AppResult<Vec<HostGroup>> {
        let _guard = self.write_lock.lock().await;
        let mut groups = self.groups.list().await?;
        let expected: HashSet<_> = groups.iter().map(|group| group.id).collect();
        let ordered: HashSet<_> = request.ordered_ids.iter().copied().collect();
        if request.ordered_ids.len() != groups.len() || ordered != expected {
            return Err(AppError::InvalidGroup);
        }
        let now = timestamp();
        for (sort_order, id) in request.ordered_ids.into_iter().enumerate() {
            let group = groups
                .iter_mut()
                .find(|group| group.id == id)
                .ok_or(AppError::InvalidGroup)?;
            group.sort_order = i32::try_from(sort_order).map_err(|_| AppError::InvalidGroup)?;
            group.updated_at = now.clone();
        }
        groups.sort_by_key(|group| group.sort_order);
        self.groups.save(&groups).await?;
        Ok(groups)
    }
}

fn validate_name(name: String) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 50 {
        return Err(AppError::InvalidGroup);
    }
    Ok(trimmed.to_owned())
}

fn next_sort_order(groups: &[HostGroup]) -> i32 {
    groups
        .iter()
        .map(|group| group.sort_order)
        .max()
        .unwrap_or(-1)
        .saturating_add(1)
}

pub(crate) fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{AuthMethod, ServerProfile},
        storage::JsonRepository,
    };

    #[tokio::test]
    async fn deleting_group_moves_profiles_to_ungrouped() {
        let directory = tempfile::tempdir().expect("temp directory");
        let group_repository =
            GroupRepository::new(JsonRepository::new(directory.path().join("groups.json")));
        let profile_repository =
            ProfileRepository::new(JsonRepository::new(directory.path().join("profiles.json")));
        let service = GroupService::new(
            group_repository,
            profile_repository.clone(),
            Arc::new(Mutex::new(())),
        );
        let group = service
            .create(CreateGroupRequest {
                name: "Production".into(),
            })
            .await
            .expect("create group");
        profile_repository
            .save(&[ServerProfile {
                id: Uuid::new_v4(),
                name: "API".into(),
                host: "127.0.0.1".into(),
                port: 22,
                username: "root".into(),
                group_id: Some(group.id),
                auth_method: AuthMethod::Password,
                key_source: None,
                sort_order: 0,
                created_at: timestamp(),
                updated_at: timestamp(),
                last_connected_at: None,
                os_distribution: None,
            }])
            .await
            .expect("save profile");
        service.delete(group.id).await.expect("delete group");
        assert_eq!(
            profile_repository.list().await.expect("profiles")[0].group_id,
            None
        );
    }

    #[tokio::test]
    async fn reorders_all_groups_atomically_and_rejects_duplicate_ids() {
        let directory = tempfile::tempdir().expect("temp directory");
        let service = GroupService::new(
            GroupRepository::new(JsonRepository::new(directory.path().join("groups.json"))),
            ProfileRepository::new(JsonRepository::new(directory.path().join("profiles.json"))),
            Arc::new(Mutex::new(())),
        );
        let first = service
            .create(CreateGroupRequest { name: "A".into() })
            .await
            .expect("first");
        let second = service
            .create(CreateGroupRequest { name: "B".into() })
            .await
            .expect("second");
        let third = service
            .create(CreateGroupRequest { name: "C".into() })
            .await
            .expect("third");

        let reordered = service
            .reorder(ReorderGroupsRequest {
                ordered_ids: vec![third.id, first.id, second.id],
            })
            .await
            .expect("reorder");
        assert_eq!(
            reordered.iter().map(|group| group.id).collect::<Vec<_>>(),
            vec![third.id, first.id, second.id]
        );

        let invalid = service
            .reorder(ReorderGroupsRequest {
                ordered_ids: vec![first.id, first.id, second.id],
            })
            .await;
        assert!(matches!(invalid, Err(AppError::InvalidGroup)));
        assert_eq!(
            service
                .list()
                .await
                .expect("unchanged")
                .iter()
                .map(|group| group.id)
                .collect::<Vec<_>>(),
            vec![third.id, first.id, second.id]
        );
    }
}
