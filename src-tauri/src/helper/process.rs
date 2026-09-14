//! Process spawning and tree cleanup for vendor CLIs.

use std::path::PathBuf;
use std::process::Stdio as StdStdio;
use std::sync::Arc;

use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::HelperError;

/// Spec for spawning a helper without embedding secrets in argv logging.
#[derive(Clone, Debug)]
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub stdin: bool,
    pub stdout: bool,
    pub stderr: bool,
}

impl ProcessSpec {
    pub fn new(executable: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            executable: executable.into(),
            args,
            working_directory: None,
            env: Vec::new(),
            stdin: false,
            stdout: true,
            stderr: true,
        }
    }
}

/// Live helper process handle. Drop does not auto-kill; callers must terminate.
#[derive(Debug)]
pub struct HelperProcess {
    id: Uuid,
    executable: PathBuf,
    pid: Option<u32>,
    child: Arc<Mutex<Option<Child>>>,
}

impl HelperProcess {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn executable(&self) -> &PathBuf {
        &self.executable
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub async fn take_child(&self) -> Option<Child> {
        self.child.lock().await.take()
    }

    pub async fn restore_child(&self, child: Child) {
        *self.child.lock().await = Some(child);
    }

    /// Shared child slot for registry tracking without duplicating the OS process.
    pub(crate) fn child_arc(&self) -> Arc<Mutex<Option<Child>>> {
        Arc::clone(&self.child)
    }

    /// Build a registry twin that shares the same Child slot.
    pub fn share(&self) -> Self {
        Self {
            id: self.id,
            executable: self.executable.clone(),
            pid: self.pid,
            child: Arc::clone(&self.child),
        }
    }

    pub async fn try_wait(&self) -> Result<Option<std::process::ExitStatus>, HelperError> {
        let mut guard = self.child.lock().await;
        match guard.as_mut() {
            Some(child) => child.try_wait().map_err(|_| HelperError::Crashed),
            None => Ok(None),
        }
    }
}

pub fn locate_binary(names: &[&str]) -> Result<PathBuf, HelperError> {
    locate_binary_with_override(None, names)
}

/// Prefer an explicit absolute/relative path from provider config, then PATH.
pub fn locate_binary_with_override(
    override_path: Option<&str>,
    names: &[&str],
) -> Result<PathBuf, HelperError> {
    if let Some(raw) = override_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(path);
        }
        #[cfg(windows)]
        {
            if !raw.to_ascii_lowercase().ends_with(".exe") {
                let with_exe = PathBuf::from(format!("{raw}.exe"));
                if with_exe.is_file() {
                    return Ok(with_exe);
                }
            }
        }
        tracing::warn!(
            path = %path.display(),
            "configured helper CLI path was not found"
        );
        return Err(HelperError::Missing);
    }
    for name in names {
        if let Ok(path) = which_binary(name) {
            return Ok(path);
        }
    }
    // Common Windows install locations when PATH was not updated.
    #[cfg(windows)]
    {
        if let Some(path) = windows_common_helper_paths(names)
            .into_iter()
            .find(|candidate| candidate.is_file())
        {
            return Ok(path);
        }
    }
    Err(HelperError::Missing)
}

#[cfg(windows)]
fn windows_common_helper_paths(names: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let local = std::env::var_os("LOCALAPPDATA");
    let program_files = std::env::var_os("ProgramFiles");
    for name in names {
        let exe = if name.to_ascii_lowercase().ends_with(".exe") {
            name.to_string()
        } else {
            format!("{name}.exe")
        };
        if let Some(ref local) = local {
            out.push(PathBuf::from(local).join("Programs").join(name).join(&exe));
            out.push(PathBuf::from(local).join(name).join(&exe));
        }
        if let Some(ref pf) = program_files {
            out.push(
                PathBuf::from(pf)
                    .join("HashiCorp")
                    .join("Boundary")
                    .join(&exe),
            );
            out.push(PathBuf::from(pf).join("Teleport").join(&exe));
            out.push(PathBuf::from(pf).join(name).join(&exe));
        }
    }
    out
}

fn which_binary(name: &str) -> Result<PathBuf, HelperError> {
    let path_env = std::env::var_os("PATH").ok_or(HelperError::Missing)?;
    for dir in std::env::split_paths(&path_env) {
        #[cfg(windows)]
        {
            // Prefer a real .exe over bare name / .CMD shims (ConPTY + WinGet Links).
            let with_exe = dir.join(format!("{name}.exe"));
            if with_exe.is_file() {
                return Ok(with_exe);
            }
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(HelperError::Missing)
}

pub async fn spawn(spec: ProcessSpec) -> Result<HelperProcess, HelperError> {
    let mut command = Command::new(&spec.executable);
    command.args(&spec.args);
    if let Some(dir) = &spec.working_directory {
        command.current_dir(dir);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    command.stdin(if spec.stdin {
        StdStdio::piped()
    } else {
        StdStdio::null()
    });
    command.stdout(if spec.stdout {
        StdStdio::piped()
    } else {
        StdStdio::null()
    });
    command.stderr(if spec.stderr {
        StdStdio::piped()
    } else {
        StdStdio::null()
    });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.as_std_mut().creation_flags(0x0800_0000);
    }

    let child = command.spawn().map_err(|error| {
        tracing::warn!(
            executable = %spec.executable.display(),
            error = %error,
            "helper spawn failed"
        );
        HelperError::SpawnFailed
    })?;

    let id = Uuid::new_v4();
    let pid = child.id();
    tracing::info!(
        helper_id = %id,
        executable = %spec.executable.display(),
        pid = ?pid,
        "helper process spawned"
    );

    Ok(HelperProcess {
        id,
        executable: spec.executable,
        pid,
        child: Arc::new(Mutex::new(Some(child))),
    })
}

pub async fn terminate(process: &HelperProcess) -> Result<(), HelperError> {
    kill_process_tree(process).await
}

pub async fn kill_process_tree(process: &HelperProcess) -> Result<(), HelperError> {
    let Some(pid) = process.pid() else {
        let _ = process.take_child().await;
        return Ok(());
    };

    #[cfg(windows)]
    let result = kill_windows_process_tree(pid).await;
    #[cfg(not(windows))]
    let result = kill_unix_process_group(pid).await;

    let _ = process.take_child().await;
    result
}

#[cfg(windows)]
async fn kill_windows_process_tree(pid: u32) -> Result<(), HelperError> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(StdStdio::null())
        .stdout(StdStdio::null())
        .stderr(StdStdio::null())
        .status()
        .await
        .map_err(|_| HelperError::Crashed)?;
    if status.success() || status.code() == Some(128) {
        Ok(())
    } else {
        Err(HelperError::Crashed)
    }
}

#[cfg(not(windows))]
async fn kill_unix_process_group(pid: u32) -> Result<(), HelperError> {
    let status = Command::new("kill")
        .args(["-TERM", &format!("-{pid}")])
        .stdin(StdStdio::null())
        .stdout(StdStdio::null())
        .stderr(StdStdio::null())
        .status()
        .await;
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stdin(StdStdio::null())
                .stdout(StdStdio::null())
                .stderr(StdStdio::null())
                .status()
                .await;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_missing_binary_returns_missing() {
        let err = locate_binary(&["runory-definitely-missing-helper-xyz"]).unwrap_err();
        assert_eq!(err, HelperError::Missing);
    }
}
