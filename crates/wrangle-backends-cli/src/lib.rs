use anyhow::Result;
use which::which;
use wrangle_core::{
    AgentBackend, BackendCapabilities, BackendDescriptor, BackendKind, ExecutionError,
    ExecutionRequest, PermissionPolicy, RuntimeConfig, TransportMode,
};
use wrangle_transport::{PersistentBackendTransport, request_to_target};

pub struct CliBackend {
    descriptor: BackendDescriptor,
}

const ONE_SHOT_ONLY: &[TransportMode] = &[TransportMode::OneShotProcess];
const ONE_SHOT_AND_PERSISTENT: &[TransportMode] = &[
    TransportMode::OneShotProcess,
    TransportMode::PersistentBackend,
];
const ALL_POLICIES: &[PermissionPolicy] = &[
    PermissionPolicy::Default,
    PermissionPolicy::Auto,
    PermissionPolicy::Bypass,
];
const DEFAULT_ONLY: &[PermissionPolicy] = &[PermissionPolicy::Default];
const DEFAULT_AND_BYPASS: &[PermissionPolicy] =
    &[PermissionPolicy::Default, PermissionPolicy::Bypass];

impl CliBackend {
    fn new(kind: BackendKind, supports_persistent_backend: bool) -> Self {
        let permission_policies: &'static [PermissionPolicy] = match kind {
            BackendKind::Codex => ALL_POLICIES,
            BackendKind::Claude | BackendKind::Gemini | BackendKind::Qwen => DEFAULT_AND_BYPASS,
            BackendKind::Opencode => DEFAULT_ONLY,
        };
        Self {
            descriptor: BackendDescriptor {
                kind,
                name: kind.as_str(),
                transport_modes: if supports_persistent_backend {
                    ONE_SHOT_AND_PERSISTENT
                } else {
                    ONE_SHOT_ONLY
                },
                supports_resume: true,
                supports_persistent_backend,
                permission_policies,
            },
        }
    }
}

fn map_permission_flag(kind: BackendKind, permission: PermissionPolicy) -> Option<&'static str> {
    match (kind, permission) {
        (_, PermissionPolicy::Default) => None,
        (_, PermissionPolicy::Ask) => None,
        (BackendKind::Codex, PermissionPolicy::Auto) => Some("--auto-edit"),
        (BackendKind::Codex, PermissionPolicy::Bypass) => Some("--full-auto"),
        (BackendKind::Claude, PermissionPolicy::Bypass) => Some("--dangerously-skip-permissions"),
        (BackendKind::Gemini, PermissionPolicy::Bypass) => Some("-y"),
        (BackendKind::Qwen, PermissionPolicy::Bypass) => Some("-y"),
        (BackendKind::Opencode, PermissionPolicy::Auto)
        | (BackendKind::Opencode, PermissionPolicy::Bypass)
        | (BackendKind::Claude, PermissionPolicy::Auto)
        | (BackendKind::Gemini, PermissionPolicy::Auto)
        | (BackendKind::Qwen, PermissionPolicy::Auto) => None,
    }
}

fn build_args(
    kind: BackendKind,
    config: &RuntimeConfig,
    request: &ExecutionRequest,
    target: &str,
    _transport: TransportMode,
) -> Vec<String> {
    let mut args = match kind {
        BackendKind::Codex => vec![
            "e".to_string(),
            "-C".to_string(),
            request.work_dir.display().to_string(),
            "--json".to_string(),
        ],
        BackendKind::Claude => vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
        ],
        BackendKind::Gemini | BackendKind::Qwen => {
            vec!["-o".to_string(), "stream-json".to_string()]
        }
        BackendKind::Opencode => vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ],
    };

    if let Some(model) = request.model.as_ref().or(config.model.as_ref()) {
        match kind {
            BackendKind::Claude => {
                args.push("--model".to_string());
                args.push(model.clone());
            }
            BackendKind::Codex
            | BackendKind::Gemini
            | BackendKind::Qwen
            | BackendKind::Opencode => {
                args.push("-m".to_string());
                args.push(model.clone());
            }
        }
    }

    if let Some(session) = &request.session {
        match kind {
            BackendKind::Codex | BackendKind::Claude | BackendKind::Gemini | BackendKind::Qwen => {
                args.push("-r".to_string());
                args.push(session.id.clone());
            }
            BackendKind::Opencode => {
                args.push("-s".to_string());
                args.push(session.id.clone());
            }
        }
    }

    if let Some(flag) = map_permission_flag(kind, request.permission_policy) {
        if !args.iter().any(|arg| arg == flag) {
            args.push(flag.to_string());
        }
    }

    args.push(target.to_string());
    args
}

impl AgentBackend for CliBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn is_available(&self) -> bool {
        which(self.descriptor.name).is_ok()
    }

    fn build_command(
        &self,
        config: &RuntimeConfig,
        request: &ExecutionRequest,
        transport: TransportMode,
    ) -> Result<wrangle_core::CommandSpec> {
        let (target, stdin) = request_to_target(request)?;
        let args = build_args(self.descriptor.kind, config, request, &target, transport);
        let env = request.extra_env.clone();

        Ok(wrangle_core::CommandSpec {
            program: self.descriptor.name,
            args,
            current_dir: request.work_dir.clone(),
            env,
            stdin,
        })
    }
}

