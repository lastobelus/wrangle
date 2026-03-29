//! Programmatic runner API for downstream orchestration systems.
//!
//! `wrangle-runner` is the first-class library integration surface for `wrangle`.
//! Downstream callers should depend on this crate directly and use the types and
//! functions re-exported here rather than shelling out to the `wrangle` CLI.
//!
//! # Integration model
//!
//! There are two ways to use the runner:
//!
//! 1. **Standalone functions** — stateless free functions for simple use cases.
//!    Start with [`available_backends()`], [`preview_request()`], [`execute_request()`].
//!
//! 2. **[`Runner`] struct** — holds configuration and provides methods for repeated
//!    operations. Preferred when your orchestration system needs to make multiple
//!    calls with the same base configuration.
//!
//! # Quick start
//!
//! ```ignore
//! use wrangle_runner::{Runner, RuntimeConfig, ExecutionRequest, PlaybookInvocation, PlaybookName};
//!
//! // Inspect what backends are available
//! let backends = Runner::available_backends();
//! for b in &backends {
//!     println!("{}: available={}", b.name, b.available);
//! }
//!
//! // Preview and execute a request
//! let config = RuntimeConfig::default();
//! let request = ExecutionRequest::simple("Fix the bug", "/tmp/project");
//! let plan = Runner::new(config).preview(request).await?;
//! let result = Runner::new(config).execute_task("Fix the bug").await?;
//! ```
//!
//! # Stable API surface
//!
//! The following types and functions are considered the stable public API:
//!
//! - [`Runner`] — primary entry point for programmatic usage
//! - [`available_backends()`], [`find_backend()`], [`is_backend_available()`]
//! - [`preview_request()`], [`execute_request()`], [`execute_parallel()`]
//! - [`build_playbook()`], [`build_playbook_plan()`], [`Runner::execute_playbook()`]
//! - [`CommandPreview`], [`ExecutionPlan`], [`PlaybookInvocation`], [`PlaybookPlan`]
//!
//! All types from `wrangle-core` needed for runner usage are re-exported here
//! so downstream crates only need to depend on `wrangle-runner`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task::JoinSet;
use wrangle_backends_cli::{
    backend_capabilities, ensure_permission_supported, ensure_transport_supported,
    select_cli_backend,
};
use wrangle_core::{
    AgentBackend, BackendTransport, ExecutionError, ExecutionRequest, ExecutionResult,
    ParallelTaskSpec, PermissionPolicy, RuntimeConfig, TransportMode, ensure_parallel_tasks,
    get_default_max_parallel_workers, resolve_agent_for_runtime_config,
};
use wrangle_transport::SubprocessTransport;

// Re-export core types so downstream callers only need wrangle-runner.
pub use wrangle_core::{
    BackendCapabilities, BackendDescriptor, BackendKind, CommandSpec, ExecutionEvent,
    ExecutionRequest as CoreExecutionRequest, ExecutionResult as CoreExecutionResult,
    ParallelConfig, ParallelTaskSpec as CoreParallelTaskSpec,
    PermissionPolicy as CorePermissionPolicy, RuntimeConfig as CoreRuntimeConfig, RuntimeMode,
    SessionHandle, SessionState, TransportMode as CoreTransportMode,
};

// ---------------------------------------------------------------------------
// Preview and plan types
// ---------------------------------------------------------------------------

/// A preview of the command that would be executed, without running it.
///
/// Returned by [`preview_request()`] and [`Runner::preview()`] so callers can
/// inspect the exact program, arguments, and working directory before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPreview {
    pub program: String,
    pub args: Vec<String>,
    pub current_dir: PathBuf,
    pub stdin_bytes: usize,
}

/// The full execution plan for a request, including backend capabilities and
/// the command preview.
///
/// This is what a caller gets back from a preview/dry-run so they can decide
/// whether to proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    pub backend: BackendCapabilities,
    pub transport: TransportMode,
    pub request: ExecutionRequest,
    pub command: CommandPreview,
}

