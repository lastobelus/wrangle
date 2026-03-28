//! Error types for codeagent-wrapper
//!
//! Error types are defined for all possible error conditions to maintain
//! compatibility with the Node.js version and support future error handling.

#![allow(dead_code)] // Reserved API: comprehensive error types for future use

use thiserror::Error;

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid parameter '{0}': {1}")]
    InvalidParameter(String, String),

    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    #[error("Invalid file path: {0}")]
    InvalidFilePath(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid session ID: {0}")]
    InvalidSessionId(String),

    #[error("Invalid task: {0}")]
    InvalidTask(String),
}

/// Backend-related errors
#[derive(Error, Debug)]
pub enum BackendError {
    #[error("Backend not found: {0}. Available: codex, claude, gemini, opencode")]
    NotFound(String),

    #[error("Backend '{0}' is not available. Please install: {1}")]
    NotAvailable(String, String),

    #[error("Backend execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Backend timeout after {0} seconds")]
    Timeout(u64),
}

/// Execution-related errors
#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Process spawn failed: {0}")]
    SpawnFailed(String),

    #[error("Process terminated with signal: {0}")]
    SignalTerminated(i32),

    #[error("Task failed with exit code: {0}")]
    TaskFailed(i32),

    #[error("Parallel execution failed: {0}")]
    ParallelFailed(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),
}

/// Parser-related errors
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Message too large: {0} bytes (max: {1})")]
    MessageTooLarge(usize, usize),

    #[error("Unexpected EOF while parsing")]
    UnexpectedEof,
}

/// Exit codes matching Node.js version
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const INVALID_ARGUMENT: i32 = 2;
    pub const BACKEND_NOT_FOUND: i32 = 3;
    pub const BACKEND_FAILED: i32 = 4;
    pub const TIMEOUT: i32 = 5;
    pub const SIGNAL_TERMINATED: i32 = 128;
}

/// Get exit code for an error
pub fn get_exit_code(err: &anyhow::Error) -> i32 {
    if let Some(e) = err.downcast_ref::<ConfigError>() {
        match e {
            ConfigError::InvalidParameter(_, _) => exit_codes::INVALID_ARGUMENT,
            ConfigError::MissingParameter(_) => exit_codes::INVALID_ARGUMENT,
            _ => exit_codes::GENERAL_ERROR,
        }
    } else if let Some(e) = err.downcast_ref::<BackendError>() {
        match e {
            BackendError::NotFound(_) => exit_codes::BACKEND_NOT_FOUND,
            BackendError::NotAvailable(_, _) => exit_codes::BACKEND_NOT_FOUND,
            BackendError::ExecutionFailed(_) => exit_codes::BACKEND_FAILED,
            BackendError::Timeout(_) => exit_codes::TIMEOUT,
        }
    } else if let Some(e) = err.downcast_ref::<ExecutionError>() {
        match e {
            ExecutionError::SignalTerminated(sig) => exit_codes::SIGNAL_TERMINATED + sig,
            ExecutionError::TaskFailed(code) => *code,
            _ => exit_codes::GENERAL_ERROR,
        }
    } else {
        exit_codes::GENERAL_ERROR
    }
}