pub fn all_cli_backends() -> Vec<CliBackend> {
    vec![
        CliBackend::new(BackendKind::Codex, false),
        CliBackend::new(BackendKind::Claude, false),
        CliBackend::new(BackendKind::Gemini, false),
        CliBackend::new(BackendKind::Opencode, true),
        CliBackend::new(BackendKind::Qwen, false),
    ]
}

pub fn backend_capabilities() -> Vec<BackendCapabilities> {
    all_cli_backends()
        .into_iter()
        .map(|backend| {
            let descriptor = backend.descriptor();
            let mut caps =
                BackendCapabilities::from_descriptor(&descriptor, backend.is_available());
            if caps.supports_persistent_backend
                && !PersistentBackendTransport::is_persistent_available(&backend)
            {
                caps.supports_persistent_backend = false;
                caps.transport_modes
                    .retain(|m| *m != TransportMode::PersistentBackend);
            }
            caps
        })
        .collect()
}

pub fn select_cli_backend(name: Option<&str>) -> Result<CliBackend> {
    if let Some(name) = name {
        return all_cli_backends()
            .into_iter()
            .find(|backend| backend.descriptor.name == name)
            .ok_or_else(|| wrangle_core::BackendError::NotFound(name.to_string()).into());
    }

    all_cli_backends()
        .into_iter()
        .find(|backend| backend.is_available())
        .ok_or_else(|| {
            wrangle_core::BackendError::NotAvailable("no supported CLI backends found".to_string())
                .into()
        })
}

pub fn ensure_transport_supported(backend: &CliBackend, mode: TransportMode) -> Result<()> {
    if backend.descriptor.transport_modes.contains(&mode) {
        return Ok(());
    }
    Err(ExecutionError::UnsupportedTransport {
        backend: backend.descriptor.name.to_string(),
        transport: match mode {
            TransportMode::OneShotProcess => "one-shot-process",
            TransportMode::PersistentBackend => "persistent-backend",
            TransportMode::WrangleServer => "wrangle-server",
        }
        .to_string(),
    }
    .into())
}

pub fn ensure_permission_supported(backend: &CliBackend, policy: PermissionPolicy) -> Result<()> {
    if backend.descriptor.permission_policies.contains(&policy) {
        return Ok(());
    }
    Err(ExecutionError::UnsupportedPermissionPolicy {
        backend: backend.descriptor.name.to_string(),
        policy: policy.as_str().to_string(),
    }
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wrangle_core::{ExecutionRequest, SessionHandle, SessionState};

    fn sample_request() -> ExecutionRequest {
        ExecutionRequest {
            task: "test".to_string(),
            work_dir: "/tmp".into(),
            model: Some("model-x".to_string()),
            session: Some(SessionHandle {
                id: "abc123".to_string(),
                state: SessionState::Resumable,
                transport: TransportMode::OneShotProcess,
            }),
            permission_policy: PermissionPolicy::Bypass,
            prompt_file: None,
            extra_env: HashMap::new(),
        }
    }

    #[test]
    fn qwen_looks_like_gemini() {
        let backend = CliBackend::new(BackendKind::Qwen, false);
        let args = build_args(
            backend.descriptor.kind,
            &RuntimeConfig::default(),
            &sample_request(),
            "target",
            TransportMode::OneShotProcess,
        );
        assert!(args.contains(&"stream-json".to_string()));
        assert!(args.contains(&"-r".to_string()));
    }

    #[test]
    fn opencode_persistent_build_args_match_one_shot_shape() {
        let backend = CliBackend::new(BackendKind::Opencode, true);
        let one_shot = build_args(
            backend.descriptor.kind,
            &RuntimeConfig::default(),
            &sample_request(),
            "target",
            TransportMode::OneShotProcess,
        );
        let persistent = build_args(
            backend.descriptor.kind,
            &RuntimeConfig::default(),
            &sample_request(),
            "target",
            TransportMode::PersistentBackend,
        );
        assert_eq!(one_shot, persistent);
    }

    #[test]
    fn opencode_descriptor_supports_persistent() {
        let backend = CliBackend::new(BackendKind::Opencode, true);
        assert!(backend.descriptor.supports_persistent_backend);
        assert!(
            backend
                .descriptor
                .transport_modes
                .contains(&TransportMode::PersistentBackend)
        );
    }

    #[test]
    fn codex_descriptor_does_not_support_persistent() {
        let backend = CliBackend::new(BackendKind::Codex, false);
        assert!(!backend.descriptor.supports_persistent_backend);
    }

    #[test]
    fn backend_capabilities_reflects_actual_persistent_availability() {
        let caps = backend_capabilities();
        let opencode = caps.iter().find(|c| c.name == "opencode").unwrap();
        if which("opencode").is_ok() {
            assert!(opencode.supports_persistent_backend);
            assert!(
                opencode
                    .transport_modes
                    .contains(&TransportMode::PersistentBackend)
            );
        } else {
            assert!(!opencode.available);
        }
    }

    #[test]
    fn non_opencode_backends_do_not_advertise_persistent() {
        let caps = backend_capabilities();
        for cap in &caps {
            if cap.name != "opencode" {
                assert!(!cap.supports_persistent_backend);
                assert!(
                    !cap.transport_modes
                        .contains(&TransportMode::PersistentBackend)
                );
            }
        }
    }
}
