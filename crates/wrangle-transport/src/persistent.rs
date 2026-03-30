use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};
use wrangle_core::{
    AgentBackend, BackendDescriptor, BackendKind, BackendTransport, CommandSpec, ExecutionRequest,
    ExecutionResult, RuntimeConfig, SessionState, TransportMode,
};

use crate::subprocess::{
    SubprocessTransport, TRANSPORT_LABEL_PERSISTENT, build_process_env, request_to_target,
    run_subprocess,
};

const OPENCODE_ATTACH_HOST: &str = "127.0.0.1";
const SERVER_WAIT_ATTEMPTS: usize = 50;
const SERVER_WAIT_INTERVAL_MS: u64 = 100;

#[derive(Debug, Default)]
pub struct PersistentBackendTransport {
    fallback: SubprocessTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpencodeServerMetadata {
    url: String,
    port: u16,
    pid: u32,
    work_dir: PathBuf,
}

struct OpencodePersistentClientBackend {
    descriptor: BackendDescriptor,
    attach_url: String,
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

        let descriptor = backend.descriptor();
        if !descriptor.supports_persistent_backend || descriptor.kind != BackendKind::Opencode {
            return false;
        }

        opencode_persistent_cli_available()
    }

    async fn ensure_opencode_server(
        &self,
        config: &RuntimeConfig,
        request: &ExecutionRequest,
    ) -> Result<OpencodeServerMetadata> {
        let registry_path = registry_path(&request.work_dir)?;
        if let Some(existing) = load_registry(&registry_path)? {
            if can_connect(existing.port).await {
                return Ok(existing);
            }

            let _ = std::fs::remove_file(&registry_path);
        }

        let port = choose_port()?;
        let url = format!("http://{OPENCODE_ATTACH_HOST}:{port}");
        let mut child = Command::new("opencode");
        child
            .arg("serve")
            .arg("--hostname")
            .arg(OPENCODE_ATTACH_HOST)
            .arg("--port")
            .arg(port.to_string())
            .current_dir(&request.work_dir)
            .env_clear()
            .envs(build_process_env(config, &request.extra_env))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let server = child
            .spawn()
            .context("failed to start opencode persistent server")?;
        let pid = server
            .id()
            .ok_or_else(|| anyhow!("failed to determine opencode server pid"))?;

        wait_for_server(port).await?;

        let metadata = OpencodeServerMetadata {
            url,
            port,
            pid,
            work_dir: request.work_dir.clone(),
        };
        save_registry(&registry_path, &metadata)?;
        Ok(metadata)
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
        let server = match self.ensure_opencode_server(config, &request).await {
            Ok(server) => server,
            Err(err) => {
                warn!(
                    backend = descriptor.name,
                    error = %err,
                    "Failed to start or attach to persistent server, falling back to one-shot"
                );
                return self.fallback.execute(backend, config, request).await;
            }
        };

        let attached_backend = OpencodePersistentClientBackend {
            descriptor,
            attach_url: server.url,
        };

        match run_subprocess(
            &attached_backend,
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
                    backend = attached_backend.descriptor.name,
                    error = %err,
                    "Persistent backend execution failed, falling back to one-shot"
                );
                self.fallback.execute(backend, config, request).await
            }
        }
    }
}

impl AgentBackend for OpencodePersistentClientBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn is_available(&self) -> bool {
        true
    }

    fn build_command(
        &self,
        config: &RuntimeConfig,
        request: &ExecutionRequest,
        transport: TransportMode,
    ) -> Result<CommandSpec> {
        assert_eq!(transport, TransportMode::PersistentBackend);
        let (target, stdin) = request_to_target(request)?;
        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--attach".to_string(),
            self.attach_url.clone(),
            "--dir".to_string(),
            request.work_dir.display().to_string(),
        ];

        if let Some(model) = request.model.as_ref().or(config.model.as_ref()) {
            args.push("-m".to_string());
            args.push(model.clone());
        }

        if let Some(agent) = config.agent.as_ref() {
            args.push("--agent".to_string());
            args.push(agent.clone());
        }

        if let Some(session) = &request.session {
            args.push("-s".to_string());
            args.push(session.id.clone());
        }

        args.push(target);

        Ok(CommandSpec {
            program: "opencode",
            args,
            current_dir: request.work_dir.clone(),
            env: request.extra_env.clone(),
            stdin,
        })
    }
}

pub fn preview_persistent_command(
    descriptor: BackendDescriptor,
    config: &RuntimeConfig,
    request: &ExecutionRequest,
) -> Result<CommandSpec> {
    let backend = OpencodePersistentClientBackend {
        descriptor,
        attach_url: "http://127.0.0.1:<managed-by-wrangle>".to_string(),
    };
    backend.build_command(config, request, TransportMode::PersistentBackend)
}

fn opencode_persistent_cli_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let serve_help = std::process::Command::new("opencode")
            .args(["serve", "--help"])
            .output();
        let run_help = std::process::Command::new("opencode")
            .args(["run", "--help"])
            .output();

        match (serve_help, run_help) {
            (Ok(serve), Ok(run)) if serve.status.success() && run.status.success() => {
                let serve_text = String::from_utf8_lossy(&serve.stdout);
                let run_text = String::from_utf8_lossy(&run.stdout);
                serve_text.contains("headless opencode server") && run_text.contains("--attach")
            }
            _ => false,
        }
    })
}

