//! Composition root for `ghs`.
//!
//! Responsible only for *wiring*: parse args, load config, construct the
//! concrete [`GithubProvider`] adapter, inject it behind the
//! [`StatusProvider`](github_status::ports::StatusProvider) port, and dispatch
//! to the chosen command. No business logic lives here.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};

use github_status::adapters::{FileProjectStore, GithubProvider};
use github_status::cli::{Cli, CommandKind};
use github_status::commands;
use github_status::config::Config;
use github_status::ports::{ProjectStore, StatusProvider};
use github_status::tui::{Runtime, TerminalGuard};
use github_status::{app::AppState, app::StatusService};

#[tokio::main]
async fn main() -> Result<()> {
    install_panic_hook()?;
    let cli = Cli::parse();
    let config_path = resolve_config_path(cli.config.clone())?;

    // --- Commands that only touch the config file (no network/provider). ---
    match cli.command() {
        CommandKind::Init { force } => return commands::init(&config_path, *force),
        CommandKind::Where => {
            println!("{}", config_path.display());
            return Ok(());
        }
        CommandKind::Add { repos } => return commands::add(&config_path, repos),
        CommandKind::Remove { repos } => return commands::remove(&config_path, repos),
        _ => {}
    }

    // --- Commands that need config + a GitHub-backed adapter. ---
    // First run is friction-free: a missing config yields an empty default
    // rather than a hard error, so `ghs` always launches.
    let (config, path) = Config::load_or_default(Some(&config_path))?;
    tracing_init();
    tracing::info!(config = %path.display(), projects = config.projects.len(), "loaded config");

    let token = config.resolve_token();
    let github =
        Arc::new(GithubProvider::new(token.clone()).wrap_err("failed to init GitHub client")?);
    let provider: Arc<dyn StatusProvider> = github.clone();

    match cli.command() {
        CommandKind::List => commands::list(provider, &config).await,
        CommandKind::Check => commands::check(provider, &config).await,
        CommandKind::Doctor => commands::doctor(&config_path).await,
        CommandKind::Import(args) => {
            commands::require_token_for_remote(args, token.is_some())?;
            commands::import(github, &config_path, args).await
        }
        CommandKind::Tui
        | CommandKind::Init { .. }
        | CommandKind::Where
        | CommandKind::Add { .. }
        | CommandKind::Remove { .. } => run_tui(provider, config, config_path).await,
    }
}

async fn run_tui(
    provider: Arc<dyn StatusProvider>,
    config: Config,
    config_path: PathBuf,
) -> Result<()> {
    let projects = config.projects();
    let state = AppState::new(projects);
    let service = StatusService::with_concurrency(
        provider,
        config.settings.runs_per_project,
        config.settings.max_concurrency,
    );
    // Inject the file-backed store so in-TUI deletes persist to the config.
    let store: Arc<dyn ProjectStore> = Arc::new(FileProjectStore::new(config_path));
    let runtime = Runtime::new(state, service, store, config.settings.refresh_interval_secs);
    runtime.run().await
}

fn resolve_config_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    match explicit {
        Some(p) => Ok(p),
        None => Config::default_path().wrap_err("could not determine config path"),
    }
}

/// Install a panic hook that restores the terminal before printing the report,
/// so a crash never leaves the user with a broken terminal.
fn install_panic_hook() -> Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();
    eyre_hook.install()?;
    let panic_hook = std::sync::Arc::new(panic_hook);
    std::panic::set_hook(Box::new(move |info| {
        let _ = TerminalGuard::restore();
        eprintln!("{}", panic_hook.panic_report(info));
    }));
    Ok(())
}

/// Initialize file-based tracing (logging to stdout would corrupt the TUI).
fn tracing_init() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let Some(dir) = directories::ProjectDirs::from("dev", "robert", "github-status") else {
        return;
    };
    let log_dir = dir.data_local_dir().join("logs");
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let file_appender = tracing_appender::rolling::daily(&log_dir, "ghs.log");
    let filter = EnvFilter::try_from_env("GHS_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_appender).with_ansi(false))
        .try_init();
}
