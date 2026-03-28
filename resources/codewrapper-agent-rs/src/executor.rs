//! Task executor for running backend commands

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::agent_config::resolve_agent_for_runtime_config;
use crate::backend::Backend;
use crate::cli::Cli;
use crate::config::{Config, ParallelConfig, TaskSpec};
use crate::logger::Logger;
use crate::parser::JsonStreamParser;
use crate::signal::setup_signal_handler;

/// Essential environment variables for AI CLI backends
const ESSENTIAL_ENV_VARS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    // AI backend API keys
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "AZURE_OPENAI_API_KEY",
    // Proxy settings
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    // Common development tools
    "NODE_PATH",
    "PYTHONPATH",
    "GEM_PATH",
    "GOPATH",
    // Terminal and display
    "DISPLAY",
    "COLORTERM",
    "TERM_PROGRAM",
    // SSH and auth
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    // Codex/Codeagent specific
    "CODEX_TIMEOUT",
    "CODEX_MODEL",
    "CODEX_BACKEND",
    "CODEAGENT_QUIET",
    "CODEAGENT_ASCII_MODE",
    "CODEAGENT_PERFORMANCE_METRICS",
];

/// Build process environment based on minimal_env setting
fn build_process_env(minimal_env: bool) -> HashMap<String, String> {
    if !minimal_env {
        return std::env::vars().collect();
    }

    // Build minimal environment with only essential variables
    let mut env = HashMap::new();
    for &key in ESSENTIAL_ENV_VARS {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.to_string(), value);
        }
    }

    // Also include any environment variables that start with known prefixes
    let prefixes = [
        "CODEX_",
        "CODEAGENT_",
        "OPENAI_",
        "ANTHROPIC_",
        "GEMINI_",
        "GOOGLE_",
    ];
    for (key, value) in std::env::vars() {
        if prefixes.iter().any(|&prefix| key.starts_with(prefix)) {
            env.insert(key, value);
        }
    }

    env
}

/// Task execution result
#[derive(Debug, Clone, Default)]
pub struct TaskResult {
    /// Whether task succeeded
    pub success: bool,
    /// Exit code
    pub exit_code: i32,
    /// Task duration
    pub duration: Duration,
    /// Session ID (if returned by backend)
    pub session_id: Option<String>,
    /// Parsed events
    pub events: Vec<serde_json::Value>,
    /// Stderr output
    #[allow(dead_code)] // Reserved: stderr will be used for error reporting
    pub stderr: String,
    /// Files changed count
    pub files_changed: Option<usize>,
    /// Coverage percentage
    pub coverage: Option<f64>,
}

/// Task executor
pub struct TaskExecutor {
    backend: Arc<dyn Backend>,
    config: Config,
    #[allow(dead_code)] // Reserved: task-specific logging will be enabled later
    logger: Logger,
}

impl TaskExecutor {
    /// Create a new task executor
    pub fn new(backend: Arc<dyn Backend>, config: &Config) -> Result<Self> {
        Ok(Self {
            backend,
            config: config.clone(),
            logger: Logger::new(None),
        })
    }

    /// Run the task
    pub async fn run(&self) -> Result<TaskResult> {
        let start = Instant::now();

        // Build command arguments
        let task_content = self.get_target()?;
        let use_stdin = should_use_stdin(&task_content);
        let target = if use_stdin {
            "-".to_string()
        } else {
            task_content.clone()
        };
        let args = self.backend.build_args(&self.config, &target);

        info!(
            backend = self.backend.name(),
            args = ?args,
            "Executing task"
        );

        // Build environment (use minimal env if requested for performance)
        let process_env = build_process_env(self.config.minimal_env);

        // Spawn process
        let mut child = Command::new(self.backend.command())
            .args(&args)
            .current_dir(&self.config.work_dir)
            .env_clear()
            .envs(&process_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn {}", self.backend.command()))?;

        // Setup signal handler
        let child_id = child.id().unwrap_or(0);
        let _signal_guard = setup_signal_handler(child_id);

        // Write to stdin if using stdin mode
        if let Some(mut stdin) = child.stdin.take() {
            if use_stdin {
                stdin.write_all(task_content.as_bytes()).await?;
            }
            drop(stdin);
        }

        // Read stdout with JSON parser
        let stdout = child.stdout.take().unwrap();
        let stdout_reader = BufReader::new(stdout);
        let mut parser = JsonStreamParser::new(stdout_reader);

        // Collect stderr in background
        let stderr = child.stderr.take().unwrap();
        let stderr_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            while reader.read_line(&mut buf).await.unwrap_or(0) > 0 {}
            buf
        });

        // Parse events with timeout
        let timeout_duration = Duration::from_secs(self.config.timeout);
        let mut events = Vec::new();
        let mut session_id = None;

        let parse_result = timeout(timeout_duration, async {
            while let Some(event) = parser.next_event().await {
                match event {
                    Ok(value) => {
                        // Extract session ID if present
                        if let Some(id) = extract_session_id(&value) {
                            session_id = Some(id);
                        }
                        events.push(value);
                    }
                    Err(e) => {
                        warn!("Parse error: {}", e);
                    }
                }
            }
        })
        .await;

        if parse_result.is_err() {
            warn!("Task timed out after {} seconds", self.config.timeout);
            let _ = child.kill().await;
        }

