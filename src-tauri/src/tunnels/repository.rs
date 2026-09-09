use super::model::TunnelRule;
use crate::{
    domain::{AppError, AppResult},
    storage::JsonRepository,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TunnelCatalog {
    schema_version: u8,
    rules: Vec<TunnelRule>,
}
impl Default for TunnelCatalog {
    fn default() -> Self {
        Self {
            schema_version: 1,
            rules: Vec::new(),
        }
    }
}

pub struct TunnelRuleRepository {
    json: JsonRepository<TunnelCatalog>,
}
impl TunnelRuleRepository {
    pub fn new(path: PathBuf) -> Self {
        Self {
            json: JsonRepository::new(path),
        }
    }
    pub async fn load(&self) -> AppResult<Vec<TunnelRule>> {
        let catalog = self.json.load_or_default().await?;
        let mut ids = HashSet::new();
        if catalog.schema_version != 1
            || catalog.rules.len() > 100
            || catalog
                .rules
                .iter()
                .any(|rule| rule.validate().is_err() || !ids.insert(rule.id))
        {
            return Err(AppError::Storage);
        }
        Ok(catalog.rules)
    }
    pub async fn save(&self, rules: &[TunnelRule]) -> AppResult<()> {
        self.json
            .save_atomic(&TunnelCatalog {
                schema_version: 1,
                rules: rules.to_vec(),
            })
            .await
    }
}
