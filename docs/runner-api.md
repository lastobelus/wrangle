# Runner API Guide

`wrangle-runner` is the first-class library integration surface for `wrangle`.
Downstream orchestration systems and playbook runners should depend on this crate
directly instead of shelling out to the `wrangle` CLI.

## Setup

Add to your `Cargo.toml`:

```toml
[dependencies]
wrangle-runner = { path = "../path/to/wrangle/crates/wrangle-runner" }
```

All types needed for runner usage are re-exported from `wrangle-runner`, so you
do not need separate dependencies on `wrangle-core` or `wrangle-transport`.

## Usage patterns

There are two ways to use the runner API:

1. **Standalone functions** — for simple, stateless calls
2. **`Runner` struct** — holds configuration and is preferred for repeated calls

### Inspect backend capabilities

```rust
use wrangle_runner::{available_backends, find_backend, is_backend_available};

// List all known backends and what they support
let backends = available_backends();
for b in &backends {
    println!("{}: available={}, transports={:?}, permissions={:?}",
        b.name, b.available, b.transport_modes, b.supported_permission_policies);
}

// Check a specific backend
if let Some(backend) = find_backend("qwen") {
    println!("qwen supports resume: {}", backend.supports_resume);
}

// Quick availability check
if is_backend_available("claude") {
    println!("claude is installed and on PATH");
}
```

### Preview a request (dry-run)

Preview shows the exact command that would be executed without running it:

```rust
use wrangle_runner::{Runner, RuntimeConfig, ExecutionRequest, PermissionPolicy, TransportMode};
use std::path::PathBuf;

let config = RuntimeConfig {
    backend: Some("qwen".to_string()),
    work_dir: PathBuf::from("/my/project"),
    permission_policy: PermissionPolicy::Default,
    transport_mode: TransportMode::OneShotProcess,
    ..RuntimeConfig::default()
};

let request = ExecutionRequest {
    task: "fix the login bug".to_string(),
    work_dir: PathBuf::from("/my/project"),
    model: None,
    session: None,
    permission_policy: PermissionPolicy::Default,
    prompt_file: None,
    extra_env: Default::default(),
};

let plan = Runner::new(config).preview(request).await?;
println!("program: {}", plan.command.program);
println!("args: {:?}", plan.command.args);
println!("backend: {} (available: {})", plan.backend.name, plan.backend.available);
```

### Execute a request

```rust
use wrangle_runner::{ExecutionRequest, PermissionPolicy, Runner, RuntimeConfig};

let runner = Runner::new(RuntimeConfig {
    backend: Some("qwen".to_string()),
    work_dir: std::path::PathBuf::from("/my/project"),
    ..RuntimeConfig::default()
});

// Simple task string
let result = runner.execute_task("write a hello world program").await?;
println!("success: {}, exit_code: {}, duration_ms: {}",
    result.success, result.exit_code, result.duration_ms);

// Structured request with model override
let request = ExecutionRequest {
    task: "refactor the auth module".to_string(),
    work_dir: std::path::PathBuf::from("/my/project"),
    model: Some("qwen3".to_string()),
    session: None,
    permission_policy: PermissionPolicy::Bypass,
    prompt_file: None,
    extra_env: Default::default(),
};
let result = runner.execute(request).await?;
if !result.success {
    eprintln!("stderr: {:?}", result.stderr_excerpt);
}
```

### Quiet mode and progress file

When running inside an automation host that should avoid intermediate stderr
noise, set `quiet_until_complete` and provide a `progress_file` path:

```rust
use std::path::PathBuf;
use wrangle_runner::RuntimeConfig;

let config = RuntimeConfig {
    backend: Some("qwen".to_string()),
    quiet_until_complete: true,
    progress_file: Some(PathBuf::from("/tmp/wrangle-progress.jsonl")),
    ..RuntimeConfig::default()
};
```

With this config, `wrangle` keeps its own stderr logging suppressed until the
final result is ready, writes intermediate backend events as JSON Lines to the
progress file, and still emits the final JSON result on stdout once execution
completes.

### Build and run a playbook

Playbooks are named workflows with opinionated prompts and agent defaults:

```rust
use wrangle_runner::{Runner, RuntimeConfig, PlaybookInvocation, PlaybookName};
use std::path::PathBuf;

let runner = Runner::new(RuntimeConfig {
    backend: Some("codex".to_string()),
    ..RuntimeConfig::default()
});

let invocation = PlaybookInvocation {
    name: PlaybookName::LandWork,
    task: "Ship the authentication refactor".to_string(),
    work_dir: PathBuf::from("/my/project"),
    backend: Some("codex".to_string()),
    model: None,
    agent: None, // defaults to "develop" for land-work
    permission_policy: PermissionPolicy::Default,
    transport_mode: TransportMode::OneShotProcess,
};

// Preview the playbook plan without executing
let plan = runner.plan_playbook(invocation.clone());
println!("playbook: {}", plan.playbook);
println!("agent: {:?}", plan.config.agent);

// Execute the playbook
let result = runner.execute_playbook(invocation).await?;
println!("success: {}", result.success);
```

### Execute tasks in parallel

Use `ParallelTaskSpec` when an orchestration caller wants to schedule multiple
requests with explicit dependency ordering:

