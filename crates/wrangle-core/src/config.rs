use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::errors::ConfigError;
use crate::protocol::{PermissionPolicy, SessionHandle, SessionState, TransportMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeMode {
    #[default]
    New,
    Resume,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub mode: RuntimeMode,
    pub backend: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub work_dir: PathBuf,
    pub timeout_secs: u64,
    pub quiet: bool,
    pub debug: bool,
    pub transport_mode: TransportMode,
    pub permission_policy: PermissionPolicy,
    pub allow_task_prompt_files: bool,
    pub inherit_env: bool,
    pub max_events: usize,
    pub max_stderr_bytes: usize,
    pub max_parallel_workers: Option<usize>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            mode: RuntimeMode::New,
            backend: None,
            agent: None,
            model: None,
            work_dir: std::env::current_dir().unwrap_or_default(),
            timeout_secs: 7200,
            quiet: false,
            debug: false,
            transport_mode: TransportMode::OneShotProcess,
            permission_policy: PermissionPolicy::Default,
            allow_task_prompt_files: false,
            inherit_env: false,
            max_events: 512,
            max_stderr_bytes: 32 * 1024,
            max_parallel_workers: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParallelTaskSpec {
    pub id: String,
    pub task: String,
    #[serde(default)]
    pub work_dir: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub prompt_file: Option<String>,
    #[serde(default)]
    pub permission_policy: Option<PermissionPolicy>,
    #[serde(default)]
    pub transport_mode: Option<TransportMode>,
}

impl ParallelTaskSpec {
    pub fn to_request(&self, base: &RuntimeConfig) -> Result<crate::protocol::ExecutionRequest> {
        if self.prompt_file.is_some() && !base.allow_task_prompt_files {
            return Err(ConfigError::TaskPromptFilesDisabled.into());
        }

        let session = self.session_id.as_ref().map(|id| SessionHandle {
            id: id.clone(),
            state: SessionState::Resumable,
            transport: self.transport_mode.unwrap_or(base.transport_mode),
        });

        Ok(crate::protocol::ExecutionRequest {
            task: self.task.clone(),
            work_dir: self
                .work_dir
                .as_ref()
                .map(PathBuf::from)
                .unwrap_or_else(|| base.work_dir.clone()),
            model: self.model.clone().or_else(|| base.model.clone()),
            session,
            permission_policy: self.permission_policy.unwrap_or(base.permission_policy),
            prompt_file: self.prompt_file.as_ref().map(PathBuf::from),
            extra_env: Default::default(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParallelConfig {
    pub tasks: Vec<ParallelTaskSpec>,
}

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

        let task: ParallelTaskSpec = serde_json::from_str(line)
            .with_context(|| format!("failed to parse task spec: {}", line))?;
        tasks.push(task);
    }

    Ok(ParallelConfig { tasks })
}

pub fn is_valid_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 128
        && session_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

pub fn make_resume_session(id: &str, transport: TransportMode) -> Result<SessionHandle> {
    if !is_valid_session_id(id) {
        return Err(ConfigError::InvalidSessionId(id.to_string()).into());
    }

    Ok(SessionHandle {
        id: id.to_string(),
        state: SessionState::Resumable,
        transport,
    })
}

pub async fn read_stdin_task() -> Result<String> {
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut content = Vec::new();

    while let Some(line) = lines.next_line().await? {
        content.push(line);
    }

    let task = content.join("\n");
    if task.trim().is_empty() {
        return Err(ConfigError::EmptyTaskFromStdin.into());
    }

    Ok(task)
}

pub fn ensure_parallel_tasks(config: &ParallelConfig) -> Result<()> {
    if config.tasks.is_empty() {
        bail!("no tasks provided for parallel execution");
    }
    Ok(())
}

pub fn get_default_max_parallel_workers() -> usize {
    let cpu_count = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1);
    (cpu_count * 4).clamp(1, 100)
}