fn choose_port() -> Result<u16> {
    let listener = TcpListener::bind((OPENCODE_ATTACH_HOST, 0))
        .context("failed to reserve a local port for opencode server")?;
    let port = listener
        .local_addr()
        .context("failed to read reserved port")?
        .port();
    drop(listener);
    Ok(port)
}

async fn wait_for_server(port: u16) -> Result<()> {
    for _ in 0..SERVER_WAIT_ATTEMPTS {
        if can_connect(port).await {
            return Ok(());
        }
        sleep(Duration::from_millis(SERVER_WAIT_INTERVAL_MS)).await;
    }

    Err(anyhow!(
        "timed out waiting for opencode server on {OPENCODE_ATTACH_HOST}:{port}"
    ))
}

async fn can_connect(port: u16) -> bool {
    TcpStream::connect((OPENCODE_ATTACH_HOST, port))
        .await
        .is_ok()
}

fn registry_root() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join(".wrangle")
        .join("opencode-persistent"))
}

fn registry_path(work_dir: &Path) -> Result<PathBuf> {
    let root = registry_root()?;
    let canonical = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    Ok(root.join(format!("{:x}.json", hasher.finish())))
}

fn load_registry(path: &Path) -> Result<Option<OpencodeServerMetadata>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read opencode registry: {}", path.display()))?;
    let metadata = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse opencode registry: {}", path.display()))?;
    Ok(Some(metadata))
}

fn save_registry(path: &Path, metadata: &OpencodeServerMetadata) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create opencode persistent registry dir: {}",
                parent.display()
            )
        })?;
    }

    let content = serde_json::to_vec_pretty(metadata)?;
    std::fs::write(path, content)
        .with_context(|| format!("failed to write opencode registry: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wrangle_core::PermissionPolicy;
    use wrangle_core::{ExecutionRequest, SessionHandle};

    fn sample_request() -> ExecutionRequest {
        ExecutionRequest {
            task: "echo $HOME".to_string(),
            work_dir: PathBuf::from("/tmp/project"),
            model: Some("model-x".to_string()),
            session: Some(SessionHandle {
                id: "sess-123".to_string(),
                state: SessionState::PersistentAttached,
                transport: TransportMode::PersistentBackend,
            }),
            permission_policy: PermissionPolicy::Default,
            prompt_file: None,
            extra_env: HashMap::new(),
        }
    }

    #[test]
    fn opencode_persistent_backend_builds_attach_command() {
        let backend = OpencodePersistentClientBackend {
            descriptor: BackendDescriptor {
                kind: BackendKind::Opencode,
                name: "opencode",
                transport_modes: &[
                    TransportMode::OneShotProcess,
                    TransportMode::PersistentBackend,
                ],
                supports_resume: true,
                supports_persistent_backend: true,
                permission_policies: &[PermissionPolicy::Default],
            },
            attach_url: "http://127.0.0.1:4099".to_string(),
        };

        let command = backend
            .build_command(
                &RuntimeConfig::default(),
                &sample_request(),
                TransportMode::PersistentBackend,
            )
            .unwrap();

        assert_eq!(command.program, "opencode");
        assert!(command.args.contains(&"--attach".to_string()));
        assert!(command.args.contains(&"http://127.0.0.1:4099".to_string()));
        assert!(command.args.contains(&"--dir".to_string()));
        assert!(command.args.contains(&"/tmp/project".to_string()));
        assert!(command.args.contains(&"-s".to_string()));
        assert!(command.args.contains(&"sess-123".to_string()));
        assert!(command.args.contains(&"-m".to_string()));
        assert!(command.args.contains(&"model-x".to_string()));
        assert_eq!(command.stdin, Some(b"echo $HOME".to_vec()));
        assert!(command.args.contains(&"-".to_string()));
    }

    #[test]
    fn persistent_available_requires_opencode() {
        struct MockBackend;

        impl AgentBackend for MockBackend {
            fn descriptor(&self) -> BackendDescriptor {
                BackendDescriptor {
                    kind: BackendKind::Qwen,
                    name: "qwen",
                    transport_modes: &[TransportMode::OneShotProcess],
                    supports_resume: true,
                    supports_persistent_backend: false,
                    permission_policies: &[PermissionPolicy::Default],
                }
            }

            fn is_available(&self) -> bool {
                true
            }

            fn build_command(
                &self,
                _config: &RuntimeConfig,
                _request: &ExecutionRequest,
                _transport: TransportMode,
            ) -> Result<CommandSpec> {
                unreachable!()
            }
        }

        assert!(!PersistentBackendTransport::is_persistent_available(
            &MockBackend
        ));
    }

    #[test]
    fn registry_path_is_stable_for_same_workdir() {
        let first = registry_path(Path::new("/tmp/project")).unwrap();
        let second = registry_path(Path::new("/tmp/project")).unwrap();
        assert_eq!(first, second);
        assert!(
            first
                .to_string_lossy()
                .contains(".wrangle/opencode-persistent")
        );
    }
}
