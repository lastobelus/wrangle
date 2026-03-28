use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::task::JoinSet;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use wrangle_backends_cli::{ensure_transport_supported, select_cli_backend};
use wrangle_core::{
    BackendTransport, ExecutionRequest, ParallelTaskSpec, PermissionPolicy, RuntimeConfig,
    RuntimeMode, SessionHandle, SessionState, TransportMode, ensure_parallel_tasks,
    get_default_max_parallel_workers, parse_parallel_config, read_stdin_task,
    resolve_agent_for_runtime_config,
};
use wrangle_transport::SubprocessTransport;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TransportArg {
    OneShotProcess,
    PersistentBackend,
    WrangleServer,
}

impl From<TransportArg> for TransportMode {
    fn from(value: TransportArg) -> Self {
        match value {
            TransportArg::OneShotProcess => TransportMode::OneShotProcess,
            TransportArg::PersistentBackend => TransportMode::PersistentBackend,
            TransportArg::WrangleServer => TransportMode::WrangleServer,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PermissionArg {
    Default,
    Bypass,
}

impl From<PermissionArg> for PermissionPolicy {
    fn from(value: PermissionArg) -> Self {
        match value {
            PermissionArg::Default => PermissionPolicy::Default,
            PermissionArg::Bypass => PermissionPolicy::Bypass,
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "wrangle")]
#[command(version = VERSION)]
#[command(about = "Generalized agent execution and orchestration for local CLI backends")]
struct Cli {
    #[arg(value_name = "TASK")]
    task: Option<String>,

    #[arg(value_name = "WORKDIR")]
    workdir: Option<String>,

    #[arg(long, short = 'b', env = "WRANGLE_BACKEND")]
    backend: Option<String>,

    #[arg(long, short = 'm', env = "WRANGLE_MODEL")]
    model: Option<String>,

    #[arg(long, short = 'a', env = "WRANGLE_AGENT")]
    agent: Option<String>,

    #[arg(long, value_name = "PATH")]
    prompt_file: Option<String>,

    #[arg(
        long,
        value_enum,
        default_value = "default",
        env = "WRANGLE_PERMISSION_POLICY"
    )]
    permission_policy: PermissionArg,

    #[arg(
        long,
        value_enum,
        default_value = "one-shot-process",
        env = "WRANGLE_TRANSPORT"
    )]
    transport: TransportArg,

    #[arg(long, default_value = "7200", env = "WRANGLE_TIMEOUT_SECS")]
    timeout: u64,

    #[arg(long, env = "WRANGLE_INHERIT_ENV")]
    inherit_env: bool,

    #[arg(long, env = "WRANGLE_ALLOW_TASK_PROMPT_FILES")]
    allow_task_prompt_files: bool,

    #[arg(long, default_value = "512", env = "WRANGLE_MAX_EVENTS")]
    max_events: usize,

    #[arg(long, default_value = "32768", env = "WRANGLE_MAX_STDERR_BYTES")]
    max_stderr_bytes: usize,

    #[arg(long, env = "WRANGLE_MAX_PARALLEL_WORKERS")]
    max_parallel_workers: Option<usize>,

    #[arg(long)]
    parallel: bool,

    #[arg(long, short = 'q', env = "WRANGLE_QUIET")]
    quiet: bool,

    #[arg(long, short = 'd', env = "WRANGLE_DEBUG")]
    debug: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
enum Command {
    Resume {
        session_id: String,
        task: String,
        workdir: Option<String>,
    },
}

fn setup_logging(cli: &Cli) -> Result<Option<WorkerGuard>> {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".wrangle")
        .join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let level = if cli.debug {
        Level::DEBUG
    } else if cli.quiet {
        Level::ERROR
    } else {
        Level::INFO
    };

