//! GitHub adapter — implements the [`StatusProvider`] port using octocrab.
//!
//! This module is the **anti-corruption layer**: it is the only place that
//! knows octocrab exists. It translates octocrab's `Run` into the domain's
//! [`WorkflowRun`] and octocrab errors into [`ProviderError`]. Swapping the
//! backend (e.g. GraphQL, a cache) means writing another adapter — nothing in
//! the app or tui layers changes.

use async_trait::async_trait;
use octocrab::models::Repository;
use octocrab::Octocrab;

use crate::domain::{Project, RunState, WorkflowRun};
use crate::ports::{DiscoverySource, ProviderError, RepoDiscovery, StatusProvider};

/// A [`StatusProvider`] backed by the GitHub REST API via octocrab.
pub struct GithubProvider {
    client: Octocrab,
}

impl GithubProvider {
    /// Build a provider. A `token` (PAT / `GITHUB_TOKEN`) is strongly
    /// recommended: it lifts rate limits and enables private repos.
    pub fn new(token: Option<String>) -> Result<Self, ProviderError> {
        let mut builder = Octocrab::builder();
        if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
            builder = builder.personal_token(token);
        }
        let client = builder
            .build()
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        Ok(Self { client })
    }

    /// Inject a pre-built client (useful for tests / custom configuration).
    pub fn with_client(client: Octocrab) -> Self {
        Self { client }
    }
}

#[async_trait]
impl StatusProvider for GithubProvider {
    async fn fetch_runs(
        &self,
        project: &Project,
        limit: u8,
    ) -> Result<Vec<WorkflowRun>, ProviderError> {
        let handler = self.client.workflows(&project.owner, &project.repo);
        let mut request = handler.list_all_runs().per_page(limit);
        if let Some(branch) = &project.branch {
            request = request.branch(branch.as_str());
        }

        let page = request.send().await.map_err(map_error)?;

        let runs = page.items.into_iter().map(map_run).collect();
        Ok(runs)
    }
}

#[async_trait]
impl RepoDiscovery for GithubProvider {
    async fn discover(&self, source: DiscoverySource) -> Result<Vec<Project>, ProviderError> {
        let page = match source {
            DiscoverySource::AuthenticatedUser => self
                .client
                .current()
                .list_repos_for_authenticated_user()
                .per_page(100)
                .send()
                .await
                .map_err(map_error)?,
            DiscoverySource::Org(org) => self
                .client
                .orgs(&org)
                .list_repos()
                .per_page(100)
                .send()
                .await
                .map_err(map_error)?,
            DiscoverySource::User(user) => {
                // No typed handler for another user's repos; use the REST route.
                let route = format!("/users/{user}/repos?per_page=100&sort=pushed");
                self.client
                    .get(route, None::<&()>)
                    .await
                    .map_err(map_error)?
            }
        };

        let projects = page
            .items
            .into_iter()
            .filter(|r| !r.archived.unwrap_or(false))
            .filter_map(map_repo)
            .collect();
        Ok(projects)
    }
}

/// Map an octocrab [`Repository`] to a domain [`Project`], skipping repos with
/// an indeterminate owner.
fn map_repo(repo: Repository) -> Option<Project> {
    let owner = repo.owner.map(|o| o.login)?;
    Some(Project::new(owner, repo.name))
}

/// Map octocrab's `Run` to the domain [`WorkflowRun`] (anti-corruption).
fn map_run(run: octocrab::models::workflows::Run) -> WorkflowRun {
    let short_sha = run.head_sha.chars().take(7).collect();
    WorkflowRun {
        name: run.name,
        run_number: run.run_number,
        state: RunState::from_api(&run.status, run.conclusion.as_deref()),
        branch: run.head_branch,
        short_sha,
        event: run.event,
        created_at: run.created_at,
        updated_at: run.updated_at,
        html_url: run.html_url.to_string(),
    }
}

/// Normalize octocrab errors into the port's [`ProviderError`] vocabulary.
fn map_error(err: octocrab::Error) -> ProviderError {
    if let octocrab::Error::GitHub { source, .. } = &err {
        let status = source.status_code.as_u16();
        let msg = source.message.clone();
        return match status {
            401 | 403 => {
                if msg.to_lowercase().contains("rate limit") {
                    ProviderError::RateLimited(msg)
                } else {
                    ProviderError::Auth(msg)
                }
            }
            404 => ProviderError::NotFound(msg),
            429 => ProviderError::RateLimited(msg),
            _ => ProviderError::Other(format!("HTTP {status}: {msg}")),
        };
    }
    ProviderError::Other(err.to_string())
}
