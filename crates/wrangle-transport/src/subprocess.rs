use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::fs::{OpenOptions, create_dir_all};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};
use wrangle_core::{
    AgentBackend, BackendTransport, ExecutionEvent, ExecutionRequest, ExecutionResult,
    RuntimeConfig, SessionHandle, SessionState, TransportMode,
};

use crate::parser::JsonLineParser;
use crate::signal::setup_signal_handler;

pub(crate) const TRANSPORT_LABEL_ONE_SHOT: &str = "one-shot-process";
pub(crate) const TRANSPORT_LABEL_PERSISTENT: &str = "persistent-backend";

const REDUCED_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSH_AUTH_SOCK",
];

#[derive(Debug, Default)]
pub struct SubprocessTransport;

pub(crate) struct ProgressWriter {
    file: Option<tokio::fs::File>,
}

impl ProgressWriter {
    pub(crate) async fn new(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self { file: None });
        };

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            create_dir_all(parent).await?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .with_context(|| format!("failed to open progress file: {}", path.display()))?;

        Ok(Self { file: Some(file) })
    }

    async fn write_json(&mut self, value: &serde_json::Value) -> Result<()> {
        let Some(file) = self.file.as_mut() else {
            return Ok(());
        };
        let line = serde_json::to_vec(value)?;
        file.write_all(&line).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }
}

pub(crate) fn build_process_env(
    config: &RuntimeConfig,
    extra: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env = if config.inherit_env {
        std::env::vars().collect::<HashMap<_, _>>()
    } else {
        let mut env = HashMap::new();
        for &key in REDUCED_ENV_VARS {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.to_string(), value);
            }
        }
        env
    };

    for (key, value) in extra {
        env.insert(key.clone(), value.clone());
    }
    env
}

fn should_use_stdin(task: &str) -> bool {
    task.len() > 800
        || task.chars().any(|c| {
            [
                '\'', '"', '`', '$', '\\', '\n', '\r', '|', '&', ';', '<', '>',
            ]
            .contains(&c)
        })
}

