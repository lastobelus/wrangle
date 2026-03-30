use anyhow::{Context, Result, anyhow};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};
use which::which;
use wrangle_core::{CommandSpec, ExecutionRequest, ExecutionResult, RuntimeConfig};
use wrangle_server::{
    ServerMetadata, ServerRequest, ServerResponse, can_connect, load_registry, registry_path,
    save_registry, send_request,
};

use crate::subprocess::build_process_env;

const WRANGLE_SERVER_HOST: &str = "127.0.0.1";
const SERVER_WAIT_ATTEMPTS: usize = 50;
const SERVER_WAIT_INTERVAL_MS: u64 = 100;

pub struct WrangleServerTransport;

impl WrangleServerTransport {
    pub fn launcher_available() -> bool {
        discover_wrangle_launcher().is_some()
    }

    pub async fn execute(
        backend_name: &str,
        config: &RuntimeConfig,
        request: ExecutionRequest,
    ) -> Result<ExecutionResult> {
        let server = ensure_server(config, &request).await?;
        let response = send_request(
            &server.addr,
            &ServerRequest::Execute {
                backend_name: backend_name.to_string(),
                config: config.clone(),
                request,
            },
        )
        .await?;

        match response {
            ServerResponse::ExecuteResult { result } => Ok(result),
            ServerResponse::Error { message } => Err(anyhow!(message)),
            ServerResponse::Pong => Err(anyhow!(
                "wrangle server returned health response to execute request"
            )),
        }
    }
}

pub fn preview_wrangle_server_command(
    backend_name: &str,
    request: &ExecutionRequest,
) -> Result<CommandSpec> {
    let program = discover_wrangle_launcher()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "wrangle".to_string());
    Ok(CommandSpec {
        program,
        args: vec![
            "server".to_string(),
            format!("backend={backend_name}"),
            format!("workdir={}", request.work_dir.display()),
        ],
        current_dir: request.work_dir.clone(),
        env: request.extra_env.clone(),
        stdin: None,
    })
}

async fn ensure_server(
    config: &RuntimeConfig,
    request: &ExecutionRequest,
) -> Result<ServerMetadata> {
    let registry_path = registry_path(&request.work_dir)?;
    if let Some(existing) = load_registry(&registry_path)?
        && can_connect(&existing.addr).await
    {
        return Ok(existing);
    }

    let launcher = discover_wrangle_launcher()
        .ok_or_else(|| anyhow!("wrangle server launcher is not available"))?;
    let port = choose_port()?;
    let addr = format!("{WRANGLE_SERVER_HOST}:{port}");

    let mut child = Command::new(&launcher);
    child
        .arg("server")
        .arg("--host")
        .arg(WRANGLE_SERVER_HOST)
        .arg("--port")
        .arg(port.to_string())
        .arg("--workdir")
        .arg(request.work_dir.display().to_string())
        .current_dir(&request.work_dir)
        .env_clear()
        .envs(build_process_env(config, &request.extra_env))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = child.spawn().with_context(|| {
        format!(
            "failed to start wrangle server launcher {}",
            launcher.display()
        )
    })?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow!("failed to determine wrangle server pid"))?;

    wait_for_server(&addr).await?;

    let metadata = ServerMetadata {
        addr,
        pid,
        work_dir: request.work_dir.clone(),
    };
    save_registry(&registry_path, &metadata)?;
    info!(backend = "wrangle-server", addr = %metadata.addr, "Started wrangle server");
    Ok(metadata)
}

fn discover_wrangle_launcher() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("WRANGLE_SERVER_BIN") {
        let path = PathBuf::from(value);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(current) = std::env::current_exe()
        && current
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "wrangle")
    {
        return Some(current);
    }

    if let Ok(path) = which("wrangle") {
        return Some(path);
    }

    None
}

fn choose_port() -> Result<u16> {
    let listener = TcpListener::bind((WRANGLE_SERVER_HOST, 0))
        .context("failed to reserve a local port for wrangle server")?;
    let port = listener
        .local_addr()
        .context("failed to read reserved port")?
        .port();
    drop(listener);
    Ok(port)
}

async fn wait_for_server(addr: &str) -> Result<()> {
    for _ in 0..SERVER_WAIT_ATTEMPTS {
        if can_connect(addr).await {
            return Ok(());
        }
        sleep(Duration::from_millis(SERVER_WAIT_INTERVAL_MS)).await;
    }

    warn!(addr, "Timed out waiting for wrangle server");
    Err(anyhow!("timed out waiting for wrangle server at {addr}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_wrangle_launcher_shape() {
        let request = ExecutionRequest {
            task: "test".to_string(),
            work_dir: PathBuf::from("/tmp"),
            model: None,
            session: None,
            permission_policy: wrangle_core::PermissionPolicy::Default,
            prompt_file: None,
            extra_env: Default::default(),
        };

        let command = preview_wrangle_server_command("qwen", &request).unwrap();
        assert!(command.args.contains(&"server".to_string()));
        assert!(command.args.iter().any(|arg| arg.contains("backend=qwen")));
    }
}