        // Wait for process
        let status = child.wait().await?;
        let stderr_output = stderr_handle.await.unwrap_or_default();

        let duration = start.elapsed();
        let exit_code = status.code().unwrap_or(-1);

        info!(
            success = status.success(),
            exit_code = exit_code,
            duration_ms = duration.as_millis(),
            events_count = events.len(),
            "Task completed"
        );

        Ok(TaskResult {
            success: status.success(),
            exit_code,
            duration,
            session_id,
            events,
            stderr: stderr_output,
            files_changed: None,
            coverage: None,
        })
    }

    /// Get the target argument (task or prompt file content)
    fn get_target(&self) -> Result<String> {
        if let Some(ref prompt_file) = self.config.prompt_file {
            std::fs::read_to_string(prompt_file)
                .with_context(|| format!("Failed to read prompt file: {}", prompt_file.display()))
        } else {
            Ok(self.config.task.clone())
        }
    }
}

/// Determine if task should use stdin for input
/// Use stdin for long tasks or tasks with special characters that may cause shell issues
fn should_use_stdin(task: &str) -> bool {
    const STDIN_THRESHOLD: usize = 800;

    // Use stdin for long tasks
    if task.len() > STDIN_THRESHOLD {
        return true;
    }

    // Use stdin for tasks with special characters
    let special_chars = [
        '\'', '"', '`', '$', '\\', '\n', '\r', '|', '&', ';', '<', '>',
    ];
    task.chars().any(|c| special_chars.contains(&c))
}

/// Run tasks in parallel
pub async fn run_parallel_tasks(cli: &Cli, config: ParallelConfig) -> Result<Vec<TaskResult>> {
    use tokio::sync::mpsc;

    let max_workers = cli
        .max_parallel_workers
        .unwrap_or_else(crate::config::get_default_max_parallel_workers);

    debug!(
        task_count = config.tasks.len(),
        max_workers = max_workers,
        "Starting parallel execution"
    );

    // Build dependency graph
    let mut results: HashMap<String, TaskResult> = HashMap::new();
    let mut pending: Vec<TaskSpec> = config.tasks.clone();
    let (tx, mut rx) = mpsc::channel::<(String, TaskResult)>(max_workers);

    let mut running = 0;

    while !pending.is_empty() || running > 0 {
        // Start tasks with satisfied dependencies
        while running < max_workers && !pending.is_empty() {
            let ready_idx = pending.iter().position(|task| {
                task.dependencies
                    .iter()
                    .all(|dep| results.contains_key(dep))
            });

            if let Some(idx) = ready_idx {
                let task = pending.remove(idx);
                let task_id = task.id.clone();
                let tx = tx.clone();
                let cli = cli.clone();

                tokio::spawn(async move {
                    let result = run_single_task(&cli, task).await;
                    let _ = tx.send((task_id, result.unwrap_or_default())).await;
                });

                running += 1;
            } else if running == 0 {
                // No tasks can run and none are running - circular dependency
                return Err(anyhow::anyhow!(
                    "Circular dependency detected in tasks: {:?}",
                    pending.iter().map(|t| &t.id).collect::<Vec<_>>()
                ));
            } else {
                break;
            }
        }

        // Wait for a task to complete
        if running > 0
            && let Some((task_id, result)) = rx.recv().await
        {
            results.insert(task_id, result);
            running -= 1;
        }
    }

    // Return results in original order
    Ok(config
        .tasks
        .iter()
        .filter_map(|t| results.remove(&t.id))
        .collect())
}

/// Run a single task from parallel config
async fn run_single_task(cli: &Cli, spec: TaskSpec) -> Result<TaskResult> {
    let mut config = Config {
        mode: if spec.session_id.is_some() {
            "resume"
        } else {
            "new"
        }
        .to_string(),
        task: spec.task,
        session_id: spec.session_id,
        work_dir: spec
            .work_dir
            .map(Into::into)
            .unwrap_or_else(|| std::env::current_dir().unwrap()),
        model: spec.model.or_else(|| cli.model.clone()),
        backend: spec.backend.or_else(|| cli.backend.clone()),
        agent: spec.agent.or_else(|| cli.agent.clone()),
        prompt_file: spec.prompt_file.map(Into::into),
        timeout: cli.timeout,
        skip_permissions: spec.skip_permissions || cli.skip_permissions,
        reasoning_effort: spec
            .reasoning_effort
            .or_else(|| cli.reasoning_effort.clone()),
        minimal_env: spec.minimal_env || cli.minimal_env,
        quiet: cli.quiet,
        backend_output: cli.backend_output,
        debug: cli.debug,
    };

    resolve_agent_for_runtime_config(&mut config).await?;

    let backend = crate::backend::select_backend(config.backend.as_deref())?;
    let executor = TaskExecutor::new(backend, &config)?;
    executor.run().await
}

/// Extract session ID from a JSON event
fn extract_session_id(value: &serde_json::Value) -> Option<String> {
    value
        .get("session_id")
        .or_else(|| value.get("sessionId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_session_id() {
        let value = serde_json::json!({"session_id": "abc123"});
        assert_eq!(extract_session_id(&value), Some("abc123".to_string()));

        let value = serde_json::json!({"sessionId": "def456"});
        assert_eq!(extract_session_id(&value), Some("def456".to_string()));

        let value = serde_json::json!({"other": "data"});
        assert_eq!(extract_session_id(&value), None);
    }
}
