use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid session id: {0}")]
    InvalidSessionId(String),
    #[error("prompt files from task specs are disabled without --allow-task-prompt-files")]
    TaskPromptFilesDisabled,
    #[error("no task provided via stdin")]
    EmptyTaskFromStdin,
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
}

