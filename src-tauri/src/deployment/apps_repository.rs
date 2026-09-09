use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::deployment::validation::{
    validate_app_name, validate_branch, validate_remote_path, validate_remote_url, validate_restart,
};
use crate::domain::{AppError, AppResult, BuildPreset, DeploymentApp, RestartTarget};
use crate::storage::JsonRepository;

const MAX_APPS_PER_PROFILE: usize = 50;

pub struct DeploymentAppsRepository {
    repository: JsonRepository<Vec<DeploymentApp>>,
    write_lock: Mutex<()>,
}

impl DeploymentAppsRepository {
    pub fn new(repository: JsonRepository<Vec<DeploymentApp>>) -> Self {
        Self {
            repository,
            write_lock: Mutex::new(()),
        }
    }

    pub async fn list(&self, profile_id: Uuid) -> AppResult<Vec<DeploymentApp>> {
        let mut apps = self
            .repository
            .load_or_default()
            .await?
            .into_iter()
            .filter(|app| app.profile_id == profile_id)
            .collect::<Vec<_>>();
        apps.sort_by(|left, right| {
            right
                .updated_at_epoch_seconds
                .cmp(&left.updated_at_epoch_seconds)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(apps)
    }

    pub async fn upsert(
        &self,
        id: Option<Uuid>,
        profile_id: Uuid,
        name: String,
        repository_path: String,
        remote_url: String,
        branch: String,
        build: BuildPreset,
        restart: RestartTarget,
    ) -> AppResult<DeploymentApp> {
        validate_app_name(&name)?;
        validate_remote_path(&repository_path)?;
        validate_remote_url(&remote_url)?;
        validate_branch(&branch)?;
        validate_restart(&restart)?;

        let _guard = self.write_lock.lock().await;
        let mut apps = self.repository.load_or_default().await?;
        let now = epoch_seconds()?;
        let app_id = id.unwrap_or_else(Uuid::new_v4);

        if let Some(existing_id) = id {
            if !apps
                .iter()
                .any(|app| app.id == existing_id && app.profile_id == profile_id)
            {
                return Err(AppError::InvalidOperation);
            }
        } else {
            let profile_count = apps
                .iter()
                .filter(|app| app.profile_id == profile_id)
                .count();
            if profile_count >= MAX_APPS_PER_PROFILE {
                return Err(AppError::InvalidOperation);
            }
        }

        if apps.iter().any(|app| {
            app.profile_id == profile_id
                && app.id != app_id
                && (app.name == name || app.repository_path == repository_path)
        }) {
            return Err(AppError::InvalidOperation);
        }

        let app = DeploymentApp {
            id: app_id,
            profile_id,
            name,
            repository_path,
            remote_url,
            branch,
            build,
            restart,
            updated_at_epoch_seconds: now,
        };

        if let Some(index) = apps.iter().position(|entry| entry.id == app_id) {
            apps[index] = app.clone();
        } else {
            apps.push(app.clone());
        }
        self.repository.save_atomic(&apps).await?;
        Ok(app)
    }

    pub async fn delete(&self, profile_id: Uuid, id: Uuid) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut apps = self.repository.load_or_default().await?;
        let before = apps.len();
        apps.retain(|app| !(app.id == id && app.profile_id == profile_id));
        if apps.len() == before {
            return Err(AppError::InvalidOperation);
        }
        self.repository.save_atomic(&apps).await
    }
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

    fn repo(path: &std::path::Path) -> DeploymentAppsRepository {
        DeploymentAppsRepository::new(JsonRepository::new(path.join("deployment-apps.json")))
    }

    fn sample(
        profile_id: Uuid,
        name: &str,
        repository_path: &str,
    ) -> (
        Option<Uuid>,
        Uuid,
        String,
        String,
        String,
        String,
        BuildPreset,
        RestartTarget,
    ) {
        (
            None,
            profile_id,
            name.to_string(),
            repository_path.to_string(),
            "https://example.com/app.git".to_string(),
            "main".to_string(),
            BuildPreset::None,
            RestartTarget::None,
        )
    }

    #[tokio::test]
    async fn upsert_enforces_unique_name_and_path_per_profile() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let other = Uuid::new_v4();

        let first = repository
            .upsert(
                None,
                profile,
                "api".into(),
                "/srv/api".into(),
                "https://example.com/api.git".into(),
                "main".into(),
                BuildPreset::Pnpm,
                RestartTarget::Systemd {
                    service: "api.service".into(),
                },
            )
            .await
            .unwrap();

        assert!(repository
            .upsert(
                None,
                profile,
                "api".into(),
                "/srv/other".into(),
                "https://example.com/other.git".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
            )
            .await
            .is_err());
        assert!(repository
            .upsert(
                None,
                profile,
                "other".into(),
                "/srv/api".into(),
                "https://example.com/other.git".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
            )
            .await
            .is_err());

        let cross = repository
            .upsert(
                None,
                other,
                "api".into(),
                "/srv/api".into(),
                "https://example.com/api.git".into(),
                "main".into(),
                BuildPreset::None,
                RestartTarget::None,
            )
            .await
            .unwrap();
        assert_ne!(first.id, cross.id);

        let listed = repository.list(profile).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, first.id);
        assert!(matches!(listed[0].build, BuildPreset::Pnpm));

        repository
            .upsert(
                Some(first.id),
                profile,
                "api".into(),
                "/srv/api".into(),
                "https://example.com/api.git".into(),
                "release".into(),
                BuildPreset::Npm,
                RestartTarget::Pm2 {
                    process: "api".into(),
                },
            )
            .await
            .unwrap();
        let updated = repository.list(profile).await.unwrap();
        assert_eq!(updated[0].branch, "release");
        assert!(matches!(updated[0].build, BuildPreset::Npm));

        repository.delete(profile, first.id).await.unwrap();
        assert!(repository.list(profile).await.unwrap().is_empty());
        assert_eq!(repository.list(other).await.unwrap().len(), 1);
        assert!(repository.delete(profile, first.id).await.is_err());
    }

    #[tokio::test]
    async fn create_rejects_invalid_fields() {
        let directory = tempdir().unwrap();
        let repository = repo(directory.path());
        let profile = Uuid::new_v4();
        let (_, profile_id, name, path, url, branch, build, restart) =
            sample(profile, "app", "/srv/app");
        assert!(repository
            .upsert(
                None,
                profile_id,
                String::new(),
                path.clone(),
                url.clone(),
                branch.clone(),
                build,
                restart.clone(),
            )
            .await
            .is_err());
        assert!(repository
            .upsert(
                None,
                profile_id,
                name,
                "/srv/../etc".into(),
                url,
                branch,
                build,
                restart,
            )
            .await
            .is_err());
    }
}
