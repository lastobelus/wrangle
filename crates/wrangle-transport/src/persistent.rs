use anyhow::Result;
use async_trait::async_trait;
use tracing::{info, warn};
use wrangle_core::{
    AgentBackend, BackendTransport, ExecutionRequest, ExecutionResult, RuntimeConfig, SessionState,
    TransportMode,
};

use crate::subprocess::{SubprocessTransport, TRANSPORT_LABEL_PERSISTENT, run_subprocess};

#[derive(Debug, Default)]
pub struct PersistentBackendTransport {
    fallback: SubprocessTransport,
}

impl PersistentBackendTransport {
    pub fn new() -> Self {
        Self {
            fallback: SubprocessTransport,
        }
    }

    pub fn is_persistent_available(backend: &dyn AgentBackend) -> bool {
        if !backend.is_available() {
            return false;
        }
        backend.descriptor().supports_persistent_backend
    }
}

#[async_trait]
impl BackendTransport for PersistentBackendTransport {
    fn mode(&self) -> TransportMode {
        TransportMode::PersistentBackend
    }

    async fn execute(
        &self,
        backend: &dyn AgentBackend,
        config: &RuntimeConfig,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult> {
        if !Self::is_persistent_available(backend) {
            info!(
                backend = backend.descriptor().name,
                "Persistent backend not available, falling back to one-shot"
            );
            return self.fallback.execute(backend, config, request).await;
        }

        let descriptor = backend.descriptor();
        info!(
            backend = descriptor.name,
            transport = TRANSPORT_LABEL_PERSISTENT,
            "Attempting persistent backend execution"
        );

        match run_subprocess(
            backend,
            config,
            request.clone(),
            TransportMode::PersistentBackend,
            SessionState::PersistentAttached,
            TRANSPORT_LABEL_PERSISTENT,
        )
        .await
        {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    backend = descriptor.name,
                    error = %err,
                    "Persistent backend execution failed, falling back to one-shot"
                );
                let fallback_request = ExecutionRequest {
                    session: None,
                    ..request
                };
                self.fallback
                    .execute(backend, config, fallback_request)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use wrangle_core::{
        BackendDescriptor, BackendKind, CommandSpec, ExecutionRequest, PermissionPolicy,
    };

    struct MockBackend {
        descriptor: BackendDescriptor,
        available: bool,
    }

    impl MockBackend {
        fn new(kind: BackendKind, persistent: bool, available: bool) -> Self {
            Self {
                descriptor: BackendDescriptor {
                    kind,
                    name: kind.as_str(),
                    transport_modes: if persistent {
                        &[
                            TransportMode::OneShotProcess,
                            TransportMode::PersistentBackend,
                        ]
                    } else {
                        &[TransportMode::OneShotProcess]
                    },
                    supports_resume: true,
                    supports_persistent_backend: persistent,
                    permission_policies: &[PermissionPolicy::Default],
                },
                available,
            }
        }
    }

    impl AgentBackend for MockBackend {
        fn descriptor(&self) -> BackendDescriptor {
            self.descriptor.clone()
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn build_command(
            &self,
            _config: &RuntimeConfig,
            request: &ExecutionRequest,
            transport: TransportMode,
        ) -> Result<CommandSpec> {
            let mut args = vec![
                "run".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ];
            if transport == TransportMode::PersistentBackend {
                args.push("--server".to_string());
            }
            args.push(request.task.clone());
            Ok(CommandSpec {
                program: "echo",
                args,
                current_dir: request.work_dir.clone(),
                env: HashMap::new(),
                stdin: None,
            })
        }
    }

    fn sample_request() -> ExecutionRequest {
        ExecutionRequest {
            task: "test task".to_string(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: None,
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        }
    }

    #[test]
    fn persistent_available_for_opencode_with_flag() {
        let backend = MockBackend::new(BackendKind::Opencode, true, true);
        assert!(PersistentBackendTransport::is_persistent_available(
            &backend
        ));
    }

    #[test]
    fn persistent_unavailable_when_not_supported() {
        let backend = MockBackend::new(BackendKind::Codex, false, true);
        assert!(!PersistentBackendTransport::is_persistent_available(
            &backend
        ));
    }

    #[test]
    fn persistent_unavailable_when_binary_missing() {
        let backend = MockBackend::new(BackendKind::Opencode, true, false);
        assert!(!PersistentBackendTransport::is_persistent_available(
            &backend
        ));
    }

    #[test]
    fn transport_mode_returns_persistent_backend() {
        let transport = PersistentBackendTransport::new();
        assert_eq!(transport.mode(), TransportMode::PersistentBackend);
    }

    #[tokio::test]
    async fn falls_back_to_one_shot_when_not_available() {
        let transport = PersistentBackendTransport::new();
        let backend = MockBackend::new(BackendKind::Codex, false, true);
        let config = RuntimeConfig::default();
        let request = sample_request();

        let result = transport.execute(&backend, &config, request).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.transport, TransportMode::OneShotProcess);
    }

    #[tokio::test]
    async fn persistent_execution_uses_persistent_transport() {
        let transport = PersistentBackendTransport::new();
        let backend = MockBackend::new(BackendKind::Opencode, true, true);
        let config = RuntimeConfig::default();
        let request = sample_request();

        let result = transport.execute(&backend, &config, request).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.transport, TransportMode::PersistentBackend);
    }

    #[test]
    fn session_state_is_persistent_attached_for_persistent_mode() {
        let handle = wrangle_core::SessionHandle {
            id: "test-session".to_string(),
            state: SessionState::PersistentAttached,
            transport: TransportMode::PersistentBackend,
        };
        assert_eq!(handle.state, SessionState::PersistentAttached);
        assert_eq!(handle.transport, TransportMode::PersistentBackend);
    }
}
