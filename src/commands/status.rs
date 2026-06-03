//! `ghs list` and `ghs check` — read-only status reporting.

use std::sync::Arc;

use color_eyre::eyre::{eyre, Result};

use crate::app::StatusService;
use crate::config::Config;
use crate::domain::{Project, RunState};
use crate::ports::StatusProvider;

/// Fetch all projects concurrently and collapse to each one's newest run state,
/// preserving the input order.
pub(super) async fn collect_states(
    provider: Arc<dyn StatusProvider>,
    projects: &[Project],
    config: &Config,
) -> Vec<(Project, Result<Option<RunState>, String>)> {
    let service = StatusService::with_concurrency(
        Arc::clone(&provider),
        config.settings.runs_per_project,
        config.settings.max_concurrency,
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    service.refresh_all(projects, tx);

    let mut results: Vec<Option<Result<Option<RunState>, String>>> = vec![None; projects.len()];
    let mut remaining = projects.len();
    while remaining > 0 {
        let Some(action) = rx.recv().await else { break };
        if let crate::app::Action::FetchCompleted { index, result } = action {
            results[index] = Some(match result {
                Ok(runs) => Ok(runs.first().map(|r| r.state)),
                Err(e) => Err(e.to_string()),
            });
            remaining -= 1;
        }
    }

    projects
        .iter()
        .cloned()
        .zip(results)
        .map(|(p, r)| (p, r.unwrap_or(Ok(None))))
        .collect()
}

/// `ghs list` — one line per project.
pub async fn list(provider: Arc<dyn StatusProvider>, config: &Config) -> Result<()> {
    let projects = config.projects();
    if projects.is_empty() {
        println!("No projects configured yet.");
        println!("Add one with:  ghs add owner/repo");
        println!("Or import:     ghs import --local   |   ghs import --me");
        return Ok(());
    }
    let states = collect_states(provider, &projects, config).await;
    for (project, result) in states {
        match result {
            Ok(Some(state)) => {
                println!("{} {:<28} {}", state.glyph(), project.slug(), state.label());
            }
            Ok(None) => println!("· {:<28} no runs", project.slug()),
            Err(msg) => println!("✗ {:<28} error: {msg}", project.slug()),
        }
    }
    Ok(())
}

/// `ghs check` — exit non-zero if any project's latest run is failing.
pub async fn check(provider: Arc<dyn StatusProvider>, config: &Config) -> Result<()> {
    let projects = config.projects();
    let states = collect_states(provider, &projects, config).await;

    let mut failing = Vec::new();
    for (project, result) in &states {
        if let Ok(Some(state)) = result {
            if state.is_failure() {
                failing.push((project.slug(), *state));
            }
        }
    }

    if failing.is_empty() {
        println!("✓ All {} project(s) green.", states.len());
        Ok(())
    } else {
        for (slug, state) in &failing {
            println!("✗ {slug}: {}", state.label());
        }
        Err(eyre!("{} project(s) failing", failing.len()))
    }
}
