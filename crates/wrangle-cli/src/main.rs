use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use wrangle_core::{
    ExecutionRequest, PermissionPolicy, RuntimeConfig, RuntimeMode, SessionHandle, SessionState,
    TransportMode, discover_config, parse_parallel_config, read_stdin_task,
};
use wrangle_runner::{
    PlaybookInvocation, PlaybookName, available_backends, build_playbook_plan, execute_parallel,
    execute_request, find_backend, preview_parallel, preview_request,
};
use wrangle_server::{ServerRequest, ServerResponse};

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
    Ask,
    Auto,
    Bypass,
}

impl From<PermissionArg> for PermissionPolicy {
    fn from(value: PermissionArg) -> Self {
        match value {
            PermissionArg::Default => PermissionPolicy::Default,
            PermissionArg::Ask => PermissionPolicy::Ask,
            PermissionArg::Auto => PermissionPolicy::Auto,
            PermissionArg::Bypass => PermissionPolicy::Bypass,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PlaybookArg {
    LandWork,
}

impl From<PlaybookArg> for PlaybookName {
    fn from(value: PlaybookArg) -> Self {
        match value {
            PlaybookArg::LandWork => PlaybookName::LandWork,
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
        help = "Permission policy to request. Note: 'ask' is reserved and currently unsupported by all backends; 'auto' is currently only supported by Codex.",
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

    #[arg(
        long,
        value_name = "PATH",
        help = "Write intermediate backend events to this file as JSON Lines.",
        env = "WRANGLE_PROGRESS_FILE"
    )]
    progress_file: Option<String>,

    #[arg(
        long,
        help = "Suppress wrangle's own non-final stderr logging until the final stdout result is ready.",
        env = "WRANGLE_QUIET_UNTIL_COMPLETE"
    )]
    quiet_until_complete: bool,

    #[arg(long, env = "WRANGLE_MAX_PARALLEL_WORKERS")]
    max_parallel_workers: Option<usize>,

    #[arg(long)]
    parallel: bool,

    #[arg(long)]
    dry_run: bool,

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
    Backends {
        #[arg(long)]
        json: bool,
    },
    ConfigPaths {
        #[arg(long)]
        json: bool,
    },
    Server {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        workdir: String,
    },
    Playbook {
        #[arg(value_enum)]
        name: PlaybookArg,
        task: String,
        workdir: Option<String>,
    },
}

async fn setup_logging(cli: &Cli) -> Result<Option<WorkerGuard>> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let discovery = discover_config(&cwd).await?;
    let log_dir = discovery.log_dir;
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

    if cli.quiet || cli.quiet_until_complete {
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
        progress_file: cli.progress_file.as_ref().map(PathBuf::from),
        quiet_until_complete: cli.quiet_until_complete,
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

async fn print_or_run(cli: &Cli, config: RuntimeConfig, request: ExecutionRequest) -> Result<()> {
    if cli.dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&preview_request(config, request).await?)?
        );
        return Ok(());
    }

    let result = execute_request(config, request).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    if !result.success {
        std::process::exit(1);
    }
    Ok(())
}

#[derive(Default)]
struct ServerState {
    sessions: tokio::sync::Mutex<HashMap<String, SessionHandle>>,
    next_session: std::sync::atomic::AtomicU64,
}

fn next_server_session_id(state: &ServerState) -> String {
    let counter = state
        .next_session
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("srv-{millis}-{counter}")
}

fn inner_transport_for(backend_name: &str) -> TransportMode {
    let _ = find_backend(backend_name);
    TransportMode::OneShotProcess
}

async fn handle_server_request(state: &ServerState, message: ServerRequest) -> ServerResponse {
    match message {
        ServerRequest::Health => ServerResponse::Pong,
        ServerRequest::Execute {
            backend_name,
            mut config,
            mut request,
        } => {
            config.backend = Some(backend_name.clone());
            config.transport_mode = inner_transport_for(&backend_name);

            let outer_session_id = request.session.as_ref().map(|session| session.id.clone());
            if let Some(session) = request.session.as_ref()
                && session.transport == TransportMode::WrangleServer
            {
                let sessions = state.sessions.lock().await;
                request.session = sessions.get(&session.id).cloned();
            }

            match execute_request(config, request).await {
                Ok(mut result) => {
                    result.transport = TransportMode::WrangleServer;
                    if let Some(inner) = result.session.clone() {
                        let outer =
                            outer_session_id.unwrap_or_else(|| next_server_session_id(state));
                        let outer_handle = SessionHandle {
                            id: outer.clone(),
                            state: SessionState::ServerAttached,
                            transport: TransportMode::WrangleServer,
                        };
                        state.sessions.lock().await.insert(outer, inner);
                        result.session = Some(outer_handle);
                    }
                    ServerResponse::ExecuteResult { result }
                }
                Err(err) => ServerResponse::Error {
                    message: err.to_string(),
                },
            }
        }
    }
}

