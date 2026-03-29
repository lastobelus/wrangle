use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid session id: {0}")]
    InvalidSessionId(String),
    #[error("prompt files from task specs are disabled without --allow-task-prompt-files")]
    TaskPromptFilesDisabled,
    #[error("no task provided via stdin")]
    EmptyTaskFromStdin,
    #[error("duplicate parallel task id: {0}")]
    DuplicateTaskId(String),
    #[error("parallel task depends on an unknown task id: {0}")]
    UnknownDependency(String),
    #[error("parallel task cannot depend on itself: {0}")]
    SelfDependency(String),
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("backend not found: {0}")]
    NotFound(String),
    #[error("backend is not available: {0}")]
    NotAvailable(String),
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("message exceeded the maximum size of {max_bytes} bytes")]
    MessageTooLarge { max_bytes: usize },
    #[error("io error: {0}")]
    Io(String),
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("task timed out after {0} seconds")]
    TimedOut(u64),
    #[error("backend exited without a status code")]
    MissingExitCode,
    #[error("circular dependency detected in parallel tasks: {0}")]
    CircularDependency(String),
    #[error("backend '{backend}' does not support transport mode '{transport}'")]
    UnsupportedTransport { backend: String, transport: String },
    #[error("transport mode '{0}' is not implemented yet")]
    UnimplementedTransport(String),
    #[error("backend '{backend}' does not support permission policy '{policy}'")]
    UnsupportedPermissionPolicy { backend: String, policy: String },
}