// ---------------------------------------------------------------------------
// Named result for parallel execution
// ---------------------------------------------------------------------------

/// An execution result tagged with the task id it corresponds to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedExecutionResult {
    pub id: String,
    pub result: ExecutionResult,
}

// ---------------------------------------------------------------------------
// Playbook types
// ---------------------------------------------------------------------------

/// Well-known playbook names supported by the runner.
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

/// A request to run a named playbook with specific parameters.
///
/// Callers construct this and pass it to [`build_playbook()`],
/// [`build_playbook_plan()`], or [`Runner::execute_playbook()`].
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

/// A playbook plan returned by [`build_playbook_plan()`], suitable for
/// inspection before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybookPlan {
    pub playbook: String,
    pub config: RuntimeConfigSnapshot,
    pub request: ExecutionRequest,
}

/// A serializable snapshot of the resolved runtime configuration.
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

// ---------------------------------------------------------------------------
// Runner — primary entry point
// ---------------------------------------------------------------------------

/// Primary entry point for programmatic use of wrangle.
///
/// Holds a [`RuntimeConfig`] and provides methods for capability inspection,
/// request preview, request execution, and playbook workflows.
///
/// Construct with [`Runner::new()`] or [`Runner::with_defaults()`].
///
/// # Example
///
/// ```ignore
/// let runner = Runner::with_defaults();
/// let backends = runner.available_backends();
/// let plan = runner.preview_task("ship it").await?;
/// let result = runner.execute_task("ship it").await?;
/// ```
pub struct Runner {
    config: RuntimeConfig,
}

impl Runner {
    /// Create a new runner with the given configuration.
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    /// Create a runner with default configuration.
    ///
    /// The default configuration uses the current directory as the working
    /// directory, `OneShotProcess` transport, and `Default` permission policy.
    pub fn with_defaults() -> Self {
        Self::new(RuntimeConfig::default())
    }

    // -- Capability inspection ------------------------------------------------

    /// List all known backends and their capabilities.
    ///
    /// This is a class-level operation that does not depend on runner config.
    /// It is also available as the free function [`available_backends()`].
    pub fn available_backends() -> Vec<BackendCapabilities> {
        available_backends()
    }

    /// Look up a specific backend by name.
    ///
    /// Returns `None` if no backend with the given name exists.
    /// This does not depend on runner config.
    pub fn find_backend(name: &str) -> Option<BackendCapabilities> {
        available_backends().into_iter().find(|b| b.name == name)
    }

    /// Check whether a specific backend is installed and available on this system.
    ///
    /// Returns `false` if the backend name is unknown or if the backend binary
    /// is not found on `PATH`.
    pub fn is_backend_available(name: &str) -> bool {
        Self::find_backend(name).is_some_and(|b| b.available)
    }

    // -- Request preview ------------------------------------------------------

    /// Preview the execution plan for a request without running it.
    ///
    /// Resolves the backend, validates the transport mode, and returns the
    /// full command that would be executed.
    pub async fn preview(&self, request: ExecutionRequest) -> Result<ExecutionPlan> {
        preview_request(self.config.clone(), request).await
    }

