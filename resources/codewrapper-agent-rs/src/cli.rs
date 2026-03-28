//! CLI argument parsing using clap

use clap::{Parser, Subcommand, ValueEnum};

/// Unified wrapper for AI CLI backends (Codex, Claude, Gemini, Opencode)
#[derive(Parser, Debug, Clone)]
#[command(name = "codeagent")]
#[command(author, version, about, long_about = None)]
#[command(after_help = r#"Examples:
  codeagent-wrapper "Fix the bug in main.rs"
  codeagent-wrapper "Implement feature X" /path/to/workdir
  codeagent-wrapper --backend claude - <<'EOF'
  Multi-line task with @file references
  EOF
  codeagent-wrapper resume abc123 "Continue work"
  codeagent-wrapper --parallel < tasks.txt
  codeagent-wrapper init --force
"#)]
pub struct Cli {
    /// Task to execute (prompt or instruction)
    #[arg(value_name = "TASK")]
    pub task: Option<String>,

    /// Working directory for the task
    #[arg(value_name = "WORKDIR")]
    pub workdir: Option<String>,

    /// Backend to use
    #[arg(long, short = 'b', env = "CODEAGENT_BACKEND")]
    pub backend: Option<String>,

    /// Model to use
    #[arg(long, short = 'm', env = "CODEAGENT_MODEL")]
    pub model: Option<String>,

    /// Agent configuration name
    #[arg(long, short = 'a', env = "CODEAGENT_AGENT")]
    pub agent: Option<String>,

    /// Path to prompt file
    #[arg(long, value_name = "PATH")]
    pub prompt_file: Option<String>,

    /// Timeout in seconds
    #[arg(long, short = 't', default_value = "7200", env = "CODEX_TIMEOUT")]
    pub timeout: u64,

    /// Skip permission checks (YOLO mode)
    #[arg(long, alias = "yolo", env = "CODEAGENT_SKIP_PERMISSIONS")]
    pub skip_permissions: bool,

    /// Reasoning effort level (for Codex backend)
    #[arg(long, value_name = "LEVEL")]
    pub reasoning_effort: Option<String>,

    /// Use minimal environment variables (performance optimization)
    #[arg(long)]
    pub minimal_env: bool,

    /// Max parallel workers
    #[arg(long, env = "CODEAGENT_MAX_PARALLEL_WORKERS")]
    pub max_parallel_workers: Option<usize>,

    /// Run in parallel mode (read tasks from stdin)
    #[arg(long)]
    pub parallel: bool,

    /// Suppress progress output
    #[arg(long, short = 'q', env = "CODEAGENT_QUIET")]
    pub quiet: bool,

    /// Show full output in parallel mode
    #[arg(long)]
    pub full_output: bool,

    /// Show backend stderr output (for debugging)
    #[arg(long, env = "CODEAGENT_BACKEND_OUTPUT")]
    pub backend_output: bool,

    /// Enable debug mode (auto-enables --backend-output)
    #[arg(long, short = 'd', env = "CODEAGENT_DEBUG")]
    pub debug: bool,

    /// Remove old log files
    #[arg(long)]
    pub cleanup: bool,

    /// Subcommands
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Available subcommands
#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Resume a previous session
    Resume {
        /// Session ID to resume
        session_id: String,
        /// Task to continue with
        task: String,
        /// Working directory
        workdir: Option<String>,
    },

    /// Install codeagent skill to ~/.claude/skills/
    Init {
        /// Force overwrite existing installation
        #[arg(long, short = 'f')]
        force: bool,
    },
}

/// Backend type enum for validation
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BackendType {
    Codex,
    Claude,
    Gemini,
    Opencode,
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Codex => write!(f, "codex"),
            BackendType::Claude => write!(f, "claude"),
            BackendType::Gemini => write!(f, "gemini"),
            BackendType::Opencode => write!(f, "opencode"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_basic_parsing() {
        let cli = Cli::try_parse_from(["codeagent", "Fix the bug"]).unwrap();
        assert_eq!(cli.task, Some("Fix the bug".to_string()));
        assert!(!cli.parallel);
        assert!(!cli.quiet);
    }

    #[test]
    fn test_cli_with_backend() {
        let cli = Cli::try_parse_from(["codeagent", "--backend", "claude", "Test task"]).unwrap();
        assert_eq!(cli.backend, Some("claude".to_string()));
    }

    #[test]
    fn test_cli_resume() {
        let cli = Cli::try_parse_from(["codeagent", "resume", "abc123", "Continue"]).unwrap();
        match cli.command {
            Some(Command::Resume {
                session_id, task, ..
            }) => {
                assert_eq!(session_id, "abc123");
                assert_eq!(task, "Continue");
            }
            _ => panic!("Expected Resume command"),
        }
    }

    #[test]
    fn test_cli_init() {
        let cli = Cli::try_parse_from(["codeagent", "init", "--force"]).unwrap();
        match cli.command {
            Some(Command::Init { force }) => {
                assert!(force);
            }
            _ => panic!("Expected Init command"),
        }
    }

    #[test]
    fn test_cli_yolo_alias() {
        let cli = Cli::try_parse_from(["codeagent", "--yolo", "Test task"]).unwrap();
        assert!(cli.skip_permissions);
    }

    #[test]
    fn test_cli_skip_permissions() {
        let cli = Cli::try_parse_from(["codeagent", "--skip-permissions", "Test task"]).unwrap();
        assert!(cli.skip_permissions);
    }

    #[test]
    fn test_cli_reasoning_effort() {
        let cli =
            Cli::try_parse_from(["codeagent", "--reasoning-effort", "high", "Test task"]).unwrap();
        assert_eq!(cli.reasoning_effort, Some("high".to_string()));
    }

    #[test]
    fn test_cli_minimal_env() {
        let cli = Cli::try_parse_from(["codeagent", "--minimal-env", "Test task"]).unwrap();
        assert!(cli.minimal_env);
    }

    #[test]
    fn test_cli_combined_flags() {
        let cli = Cli::try_parse_from([
            "codeagent",
            "--backend",
            "codex",
            "--yolo",
            "--reasoning-effort",
            "medium",
            "--minimal-env",
            "Test task",
        ])
        .unwrap();
        assert_eq!(cli.backend, Some("codex".to_string()));
        assert!(cli.skip_permissions);
        assert_eq!(cli.reasoning_effort, Some("medium".to_string()));
        assert!(cli.minimal_env);
    }
}
