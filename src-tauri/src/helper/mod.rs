//! External helper process framework (P3).
//!
//! Teleport (`tsh`) and Boundary (`boundary`) require vendor CLIs. All spawning,
//! ready-detection, stderr capture, secret redaction, and process-tree cleanup
//! go through this module so providers never own raw `Command` lifecycles.

mod lifecycle;
mod local_proxy;
mod process;
mod redaction;
mod stdio;
mod version;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

#[allow(unused_imports)]
pub use lifecycle::HelperLifecycle;
pub use local_proxy::{reserve_loopback_port, LocalProxyHandle, LocalProxySpec};
pub use process::{HelperProcess, ProcessSpec};
#[allow(unused_imports)]
pub use redaction::redact_helper_output;
pub use stdio::StdioProxyHandle;
pub use version::{HelperVersion, VersionConstraint};

/// Errors from locating / spawning / supervising vendor CLIs.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HelperError {
    #[error("helper binary was not found")]
    Missing,
    #[error("helper version is incompatible")]
    VersionMismatch,
    #[error("helper process failed to start")]
    SpawnFailed,
    #[error("helper did not become ready in time")]
    ReadyTimeout,
    #[error("helper process exited unexpectedly")]
    Crashed,
    #[error("helper was cancelled")]
    Cancelled,
    #[error("local proxy endpoint could not be discovered")]
    EndpointDiscoveryFailed,
    #[error("helper operation is not supported on this platform")]
    Unsupported,
}

impl HelperError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing => "HELPER_MISSING",
            Self::VersionMismatch => "HELPER_VERSION_MISMATCH",
            Self::SpawnFailed => "HELPER_SPAWN_FAILED",
            Self::ReadyTimeout => "HELPER_READY_TIMEOUT",
            Self::Crashed => "HELPER_CRASHED",
            Self::Cancelled => "HELPER_CANCELLED",
            Self::EndpointDiscoveryFailed => "HELPER_ENDPOINT_DISCOVERY_FAILED",
            Self::Unsupported => "HELPER_UNSUPPORTED",
        }
    }
}

/// Locate, version-check, spawn, and terminate vendor CLI helpers.
#[async_trait]
pub trait ExternalHelperManager: Send + Sync {
    fn locate_binary(&self, names: &[&str]) -> Result<PathBuf, HelperError>;

    /// Prefer an explicit profile CLI path, then PATH / common install locations.
    fn locate_binary_with_override(
        &self,
        override_path: Option<&str>,
        names: &[&str],
    ) -> Result<PathBuf, HelperError> {
        let _ = override_path;
        self.locate_binary(names)
    }

    fn check_version(
        &self,
        binary: &PathBuf,
        constraint: &VersionConstraint,
    ) -> Result<HelperVersion, HelperError>;

    async fn spawn(&self, spec: ProcessSpec) -> Result<HelperProcess, HelperError>;

    async fn spawn_stdio_proxy(&self, spec: ProcessSpec) -> Result<StdioProxyHandle, HelperError>;

    async fn spawn_local_proxy(&self, spec: LocalProxySpec) -> Result<LocalProxyHandle, HelperError>;

    async fn wait_ready(&self, process: &HelperProcess) -> Result<(), HelperError>;

    async fn terminate(&self, process: &HelperProcess) -> Result<(), HelperError>;

    async fn kill_process_tree(&self, process: &HelperProcess) -> Result<(), HelperError>;
}

/// In-process registry of live helper processes for cancel / app-exit cleanup.
#[derive(Default)]
pub struct HelperRegistry {
    inner: Mutex<Vec<Arc<HelperProcess>>>,
}

impl HelperRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn track(&self, process: Arc<HelperProcess>) {
        self.inner.lock().await.push(process);
    }

    pub async fn remove(&self, id: &Uuid) {
        self.inner.lock().await.retain(|p| p.id() != id);
    }

    pub async fn snapshot(&self) -> Vec<Arc<HelperProcess>> {
        self.inner.lock().await.clone()
    }

    pub async fn terminate_all(&self, manager: &dyn ExternalHelperManager) {
        let processes = self.snapshot().await;
        for process in processes {
            let _ = manager.kill_process_tree(&process).await;
            self.remove(process.id()).await;
        }
    }
}

/// Default manager used by Teleport / Boundary providers.
#[derive(Clone, Default)]
pub struct DefaultExternalHelperManager {
    registry: Arc<HelperRegistry>,
}

impl DefaultExternalHelperManager {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(HelperRegistry::new()),
        }
    }

    pub fn registry(&self) -> Arc<HelperRegistry> {
        Arc::clone(&self.registry)
    }
}

#[async_trait]
impl ExternalHelperManager for DefaultExternalHelperManager {
    fn locate_binary(&self, names: &[&str]) -> Result<PathBuf, HelperError> {
        process::locate_binary(names)
    }

    fn locate_binary_with_override(
        &self,
        override_path: Option<&str>,
        names: &[&str],
    ) -> Result<PathBuf, HelperError> {
        process::locate_binary_with_override(override_path, names)
    }

    fn check_version(
        &self,
        binary: &PathBuf,
        constraint: &VersionConstraint,
    ) -> Result<HelperVersion, HelperError> {
        version::check_version(binary, constraint)
    }

    async fn spawn(&self, spec: ProcessSpec) -> Result<HelperProcess, HelperError> {
        let process = process::spawn(spec).await?;
        self.registry.track(Arc::new(process.share())).await;
        Ok(process)
    }

    async fn spawn_stdio_proxy(&self, spec: ProcessSpec) -> Result<StdioProxyHandle, HelperError> {
        stdio::spawn_stdio_proxy(self, spec).await
    }

    async fn spawn_local_proxy(&self, spec: LocalProxySpec) -> Result<LocalProxyHandle, HelperError> {
        local_proxy::spawn_local_proxy(self, spec).await
    }

    async fn wait_ready(&self, process: &HelperProcess) -> Result<(), HelperError> {
        lifecycle::wait_ready(process).await
    }

    async fn terminate(&self, process: &HelperProcess) -> Result<(), HelperError> {
        let result = process::terminate(process).await;
        self.registry.remove(process.id()).await;
        result
    }

    async fn kill_process_tree(&self, process: &HelperProcess) -> Result<(), HelperError> {
        let result = process::kill_process_tree(process).await;
        self.registry.remove(process.id()).await;
        result
    }
}
