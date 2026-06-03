//! CLI surface — argument parsing (clap derive) and the non-TUI subcommands.
//!
//! The TUI is the default action; `list`, `check` and `init` provide
//! scriptable / headless entry points. All commands share the same composition
//! root in `main`, which builds the provider and config once.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Interactive dashboard for GitHub Actions status across your projects.
#[derive(Debug, Parser)]
#[command(name = "ghs", version, about, long_about = None)]
pub struct Cli {
    /// Path to the config file (defaults to the platform config dir).
    #[arg(short, long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<CommandKind>,
}

#[derive(Debug, Subcommand)]
pub enum CommandKind {
    /// Launch the interactive TUI (default when no subcommand is given).
    Tui,
    /// Print a one-line status per project and exit (script-friendly).
    List,
    /// Refresh all projects; exit non-zero if any latest run is failing.
    Check,
    /// Add one or more repositories (owner/repo[@branch], or a GitHub URL).
    Add {
        /// Repos to add, e.g. `rust-lang/rust@master ratatui/ratatui`.
        #[arg(value_name = "OWNER/REPO", required = true)]
        repos: Vec<String>,
    },
    /// Remove one or more repositories (owner/repo).
    Remove {
        /// Repos to remove.
        #[arg(value_name = "OWNER/REPO", required = true)]
        repos: Vec<String>,
    },
    /// Bulk-import repositories from git clones or your GitHub account.
    Import(ImportArgs),
    /// Write a starter config file to the default location.
    Init {
        /// Overwrite if a config already exists.
        #[arg(long)]
        force: bool,
    },
    /// Diagnose setup: token, GitHub CLI, config, and connectivity.
    Doctor,
    /// Print the resolved config file path.
    Where,
}

/// Source selection for `ghs import`. Exactly one source should be chosen;
/// defaults to scanning the current directory for git clones.
#[derive(Debug, Args)]
pub struct ImportArgs {
    /// Scan a local directory for cloned GitHub repos (defaults to `.`).
    #[arg(long, value_name = "DIR", num_args = 0..=1, default_missing_value = ".")]
    pub local: Option<PathBuf>,

    /// How many directory levels to descend when scanning locally.
    #[arg(long, default_value_t = 1, requires = "local")]
    pub depth: usize,

    /// Import repositories owned by the authenticated user (needs a token).
    #[arg(long)]
    pub me: bool,

    /// Import a specific user's public repositories.
    #[arg(long, value_name = "USER")]
    pub user: Option<String>,

    /// Import an organization's repositories.
    #[arg(long, value_name = "ORG")]
    pub org: Option<String>,

    /// Preview what would be imported without writing the config.
    #[arg(long)]
    pub dry_run: bool,
}

impl Cli {
    /// The effective command, defaulting to the TUI.
    pub fn command(&self) -> &CommandKind {
        self.command.as_ref().unwrap_or(&CommandKind::Tui)
    }
}
