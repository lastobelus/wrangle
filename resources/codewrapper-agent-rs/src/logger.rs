//! Logging system using tracing

use anyhow::Result;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use crate::cli::Cli;

/// Get the log directory path
pub fn get_log_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".codeagent").join("logs")
}

/// Setup logging based on CLI options
pub fn setup_logging(cli: &Cli) -> Result<Option<WorkerGuard>> {
    let log_dir = get_log_dir();
    std::fs::create_dir_all(&log_dir)?;

    // Determine log level
    let level = if cli.debug {
        Level::DEBUG
    } else if cli.quiet {
        Level::ERROR
    } else {
        Level::INFO
    };

    // Create file appender
    let file_appender = tracing_appender::rolling::daily(&log_dir, "codeagent.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Create env filter
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    // Setup subscriber
    let subscriber = tracing_subscriber::registry().with(env_filter).with(
        fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_span_events(FmtSpan::CLOSE),
    );

    // Add console output if not quiet
    if !cli.quiet {
        let console_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(false)
            .with_level(true)
            .compact();

        subscriber.with(console_layer).init();
    } else {
        subscriber.init();
    }

    Ok(Some(guard))
}

/// Cleanup old log files (older than 30 days)
pub async fn cleanup_old_logs() -> Result<()> {
    use std::time::{Duration, SystemTime};
    use tokio::fs;

    let log_dir = get_log_dir();
    if !log_dir.exists() {
        println!("No log directory found.");
        return Ok(());
    }

    let max_age = Duration::from_secs(30 * 24 * 60 * 60); // 30 days
    let now = SystemTime::now();
    let mut deleted_count = 0;
    let mut deleted_size = 0u64;

    let mut entries = fs::read_dir(&log_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "log")
            && let Ok(metadata) = entry.metadata().await
            && let Ok(modified) = metadata.modified()
            && let Ok(age) = now.duration_since(modified)
            && age > max_age
        {
            let size = metadata.len();
            if fs::remove_file(&path).await.is_ok() {
                deleted_count += 1;
                deleted_size += size;
            }
        }
    }

    println!(
        "Cleanup complete: {} files removed ({:.2} MB freed)",
        deleted_count,
        deleted_size as f64 / 1_048_576.0
    );

    Ok(())
}

/// Logger struct for task-specific logging
#[derive(Clone)]
pub struct Logger {
    #[allow(dead_code)] // Reserved: task_id will be used for task-specific log formatting
    task_id: Option<String>,
}

impl Logger {
    /// Create a new logger
    pub fn new(task_id: Option<String>) -> Self {
        Self { task_id }
    }

    /// Log info message
    #[allow(dead_code)] // Reserved: will be used when task-specific logging is enabled
    pub fn info(&self, message: &str) {
        if let Some(ref id) = self.task_id {
            tracing::info!(task_id = %id, "{}", message);
        } else {
            tracing::info!("{}", message);
        }
    }

    /// Log debug message
    #[allow(dead_code)] // Reserved: will be used when task-specific logging is enabled
    pub fn debug(&self, message: &str) {
        if let Some(ref id) = self.task_id {
            tracing::debug!(task_id = %id, "{}", message);
        } else {
            tracing::debug!("{}", message);
        }
    }

    /// Log error message
    #[allow(dead_code)] // Reserved: will be used when task-specific logging is enabled
    pub fn error(&self, message: &str) {
        if let Some(ref id) = self.task_id {
            tracing::error!(task_id = %id, "{}", message);
        } else {
            tracing::error!("{}", message);
        }
    }

    /// Log warning message
    #[allow(dead_code)] // Reserved: will be used when task-specific logging is enabled
    pub fn warn(&self, message: &str) {
        if let Some(ref id) = self.task_id {
            tracing::warn!(task_id = %id, "{}", message);
        } else {
            tracing::warn!("{}", message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_log_dir() {
        let log_dir = get_log_dir();
        assert!(log_dir.ends_with("logs"));
    }
}
