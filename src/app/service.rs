//! Status service — orchestrates concurrent fetching across all projects.
//!
//! Depends only on the [`StatusProvider`] port, so it is fully decoupled from
//! GitHub/octocrab. It spawns one task per project and streams each result back
//! as an [`Action::FetchCompleted`] over an mpsc channel, letting the UI update
//! incrementally as projects resolve rather than blocking on the slowest one.

use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Semaphore;

use super::action::Action;
use crate::domain::Project;
use crate::ports::StatusProvider;

/// Coordinates fetches via the injected provider.
///
/// Fetches run concurrently, but a [`Semaphore`] caps how many hit the network
/// at once. With hundreds of tracked repos this prevents a thundering-herd of
/// simultaneous requests (which would spike memory and trip GitHub's secondary
/// rate limits) while still keeping the UI responsive as results stream in.
#[derive(Clone)]
pub struct StatusService {
    provider: Arc<dyn StatusProvider>,
    runs_per_project: u8,
    permits: Arc<Semaphore>,
}

impl StatusService {
    /// Default cap on concurrent in-flight fetches.
    pub const DEFAULT_CONCURRENCY: usize = 8;

    pub fn new(provider: Arc<dyn StatusProvider>, runs_per_project: u8) -> Self {
        Self::with_concurrency(provider, runs_per_project, Self::DEFAULT_CONCURRENCY)
    }

    /// Build with an explicit concurrency limit (clamped to at least 1).
    pub fn with_concurrency(
        provider: Arc<dyn StatusProvider>,
        runs_per_project: u8,
        max_concurrency: usize,
    ) -> Self {
        Self {
            provider,
            runs_per_project: runs_per_project.max(1),
            permits: Arc::new(Semaphore::new(max_concurrency.max(1))),
        }
    }

    /// Fetch every project concurrently (bounded by the semaphore). Each
    /// completion is sent as an [`Action`] over `tx`; `index` ties the result
    /// back to its slot in [`AppState`]. Returns immediately — work happens on
    /// spawned tasks.
    pub fn refresh_all(&self, projects: &[Project], tx: UnboundedSender<Action>) {
        for (index, project) in projects.iter().cloned().enumerate() {
            let provider = Arc::clone(&self.provider);
            let permits = Arc::clone(&self.permits);
            let limit = self.runs_per_project;
            let tx = tx.clone();
            tokio::spawn(async move {
                // Acquire a permit; released on drop at end of scope. If the
                // semaphore is somehow closed, fall through without limiting.
                let _permit = permits.acquire().await;
                let result = provider.fetch_runs(&project, limit).await;
                // Receiver gone (app shutting down) — ignore the send error.
                let _ = tx.send(Action::FetchCompleted { index, result });
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RunState, WorkflowRun};
    use crate::ports::{ProviderError, StatusProvider};
    use async_trait::async_trait;
    use chrono::Utc;

    struct MockProvider;

    #[async_trait]
    impl StatusProvider for MockProvider {
        async fn fetch_runs(
            &self,
            project: &Project,
            _limit: u8,
        ) -> Result<Vec<WorkflowRun>, ProviderError> {
            if project.repo == "fail" {
                return Err(ProviderError::NotFound("nope".into()));
            }
            Ok(vec![WorkflowRun {
                name: "CI".into(),
                run_number: 1,
                state: RunState::Success,
                branch: "main".into(),
                short_sha: "abc1234".into(),
                event: "push".into(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                html_url: "https://example.com".into(),
            }])
        }
    }

    #[tokio::test]
    async fn refresh_all_emits_one_action_per_project() {
        let service = StatusService::new(Arc::new(MockProvider), 5);
        let projects = vec![Project::new("a", "ok"), Project::new("b", "fail")];
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        service.refresh_all(&projects, tx);

        let mut completed = 0;
        for _ in 0..projects.len() {
            let action = rx.recv().await.expect("action");
            if let Action::FetchCompleted { .. } = action {
                completed += 1;
            }
        }
        assert_eq!(completed, 2);
    }

    /// A provider that records peak concurrent in-flight calls.
    struct CountingProvider {
        current: std::sync::atomic::AtomicUsize,
        peak: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl StatusProvider for CountingProvider {
        async fn fetch_runs(
            &self,
            _project: &Project,
            _limit: u8,
        ) -> Result<Vec<WorkflowRun>, ProviderError> {
            use std::sync::atomic::Ordering;
            let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn concurrency_is_bounded_by_the_semaphore() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let provider = Arc::new(CountingProvider {
            current: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        });
        let service = StatusService::with_concurrency(provider.clone(), 1, 3);
        let projects: Vec<_> = (0..20)
            .map(|i| Project::new("o", format!("r{i}")))
            .collect();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        service.refresh_all(&projects, tx);
        for _ in 0..projects.len() {
            rx.recv().await.expect("action");
        }
        assert!(
            provider.peak.load(Ordering::SeqCst) <= 3,
            "peak concurrency exceeded the limit"
        );
    }
}
