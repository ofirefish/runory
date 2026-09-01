use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::domain::{AppError, AppResult};
use crate::tools::{NativeToolExecutionService, RiskLevel};

const MAX_SKILL_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillManifest {
    pub id: String,
    pub version: String,
    pub publisher: String,
    pub required_tools: Vec<String>,
    #[serde(default)]
    pub optional_tools: Vec<String>,
    pub risk_ceiling: RiskLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SkillOrigin {
    BuiltIn,
    User,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Skill {
    pub manifest: SkillManifest,
    pub origin: SkillOrigin,
    pub enabled: bool,
    pub permission_review_codes: Vec<&'static str>,
    #[serde(skip_serializing)]
    pub instructions: String,
}

pub(crate) struct SkillRegistry {
    root: PathBuf,
    skills: Arc<RwLock<BTreeMap<String, Skill>>>,
    enabled: Arc<RwLock<BTreeSet<String>>>,
}

impl SkillRegistry {
    pub(crate) fn at_path(root: PathBuf) -> AppResult<Self> {
        let mut skills = BTreeMap::new();
        for (manifest, instructions) in builtins() {
            let skill = parse_skill(manifest, instructions, SkillOrigin::BuiltIn)?;
            skills.insert(skill.manifest.id.clone(), skill);
        }
        Ok(Self {
            root,
            skills: Arc::new(RwLock::new(skills)),
            enabled: Arc::new(RwLock::new(BTreeSet::new())),
        })
    }

    pub(crate) async fn refresh(
        &self,
        tools: &NativeToolExecutionService,
    ) -> AppResult<Vec<Skill>> {
        let mut merged = BTreeMap::new();
        for (manifest, instructions) in builtins() {
            let mut skill = parse_skill(manifest, instructions, SkillOrigin::BuiltIn)?;
            skill.permission_review_codes = review(&skill, tools);
            merged.insert(skill.manifest.id.clone(), skill);
        }
        if tokio::fs::try_exists(&self.root)
            .await
            .map_err(|_| AppError::Storage)?
        {
            let mut directories = tokio::fs::read_dir(&self.root)
                .await
                .map_err(|_| AppError::Storage)?;
            while let Some(entry) = directories
                .next_entry()
                .await
                .map_err(|_| AppError::Storage)?
            {
                if !entry
                    .file_type()
                    .await
                    .map_err(|_| AppError::Storage)?
                    .is_dir()
                {
                    continue;
                }
                if let Ok(mut skill) = load_user_skill(&entry.path()).await {
                    skill.permission_review_codes = review(&skill, tools);
                    merged.entry(skill.manifest.id.clone()).or_insert(skill);
                }
            }
        }
        let enabled = self.enabled.read().await;
        for skill in merged.values_mut() {
            skill.enabled = enabled.contains(&skill.manifest.id);
        }
        drop(enabled);
        *self.skills.write().await = merged;
        self.list().await
    }

    pub(crate) async fn list(&self) -> AppResult<Vec<Skill>> {
        Ok(self.skills.read().await.values().cloned().collect())
    }

    pub(crate) async fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<Skill> {
        let mut skills = self.skills.write().await;
        let skill = skills.get_mut(id).ok_or(AppError::InvalidOperation)?;
        if !skill.permission_review_codes.is_empty() {
            return Err(AppError::InvalidOperation);
        }
        skill.enabled = enabled;
        let mut enabled_ids = self.enabled.write().await;
        if enabled {
            enabled_ids.insert(id.to_owned());
        } else {
            enabled_ids.remove(id);
        }
        Ok(skill.clone())
    }

    pub(crate) async fn enabled_instructions(&self, id: &str) -> AppResult<String> {
        let skills = self.skills.read().await;
        let skill = skills
            .get(id)
            .filter(|skill| skill.enabled)
            .ok_or(AppError::InvalidOperation)?;
        Ok(skill.instructions.clone())
    }
}

fn review(skill: &Skill, tools: &NativeToolExecutionService) -> Vec<&'static str> {
    let descriptors = tools.descriptors();
    let mut codes = Vec::new();
    for required in &skill.manifest.required_tools {
        match descriptors
            .iter()
            .find(|item| item.name.as_str() == required)
        {
            None => codes.push("required-tool-missing"),
            Some(item) if item.risk_level > skill.manifest.risk_ceiling => {
                codes.push("risk-ceiling-exceeded")
            }
            _ => {}
        }
    }
    codes.sort_unstable();
    codes.dedup();
    codes
}

async fn load_user_skill(directory: &Path) -> AppResult<Skill> {
    let manifest_path = directory.join("manifest.json");
    let instructions_path = directory.join("SKILL.md");
    for path in [&manifest_path, &instructions_path] {
        let metadata = tokio::fs::symlink_metadata(path)
            .await
            .map_err(|_| AppError::InvalidOperation)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_SKILL_BYTES {
            return Err(AppError::InvalidOperation);
        }
    }
    let manifest = tokio::fs::read_to_string(manifest_path)
        .await
        .map_err(|_| AppError::InvalidOperation)?;
    let instructions = tokio::fs::read_to_string(instructions_path)
        .await
        .map_err(|_| AppError::InvalidOperation)?;
    parse_skill(&manifest, &instructions, SkillOrigin::User)
}

fn parse_skill(manifest: &str, instructions: &str, origin: SkillOrigin) -> AppResult<Skill> {
    if manifest.len() as u64 > MAX_SKILL_BYTES || instructions.len() as u64 > MAX_SKILL_BYTES {
        return Err(AppError::InvalidOperation);
    }
    let manifest: SkillManifest =
        serde_json::from_str(manifest).map_err(|_| AppError::InvalidOperation)?;
    if !valid_id(&manifest.id)
        || !valid_version(&manifest.version)
        || manifest.publisher.trim().is_empty()
        || manifest.required_tools.len() > 32
        || manifest.optional_tools.len() > 32
    {
        return Err(AppError::InvalidOperation);
    }
    Ok(Skill {
        manifest,
        origin,
        enabled: false,
        permission_review_codes: Vec::new(),
        instructions: instructions.to_owned(),
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}
fn valid_version(value: &str) -> bool {
    value.split('.').count() == 3
        && value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn builtins() -> [(&'static str, &'static str); 4] {
    [
        (
            include_str!("../../skills/nginx-doctor/manifest.json"),
            include_str!("../../skills/nginx-doctor/SKILL.md"),
        ),
        (
            include_str!("../../skills/website-troubleshooter/manifest.json"),
            include_str!("../../skills/website-troubleshooter/SKILL.md"),
        ),
        (
            include_str!("../../skills/linux-service-doctor/manifest.json"),
            include_str!("../../skills/linux-service-doctor/SKILL.md"),
        ),
        (
            include_str!("../../skills/disk-space-doctor/manifest.json"),
            include_str!("../../skills/disk-space-doctor/SKILL.md"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_manifest_identity() {
        assert!(parse_skill(r#"{"id":"../bad","version":"1.0.0","publisher":"x","requiredTools":[],"optionalTools":[],"riskCeiling":"R1"}"#, "x", SkillOrigin::User).is_err());
    }
}
