use std::collections::HashMap;
use std::sync::Arc;

use super::provider::BastionProvider;

/// Static registry of compiled-in BastionProvider implementations.
#[derive(Default)]
pub struct BastionRegistry {
    providers: HashMap<String, Arc<dyn BastionProvider>>,
}

impl BastionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn BastionProvider>) {
        self.providers
            .insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn BastionProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}
