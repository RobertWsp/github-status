//! `ghs import` — bulk-add repositories from local clones or a GitHub account.

use std::path::Path;
use std::sync::Arc;

use color_eyre::eyre::{eyre, Result};

use crate::adapters::{resolve_root, LocalGitScanner};
use crate::cli::ImportArgs;
use crate::config::Config;
use crate::domain::Project;
use crate::ports::{DiscoverySource, RepoDiscovery};

/// Run the import. `discovery` is the GitHub-backed [`RepoDiscovery`] used for
/// the remote sources (`--me`, `--user`, `--org`); local scanning needs none.
pub async fn import(
    discovery: Arc<dyn RepoDiscovery>,
    config_path: &Path,
    args: &ImportArgs,
) -> Result<()> {
    let discovered = discover(discovery, args).await?;

    if discovered.is_empty() {
        println!("No repositories found for the given source.");
        return Ok(());
    }

    let total = discovered.len();
    println!("Found {total} repositor{}:", plural(total));
    // Print the full list when small; otherwise summarize to avoid flooding the
    // terminal for accounts with hundreds of repos.
    const PREVIEW: usize = 30;
    for p in discovered.iter().take(PREVIEW) {
        println!("  • {}", p.slug());
    }
    if total > PREVIEW {
        println!("  … and {} more", total - PREVIEW);
    }

    if args.dry_run {
        println!("\n(dry run — nothing written; archived repos are excluded)");
        return Ok(());
    }

    let (mut config, path) = Config::load_or_default(Some(config_path))?;
    let mut added = 0;
    for project in discovered {
        if config.add_project(project) {
            added += 1;
        }
    }

    if added > 0 {
        config.save(&path)?;
        println!("\n✓ Imported {added} new repo(s) into {}.", path.display());
        println!("Run `ghs` to view them.");
    } else {
        println!("\nAll discovered repos were already tracked.");
    }
    Ok(())
}

/// Resolve the chosen source into a list of projects.
async fn discover(discovery: Arc<dyn RepoDiscovery>, args: &ImportArgs) -> Result<Vec<Project>> {
    // Remote sources take precedence if explicitly requested.
    if args.me {
        return Ok(discovery
            .discover(DiscoverySource::AuthenticatedUser)
            .await?);
    }
    if let Some(user) = &args.user {
        return Ok(discovery
            .discover(DiscoverySource::User(user.clone()))
            .await?);
    }
    if let Some(org) = &args.org {
        return Ok(discovery
            .discover(DiscoverySource::Org(org.clone()))
            .await?);
    }

    // Default & `--local`: scan the filesystem.
    if let Some(dir) = &args.local {
        let root = resolve_root(Some(dir.clone()));
        return Ok(scan_local(&root, args.depth));
    }

    // No source flag at all → scan the current directory as the friendly default.
    let root = resolve_root(None);
    println!(
        "No source given — scanning {} for git clones.",
        root.display()
    );
    println!("(Use --me, --user X, --org Y, or --local DIR to choose another.)\n");
    Ok(scan_local(&root, args.depth))
}

fn scan_local(root: &Path, depth: usize) -> Vec<Project> {
    LocalGitScanner::with_depth(depth).scan(root)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        "y"
    } else {
        "ies"
    }
}

/// Helper used by `main` to fail fast with a friendly message when a remote
/// import is requested without a token.
pub fn require_token_for_remote(args: &ImportArgs, has_token: bool) -> Result<()> {
    let needs_token = args.me || args.user.is_some() || args.org.is_some();
    if needs_token && !has_token {
        return Err(eyre!(
            "this import source needs a GitHub token.\n\nSet one with:  export GITHUB_TOKEN=...\nor log in with the GitHub CLI:  gh auth login"
        ));
    }
    Ok(())
}