async fn run_server(host: &str, port: u16, workdir: &str) -> Result<()> {
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    std::env::set_current_dir(workdir)?;
    let state = Arc::new(ServerState::default());

    loop {
        let (socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let (reader, mut writer) = socket.into_split();
            let mut reader = tokio::io::BufReader::new(reader);
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return;
            }

            let response = match serde_json::from_str::<ServerRequest>(line.trim_end()) {
                Ok(message) => handle_server_request(&state, message).await,
                Err(err) => ServerResponse::Error {
                    message: format!("failed to parse wrangle server request: {err}"),
                },
            };

            if let Ok(json) = serde_json::to_vec(&response) {
                let _ = writer.write_all(&json).await;
                let _ = writer.write_all(b"\n").await;
                let _ = writer.flush().await;
            }
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let _guard = setup_logging(&cli).await?;

    match &cli.command {
        Some(Command::Backends { json: _ }) => {
            println!("{}", serde_json::to_string_pretty(&available_backends())?);
            return Ok(());
        }
        Some(Command::ConfigPaths { json }) => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let discovery = discover_config(&cwd).await?;
            if *json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "projectDir": discovery.project_dir,
                        "homeDir": discovery.home_dir,
                        "activeModelsFile": discovery.active_models_file,
                        "activeSettingsFile": discovery.active_settings_file,
                        "logDir": discovery.log_dir,
                    }))?
                );
            } else {
                println!("projectDir: {:?}", discovery.project_dir);
                println!("homeDir: {}", discovery.home_dir.display());
                println!(
                    "activeModelsFile: {}",
                    discovery.active_models_file.display()
                );
                println!(
                    "activeSettingsFile: {}",
                    discovery
                        .active_settings_file
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<none>".to_string())
                );
                println!("logDir: {}", discovery.log_dir.display());
            }
            return Ok(());
        }
        Some(Command::Server {
            host,
            port,
            workdir,
        }) => {
            run_server(host, *port, workdir).await?;
            return Ok(());
        }
        Some(Command::Playbook {
            name,
            task,
            workdir,
        }) => {
            let config = runtime_config_from_cli(&cli, workdir.as_deref(), RuntimeMode::New);
            let invocation = PlaybookInvocation {
                name: (*name).into(),
                task: task.clone(),
                work_dir: config.work_dir.clone(),
                backend: cli.backend.clone(),
                model: cli.model.clone(),
                agent: cli.agent.clone(),
                permission_policy: cli.permission_policy.into(),
                transport_mode: cli.transport.into(),
            };

            if cli.dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&build_playbook_plan(&config, invocation))?
                );
                return Ok(());
            }

            let (config, request) = wrangle_runner::build_playbook(&config, invocation);
            print_or_run(&cli, config, request).await?;
            return Ok(());
        }
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
            print_or_run(&cli, config, request).await?;
            return Ok(());
        }
        None if cli.parallel => {
            let config = runtime_config_from_cli(&cli, None, RuntimeMode::New);
            let parsed = parse_parallel_config().await?;
            if cli.dry_run {
                let plan = preview_parallel(config, parsed.tasks).await?;
                println!("{}", serde_json::to_string_pretty(&plan)?);
            } else {
                let results = execute_parallel(config, parsed.tasks).await?;
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
            return Ok(());
        }
        None => {
            let task = match &cli.task {
                Some(task) if task == "-" => read_stdin_task().await?,
                Some(task) => task.clone(),
                None => bail!("no task provided"),
            };
            let config = runtime_config_from_cli(&cli, None, RuntimeMode::New);
            let request = request_from_task(&config, task, cli.prompt_file.clone(), None);
            print_or_run(&cli, config, request).await?;
        }
    }

    Ok(())
}