    let file_appender = tracing_appender::rolling::daily(&log_dir, "wrangle.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false));

    if cli.quiet {
        subscriber.init();
    } else {
        subscriber
            .with(
                fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_target(false)
                    .compact(),
            )
            .init();
    }

    Ok(Some(guard))
}

fn runtime_config_from_cli(cli: &Cli, workdir: Option<&str>, mode: RuntimeMode) -> RuntimeConfig {
    RuntimeConfig {
        mode,
        backend: cli.backend.clone(),
        agent: cli.agent.clone(),
        model: cli.model.clone(),
        work_dir: workdir
            .map(PathBuf::from)
            .or_else(|| cli.workdir.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
        timeout_secs: cli.timeout,
        quiet: cli.quiet,
        debug: cli.debug,
        transport_mode: cli.transport.into(),
        permission_policy: cli.permission_policy.into(),
        allow_task_prompt_files: cli.allow_task_prompt_files,
        inherit_env: cli.inherit_env,
        max_events: cli.max_events,
        max_stderr_bytes: cli.max_stderr_bytes,
        max_parallel_workers: cli.max_parallel_workers,
    }
}

fn request_from_task(
    config: &RuntimeConfig,
    task: String,
    prompt_file: Option<String>,
    session: Option<SessionHandle>,
) -> ExecutionRequest {
    ExecutionRequest {
        task,
        work_dir: config.work_dir.clone(),
        model: config.model.clone(),
        session,
        permission_policy: config.permission_policy,
        prompt_file: prompt_file.map(PathBuf::from),
        extra_env: HashMap::new(),
    }
}

async fn execute_request(
    backend: &wrangle_backends_cli::CliBackend,
    config: &RuntimeConfig,
    request: ExecutionRequest,
) -> Result<wrangle_core::ExecutionResult> {
    match config.transport_mode {
        TransportMode::OneShotProcess => {
            let transport = SubprocessTransport;
            transport.execute(backend, config, request).await
        }
        TransportMode::PersistentBackend => {
            bail!(
                "persistent backend transport is designed into wrangle, but not implemented yet in v1"
            );
        }
        TransportMode::WrangleServer => {
            bail!("wrangle server transport is reserved for a future release");
        }
    }
}

async fn run_single(mut config: RuntimeConfig, request: ExecutionRequest) -> Result<()> {
    resolve_agent_for_runtime_config(&mut config).await?;
    let backend = select_cli_backend(config.backend.as_deref())?;
    ensure_transport_supported(&backend, config.transport_mode)?;
    let result = execute_request(&backend, &config, request).await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    if !result.success {
        std::process::exit(1);
    }
    Ok(())
}

async fn run_parallel(mut config: RuntimeConfig) -> Result<()> {
    resolve_agent_for_runtime_config(&mut config).await?;
    let parsed = parse_parallel_config().await?;
    ensure_parallel_tasks(&parsed)?;

    let max_workers = config
        .max_parallel_workers
        .unwrap_or_else(get_default_max_parallel_workers);

    let mut pending: Vec<ParallelTaskSpec> = parsed.tasks.clone();
    let mut results = HashMap::<String, serde_json::Value>::new();
    let mut tasks = JoinSet::new();

    while !pending.is_empty() || !tasks.is_empty() {
        while tasks.len() < max_workers {
            let Some(index) = pending.iter().position(|task| {
                task.dependencies
                    .iter()
                    .all(|dep| results.contains_key(dep))
            }) else {
                break;
            };

            let spec = pending.remove(index);
            let mut task_config = config.clone();
            task_config.backend = spec.backend.clone().or_else(|| config.backend.clone());
            task_config.agent = spec.agent.clone().or_else(|| config.agent.clone());
            task_config.model = spec.model.clone().or_else(|| config.model.clone());
            task_config.transport_mode = spec.transport_mode.unwrap_or(config.transport_mode);
            task_config.permission_policy =
                spec.permission_policy.unwrap_or(config.permission_policy);

            let request = spec.to_request(&task_config)?;
            let task_id = spec.id.clone();

            tasks.spawn(async move {
                let mut resolved = task_config;
                resolve_agent_for_runtime_config(&mut resolved).await?;
                let backend = select_cli_backend(resolved.backend.as_deref())?;
                ensure_transport_supported(&backend, resolved.transport_mode)?;
                let result = execute_request(&backend, &resolved, request).await?;
                Ok::<(String, serde_json::Value), anyhow::Error>((
                    task_id,
                    serde_json::to_value(result)?,
                ))
            });
        }

        let Some(outcome) = tasks.join_next().await else {
            if !pending.is_empty() {
                bail!("circular dependency detected in parallel tasks");
            }
            break;
        };

        let (task_id, result) = outcome??;
        results.insert(task_id, result);
    }

    let ordered: Vec<serde_json::Value> = parsed
        .tasks
        .iter()
        .filter_map(|task| results.remove(&task.id))
        .collect();

    println!("{}", serde_json::to_string_pretty(&ordered)?);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let _guard = setup_logging(&cli)?;

    match &cli.command {
        Some(Command::Resume {
            session_id,
            task,
            workdir,
        }) => {
            let task = if task == "-" {
                read_stdin_task().await?
            } else {
                task.clone()
            };
            let config = runtime_config_from_cli(&cli, workdir.as_deref(), RuntimeMode::Resume);
            let session = Some(SessionHandle {
                id: session_id.clone(),
                state: SessionState::Resumable,
                transport: config.transport_mode,
            });
            let request = request_from_task(&config, task, cli.prompt_file.clone(), session);
            run_single(config, request).await?;
        }
        None if cli.parallel => {
            let config = runtime_config_from_cli(&cli, None, RuntimeMode::New);
            run_parallel(config).await?;
        }
        None => {
            let task = match &cli.task {
                Some(task) if task == "-" => read_stdin_task().await?,
                Some(task) => task.clone(),
                None => bail!("no task provided"),
            };
            let config = runtime_config_from_cli(&cli, None, RuntimeMode::New);
            let request = request_from_task(&config, task, cli.prompt_file.clone(), None);
            run_single(config, request).await?;
        }
    }

    Ok(())
}
