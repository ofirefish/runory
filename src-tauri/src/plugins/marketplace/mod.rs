//! Marketplace client + local catalog helpers (PP5/PP6).

pub mod client;
#[cfg(test)]
mod client_tests;
pub mod model;
pub mod revocation;
pub mod update;

pub use client::MarketplaceClient;
pub use model::{MarketplacePlugin, MarketplacePluginVersion, RevocationEntry};
pub use revocation::apply_revocation_feed;
pub use update::{permission_diff_ids, UpdateCheckResult};