pub(crate) fn extract_session_id(value: &serde_json::Value) -> Option<String> {
    value
        .get("session_id")
        .or_else(|| value.get("sessionId"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

pub(crate) fn truncate_push(buf: &mut String, line: &str, max_bytes: usize, truncated: &mut bool) {
    if buf.len() >= max_bytes {
        *truncated = true;
        return;
    }
    let remaining = max_bytes - buf.len();
    if line.len() <= remaining {
        buf.push_str(line);
    } else {
        let mut taken = 0usize;
        for ch in line.chars() {
            let ch_len = ch.len_utf8();
            if taken + ch_len > remaining {
                break;
            }
            taken += ch_len;
        }
        if taken > 0 {
            buf.push_str(&line[..taken]);
        }
        *truncated = true;
    }
}

pub(crate) async fn run_subprocess(
    backend: &dyn AgentBackend,
    config: &RuntimeConfig,
    request: ExecutionRequest,
    transport_mode: TransportMode,
    session_state: SessionState,
    transport_label: &'static str,
) -> Result<ExecutionResult> {
    let started = Instant::now();
    let command = backend.build_command(config, &request, transport_mode)?;
    let mut progress = ProgressWriter::new(config.progress_file.as_deref()).await?;

    info!(
        backend = backend.descriptor().name,
        transport = transport_label,
        work_dir = %command.current_dir.display(),
        has_session = request.session.is_some(),
        stdin = command.stdin.is_some(),
        "Executing backend request"
    );

    progress
        .write_json(&serde_json::json!({
            "type": "lifecycle",
            "state": "started",
            "backend": backend.descriptor().name,
            "transport": transport_label,
            "workDir": command.current_dir.display().to_string(),
            "hasSession": request.session.is_some(),
        }))
        .await?;

    let mut child = Command::new(command.program)
        .args(&command.args)
        .current_dir(&command.current_dir)
        .env_clear()
        .envs(build_process_env(config, &command.env))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", command.program))?;

    let child_id = child.id().unwrap_or(0);
    let _signal_guard = setup_signal_handler(child_id);

    if let Some(input) = command.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&input).await?;
        }
    } else {
        drop(child.stdin.take());
    }

    let stdout = child.stdout.take().context("missing stdout pipe")?;
    let stderr = child.stderr.take().context("missing stderr pipe")?;

    let stderr_limit = config.max_stderr_bytes;
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        let mut collected = String::new();
        let mut truncated = false;
        loop {
            line.clear();
            let read = reader.read_line(&mut line).await.unwrap_or(0);
            if read == 0 {
                break;
            }
            truncate_push(&mut collected, &line, stderr_limit, &mut truncated);
            if truncated {
                break;
            }
        }
        (collected, truncated)
    });

    let mut parser = JsonLineParser::new(BufReader::new(stdout));
    let mut events = Vec::new();
    let mut session = request.session.clone();
    let timeout_secs = config.timeout_secs;
    let backend_kind = backend.descriptor().kind;
    let max_events = config.max_events;
    let backend_name = backend.descriptor().name;

    let parse = timeout(Duration::from_secs(timeout_secs), async {
        while let Some(item) = parser.next_event().await {
            match item {
                Ok(value) => {
                    progress
                        .write_json(&serde_json::json!({
                            "type": "backendEvent",
                            "backend": backend_name,
                            "transport": transport_label,
                            "payload": value,
                        }))
                        .await?;
                    if events.len() < max_events {
                        if let Some(id) = extract_session_id(&value) {
                            session = Some(SessionHandle {
                                id,
                                state: session_state.clone(),
                                transport: transport_mode,
                            });
                        }
                        events.push(ExecutionEvent {
                            backend: backend_kind,
                            transport: transport_mode,
                            payload: value,
                        });
                    }
                }
                Err(err) => {
                    progress
                        .write_json(&serde_json::json!({
                            "type": "parseError",
                            "backend": backend_name,
                            "message": err.to_string(),
                        }))
                        .await?;
                    warn!(backend = backend_name, error = %err, "Failed to parse backend output");
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    })
    .await;

    match parse {
        Ok(Ok(())) => {}
        Ok(Err(err)) => return Err(err),
        Err(_) => {
            progress
                .write_json(&serde_json::json!({
                    "type": "lifecycle",
                    "state": "timeout",
                    "backend": backend_name,
                    "transport": transport_label,
                    "timeoutSecs": timeout_secs,
                }))
                .await?;
            warn!(backend = backend_name, timeout_secs, "Task timed out");
            let _ = child.kill().await;
        }
    }

    let status = child.wait().await?;
    let exit_code = status.code().unwrap_or(-1);
    let (stderr_excerpt, stderr_truncated) = stderr_task.await.unwrap_or_default();

    if let Some(stderr) = (!stderr_excerpt.is_empty()).then_some(stderr_excerpt.as_str()) {
        progress
            .write_json(&serde_json::json!({
                "type": "stderrSummary",
                "backend": backend_name,
                "transport": transport_label,
                "truncated": stderr_truncated,
                "excerpt": stderr,
            }))
            .await?;
    }

    progress
        .write_json(&serde_json::json!({
            "type": "lifecycle",
            "state": "completed",
            "backend": backend_name,
            "transport": transport_label,
            "success": status.success(),
            "exitCode": exit_code,
            "durationMs": started.elapsed().as_millis(),
        }))
        .await?;

    Ok(ExecutionResult {
        success: status.success(),
        exit_code,
        duration_ms: started.elapsed().as_millis(),
        backend: backend_kind,
        transport: transport_mode,
        session,
        events,
        stderr_truncated,
        stderr_excerpt: if stderr_excerpt.is_empty() {
            None
        } else {
            Some(stderr_excerpt)
        },
    })
}

#[async_trait]
impl BackendTransport for SubprocessTransport {
    fn mode(&self) -> TransportMode {
        TransportMode::OneShotProcess
    }

    async fn execute(
        &self,
        backend: &dyn AgentBackend,
        config: &RuntimeConfig,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult> {
        run_subprocess(
            backend,
            config,
            request,
            TransportMode::OneShotProcess,
            SessionState::Resumable,
            TRANSPORT_LABEL_ONE_SHOT,
        )
        .await
    }
}

pub fn request_to_target(request: &ExecutionRequest) -> Result<(String, Option<Vec<u8>>)> {
    if let Some(prompt_file) = &request.prompt_file {
        let content = std::fs::read_to_string(prompt_file)
            .with_context(|| format!("failed to read prompt file: {}", prompt_file.display()))?;
        if should_use_stdin(&content) {
            return Ok(("-".to_string(), Some(content.into_bytes())));
        }
        return Ok((content, None));
    }

    if should_use_stdin(&request.task) {
        return Ok(("-".to_string(), Some(request.task.clone().into_bytes())));
    }

    Ok((request.task.clone(), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn progress_writer_appends_json_lines() {
        let path = std::env::temp_dir().join(format!(
            "wrangle-progress-writer-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut writer = ProgressWriter::new(Some(path.as_path())).await.unwrap();

        writer
            .write_json(&serde_json::json!({"type":"test","value":1}))
            .await
            .unwrap();
        writer
            .write_json(&serde_json::json!({"type":"test","value":2}))
            .await
            .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"value\":1"));
        assert!(content.contains("\"value\":2"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn uses_stdin_for_shell_sensitive_input() {
        assert!(should_use_stdin("echo $HOME"));
        assert!(!should_use_stdin("simple task"));
    }
}
