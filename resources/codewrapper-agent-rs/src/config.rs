//! Configuration parsing and validation

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::cli::Cli;
use crate::errors::ConfigError;

/// Runtime configuration
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Execution mode: "new" or "resume"
    pub mode: String,
    /// Task content
    pub task: String,
    /// Session ID for resume mode
    pub session_id: Option<String>,
    /// Working directory
    pub work_dir: PathBuf,
    /// Model name
    pub model: Option<String>,
    /// Backend name
    pub backend: Option<String>,
    /// Agent configuration name
    pub agent: Option<String>,
    /// Prompt file path
    pub prompt_file: Option<PathBuf>,
    /// Timeout in seconds
    pub timeout: u64,
    /// Skip permission checks
    pub skip_permissions: bool,
    /// Reasoning effort level (for Codex backend)
    pub reasoning_effort: Option<String>,
    /// Use minimal environment variables (performance optimization)
    pub minimal_env: bool,
    /// Quiet mode
    pub quiet: bool,
    /// Show backend output
    pub backend_output: bool,
    /// Debug mode
    pub debug: bool,
}

impl Config {
    /// Create config from CLI arguments for a new task
    pub fn from_cli(cli: &Cli, task: &str) -> Result<Self> {
        let work_dir = cli
            .workdir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        Ok(Self {
            mode: "new".to_string(),
            task: task.to_string(),
            session_id: None,
            work_dir,
            model: cli.model.clone(),
            backend: cli.backend.clone(),
            agent: cli.agent.clone(),
            prompt_file: cli.prompt_file.as_ref().map(PathBuf::from),
            timeout: cli.timeout,
            skip_permissions: cli.skip_permissions,
            reasoning_effort: cli.reasoning_effort.clone(),
            minimal_env: cli.minimal_env,
            quiet: cli.quiet,
            backend_output: cli.backend_output || cli.debug,
            debug: cli.debug,
        })
    }

    /// Create config from CLI arguments for resume mode
    pub fn from_resume(
        cli: &Cli,
        session_id: &str,
        task: &str,
        workdir: Option<&str>,
    ) -> Result<Self> {
        let work_dir = workdir
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        // Validate session ID format
        if !is_valid_session_id(session_id) {
            return Err(ConfigError::InvalidSessionId(session_id.to_string()).into());
        }

        Ok(Self {
            mode: "resume".to_string(),
            task: task.to_string(),
            session_id: Some(session_id.to_string()),
            work_dir,
            model: cli.model.clone(),
            backend: cli.backend.clone(),
            agent: cli.agent.clone(),
            prompt_file: cli.prompt_file.as_ref().map(PathBuf::from),
            timeout: cli.timeout,
            skip_permissions: cli.skip_permissions,
            reasoning_effort: cli.reasoning_effort.clone(),
            minimal_env: cli.minimal_env,
            quiet: cli.quiet,
            backend_output: cli.backend_output || cli.debug,
            debug: cli.debug,
        })
    }
}

/// Task specification for parallel mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Task ID
    pub id: String,
    /// Task content
    pub task: String,
    /// Working directory
    #[serde(default, rename = "workDir")]
    pub work_dir: Option<String>,
    /// Dependencies (other task IDs)
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Session ID
    #[serde(default, rename = "sessionId")]
    pub session_id: Option<String>,
    /// Backend name
    #[serde(default)]
    pub backend: Option<String>,
    /// Model name
    #[serde(default)]
    pub model: Option<String>,
    /// Agent name
    #[serde(default)]
    pub agent: Option<String>,
    /// Prompt file path
    #[serde(default, rename = "promptFile")]
    pub prompt_file: Option<String>,
    /// Skip permissions
    #[serde(default, rename = "skipPermissions")]
    pub skip_permissions: bool,
    /// Reasoning effort level
    #[serde(default, rename = "reasoningEffort")]
    pub reasoning_effort: Option<String>,
    /// Use minimal environment variables
    #[serde(default, rename = "minimalEnv")]
    pub minimal_env: bool,
}

/// Parallel execution configuration
#[derive(Debug, Clone, Default)]
pub struct ParallelConfig {
    /// List of tasks
    pub tasks: Vec<TaskSpec>,
}

/// Parse parallel config from stdin
pub async fn parse_parallel_config() -> Result<ParallelConfig> {
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut tasks = Vec::new();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let task: TaskSpec = serde_json::from_str(line)
            .with_context(|| format!("Failed to parse task: {}", line))?;
        tasks.push(task);
    }

    Ok(ParallelConfig { tasks })
}

/// Validate session ID format
fn is_valid_session_id(session_id: &str) -> bool {
    // Session ID should be alphanumeric with optional hyphens/underscores
    !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Get default max parallel workers based on CPU count
pub fn get_default_max_parallel_workers() -> usize {
    let cpu_count = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1);
    let adaptive = cpu_count * 4;
    adaptive.clamp(1, 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_session_id() {
        assert!(is_valid_session_id("abc123"));
        assert!(is_valid_session_id("session-123"));
        assert!(is_valid_session_id("session_456"));
        assert!(!is_valid_session_id(""));
        assert!(!is_valid_session_id("invalid session"));
    }

    #[test]
    fn test_task_spec_parsing() {
        let json = r#"{"id": "task1", "task": "Test task", "dependencies": ["task0"]}"#;
        let spec: TaskSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.id, "task1");
        assert_eq!(spec.task, "Test task");
        assert_eq!(spec.dependencies, vec!["task0"]);
    }

    #[test]
    fn test_default_max_parallel_workers() {
        let workers = get_default_max_parallel_workers();
        assert!(workers >= 1);
        assert!(workers <= 100);
    }
}