    /// Convenience method: preview an execution plan for a simple task string.
    ///
    /// Uses the runner's working directory, model, and permission policy.
    pub async fn preview_task(&self, task: impl Into<String>) -> Result<ExecutionPlan> {
        let request = ExecutionRequest {
            task: task.into(),
            work_dir: self.config.work_dir.clone(),
            model: self.config.model.clone(),
            session: None,
            permission_policy: self.config.permission_policy,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        self.preview(request).await
    }

    // -- Request execution ----------------------------------------------------

    /// Execute a request through the configured backend and transport.
    ///
    /// Returns a structured [`ExecutionResult`] without requiring CLI output parsing.
    pub async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResult> {
        execute_request(self.config.clone(), request).await
    }

    /// Convenience method: execute a simple task string through the configured
    /// backend and transport.
    ///
    /// Uses the runner's working directory, model, and permission policy.
    pub async fn execute_task(&self, task: impl Into<String>) -> Result<ExecutionResult> {
        let request = ExecutionRequest {
            task: task.into(),
            work_dir: self.config.work_dir.clone(),
            model: self.config.model.clone(),
            session: None,
            permission_policy: self.config.permission_policy,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        self.execute(request).await
    }

    // -- Parallel execution ---------------------------------------------------

    /// Execute a set of tasks in parallel with dependency ordering.
    pub async fn execute_parallel(
        &self,
        tasks: Vec<ParallelTaskSpec>,
    ) -> Result<Vec<NamedExecutionResult>> {
        execute_parallel(self.config.clone(), tasks).await
    }

    // -- Playbook workflows ---------------------------------------------------

    /// Build a playbook plan for inspection without executing.
    pub fn plan_playbook(&self, invocation: PlaybookInvocation) -> PlaybookPlan {
        build_playbook_plan(&self.config, invocation)
    }

    /// Build and execute a playbook invocation.
    pub async fn execute_playbook(
        &self,
        invocation: PlaybookInvocation,
    ) -> Result<ExecutionResult> {
        let (config, request) = build_playbook(&self.config, invocation);
        execute_request(config, request).await
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Standalone functions (stateless API)
// ---------------------------------------------------------------------------

/// List all known backends and their capabilities.
///
/// Each entry includes the backend name, supported transport modes,
/// supported permission policies, and whether the backend binary is
/// available on the current system.
pub fn available_backends() -> Vec<BackendCapabilities> {
    backend_capabilities()
}

/// Look up a specific backend by name.
///
/// Returns `None` if no backend with that name is known.
pub fn find_backend(name: &str) -> Option<BackendCapabilities> {
    available_backends().into_iter().find(|b| b.name == name)
}

/// Check whether a specific backend is installed and available on this system.
pub fn is_backend_available(name: &str) -> bool {
    find_backend(name).is_some_and(|b| b.available)
}

/// Build the runtime config and request for a named playbook invocation.
///
/// Returns the resolved config and request so callers can inspect or modify
/// them before passing to [`preview_request()`] or [`execute_request()`].
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

/// Build a playbook plan suitable for inspection and dry-run display.
///
/// Does not execute anything. Returns a [`PlaybookPlan`] containing the
/// resolved config snapshot and the request that would be executed.
pub fn build_playbook_plan(config: &RuntimeConfig, invocation: PlaybookInvocation) -> PlaybookPlan {
    let (config, request) = build_playbook(config, invocation.clone());
    PlaybookPlan {
        playbook: invocation.name.as_str().to_string(),
        config: snapshot_config(&config),
        request,
    }
}

async fn build_execution_plan(
    mut config: RuntimeConfig,
    request: ExecutionRequest,
) -> Result<ExecutionPlan> {
    resolve_agent_for_runtime_config(&mut config).await?;
    let backend = select_cli_backend(config.backend.as_deref())?;
    ensure_transport_supported(&backend, config.transport_mode)?;
    ensure_permission_supported(&backend, request.permission_policy)?;
    let descriptor = backend.descriptor();
    let command = backend.build_command(&config, &request, config.transport_mode)?;

    Ok(ExecutionPlan {
        backend: BackendCapabilities::from_descriptor(&descriptor, backend.is_available()),
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

/// Preview the execution plan for a request without running it.
///
/// Resolves the backend, validates the transport mode, and returns the
/// full [`ExecutionPlan`] including the command that would be executed.
///
/// This is the primary "dry-run" entry point for orchestration callers.
pub async fn preview_request(
    config: RuntimeConfig,
    request: ExecutionRequest,
) -> Result<ExecutionPlan> {
    build_execution_plan(config, request).await
}

/// Execute a request through the configured backend and transport.
///
/// Returns a structured [`ExecutionResult`] with success status, exit code,
/// duration, session info, and parsed events — no CLI output scraping needed.
pub async fn execute_request(
    mut config: RuntimeConfig,
    request: ExecutionRequest,
) -> Result<ExecutionResult> {
    resolve_agent_for_runtime_config(&mut config).await?;
    let backend = select_cli_backend(config.backend.as_deref())?;
    ensure_transport_supported(&backend, config.transport_mode)?;
    ensure_permission_supported(&backend, request.permission_policy)?;

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

/// Execute a set of tasks in parallel with dependency-aware scheduling.
///
/// Tasks are scheduled respecting their `dependencies` fields. The maximum
/// number of concurrent workers is determined by the runtime config or the
/// system default.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RuntimeConfig {
        RuntimeConfig {
            backend: Some("qwen".to_string()),
            work_dir: PathBuf::from("/tmp"),
            ..RuntimeConfig::default()
        }
    }

    // -- Playbook tests -------------------------------------------------------

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
        assert!(request.task.contains("Ship the fix"));
    }

    #[test]
    fn land_work_respects_explicit_agent() {
        let config = RuntimeConfig::default();
        let invocation = PlaybookInvocation {
            name: PlaybookName::LandWork,
            task: "Deploy".to_string(),
            work_dir: PathBuf::from("/tmp"),
            backend: None,
            model: None,
            agent: Some("oracle".to_string()),
            permission_policy: PermissionPolicy::Default,
            transport_mode: TransportMode::OneShotProcess,
        };
        let (config, _request) = build_playbook(&config, invocation);
        assert_eq!(config.agent.as_deref(), Some("oracle"));
    }

    #[test]
    fn playbook_plan_snapshot_matches_playbook_config() {
        let config = test_config();
        let invocation = PlaybookInvocation {
            name: PlaybookName::LandWork,
            task: "Refactor module X".to_string(),
            work_dir: PathBuf::from("/tmp/project"),
            backend: Some("qwen".to_string()),
            model: Some("qwen3".to_string()),
            agent: None,
            permission_policy: PermissionPolicy::Bypass,
            transport_mode: TransportMode::OneShotProcess,
        };
        let plan = build_playbook_plan(&config, invocation.clone());
        assert_eq!(plan.playbook, "land-work");
        assert_eq!(plan.config.backend.as_deref(), Some("qwen"));
        assert_eq!(plan.config.model.as_deref(), Some("qwen3"));
        assert_eq!(plan.config.permission_policy, PermissionPolicy::Bypass);
        assert_eq!(plan.config.agent.as_deref(), Some("develop"));
        assert!(plan.request.task.contains("Refactor module X"));
    }

    #[test]
    fn playbook_invocation_serializes_camel_case() {
        let invocation = PlaybookInvocation {
            name: PlaybookName::LandWork,
            task: "test".to_string(),
            work_dir: PathBuf::from("/tmp"),
            backend: None,
            model: None,
            agent: None,
            permission_policy: PermissionPolicy::Default,
            transport_mode: TransportMode::OneShotProcess,
        };
        let json = serde_json::to_string(&invocation).unwrap();
        assert!(json.contains("\"workDir\""));
        assert!(json.contains("\"transportMode\""));
        assert!(json.contains("\"permissionPolicy\""));
    }

    #[test]
    fn playbook_plan_serializes_camel_case() {
        let config = test_config();
        let invocation = PlaybookInvocation {
            name: PlaybookName::LandWork,
            task: "test".to_string(),
            work_dir: PathBuf::from("/tmp"),
            backend: None,
            model: None,
            agent: None,
            permission_policy: PermissionPolicy::Default,
            transport_mode: TransportMode::OneShotProcess,
        };
        let plan = build_playbook_plan(&config, invocation);
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"playbook\":"));
    }

    // -- Capability inspection tests ------------------------------------------

    #[test]
    fn available_backends_returns_known_backends() {
        let backends = available_backends();
        let names: Vec<&str> = backends.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"qwen"));
        assert!(names.contains(&"opencode"));
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"codex"));
        assert!(names.contains(&"gemini"));
    }

    #[test]
    fn find_backend_returns_known_backend() {
        let backend = find_backend("qwen").unwrap();
        assert_eq!(backend.name, "qwen");
    }

    #[test]
    fn find_backend_returns_none_for_unknown() {
        assert!(find_backend("nonexistent").is_none());
    }

    #[test]
    fn is_backend_available_returns_false_for_unknown() {
        assert!(!is_backend_available("nonexistent"));
    }

    // -- Runner struct tests --------------------------------------------------

    #[test]
    fn runner_with_defaults_uses_default_config() {
        let runner = Runner::with_defaults();
        assert_eq!(runner.config.transport_mode, TransportMode::OneShotProcess);
        assert_eq!(runner.config.permission_policy, PermissionPolicy::Default);
    }

    #[test]
    fn runner_new_accepts_custom_config() {
        let config = RuntimeConfig {
            backend: Some("qwen".to_string()),
            timeout_secs: 60,
            ..RuntimeConfig::default()
        };
        let runner = Runner::new(config);
        assert_eq!(runner.config.backend.as_deref(), Some("qwen"));
        assert_eq!(runner.config.timeout_secs, 60);
    }

    #[test]
    fn runner_available_backends_matches_free_function() {
        let from_struct = Runner::available_backends();
        let from_fn = available_backends();
        assert_eq!(from_struct.len(), from_fn.len());
    }

    #[test]
    fn runner_find_backend_delegates() {
        let backend = Runner::find_backend("claude").unwrap();
        assert_eq!(backend.name, "claude");
        assert!(Runner::find_backend("nonexistent").is_none());
    }

    #[test]
    fn runner_plan_playbook_matches_build_playbook_plan() {
        let config = test_config();
        let runner = Runner::new(config.clone());
        let invocation = PlaybookInvocation {
            name: PlaybookName::LandWork,
            task: "Do the thing".to_string(),
            work_dir: PathBuf::from("/tmp"),
            backend: Some("qwen".to_string()),
            model: None,
            agent: None,
            permission_policy: PermissionPolicy::Default,
            transport_mode: TransportMode::OneShotProcess,
        };
        let from_runner = runner.plan_playbook(invocation.clone());
        let from_fn = build_playbook_plan(&config, invocation);
        assert_eq!(from_runner.playbook, from_fn.playbook);
        assert_eq!(from_runner.config.backend, from_fn.config.backend);
        assert_eq!(from_runner.request.task, from_fn.request.task);
    }

    // -- ExecutionRequest construction tests ----------------------------------

    #[test]
    fn execution_request_fields_are_set_correctly() {
        let request = ExecutionRequest {
            task: "fix the login bug".to_string(),
            work_dir: PathBuf::from("/home/user/project"),
            model: Some("gpt-5".to_string()),
            session: None,
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        assert_eq!(request.task, "fix the login bug");
        assert_eq!(request.work_dir, PathBuf::from("/home/user/project"));
        assert_eq!(request.model.as_deref(), Some("gpt-5"));
        assert!(request.session.is_none());
    }

    #[test]
    fn execution_request_with_session() {
        let request = ExecutionRequest {
            task: "continue".to_string(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: Some(SessionHandle {
                id: "sess-123".to_string(),
                state: SessionState::Resumable,
                transport: TransportMode::OneShotProcess,
            }),
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        assert_eq!(request.session.unwrap().id, "sess-123");
    }

    // -- Transport mode tests -------------------------------------------------

    #[test]
    fn transport_mode_serializes_camel_case() {
        let json = serde_json::to_string(&TransportMode::OneShotProcess).unwrap();
        assert!(json.contains("oneShotProcess"));
    }

    #[test]
    fn permission_policy_serializes_camel_case() {
        let json = serde_json::to_string(&PermissionPolicy::Bypass).unwrap();
        assert!(json.contains("bypass"));
    }

    // -- Preview request test (async, needs backend resolution) ---------------

    #[tokio::test]
    async fn preview_request_builds_plan_for_known_backend() {
        let config = RuntimeConfig {
            backend: Some("qwen".to_string()),
            work_dir: PathBuf::from("/tmp"),
            ..RuntimeConfig::default()
        };
        let request = ExecutionRequest {
            task: "write a hello world".to_string(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: None,
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        let plan = preview_request(config, request).await.unwrap();
        assert_eq!(plan.backend.name, "qwen");
        assert_eq!(plan.transport, TransportMode::OneShotProcess);
        assert_eq!(plan.command.program, "qwen");
        assert!(plan.command.args.contains(&"stream-json".to_string()));
    }

    #[tokio::test]
    async fn preview_request_rejects_unknown_backend() {
        let config = RuntimeConfig {
            backend: Some("nonexistent".to_string()),
            ..RuntimeConfig::default()
        };
        let request = ExecutionRequest {
            task: "test".to_string(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: None,
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        assert!(preview_request(config, request).await.is_err());
    }

    #[tokio::test]
    async fn runner_preview_task_builds_plan() {
        let config = RuntimeConfig {
            backend: Some("qwen".to_string()),
            work_dir: PathBuf::from("/tmp"),
            ..RuntimeConfig::default()
        };
        let runner = Runner::new(config);
        let plan = runner.preview_task("say hello").await.unwrap();
        assert_eq!(plan.backend.name, "qwen");
        assert_eq!(plan.request.task, "say hello");
    }

    #[tokio::test]
    async fn preview_request_includes_stdin_for_long_task() {
        let long_task = "x".repeat(1000);
        let config = RuntimeConfig {
            backend: Some("qwen".to_string()),
            work_dir: PathBuf::from("/tmp"),
            ..RuntimeConfig::default()
        };
        let request = ExecutionRequest {
            task: long_task.clone(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: None,
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        let plan = preview_request(config, request).await.unwrap();
        assert!(plan.command.stdin_bytes > 0);
    }

    #[tokio::test]
    async fn preview_request_with_bypass_includes_flag() {
        let config = RuntimeConfig {
            backend: Some("qwen".to_string()),
            work_dir: PathBuf::from("/tmp"),
            ..RuntimeConfig::default()
        };
        let request = ExecutionRequest {
            task: "test".to_string(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: None,
            permission_policy: PermissionPolicy::Bypass,
            prompt_file: None,
            extra_env: HashMap::new(),
        };
        let plan = preview_request(config, request).await.unwrap();
        assert!(plan.command.args.contains(&"-y".to_string()));
    }

    fn minimal_task_spec(id: &str, task: &str) -> ParallelTaskSpec {
        ParallelTaskSpec {
            id: id.to_string(),
            task: task.to_string(),
            work_dir: None,
            dependencies: vec![],
            session_id: None,
            backend: None,
            model: None,
            agent: None,
            prompt_file: None,
            permission_policy: None,
            transport_mode: None,
        }
    }

    #[test]
    fn parallel_config_rejects_empty_tasks() {
        let config = wrangle_core::ParallelConfig { tasks: vec![] };
        assert!(ensure_parallel_tasks(&config).is_err());
    }

    #[test]
    fn parallel_config_rejects_duplicate_ids() {
        let config = wrangle_core::ParallelConfig {
            tasks: vec![
                minimal_task_spec("a", "task 1"),
                minimal_task_spec("a", "task 2"),
            ],
        };
        assert!(ensure_parallel_tasks(&config).is_err());
    }

    #[test]
    fn parallel_config_rejects_unknown_dependency() {
        let mut spec = minimal_task_spec("a", "task 1");
        spec.dependencies = vec!["nonexistent".to_string()];
        let config = wrangle_core::ParallelConfig { tasks: vec![spec] };
        assert!(ensure_parallel_tasks(&config).is_err());
    }

    #[test]
    fn parallel_config_accepts_valid_tasks() {
        let mut spec_b = minimal_task_spec("b", "task 2");
        spec_b.dependencies = vec!["a".to_string()];
        let config = wrangle_core::ParallelConfig {
            tasks: vec![minimal_task_spec("a", "task 1"), spec_b],
        };
        assert!(ensure_parallel_tasks(&config).is_ok());
    }
}
