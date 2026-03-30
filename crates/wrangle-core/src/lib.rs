pub mod agent_config;
pub mod config;
pub mod errors;
pub mod protocol;
pub mod task_graph;

pub use agent_config::{
    AgentConfig, ModelsConfig, apply_agent_to_runtime_config, default_models_config,
    get_agent_config, load_models_config, resolve_agent_for_runtime_config,
};
pub use config::{
    ParallelConfig, ParallelTaskSpec, RuntimeConfig, RuntimeMode, ensure_parallel_tasks,
    get_default_max_parallel_workers, is_valid_session_id, parse_parallel_config, read_stdin_task,
};
pub use errors::{BackendError, ConfigError, ExecutionError, ParseError};
pub use protocol::{
    AgentBackend, BackendCapabilities, BackendDescriptor, BackendKind, BackendTransport,
    CommandSpec, ExecutionEvent, ExecutionRequest, ExecutionResult, PermissionPolicy,
    SessionHandle, SessionState, TransportMode,
};
