use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use wrangle_core::{
    ExecutionRequest, PermissionPolicy, RuntimeConfig, RuntimeMode, SessionHandle, SessionState,
    TransportMode, parse_parallel_config, read_stdin_task,
};
use wrangle_runner::{
    PlaybookInvocation, PlaybookName, available_backends, build_playbook_plan, execute_parallel,
    execute_request, preview_request,
};

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
    Playbook {
        #[arg(value_enum)]
        name: PlaybookArg,
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let _guard = setup_logging(&cli)?;

    match &cli.command {
        Some(Command::Backends { json: _ }) => {
            println!("{}", serde_json::to_string_pretty(&available_backends())?);
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
            if cli.dry_run {
                bail!("--dry-run is not supported with --parallel");
            }
            let config = runtime_config_from_cli(&cli, None, RuntimeMode::New);
            let parsed = parse_parallel_config().await?;
            let results = execute_parallel(config, parsed.tasks).await?;
            println!("{}", serde_json::to_string_pretty(&results)?);
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
