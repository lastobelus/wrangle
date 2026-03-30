use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;
use wrangle_core::{
    ApiBackend, BackendCapabilities, BackendDescriptor, BackendImplementation, BackendKind,
    CommandSpec, ExecutionEvent, ExecutionRequest, ExecutionResult, PermissionPolicy,
    RuntimeConfig, TransportMode,
};

const ONE_SHOT_AND_SERVER: &[TransportMode] =
    &[TransportMode::OneShotProcess, TransportMode::WrangleServer];
const DEFAULT_ONLY: &[PermissionPolicy] = &[PermissionPolicy::Default];

pub struct CodexApiBackend {
    api_base: String,
    descriptor: BackendDescriptor,
}

impl CodexApiBackend {
    pub fn new() -> Self {
        Self {
            api_base: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            descriptor: BackendDescriptor {
                kind: BackendKind::Codex,
                name: "codex-api",
                implementation: BackendImplementation::Api,
                transport_modes: ONE_SHOT_AND_SERVER,
                supports_resume: false,
                supports_persistent_backend: false,
                permission_policies: DEFAULT_ONLY,
            },
        }
    }

    #[must_use]
    pub fn with_base_url(api_base: impl Into<String>) -> Self {
        let mut backend = Self::new();
        backend.api_base = api_base.into();
        backend
    }
}

impl Default for CodexApiBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ApiBackend for CodexApiBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn is_available(&self) -> bool {
        std::env::var("OPENAI_API_KEY")
            .map(|key| !key.trim().is_empty())
            .unwrap_or(false)
    }

    fn preview_command(
        &self,
        config: &RuntimeConfig,
        request: &ExecutionRequest,
    ) -> Result<CommandSpec> {
        let model = request
            .model
            .clone()
            .or_else(|| config.model.clone())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());

        Ok(CommandSpec {
            program: "codex-api".to_string(),
            args: vec![
                "responses.create".to_string(),
                format!("base={}", self.api_base),
                format!("model={model}"),
            ],
            current_dir: request.work_dir.clone(),
            env: request.extra_env.clone(),
            stdin: None,
        })
    }

    async fn execute_api(
        &self,
        config: &RuntimeConfig,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY is required for the codex-api backend")?;
        let model = request
            .model
            .clone()
            .or_else(|| config.model.clone())
            .unwrap_or_else(|| "gpt-4o-mini".to_string());

        info!(
            backend = self.descriptor.name,
            model, "Executing API-backed request"
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .context("failed to construct reqwest client")?;

        let started = std::time::Instant::now();
        let response = client
            .post(format!("{}/chat/completions", self.api_base))
            .bearer_auth(api_key)
            .json(&serde_json::json!({
                "model": model,
                "messages": [
                    {"role": "user", "content": request.task}
                ]
            }))
            .send()
            .await
            .context("failed to send API-backed request")?;

        let status = response.status();
        let payload: serde_json::Value = response
            .json()
            .await
            .context("failed to parse API-backed response body")?;

        let success = status.is_success();
        let stderr_excerpt = (!success).then(|| {
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        });

        Ok(ExecutionResult {
            success,
            exit_code: if success {
                0
            } else {
                i32::from(status.as_u16())
            },
            duration_ms: started.elapsed().as_millis() as u64,
            backend: self.descriptor.kind,
            transport: TransportMode::OneShotProcess,
            session: None,
            events: vec![ExecutionEvent {
                backend: self.descriptor.kind,
                transport: TransportMode::OneShotProcess,
                payload,
            }],
            stderr_truncated: false,
            stderr_excerpt,
        })
    }
}

pub fn all_api_backends() -> Vec<Box<dyn ApiBackend>> {
    vec![Box::new(CodexApiBackend::new())]
}

pub fn api_backend_capabilities() -> Vec<BackendCapabilities> {
    all_api_backends()
        .into_iter()
        .map(|backend| {
            let descriptor = backend.descriptor();
            BackendCapabilities::from_descriptor(&descriptor, backend.is_available())
        })
        .collect()
}

pub fn select_api_backend(name: Option<&str>) -> Result<Box<dyn ApiBackend>> {
    if let Some(name) = name {
        return all_api_backends()
            .into_iter()
            .find(|backend| backend.descriptor().name == name)
            .ok_or_else(|| wrangle_core::BackendError::NotFound(name.to_string()).into());
    }

    all_api_backends()
        .into_iter()
        .find(|backend| backend.is_available())
        .ok_or_else(|| {
            wrangle_core::BackendError::NotAvailable("no supported API backends found".to_string())
                .into()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_api_descriptor_reports_api_implementation() {
        let backend = CodexApiBackend::new();
        let descriptor = backend.descriptor();
        assert_eq!(descriptor.name, "codex-api");
        assert_eq!(descriptor.implementation, BackendImplementation::Api);
        assert_eq!(descriptor.transport_modes, ONE_SHOT_AND_SERVER);
    }

    #[test]
    fn capabilities_surface_api_implementation() {
        let caps = api_backend_capabilities();
        let codex_api = caps.iter().find(|cap| cap.name == "codex-api").unwrap();
        assert_eq!(codex_api.implementation, BackendImplementation::Api);
        assert_eq!(codex_api.kind, BackendKind::Codex);
    }
}
