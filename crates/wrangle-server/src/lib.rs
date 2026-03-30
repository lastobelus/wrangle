use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use wrangle_core::{ExecutionRequest, ExecutionResult, RuntimeConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerRequest {
    Health,
    Execute {
        backend_name: String,
        config: RuntimeConfig,
        request: ExecutionRequest,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerResponse {
    Pong,
    ExecuteResult { result: ExecutionResult },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMetadata {
    pub addr: String,
    pub pid: u32,
    pub work_dir: PathBuf,
}

pub async fn send_request(addr: &str, request: &ServerRequest) -> Result<ServerResponse> {
    let mut stream = TcpStream::connect(addr)
        .await
        .with_context(|| format!("failed to connect to wrangle server at {addr}"))?;

    let request_json = serde_json::to_vec(request)?;
    stream.write_all(&request_json).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = reader.read_line(&mut line).await?;
    if read == 0 {
        return Err(anyhow!(
            "wrangle server closed connection without a response"
        ));
    }

    serde_json::from_str(line.trim_end())
        .with_context(|| format!("failed to parse wrangle server response from {addr}"))
}

pub async fn can_connect(addr: &str) -> bool {
    TcpStream::connect(addr).await.is_ok()
}

pub fn registry_root() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".wrangle").join("server"))
}

pub fn registry_path(work_dir: &Path) -> Result<PathBuf> {
    let canonical = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    Ok(registry_root()?.join(format!("{:x}.json", hasher.finish())))
}

pub fn load_registry(path: &Path) -> Result<Option<ServerMetadata>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read wrangle server registry: {}", path.display()))?;
    let metadata = serde_json::from_str(&content).with_context(|| {
        format!(
            "failed to parse wrangle server registry: {}",
            path.display()
        )
    })?;
    Ok(Some(metadata))
}

pub fn save_registry(path: &Path, metadata: &ServerMetadata) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(metadata)?).with_context(|| {
        format!(
            "failed to write wrangle server registry: {}",
            path.display()
        )
    })?;
    Ok(())
}
