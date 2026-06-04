//! File-backed [`CacheStore`] — persists fetched runs as JSON between sessions.
//!
//! Keeps its **own** serializable DTOs (anti-corruption layer) so the pure
//! domain [`WorkflowRun`] doesn't have to depend on serde. On startup the app
//! hydrates from this cache so the dashboard is populated instantly without a
//! burst of API calls; the adaptive scheduler then only re-fetches stale repos.
//!
//! The cache lives under the platform data dir, e.g.
//! `~/.local/share/github-status/cache.json` on Linux.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::domain::{RunState, WorkflowRun};
use crate::ports::{CacheStore, CachedProject, StoreError};

/// A [`CacheStore`] writing a single JSON document at `path`.
pub struct FileCacheStore {
    path: PathBuf,
}

impl FileCacheStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The default platform cache path, if determinable.
    pub fn default_path() -> Option<PathBuf> {
        ProjectDirs::from("dev", "robert", "github-status")
            .map(|d| d.data_local_dir().join("cache.json"))
    }
}

impl CacheStore for FileCacheStore {
    fn load(&self) -> Vec<CachedProject> {
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let doc: CacheDoc = serde_json::from_str(&raw).unwrap_or_default();
        doc.projects.into_iter().map(CachedProject::from).collect()
    }

    fn save(&self, entries: &[CachedProject]) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StoreError(e.to_string()))?;
        }
        let doc = CacheDoc {
            version: 1,
            projects: entries
                .iter()
                .cloned()
                .map(CachedProjectDto::from)
                .collect(),
        };
        let body = serde_json::to_string(&doc).map_err(|e| StoreError(e.to_string()))?;
        std::fs::write(&self.path, body).map_err(|e| StoreError(e.to_string()))
    }
}

// --- serializable DTOs (anti-corruption) ------------------------------------

#[derive(Default, Serialize, Deserialize)]
struct CacheDoc {
    version: u32,
    projects: Vec<CachedProjectDto>,
}

#[derive(Serialize, Deserialize)]
struct CachedProjectDto {
    slug: String,
    fetched_at: DateTime<Utc>,
    runs: Vec<RunDto>,
}

#[derive(Serialize, Deserialize)]
struct RunDto {
    name: String,
    run_number: i64,
    status: String,
    conclusion: Option<String>,
    branch: String,
    short_sha: String,
    event: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    html_url: String,
}

impl From<CachedProject> for CachedProjectDto {
    fn from(c: CachedProject) -> Self {
        Self {
            slug: c.slug,
            fetched_at: c.fetched_at,
            runs: c.runs.into_iter().map(RunDto::from).collect(),
        }
    }
}

impl From<CachedProjectDto> for CachedProject {
    fn from(d: CachedProjectDto) -> Self {
        Self {
            slug: d.slug,
            fetched_at: d.fetched_at,
            runs: d.runs.into_iter().map(WorkflowRun::from).collect(),
        }
    }
}

impl From<WorkflowRun> for RunDto {
    fn from(r: WorkflowRun) -> Self {
        // Persist the raw state label so it round-trips without re-deriving.
        Self {
            name: r.name,
            run_number: r.run_number,
            status: r.state.persist_key().to_string(),
            conclusion: None,
            branch: r.branch,
            short_sha: r.short_sha,
            event: r.event,
            created_at: r.created_at,
            updated_at: r.updated_at,
            html_url: r.html_url,
        }
    }
}

impl From<RunDto> for WorkflowRun {
    fn from(d: RunDto) -> Self {
        // `status` holds our persisted RunState key; conclusion is unused.
        let state = RunState::from_persist_key(&d.status)
            .unwrap_or_else(|| RunState::from_api(&d.status, d.conclusion.as_deref()));
        WorkflowRun {
            name: d.name,
            run_number: d.run_number,
            state,
            branch: d.branch,
            short_sha: d.short_sha,
            event: d.event,
            created_at: d.created_at,
            updated_at: d.updated_at,
            html_url: d.html_url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(state: RunState) -> WorkflowRun {
        WorkflowRun {
            name: "CI".into(),
            run_number: 7,
            state,
            branch: "main".into(),
            short_sha: "abc1234".into(),
            event: "push".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            html_url: "https://example.com".into(),
        }
    }

    #[test]
    fn roundtrips_through_disk() {
        let dir = std::env::temp_dir().join(format!("ghs-cache-{}", std::process::id()));
        let store = FileCacheStore::new(dir.join("cache.json"));

        let entries = vec![CachedProject {
            slug: "o/r".into(),
            runs: vec![run(RunState::Failed)],
            fetched_at: Utc::now(),
        }];
        store.save(&entries).unwrap();

        let loaded = store.load();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].slug, "o/r");
        assert_eq!(loaded[0].runs[0].state, RunState::Failed);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_loads_empty() {
        let store = FileCacheStore::new(PathBuf::from("/nonexistent/ghs/cache.json"));
        assert!(store.load().is_empty());
    }
}
