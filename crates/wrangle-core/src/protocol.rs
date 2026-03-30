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
pub enum BackendImplementation {
    Cli,
    Api,
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
    Ask,
    Auto,
    Bypass,
}

impl PermissionPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Ask => "ask",
            Self::Auto => "auto",
            Self::Bypass => "bypass",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    Ephemeral,
    Resumable,
    PersistentAttached,
    ServerAttached,
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
    pub duration_ms: u64,
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
    pub implementation: BackendImplementation,
    pub transport_modes: &'static [TransportMode],
    pub supports_resume: bool,
    pub supports_persistent_backend: bool,
    pub permission_policies: &'static [PermissionPolicy],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendCapabilities {
    pub kind: BackendKind,
    pub name: String,
    pub implementation: BackendImplementation,
    pub transport_modes: Vec<TransportMode>,
    pub supports_resume: bool,
    pub supports_persistent_backend: bool,
    pub supported_permission_policies: Vec<PermissionPolicy>,
    pub available: bool,
}

impl BackendCapabilities {
    pub fn from_descriptor(descriptor: &BackendDescriptor, available: bool) -> Self {
        Self {
            kind: descriptor.kind,
            name: descriptor.name.to_string(),
            implementation: descriptor.implementation,
            transport_modes: descriptor.transport_modes.to_vec(),
            supports_resume: descriptor.supports_resume,
            supports_persistent_backend: descriptor.supports_persistent_backend,
            supported_permission_policies: descriptor.permission_policies.to_vec(),
            available,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
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
pub trait ApiBackend: Send + Sync {
    fn descriptor(&self) -> BackendDescriptor;
    fn is_available(&self) -> bool;

    fn preview_command(
        &self,
        config: &RuntimeConfig,
        request: &ExecutionRequest,
    ) -> Result<CommandSpec>;

    async fn execute_api(
        &self,
        config: &RuntimeConfig,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult>;
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
