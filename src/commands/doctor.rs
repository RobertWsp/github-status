//! `ghs doctor` — diagnose the user's setup and print actionable hints.

use std::path::Path;
use std::sync::Arc;

use color_eyre::eyre::Result;

use crate::config::Config;
use crate::ports::StatusProvider;

/// Run all checks. Builds its own provider lazily for the connectivity probe.
pub async fn doctor(config_path: &Path) -> Result<()> {
    println!("ghs doctor — checking your setup\n");

    // 1. Config file.
    let (config, resolved_path) = match Config::load(Some(config_path)) {
        Ok((cfg, path)) => {
            let n = cfg.projects.len();
            ok(&format!(
                "config found at {} ({n} project{})",
                path.display(),
                if n == 1 { "" } else { "s" }
            ));
            (cfg, path)
        }
        Err(_) => {
            let path = Config::resolve_path(Some(config_path))?;
            warn(&format!("no config at {}", path.display()));
            hint("create one with `ghs init`, or just `ghs add owner/repo`");
            (Config::default(), path)
        }
    };
    let _ = resolved_path;

    // 2. GitHub CLI presence.
    if which_gh() {
        ok("GitHub CLI (`gh`) is installed");
    } else {
        info("GitHub CLI (`gh`) not found (optional — only used for auto-auth)");
    }

    // 3. Token resolution.
    let token = config.resolve_token_source();
    match &token {
        Some((_, source)) => ok(&format!("token detected via {}", source.describe())),
        None => {
            warn("no GitHub token found");
            hint("public repos still work but with low rate limits");
            hint("set GITHUB_TOKEN, or run `gh auth login`");
        }
    }

    // 4. Connectivity probe (only if there's at least one project).
    if let Some(project) = config.projects().into_iter().next() {
        match crate::adapters::GithubProvider::new(token.map(|(t, _)| t)) {
            Ok(provider) => {
                let provider: Arc<dyn StatusProvider> = Arc::new(provider);
                match provider.fetch_runs(&project, 1).await {
                    Ok(_) => ok(&format!("GitHub API reachable (probed {})", project.slug())),
                    Err(e) => {
                        warn(&format!("API probe failed for {}: {e}", project.slug()));
                        hint("check your token scopes and network connection");
                    }
                }
            }
            Err(e) => warn(&format!("could not build GitHub client: {e}")),
        }
    } else {
        info("no projects configured yet — skipping API probe");
        hint("add one with `ghs add owner/repo` or `ghs import`");
    }

    println!("\nAll set? Launch the dashboard with `ghs`.");
    Ok(())
}

fn which_gh() -> bool {
    std::process::Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ok(msg: &str) {
    println!("  \x1b[32m✓\x1b[0m {msg}");
}
fn warn(msg: &str) {
    println!("  \x1b[33m!\x1b[0m {msg}");
}
fn info(msg: &str) {
    println!("  \x1b[2m·\x1b[0m {msg}");
}
fn hint(msg: &str) {
    println!("      \x1b[2m↳ {msg}\x1b[0m");
}
