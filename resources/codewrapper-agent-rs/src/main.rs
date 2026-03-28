//! codeagent-wrapper - A high-performance wrapper for AI CLI backends
//!
//! This is the main entry point for the codeagent CLI tool.

mod agent_config;
mod backend;
mod cli;
mod config;
mod errors;
mod executor;
mod filter;
mod init;
mod logger;
mod parser;
mod signal;
mod utils;

use anyhow::Result;
use clap::Parser;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::info;

use crate::agent_config::resolve_agent_for_runtime_config;
use crate::backend::select_backend;
use crate::cli::{Cli, Command};
use crate::config::Config;
use crate::executor::TaskExecutor;
use crate::logger::setup_logging;

/// Package version from Cargo.toml
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Setup logging
    let _guard = setup_logging(&cli)?;

    info!(version = VERSION, "codeagent-wrapper starting");

    // Handle subcommands
    match &cli.command {
        Some(Command::Init { force }) => {
            init::run_init(*force).await?;
            return Ok(());
        }
        Some(Command::Resume {
            session_id,
            task,
            workdir,
        }) => {
            // Handle "-" as stdin marker for resume mode
            let actual_task = if task == "-" {
                read_stdin_task().await?
            } else {
                task.clone()
            };
            let config = Config::from_resume(&cli, session_id, &actual_task, workdir.as_deref())?;
            run_task(config).await?;
        }
        None => {
            // Check for special modes
            if cli.cleanup {
                logger::cleanup_old_logs().await?;
                return Ok(());
            }

            if cli.parallel {
                run_parallel(&cli).await?;
            } else if let Some(ref task) = cli.task {
                // Handle "-" as stdin marker
                let actual_task = if task == "-" {
                    read_stdin_task().await?
                } else {
                    task.clone()
                };
                let config = Config::from_cli(&cli, &actual_task)?;
                run_task(config).await?;
            } else {
                // Print help if no task provided
                use clap::CommandFactory;
                Cli::command().print_help()?;
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Run a single task
async fn run_task(mut config: Config) -> Result<()> {
    resolve_agent_for_runtime_config(&mut config).await?;

    let backend = select_backend(config.backend.as_deref())?;
    let executor = TaskExecutor::new(backend, &config)?;
    let result = executor.run().await?;

    // Generate and print final output
    let output = utils::generate_final_output(&result)?;
    println!("{}", output);

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}

/// Run tasks in parallel mode
async fn run_parallel(cli: &Cli) -> Result<()> {
    use crate::config::parse_parallel_config;
    use crate::executor::run_parallel_tasks;

    let parallel_config = parse_parallel_config().await?;
    let results = run_parallel_tasks(cli, parallel_config).await?;

    // Generate and print final output
    let output = utils::generate_parallel_output(&results)?;
    println!("{}", output);

    let all_success = results.iter().all(|r| r.success);
    if !all_success {
        std::process::exit(1);
    }

    Ok(())
}

/// Read task content from stdin
async fn read_stdin_task() -> Result<String> {
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut content = Vec::new();

    while let Some(line) = lines.next_line().await? {
        content.push(line);
    }

    let task = content.join("\n");
    if task.trim().is_empty() {
        anyhow::bail!("No task provided via stdin");
    }

    Ok(task)
}
