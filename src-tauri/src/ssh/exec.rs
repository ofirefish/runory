use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::ChannelMsg;

use crate::domain::{AppError, AppResult};
use crate::ssh::service::HostKeyHandler;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;

pub(crate) struct RemoteCommand {
    program: &'static str,
    args: Vec<String>,
    stdin: Vec<u8>,
    timeout: Duration,
    output_limit: usize,
}

impl RemoteCommand {
    pub(crate) fn program(program: &'static str, args: Vec<String>) -> Self {
        Self {
            program,
            args,
            stdin: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
            output_limit: MAX_OUTPUT_BYTES,
        }
    }

    pub(crate) fn script(script: &'static str, args: Vec<String>) -> Self {
        let mut command_args = vec![script.to_owned(), "runory".to_owned()];
        command_args.extend(args);
        Self::program("sh", vec!["-c".to_owned()])
            .with_args(command_args)
            .with_timeout(Duration::from_secs(30))
    }

    /// Build a command from a raw shell line. The line is passed to `sh -c`
    /// exactly as the user typed it (intentional: an interactive shell would
    /// resolve the same quoting), so it must not be re-quoted here. Only callers
    /// that display the command for review may construct this.
    pub(crate) fn freeform(script: String) -> AppResult<Self> {
        if script.trim().is_empty() || script.len() > 16 * 1024 || script.as_bytes().contains(&0) {
            return Err(AppError::InvalidOperation);
        }
        Ok(Self::program("sh", vec!["-c".to_owned()])
            .with_args(vec![script])
            .with_timeout(Duration::from_secs(120)))
    }

    pub(crate) fn with_stdin(mut self, stdin: Vec<u8>) -> AppResult<Self> {
        if stdin.len() > MAX_INPUT_BYTES {
            return Err(AppError::InvalidOperation);
        }
        self.stdin = stdin;
        Ok(self)
    }

    pub(crate) fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout.min(Duration::from_secs(600));
        self
    }

    pub(crate) fn with_output_limit(mut self, limit: usize) -> Self {
        self.output_limit = limit.min(MAX_OUTPUT_BYTES);
        self
    }

    fn with_args(mut self, args: Vec<String>) -> Self {
        self.args.extend(args);
        self
    }

    fn render(&self) -> AppResult<String> {
        if !is_safe_program(self.program) {
            return Err(AppError::InvalidOperation);
        }
        let mut rendered = self.program.to_owned();
        for argument in &self.args {
            rendered.push(' ');
            rendered.push_str(&shell_quote(argument)?);
        }
        Ok(rendered)
    }
}

pub(crate) struct RemoteExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u32,
}

pub(crate) struct ExecChannel;

impl ExecChannel {
    pub(crate) async fn run(
        client: Arc<client::Handle<HostKeyHandler>>,
        command: RemoteCommand,
    ) -> AppResult<RemoteExecResult> {
        let rendered = command.render()?;
        tokio::time::timeout(command.timeout, async move {
            let mut channel = client
                .channel_open_session()
                .await
                .map_err(|_| AppError::ExecFailed)?;
            channel
                .exec(true, rendered)
                .await
                .map_err(|_| AppError::ExecFailed)?;
            if !command.stdin.is_empty() {
                channel
                    .data(&command.stdin[..])
                    .await
                    .map_err(|_| AppError::ExecFailed)?;
            }
            channel.eof().await.map_err(|_| AppError::ExecFailed)?;

            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_code = None;
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        append_bounded(&mut stdout, &data, command.output_limit)?;
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        append_bounded(&mut stderr, &data, command.output_limit)?;
                    }
                    ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                    // ExitStatus can arrive after EOF; wait until the channel is fully drained.
                    ChannelMsg::Close | ChannelMsg::Eof => {}
                    _ => {}
                }
            }
            Ok(RemoteExecResult {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_code: exit_code.ok_or(AppError::ExecFailed)?,
            })
        })
        .await
        .map_err(|_| AppError::ExecTimedOut)?
    }
}

fn append_bounded(target: &mut Vec<u8>, bytes: &[u8], limit: usize) -> AppResult<()> {
    if target.len().saturating_add(bytes.len()) > limit {
        return Err(AppError::ExecOutputLimit);
    }
    target.extend_from_slice(bytes);
    Ok(())
}

fn is_safe_program(program: &str) -> bool {
    !program.is_empty()
        && program
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn shell_quote(value: &str) -> AppResult<String> {
    if value.contains('\0') || value.len() > 16 * 1024 {
        return Err(AppError::InvalidOperation);
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_arguments_are_posix_quoted() {
        let command = RemoteCommand::program(
            "git",
            vec![
                "-C".into(),
                "/srv/app name".into(),
                "a'; touch /tmp/pwn".into(),
            ],
        );
        assert_eq!(
            command.render().expect("render"),
            "git '-C' '/srv/app name' 'a'\\''; touch /tmp/pwn'"
        );
    }

    #[test]
    fn only_core_owned_program_names_are_rendered() {
        let command = RemoteCommand::program("sh;id", Vec::new());
        assert!(matches!(command.render(), Err(AppError::InvalidOperation)));
    }
}
