//! Ports — abstractions the application core depends on.
//!
//! Following hexagonal architecture, the app talks to the outside world only
//! through these traits. Concrete implementations (octocrab, a mock, a cache)
//! live in `adapters/` and are injected at the composition root (`main.rs`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::{Project, WorkflowRun};

/// Source of the current time — the single source of truth for "now".
///
/// Injecting the clock (rather than calling `Utc::now()` ad hoc) keeps the
/// application reducer deterministic and unit-testable: tests supply a fixed
/// clock, production supplies [`crate::adapters::SystemClock`]. Every
/// time-dependent decision in [`AppState`](crate::app::AppState) reads from
/// here, so time is sampled in exactly one place per reduction.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Errors a [`StatusProvider`] can surface, normalized away from any specific
/// HTTP/client library so the app never depends on octocrab's error type.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// Authentication/authorization failure (bad or missing token).
    #[error("authentication failed: {0}")]
    Auth(String),
    /// The repository or its workflows could not be found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Rate limited by the upstream API.
    #[error("rate limited: {0}")]
    RateLimited(String),
    /// Any other transport/parsing failure.
    #[error("provider error: {0}")]
    Other(String),
}

/// Source of workflow-run status for projects. The single port the UI needs.
#[async_trait]
pub trait StatusProvider: Send + Sync {
    /// Fetch the most recent workflow runs for `project`, newest first.
    ///
    /// `limit` bounds how many runs to return (per-page hint to the backend).
    async fn fetch_runs(
        &self,
        project: &Project,
        limit: u8,
    ) -> Result<Vec<WorkflowRun>, ProviderError>;
}

/// Where to discover repositories from, for the `import` command.
#[derive(Debug, Clone)]
pub enum DiscoverySource {
    /// Repositories owned by the authenticated user.
    AuthenticatedUser,
    /// Public repositories of a specific user.
    User(String),
    /// Repositories of an organization.
    Org(String),
}

/// Discovers repositories in bulk so users don't have to add them one by one.
///
/// A second port (alongside [`StatusProvider`]) keeps bulk-import concerns
/// cleanly separated and independently testable / swappable.
#[async_trait]
pub trait RepoDiscovery: Send + Sync {
    /// List repositories from `source`. Returns bare [`Project`]s (no branch).
    async fn discover(&self, source: DiscoverySource) -> Result<Vec<Project>, ProviderError>;
}

/// Error persisting the tracked-project list.
#[derive(Debug, thiserror::Error)]
#[error("failed to persist projects: {0}")]
pub struct StoreError(pub String);

/// Persists the set of tracked projects (the config file's project list).
///
/// This port lets the interactive TUI mutate which repositories are tracked
/// (e.g. remove one repo, or a whole company) and have the change written to
/// disk, **without** the pure [`AppState`](crate::app::AppState) ever touching
/// the filesystem. The runtime shell calls this; the reducer only signals
/// intent via a `Command`.
pub trait ProjectStore: Send + Sync {
    /// Replace the persisted project list with `projects` (the new full set).
    fn save(&self, projects: &[Project]) -> Result<(), StoreError>;
}

/// A cached snapshot of one project's last successful fetch.
#[derive(Debug, Clone)]
pub struct CachedProject {
    pub slug: String,
    pub runs: Vec<WorkflowRun>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// Persists fetched run data between sessions so the dashboard hydrates
/// instantly on startup instead of re-querying every repo (which would be a
/// rate-limit-spiking burst). This is a *cache* — losing it only costs a
/// refresh, never correctness.
pub trait CacheStore: Send + Sync {
    /// Load all cached project snapshots (empty if none / unreadable).
    fn load(&self) -> Vec<CachedProject>;
    /// Persist the given snapshots, replacing any previous cache.
    fn save(&self, entries: &[CachedProject]) -> Result<(), StoreError>;
}