```rust
use wrangle_runner::{NamedExecutionResult, ParallelTaskSpec, PermissionPolicy, Runner, RuntimeConfig};
use std::path::PathBuf;

let runner = Runner::new(RuntimeConfig {
    backend: Some("qwen".to_string()),
    work_dir: PathBuf::from("/my/project"),
    ..RuntimeConfig::default()
});

let tasks = vec![
    ParallelTaskSpec {
        id: "plan".to_string(),
        task: "inspect the repo and propose a plan".to_string(),
        work_dir: None,
        dependencies: vec![],
        session_id: None,
        backend: None,
        model: None,
        agent: None,
        prompt_file: None,
        permission_policy: Some(PermissionPolicy::Default),
        transport_mode: None,
    },
    ParallelTaskSpec {
        id: "implement".to_string(),
        task: "implement the approved plan".to_string(),
        work_dir: None,
        dependencies: vec!["plan".to_string()],
        session_id: None,
        backend: None,
        model: None,
        agent: None,
        prompt_file: None,
        permission_policy: Some(PermissionPolicy::Bypass),
        transport_mode: None,
    },
];

let results: Vec<NamedExecutionResult> = runner.execute_parallel(tasks).await?;
for result in &results {
    println!("{} => success={}", result.id, result.result.success);
}
```

`execute_parallel` validates dependency structure and backend capability support
before it starts spawning work, so unsupported permission policies fail before
partially executing any tasks.

### Resume a session

```rust
use wrangle_runner::{Runner, RuntimeConfig, ExecutionRequest, SessionHandle, SessionState, TransportMode};

let runner = Runner::new(RuntimeConfig {
    backend: Some("qwen".to_string()),
    ..RuntimeConfig::default()
});

let request = ExecutionRequest {
    task: "continue working on the feature".to_string(),
    work_dir: std::path::PathBuf::from("/my/project"),
    model: None,
    session: Some(SessionHandle {
        id: "session-id-from-previous-run".to_string(),
        state: SessionState::Resumable,
        transport: TransportMode::OneShotProcess,
    }),
    permission_policy: PermissionPolicy::Default,
    prompt_file: None,
    extra_env: Default::default(),
};

let result = runner.execute(request).await?;
// result.session contains the (possibly updated) session handle
```

## Error handling

All runner functions return `anyhow::Result`. Errors from backend resolution,
transport validation, and execution are all returned as structured errors:

```rust
use wrangle_runner::{Runner, RuntimeConfig, ExecutionRequest};

let runner = Runner::new(RuntimeConfig {
    backend: Some("nonexistent".to_string()),
    ..RuntimeConfig::default()
});

match runner.execute_task("do something").await {
    Ok(result) => println!("success: {}", result.success),
    Err(e) => eprintln!("execution failed: {e:#}"),
}
```

Common errors a caller should expect include:

- `BackendError::NotFound`: the requested backend name is unknown
- `BackendError::NotAvailable`: the backend is known but not installed on `PATH`
- `ExecutionError::UnsupportedTransport`: the backend cannot honor the requested transport
- `ExecutionError::UnsupportedPermissionPolicy`: the backend cannot honor the requested permission policy
- `ExecutionError::CircularDependency`: a parallel task graph cannot make progress

## Stable API surface

The following are considered the stable, documented public API:

| Type / Function | Purpose |
|---|---|
| `Runner` | Primary entry point for programmatic usage |
| `Runner::new(config)` | Create a runner with explicit config |
| `Runner::with_defaults()` | Create a runner with default config |
| `Runner::available_backends()` | List all known backends |
| `Runner::find_backend(name)` | Look up a backend by name |
| `Runner::is_backend_available(name)` | Check if a backend is installed |
| `Runner::preview(request)` | Preview execution plan without running |
| `Runner::preview_task(task)` | Convenience preview for a task string |
| `Runner::execute(request)` | Execute a structured request |
| `Runner::execute_task(task)` | Convenience execute for a task string |
| `Runner::execute_parallel(tasks)` | Execute tasks in parallel with dependencies |
| `Runner::plan_playbook(invocation)` | Build a playbook plan for inspection |
| `Runner::execute_playbook(invocation)` | Build and execute a playbook |
| `available_backends()` | Standalone: list backends |
| `find_backend(name)` | Standalone: look up backend |
| `is_backend_available(name)` | Standalone: check availability |
| `preview_request(config, request)` | Standalone: preview execution |
| `execute_request(config, request)` | Standalone: execute request |
| `execute_parallel(config, tasks)` | Standalone: parallel execution |
| `build_playbook(base_config, invocation)` | Build config + request for a playbook |
| `build_playbook_plan(base_config, invocation)` | Build a playbook plan for inspection |
| `CommandPreview` | Command that would be executed |
| `ExecutionPlan` | Full execution plan from preview |
| `PlaybookInvocation` | Request to run a named playbook |
| `PlaybookPlan` | Resolved playbook plan |
| `PlaybookName` | Well-known playbook identifiers |
| `RuntimeConfigSnapshot` | Serializable config snapshot |
| `NamedExecutionResult` | Task result tagged with id |

All types re-exported from `wrangle-core` are also part of the stable surface:
`RuntimeConfig`, `ExecutionRequest`, `ExecutionResult`, `BackendCapabilities`,
`TransportMode`, `PermissionPolicy`, `SessionHandle`, `SessionState`,
`ParallelTaskSpec`, `BackendKind`, `CommandSpec`.
