//! Agent configuration and preset loading
//!
//! This module is part of the active runtime path for resolving `--agent`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Agent configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name
    #[serde(default)]
    pub name: String,
    /// Model to use
    #[serde(default)]
    pub model: Option<String>,
    /// Backend to use
    #[serde(default)]
    pub backend: Option<String>,
    /// Skip permissions
    #[serde(default, rename = "skipPermissions")]
    pub skip_permissions: bool,
    /// Additional environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Prompt prefix
    #[serde(default, rename = "promptPrefix")]
    pub prompt_prefix: Option<String>,
    /// Prompt file path
    #[serde(default, rename = "promptFile")]
    pub prompt_file: Option<String>,
    /// Reasoning effort
    #[serde(default, rename = "reasoningEffort")]
    pub reasoning_effort: Option<String>,
}

/// Models configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    /// Default backend
    #[serde(rename = "defaultBackend")]
    pub default_backend: String,
    /// Default model
    #[serde(rename = "defaultModel")]
    pub default_model: String,
    /// Agent configurations
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        default_models_config()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ModelsConfigOverrides {
    #[serde(default, rename = "defaultBackend")]
    default_backend: Option<String>,
    #[serde(default, rename = "defaultModel")]
    default_model: Option<String>,
    #[serde(default)]
    agents: HashMap<String, AgentConfig>,
}

/// Get config directory path
fn get_config_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".codeagent")
}

