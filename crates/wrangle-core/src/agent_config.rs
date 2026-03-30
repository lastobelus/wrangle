use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::RuntimeConfig;
use crate::project_config::discover_config;
use crate::protocol::PermissionPolicy;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub prompt_file: Option<String>,
    #[serde(default)]
    pub permission_policy: Option<PermissionPolicy>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsConfig {
    pub default_backend: String,
    pub default_model: String,
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsConfigOverrides {
    #[serde(default)]
    default_backend: Option<String>,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    agents: HashMap<String, AgentConfig>,
}

pub fn default_models_config() -> ModelsConfig {
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

fn normalize_agent(name: &str, mut agent: AgentConfig) -> AgentConfig {
    if agent.name.trim().is_empty() {
        agent.name = name.to_string();
    }
    agent.backend = normalize_optional(agent.backend);
    agent.model = normalize_optional(agent.model);
    agent.prompt_file = normalize_optional(agent.prompt_file);
    agent
}

async fn load_models_config_from_file(models_file: &Path) -> Result<ModelsConfig> {
    let mut config = default_models_config();
    if !models_file.exists() {
        return Ok(config);
    }

    let content = tokio::fs::read_to_string(models_file)
        .await
        .with_context(|| format!("failed to read models config: {}", models_file.display()))?;

    let user_config: ModelsConfigOverrides = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse models config: {}", models_file.display()))?;

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

pub async fn load_models_config_for(start: &Path) -> Result<ModelsConfig> {
    let discovery = discover_config(start).await?;
    load_models_config_from_file(&discovery.active_models_file).await
}

pub async fn load_models_config() -> Result<ModelsConfig> {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    load_models_config_for(&start).await
}

pub async fn get_agent_config_for(name: &str, start: &Path) -> Result<AgentConfig> {
    let models = load_models_config_for(start).await?;
    models.agents.get(name).cloned().map_or_else(
        || anyhow::bail!("agent not found: {}", name),
        |agent| Ok(normalize_agent(name, agent)),
    )
}

pub async fn get_agent_config(name: &str) -> Result<AgentConfig> {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    get_agent_config_for(name, &start).await
}

pub fn apply_agent_to_runtime_config(config: &mut RuntimeConfig, agent: &AgentConfig) {
    if config.backend.is_none() {
        config.backend = agent.backend.clone();
    }
    if config.model.is_none() {
        config.model = agent.model.clone();
    }
    if config.permission_policy == PermissionPolicy::Default {
        if let Some(policy) = agent.permission_policy {
            config.permission_policy = policy;
        }
    }
}

pub async fn resolve_agent_for_runtime_config(config: &mut RuntimeConfig) -> Result<()> {
    let Some(agent_name) = config.agent.clone() else {
        return Ok(());
    };
    let agent = get_agent_config_for(&agent_name, &config.work_dir).await?;
    apply_agent_to_runtime_config(config, &agent);
    Ok(())
}
