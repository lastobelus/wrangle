use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::RuntimeConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendKind {
    Codex,
    Claude,
    Gemini,
    Opencode,
    Qwen,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Opencode => "opencode",
            Self::Qwen => "qwen",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportMode {
    OneShotProcess,
    PersistentBackend,
    WrangleServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PermissionPolicy {
    #[default]
    Default,
    Bypass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    Ephemeral,
    Resumable,
    PersistentAttached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandle {
    pub id: String,
    pub state: SessionState,
    pub transport: TransportMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequest {
    pub task: String,
    pub work_dir: PathBuf,
    pub model: Option<String>,
    pub session: Option<SessionHandle>,
    pub permission_policy: PermissionPolicy,
    pub prompt_file: Option<PathBuf>,
    pub extra_env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEvent {
    pub backend: BackendKind,
    pub transport: TransportMode,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    pub success: bool,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub backend: BackendKind,
    pub transport: TransportMode,
    pub session: Option<SessionHandle>,
    pub events: Vec<ExecutionEvent>,
    pub stderr_truncated: bool,
    pub stderr_excerpt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BackendDescriptor {
    pub kind: BackendKind,
    pub name: &'static str,
    pub transport_modes: &'static [TransportMode],
    pub supports_resume: bool,
    pub supports_persistent_backend: bool,
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: &'static str,
    pub args: Vec<String>,
    pub current_dir: PathBuf,
    pub env: HashMap<String, String>,
    pub stdin: Option<Vec<u8>>,
}

#[async_trait]
pub trait AgentBackend: Send + Sync {
    fn descriptor(&self) -> BackendDescriptor;
    fn is_available(&self) -> bool;
    fn build_command(
        &self,
        config: &RuntimeConfig,
        request: &ExecutionRequest,
        transport: TransportMode,
    ) -> Result<CommandSpec>;
}

#[async_trait]
pub trait BackendTransport: Send + Sync {
    fn mode(&self) -> TransportMode;

    async fn execute(
        &self,
        backend: &dyn AgentBackend,
        config: &RuntimeConfig,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult>;
}