fn default_models_config() -> ModelsConfig {
    let mut agents = HashMap::new();
    agents.insert(
        "oracle".to_string(),
        AgentConfig {
            name: "oracle".to_string(),
            backend: Some("claude".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            ..Default::default()
        },
    );
    agents.insert(
        "librarian".to_string(),
        AgentConfig {
            name: "librarian".to_string(),
            backend: Some("claude".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            ..Default::default()
        },
    );
    agents.insert(
        "explore".to_string(),
        AgentConfig {
            name: "explore".to_string(),
            backend: Some("codex".to_string()),
            ..Default::default()
        },
    );
    agents.insert(
        "develop".to_string(),
        AgentConfig {
            name: "develop".to_string(),
            backend: Some("codex".to_string()),
            ..Default::default()
        },
    );
    agents.insert(
        "frontend-ui-ux-engineer".to_string(),
        AgentConfig {
            name: "frontend-ui-ux-engineer".to_string(),
            backend: Some("gemini".to_string()),
            ..Default::default()
        },
    );
    agents.insert(
        "document-writer".to_string(),
        AgentConfig {
            name: "document-writer".to_string(),
            backend: Some("gemini".to_string()),
            ..Default::default()
        },
    );

    ModelsConfig {
        default_backend: "opencode".to_string(),
        default_model: "openai/gpt-5.3-codex".to_string(),
        agents,
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn expand_home_path(path: &str) -> PathBuf {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(&path[2..]);
    }
    PathBuf::from(path)
}

fn normalize_agent(name: &str, mut agent: AgentConfig) -> AgentConfig {
    if agent.name.trim().is_empty() {
        agent.name = name.to_string();
    }
    agent.backend = normalize_optional(agent.backend);
    agent.model = normalize_optional(agent.model);
    agent.prompt_file = normalize_optional(agent.prompt_file);
    agent.prompt_prefix = normalize_optional(agent.prompt_prefix);
    agent.reasoning_effort = normalize_optional(agent.reasoning_effort);
    agent
}

fn get_agent_from_models(name: &str, models: &ModelsConfig) -> Result<AgentConfig> {
    models.agents.get(name).cloned().map_or_else(
        || {
            let mut available: Vec<&str> = models.agents.keys().map(String::as_str).collect();
            available.sort_unstable();
            anyhow::bail!(
                "Agent not found: {}. Available: {}",
                name,
                available.join(", ")
            );
        },
        |agent| Ok(normalize_agent(name, agent)),
    )
}

async fn load_models_config_from_file(models_file: &Path) -> Result<ModelsConfig> {
    let mut config = default_models_config();

    if !models_file.exists() {
        return Ok(config);
    }

    let content = tokio::fs::read_to_string(models_file)
        .await
        .with_context(|| format!("Failed to read models config: {}", models_file.display()))?;

    let user_config: ModelsConfigOverrides = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse models config: {}", models_file.display()))?;

    if let Some(default_backend) = normalize_optional(user_config.default_backend) {
        config.default_backend = default_backend;
    }
    if let Some(default_model) = normalize_optional(user_config.default_model) {
        config.default_model = default_model;
    }

    for (name, agent) in user_config.agents {
        config
            .agents
            .insert(name.clone(), normalize_agent(&name, agent));
    }

    Ok(config)
}

/// Load agent configuration by name
pub async fn get_agent_config(name: &str) -> Result<AgentConfig> {
    let models = load_models_config().await?;
    get_agent_from_models(name, &models)
}

/// Load models configuration
pub async fn load_models_config() -> Result<ModelsConfig> {
    let config_dir = get_config_dir();
    let models_file = config_dir.join("models.json");
    load_models_config_from_file(&models_file).await
}

/// Merge agent config into runtime config
pub fn merge_agent_config(
    model: Option<String>,
    backend: Option<String>,
    agent_config: &AgentConfig,
) -> (Option<String>, Option<String>) {
    let model =
        normalize_optional(model).or_else(|| normalize_optional(agent_config.model.clone()));
    let backend =
        normalize_optional(backend).or_else(|| normalize_optional(agent_config.backend.clone()));
    (model, backend)
}

/// Apply agent config values to runtime config when CLI values are not provided.
pub fn apply_agent_to_runtime_config(
    config: &mut crate::config::Config,
    agent_config: &AgentConfig,
) {
    let (model, backend) =
        merge_agent_config(config.model.clone(), config.backend.clone(), agent_config);
    config.model = model;
    config.backend = backend;

    if config.prompt_file.is_none()
        && let Some(prompt_file) = normalize_optional(agent_config.prompt_file.clone())
    {
        config.prompt_file = Some(expand_home_path(&prompt_file));
    }

    if config.reasoning_effort.is_none() {
        config.reasoning_effort = normalize_optional(agent_config.reasoning_effort.clone());
    }

    if !config.skip_permissions {
        config.skip_permissions = agent_config.skip_permissions;
    }
}

/// Load and apply agent config to runtime config.
pub async fn resolve_agent_for_runtime_config(config: &mut crate::config::Config) -> Result<()> {
    let Some(agent_name) = normalize_optional(config.agent.clone()) else {
        return Ok(());
    };

    let agent_config = get_agent_config(&agent_name).await?;
    apply_agent_to_runtime_config(config, &agent_config);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::tempdir;

    #[test]
    fn test_agent_config_parsing() {
        let json = r#"{
  "name": "test-agent",
  "model": "gpt-4",
  "backend": "codex",
  "promptFile": "~/.claude/prompts/test.md",
  "reasoningEffort": "high",
  "skipPermissions": true,
  "env": {
    "TEST_VAR": "value"
  }
}"#;
        let config: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "test-agent");
        assert_eq!(config.model, Some("gpt-4".to_string()));
        assert_eq!(config.backend, Some("codex".to_string()));
        assert_eq!(
            config.prompt_file,
            Some("~/.claude/prompts/test.md".to_string())
        );
        assert_eq!(config.reasoning_effort, Some("high".to_string()));
        assert!(config.skip_permissions);
    }

    #[test]
    fn test_merge_agent_config() {
        let agent = AgentConfig {
            name: "test".to_string(),
            model: Some("default-model".to_string()),
            backend: Some("claude".to_string()),
            ..Default::default()
        };

        // CLI overrides agent config
        let (model, backend) = merge_agent_config(Some("cli-model".to_string()), None, &agent);
        assert_eq!(model, Some("cli-model".to_string()));
        assert_eq!(backend, Some("claude".to_string()));

        // Agent config used when CLI doesn't specify
        let (model, backend) = merge_agent_config(None, None, &agent);
        assert_eq!(model, Some("default-model".to_string()));
        assert_eq!(backend, Some("claude".to_string()));
    }

    #[test]
    fn test_load_models_config_uses_defaults_when_missing() {
        let dir = tempdir().unwrap();
        let models_file = dir.path().join("models.json");

        let config = tokio_test::block_on(load_models_config_from_file(&models_file)).unwrap();

        assert_eq!(config.default_backend, "opencode");
        assert_eq!(config.default_model, "openai/gpt-5.3-codex");
        assert!(config.agents.contains_key("oracle"));
        assert!(config.agents.contains_key("develop"));
    }

    #[test]
    fn test_load_models_config_merges_user_config() {
        let dir = tempdir().unwrap();
        let models_file = dir.path().join("models.json");

        std::fs::write(
            &models_file,
            r#"{
  "defaultBackend": "claude",
  "agents": {
    "develop": {
      "backend": "codex",
      "model": "gpt-5-codex"
    },
    "new-agent": {
      "backend": "gemini"
    }
  }
}"#,
        )
        .unwrap();

        let config = tokio_test::block_on(load_models_config_from_file(&models_file)).unwrap();

        assert_eq!(config.default_backend, "claude");
        assert_eq!(config.default_model, "openai/gpt-5.3-codex");
        assert_eq!(
            config.agents.get("develop").and_then(|a| a.model.clone()),
            Some("gpt-5-codex".to_string())
        );
        assert_eq!(
            config
                .agents
                .get("new-agent")
                .and_then(|a| a.backend.clone()),
            Some("gemini".to_string())
        );
        assert!(config.agents.contains_key("oracle"));
    }

    #[test]
    fn test_get_agent_from_models_lists_available_agents() {
        let models = default_models_config();
        let error = get_agent_from_models("unknown", &models).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("Agent not found: unknown"));
        assert!(message.contains("oracle"));
    }

    #[test]
    fn test_apply_agent_to_runtime_config() {
        let mut runtime = Config {
            model: None,
            backend: None,
            prompt_file: None,
            reasoning_effort: None,
            skip_permissions: false,
            ..Default::default()
        };
        let agent = AgentConfig {
            model: Some("agent-model".to_string()),
            backend: Some("claude".to_string()),
            prompt_file: Some("~/prompts/agent.md".to_string()),
            reasoning_effort: Some("high".to_string()),
            skip_permissions: true,
            ..Default::default()
        };

        apply_agent_to_runtime_config(&mut runtime, &agent);

        assert_eq!(runtime.model, Some("agent-model".to_string()));
        assert_eq!(runtime.backend, Some("claude".to_string()));
        assert!(runtime.prompt_file.is_some());
        assert_eq!(runtime.reasoning_effort, Some("high".to_string()));
        assert!(runtime.skip_permissions);
    }

    #[test]
    fn test_apply_agent_to_runtime_config_preserves_cli_overrides() {
        let mut runtime = Config {
            model: Some("cli-model".to_string()),
            backend: Some("codex".to_string()),
            prompt_file: Some(PathBuf::from("/tmp/cli.md")),
            reasoning_effort: Some("low".to_string()),
            skip_permissions: true,
            ..Default::default()
        };
        let agent = AgentConfig {
            model: Some("agent-model".to_string()),
            backend: Some("claude".to_string()),
            prompt_file: Some("~/prompts/agent.md".to_string()),
            reasoning_effort: Some("high".to_string()),
            skip_permissions: false,
            ..Default::default()
        };

        apply_agent_to_runtime_config(&mut runtime, &agent);

        assert_eq!(runtime.model, Some("cli-model".to_string()));
        assert_eq!(runtime.backend, Some("codex".to_string()));
        assert_eq!(runtime.prompt_file, Some(PathBuf::from("/tmp/cli.md")));
        assert_eq!(runtime.reasoning_effort, Some("low".to_string()));
        assert!(runtime.skip_permissions);
    }
}
