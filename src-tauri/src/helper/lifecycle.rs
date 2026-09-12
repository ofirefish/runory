//! Helper readiness / lifecycle helpers.

use std::time::Duration;

use tokio::time::{sleep, timeout};

use super::process::HelperProcess;
use super::HelperError;

/// Marker for lifecycle ownership (RAII-friendly wrapper around terminate).
pub struct HelperLifecycle {
    process: HelperProcess,
}

impl HelperLifecycle {
    pub fn new(process: HelperProcess) -> Self {
        Self { process }
    }

    pub fn process(&self) -> &HelperProcess {
        &self.process
    }

    pub fn into_process(self) -> HelperProcess {
        self.process
    }
}

pub async fn wait_ready(process: &HelperProcess) -> Result<(), HelperError> {
    // Default readiness: process still alive after a short settle window.
    // Providers that need stdout markers should use local_proxy discovery instead.
    let settle = Duration::from_millis(200);
    timeout(Duration::from_secs(5), async {
        sleep(settle).await;
        match process.try_wait().await? {
            Some(_status) => Err(HelperError::Crashed),
            None => Ok(()),
        }
    })
    .await
    .map_err(|_| HelperError::ReadyTimeout)?
}
