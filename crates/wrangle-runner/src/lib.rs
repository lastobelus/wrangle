use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task::JoinSet;
use wrangle_backends_cli::{backend_capabilities, ensure_transport_supported, select_cli_backend};
use wrangle_core::{
    AgentBackend, BackendTransport, ExecutionError, ExecutionRequest, ExecutionResult,
    ParallelTaskSpec, PermissionPolicy, RuntimeConfig, TransportMode, ensure_parallel_tasks,
    get_default_max_parallel_workers, resolve_agent_for_runtime_config,
};
use wrangle_transport::SubprocessTransport;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPreview {
    pub program: String,
    pub args: Vec<String>,
    pub current_dir: PathBuf,
    pub stdin_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    pub backend: wrangle_core::BackendCapabilities,
    pub transport: TransportMode,
    pub request: ExecutionRequest,
    pub command: CommandPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedExecutionResult {
    pub id: String,
    pub result: ExecutionResult,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaybookName {
    LandWork,
}

impl PlaybookName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LandWork => "land-work",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybookInvocation {
    pub name: PlaybookName,
    pub task: String,
    pub work_dir: PathBuf,
    pub backend: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub permission_policy: PermissionPolicy,
    pub transport_mode: TransportMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybookPlan {
    pub playbook: String,
    pub config: RuntimeConfigSnapshot,
    pub request: ExecutionRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeConfigSnapshot {
    pub backend: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub work_dir: PathBuf,
    pub transport_mode: TransportMode,
    pub permission_policy: PermissionPolicy,
}

fn snapshot_config(config: &RuntimeConfig) -> RuntimeConfigSnapshot {
    RuntimeConfigSnapshot {
        backend: config.backend.clone(),
        agent: config.agent.clone(),
        model: config.model.clone(),
        work_dir: config.work_dir.clone(),
        transport_mode: config.transport_mode,
        permission_policy: config.permission_policy,
    }
}

fn build_playbook_task(name: PlaybookName, task: &str) -> String {
    match name {
        PlaybookName::LandWork => format!(
            "Run the `land-work` playbook.\n\nGoal:\n{}\n\nFocus on safely landing the requested work, including implementation, verification, and a concise close-out summary.",
            task
        ),
    }
}

pub fn build_playbook(
    base_config: &RuntimeConfig,
    invocation: PlaybookInvocation,
) -> (RuntimeConfig, ExecutionRequest) {
    let mut config = base_config.clone();
    config.backend = invocation.backend.clone().or(config.backend.clone());
    config.model = invocation.model.clone().or(config.model.clone());
    config.agent = invocation
        .agent
        .clone()
        .or_else(|| Some("develop".to_string()));
    config.work_dir = invocation.work_dir.clone();
    config.transport_mode = invocation.transport_mode;
    config.permission_policy = invocation.permission_policy;

    let request = ExecutionRequest {
        task: build_playbook_task(invocation.name, &invocation.task),
        work_dir: invocation.work_dir,
        model: config.model.clone(),
        session: None,
        permission_policy: config.permission_policy,
        prompt_file: None,
        extra_env: HashMap::new(),
    };

    (config, request)
}

async fn build_execution_plan(
    mut config: RuntimeConfig,
    request: ExecutionRequest,
) -> Result<ExecutionPlan> {
    resolve_agent_for_runtime_config(&mut config).await?;
    let backend = select_cli_backend(config.backend.as_deref())?;
    ensure_transport_supported(&backend, config.transport_mode)?;
    let descriptor = backend.descriptor();
    let command = backend.build_command(&config, &request, config.transport_mode)?;

    Ok(ExecutionPlan {
        backend: wrangle_core::BackendCapabilities::from_descriptor(
            &descriptor,
            backend.is_available(),
        ),
        transport: config.transport_mode,
        request,
        command: CommandPreview {
            program: command.program.to_string(),
            args: command.args,
            current_dir: command.current_dir,
            stdin_bytes: command.stdin.map(|bytes: Vec<u8>| bytes.len()).unwrap_or(0),
        },
    })
}

pub async fn preview_request(
    config: RuntimeConfig,
    request: ExecutionRequest,
) -> Result<ExecutionPlan> {
    build_execution_plan(config, request).await
}

pub async fn execute_request(
    mut config: RuntimeConfig,
    request: ExecutionRequest,
) -> Result<ExecutionResult> {
    resolve_agent_for_runtime_config(&mut config).await?;
    let backend = select_cli_backend(config.backend.as_deref())?;
    ensure_transport_supported(&backend, config.transport_mode)?;

    match config.transport_mode {
        TransportMode::OneShotProcess => {
            let transport = SubprocessTransport;
            transport.execute(&backend, &config, request).await
        }
        TransportMode::PersistentBackend => {
            Err(ExecutionError::UnimplementedTransport("persistent-backend".to_string()).into())
        }
        TransportMode::WrangleServer => {
            Err(ExecutionError::UnimplementedTransport("wrangle-server".to_string()).into())
        }
    }
}

pub async fn execute_parallel(
    mut config: RuntimeConfig,
    tasks: Vec<ParallelTaskSpec>,
) -> Result<Vec<NamedExecutionResult>> {
    let parsed = wrangle_core::ParallelConfig {
        tasks: tasks.clone(),
    };
    ensure_parallel_tasks(&parsed)?;
    resolve_agent_for_runtime_config(&mut config).await?;

    let max_workers = config
        .max_parallel_workers
        .unwrap_or_else(get_default_max_parallel_workers);

    let mut pending = tasks;
    let mut results = HashMap::<String, ExecutionResult>::new();
    let mut running = JoinSet::new();

    while !pending.is_empty() || !running.is_empty() {
        while running.len() < max_workers {
            let Some(index) = pending.iter().position(|task| {
                task.dependencies
                    .iter()
                    .all(|dep| results.contains_key(dep))
            }) else {
                break;
            };

            let spec = pending.remove(index);
            let mut task_config = config.clone();
            task_config.backend = spec.backend.clone().or_else(|| config.backend.clone());
            task_config.agent = spec.agent.clone().or_else(|| config.agent.clone());
            task_config.model = spec.model.clone().or_else(|| config.model.clone());
            task_config.transport_mode = spec.transport_mode.unwrap_or(config.transport_mode);
            task_config.permission_policy =
                spec.permission_policy.unwrap_or(config.permission_policy);
            let task_id = spec.id.clone();
            let request = spec.to_request(&task_config)?;

            running.spawn(async move {
                let result = execute_request(task_config, request).await?;
                Ok::<(String, ExecutionResult), anyhow::Error>((task_id, result))
            });
        }

        let Some(next) = running.join_next().await else {
            if !pending.is_empty() {
                let ids = pending
                    .iter()
                    .map(|task| task.id.clone())
                    .collect::<Vec<_>>();
                return Err(ExecutionError::CircularDependency(ids.join(", ")).into());
            }
            break;
        };

        let (task_id, result) = next??;
        results.insert(task_id, result);

        if !pending.is_empty() && running.is_empty() {
            let ids = pending
                .iter()
                .map(|task| task.id.clone())
                .collect::<Vec<_>>();
            return Err(ExecutionError::CircularDependency(ids.join(", ")).into());
        }
    }

    Ok(parsed
        .tasks
        .iter()
        .filter_map(|task| {
            results.remove(&task.id).map(|result| NamedExecutionResult {
                id: task.id.clone(),
                result,
            })
        })
        .collect())
}

pub fn available_backends() -> Vec<wrangle_core::BackendCapabilities> {
    backend_capabilities()
}

pub fn build_playbook_plan(config: &RuntimeConfig, invocation: PlaybookInvocation) -> PlaybookPlan {
    let (config, request) = build_playbook(config, invocation.clone());
    PlaybookPlan {
        playbook: invocation.name.as_str().to_string(),
        config: snapshot_config(&config),
        request,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn land_work_sets_develop_agent_and_prompt() {
        let config = RuntimeConfig::default();
        let invocation = PlaybookInvocation {
            name: PlaybookName::LandWork,
            task: "Ship the fix".to_string(),
            work_dir: PathBuf::from("/tmp"),
            backend: Some("qwen".to_string()),
            model: None,
            agent: None,
            permission_policy: PermissionPolicy::Default,
            transport_mode: TransportMode::OneShotProcess,
        };
        let (config, request) = build_playbook(&config, invocation);
        assert_eq!(config.agent.as_deref(), Some("develop"));
        assert!(request.task.contains("land-work"));
    }
}
