//! Backend implementations for different AI CLI tools

use anyhow::Result;
use std::sync::Arc;

use crate::config::Config;
use crate::errors::BackendError;

/// Backend trait defining the interface for AI CLI backends
pub trait Backend: Send + Sync {
    /// Get backend name
    fn name(&self) -> &'static str;

    /// Get command name to execute
    fn command(&self) -> &'static str;

    /// Build command arguments
    fn build_args(&self, config: &Config, target: &str) -> Vec<String>;

    /// Check if backend is available (command exists)
    fn is_available(&self) -> bool {
        which::which(self.command()).is_ok()
    }
}

/// Codex backend implementation
pub struct CodexBackend;

impl Backend for CodexBackend {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn command(&self) -> &'static str {
        "codex"
    }

    fn build_args(&self, config: &Config, target: &str) -> Vec<String> {
        let mut args = vec![
            "e".to_string(),
            "-C".to_string(),
            config.work_dir.display().to_string(),
            "--json".to_string(),
        ];

        if let Some(ref session_id) = config.session_id {
            args.push("-r".to_string());
            args.push(session_id.clone());
        }

        if let Some(ref model) = config.model {
            args.push("-m".to_string());
            args.push(model.clone());
        }

        if let Some(ref reasoning_effort) = config.reasoning_effort {
            args.push("--reasoning-effort".to_string());
            args.push(reasoning_effort.clone());
        }

        if config.skip_permissions {
            args.push("--full-auto".to_string());
        }

        args.push(target.to_string());
        args
    }
}

/// Claude backend implementation
pub struct ClaudeBackend;

impl Backend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn command(&self) -> &'static str {
        "claude"
    }

    fn build_args(&self, config: &Config, target: &str) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
        ];

        if config.skip_permissions {
            args.push("--dangerously-skip-permissions".to_string());
        }

        if let Some(ref model) = config.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }

        if let Some(ref session_id) = config.session_id {
            args.push("-r".to_string());
            args.push(session_id.clone());
        }

        args.push(target.to_string());
        args
    }
}

/// Gemini backend implementation
pub struct GeminiBackend;

impl Backend for GeminiBackend {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn command(&self) -> &'static str {
        "gemini"
    }

    fn build_args(&self, config: &Config, target: &str) -> Vec<String> {
        let mut args = vec![
            "-o".to_string(),
            "stream-json".to_string(),
            "-y".to_string(),
        ];

        if let Some(ref model) = config.model {
            args.push("-m".to_string());
            args.push(model.clone());
        }

        if let Some(ref session_id) = config.session_id {
            args.push("-r".to_string());
            args.push(session_id.clone());
        }

        args.push(target.to_string());
        args
    }
}

/// Opencode backend implementation
pub struct OpencodeBackend;

impl Backend for OpencodeBackend {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn command(&self) -> &'static str {
        "opencode"
    }

    fn build_args(&self, config: &Config, target: &str) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];

        if let Some(ref model) = config.model {
            args.push("-m".to_string());
            args.push(model.clone());
        }

        if let Some(ref session_id) = config.session_id {
            args.push("-s".to_string());
            args.push(session_id.clone());
        }

        args.push(target.to_string());
        args
    }
}

/// Select a backend by name
pub fn select_backend(name: Option<&str>) -> Result<Arc<dyn Backend>> {
    let backend: Arc<dyn Backend> = match name.map(|s| s.to_lowercase()).as_deref() {
        Some("codex") => Arc::new(CodexBackend),
        Some("claude") => Arc::new(ClaudeBackend),
        Some("gemini") => Arc::new(GeminiBackend),
        Some("opencode") => Arc::new(OpencodeBackend),
        Some(other) => {
            return Err(BackendError::NotFound(other.to_string()).into());
        }
        None => {
            // Auto-detect: prefer Claude, then Codex, then Gemini, then Opencode
            if ClaudeBackend.is_available() {
                Arc::new(ClaudeBackend)
            } else if CodexBackend.is_available() {
                Arc::new(CodexBackend)
            } else if GeminiBackend.is_available() {
                Arc::new(GeminiBackend)
            } else if OpencodeBackend.is_available() {
                Arc::new(OpencodeBackend)
            } else {
                return Err(BackendError::NotAvailable(
                    "any".to_string(),
                    "codex, claude, gemini, or opencode".to_string(),
                )
                .into());
            }
        }
    };

    Ok(backend)
}

/// Get list of available backend names
#[allow(dead_code)]
pub fn get_available_backends() -> Vec<&'static str> {
    vec!["codex", "claude", "gemini", "opencode"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_build_args() {
        let backend = CodexBackend;
        let config = Config {
            work_dir: "/tmp".into(),
            model: Some("gpt-4".to_string()),
            skip_permissions: true,
            ..Default::default()
        };

        let args = backend.build_args(&config, "Test task");
        assert!(args.contains(&"--json".to_string()));
        assert!(args.contains(&"--full-auto".to_string()));
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"gpt-4".to_string()));
    }

    #[test]
    fn test_claude_build_args() {
        let backend = ClaudeBackend;
        let config = Config {
            skip_permissions: true,
            session_id: Some("abc123".to_string()),
            ..Default::default()
        };

        let args = backend.build_args(&config, "Test task");
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"-r".to_string()));
        assert!(args.contains(&"abc123".to_string()));
    }

    #[test]
    fn test_select_backend_by_name() {
        let backend = select_backend(Some("claude")).unwrap();
        assert_eq!(backend.name(), "claude");

        let backend = select_backend(Some("codex")).unwrap();
        assert_eq!(backend.name(), "codex");
    }

    #[test]
    fn test_select_unknown_backend() {
        let result = select_backend(Some("unknown"));
        assert!(result.is_err());
    }
}
