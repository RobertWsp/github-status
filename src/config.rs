//! Configuration layer — load the list of monitored projects + settings.
//!
//! Owns all serialization concerns (serde/TOML) and the filesystem layout, then
//! maps its on-disk DTOs into pure [`Project`] domain values. The rest of the
//! app receives domain types, never the raw config structs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::domain::{PollIntervals, Project};

/// Top-level config file (`config.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Global settings.
    pub settings: Settings,
    /// The repositories to monitor.
    #[serde(rename = "project", default)]
    pub projects: Vec<ProjectConfig>,
}

/// Default cap on concurrent API requests during a refresh. Kept here (not in
/// `app`) so the config layer stays free of any dependency on `app`.
const DEFAULT_MAX_CONCURRENCY: usize = 8;

/// Tunable behavior knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Seconds between automatic refreshes (0 disables auto-refresh).
    pub refresh_interval_secs: u64,
    /// How many recent runs to fetch per project.
    pub runs_per_project: u8,
    /// Max number of concurrent API requests during a refresh. Bounds load on
    /// the GitHub API when tracking many repositories.
    pub max_concurrency: usize,
    /// Persist fetched runs to disk so the dashboard hydrates instantly on
    /// startup without re-hitting the API for everything.
    pub cache_enabled: bool,
    /// Adaptive polling intervals (seconds) per tier. See [`crate::domain::poll`].
    pub poll_active_secs: u64,
    pub poll_recent_secs: u64,
    pub poll_stable_secs: u64,
    pub poll_dormant_secs: u64,
    pub poll_backoff_base_secs: u64,
    pub poll_backoff_cap_secs: u64,
    /// Personal access token. Prefer the `GITHUB_TOKEN` env var; this is a
    /// fallback for users who insist on storing it in the config file.
    pub token: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        let p = PollIntervals::default();
        Self {
            refresh_interval_secs: 60,
            runs_per_project: 5,
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            cache_enabled: true,
            poll_active_secs: p.active.as_secs(),
            poll_recent_secs: p.recent.as_secs(),
            poll_stable_secs: p.stable.as_secs(),
            poll_dormant_secs: p.dormant.as_secs(),
            poll_backoff_base_secs: p.backoff_base.as_secs(),
            poll_backoff_cap_secs: p.backoff_cap.as_secs(),
            token: None,
        }
    }
}

impl Settings {
    /// Build the domain [`PollIntervals`] from the configured seconds.
    pub fn poll_intervals(&self) -> PollIntervals {
        PollIntervals {
            active: Duration::from_secs(self.poll_active_secs.max(1)),
            recent: Duration::from_secs(self.poll_recent_secs.max(1)),
            stable: Duration::from_secs(self.poll_stable_secs.max(1)),
            dormant: Duration::from_secs(self.poll_dormant_secs.max(1)),
            backoff_base: Duration::from_secs(self.poll_backoff_base_secs.max(1)),
            backoff_cap: Duration::from_secs(self.poll_backoff_cap_secs.max(1)),
        }
    }
}

/// On-disk representation of one monitored project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub owner: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl From<ProjectConfig> for Project {
    fn from(c: ProjectConfig) -> Self {
        Project {
            owner: c.owner,
            repo: c.repo,
            branch: c.branch,
            label: c.label,
        }
    }
}

impl From<Project> for ProjectConfig {
    fn from(p: Project) -> Self {
        ProjectConfig {
            owner: p.owner,
            repo: p.repo,
            branch: p.branch,
            label: p.label,
        }
    }
}

/// Errors raised while resolving or parsing configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not determine a config directory for this platform")]
    NoConfigDir,
    #[error("config file not found at {0}")]
    NotFound(PathBuf),
    #[error("failed to read config at {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to write config at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl Config {
    /// The default platform config path: e.g.
    /// `~/.config/github-status/config.toml` on Linux.
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        let dirs =
            ProjectDirs::from("dev", "robert", "github-status").ok_or(ConfigError::NoConfigDir)?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// Resolve the effective config path (explicit or platform default).
    pub fn resolve_path(path: Option<&Path>) -> Result<PathBuf, ConfigError> {
        match path {
            Some(p) => Ok(p.to_path_buf()),
            None => Self::default_path(),
        }
    }

    /// Load config from an explicit path, or the platform default.
    pub fn load(path: Option<&Path>) -> Result<(Self, PathBuf), ConfigError> {
        let path = Self::resolve_path(path)?;
        if !path.exists() {
            return Err(ConfigError::NotFound(path));
        }
        let raw = std::fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
            path: path.clone(),
            source,
        })?;
        Ok((config, path))
    }

    /// Load config, or return an empty default if the file doesn't exist yet.
    ///
    /// This is what enables a friction-free first run: the app never hard-fails
    /// on a missing config; it starts empty and guides the user.
    pub fn load_or_default(path: Option<&Path>) -> Result<(Self, PathBuf), ConfigError> {
        let path = Self::resolve_path(path)?;
        match Self::load(Some(&path)) {
            Ok(pair) => Ok(pair),
            Err(ConfigError::NotFound(_)) => Ok((Config::default(), path)),
            Err(e) => Err(e),
        }
    }

    /// Persist this config to `path`, creating parent directories. Preserves a
    /// helpful header comment.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let body = toml::to_string_pretty(self)?;
        let header = CONFIG_HEADER;
        std::fs::write(path, format!("{header}{body}")).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Add a project, de-duplicating by `owner/repo` (case-insensitive).
    /// Returns `true` if it was newly added, `false` if it already existed.
    pub fn add_project(&mut self, project: Project) -> bool {
        let exists = self
            .projects
            .iter()
            .any(|p| eq_slug(&p.owner, &p.repo, &project.owner, &project.repo));
        if exists {
            return false;
        }
        self.projects.push(ProjectConfig {
            owner: project.owner,
            repo: project.repo,
            branch: project.branch,
            label: project.label,
        });
        true
    }

    /// Remove a project by `owner/repo` (case-insensitive). Returns `true` if a
    /// matching project was removed.
    pub fn remove_project(&mut self, owner: &str, repo: &str) -> bool {
        let before = self.projects.len();
        self.projects
            .retain(|p| !eq_slug(&p.owner, &p.repo, owner, repo));
        self.projects.len() != before
    }

    /// Remove every project belonging to `owner` (case-insensitive) — i.e. drop
    /// a whole company/org at once. Returns how many were removed.
    pub fn remove_owner(&mut self, owner: &str) -> usize {
        let before = self.projects.len();
        self.projects
            .retain(|p| !p.owner.eq_ignore_ascii_case(owner.trim()));
        before - self.projects.len()
    }

    /// Replace the entire project list (used when the TUI persists an edit).
    pub fn set_projects(&mut self, projects: impl IntoIterator<Item = Project>) {
        self.projects = projects.into_iter().map(ProjectConfig::from).collect();
    }

    /// Write a starter config (used by `ghs init`). Creates parent dirs.
    ///
    /// The config starts **empty** — we never inject repositories the user
    /// didn't choose (that would pollute their real dashboard). Example entries
    /// are written only as comments to show the format.
    pub fn write_scaffold(path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        // Serialize only [settings]; the empty project list would otherwise
        // render as a noisy `project = []`. Project format is shown in the
        // example comments below.
        let body = toml::to_string_pretty(&Settings::default())?;
        let content = format!("{CONFIG_HEADER}[settings]\n{body}{SCAFFOLD_EXAMPLES}");
        std::fs::write(path, content).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Map the config's project DTOs into pure domain [`Project`] values.
    pub fn projects(&self) -> Vec<Project> {
        self.projects.iter().cloned().map(Project::from).collect()
    }

    /// Resolve the GitHub token, with a friction-free precedence chain so most
    /// users never have to configure anything:
    ///
    /// 1. `GITHUB_TOKEN` env var
    /// 2. `GH_TOKEN` env var (used by the official GitHub CLI)
    /// 3. `settings.token` in the config file
    /// 4. `gh auth token` (if the GitHub CLI is installed & logged in)
    ///
    /// Returns the token plus where it came from (for diagnostics / `doctor`).
    pub fn resolve_token_source(&self) -> Option<(String, TokenSource)> {
        if let Some(t) = env_token("GITHUB_TOKEN") {
            return Some((t, TokenSource::EnvGithubToken));
        }
        if let Some(t) = env_token("GH_TOKEN") {
            return Some((t, TokenSource::EnvGhToken));
        }
        if let Some(t) = self.settings.token.clone().filter(|t| !t.trim().is_empty()) {
            return Some((t, TokenSource::ConfigFile));
        }
        if let Some(t) = gh_cli_token() {
            return Some((t, TokenSource::GhCli));
        }
        None
    }

    /// Just the token, dropping the source. See [`Config::resolve_token_source`].
    pub fn resolve_token(&self) -> Option<String> {
        self.resolve_token_source().map(|(t, _)| t)
    }
}

/// Header comment written at the top of saved config files.
const CONFIG_HEADER: &str = "# github-status (ghs) configuration\n# Edit by hand, or use `ghs add owner/repo`, `ghs remove owner/repo`, `ghs import`.\n# Token is auto-detected: GITHUB_TOKEN > GH_TOKEN > settings.token > `gh auth token`.\n\n";

/// Commented-out example projects appended to a freshly-scaffolded config so
/// the user sees the format without any real repositories being tracked.
const SCAFFOLD_EXAMPLES: &str = "\n# Add your repositories below (or run `ghs add owner/repo`). Examples:\n#\n# [[project]]\n# owner = \"your-org\"\n# repo = \"your-repo\"\n# branch = \"main\"   # optional branch filter\n# label = \"My API\"  # optional display name\n";

/// Where a resolved token came from — surfaced by `ghs doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    EnvGithubToken,
    EnvGhToken,
    ConfigFile,
    GhCli,
}

impl TokenSource {
    pub fn describe(self) -> &'static str {
        match self {
            Self::EnvGithubToken => "GITHUB_TOKEN environment variable",
            Self::EnvGhToken => "GH_TOKEN environment variable",
            Self::ConfigFile => "config file (settings.token)",
            Self::GhCli => "GitHub CLI (`gh auth token`)",
        }
    }
}

fn env_token(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|t| !t.trim().is_empty())
}

/// Best-effort token from the GitHub CLI. Never panics; returns `None` if `gh`
/// is missing, not logged in, or anything goes wrong.
fn gh_cli_token() -> Option<String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Case-insensitive `owner/repo` equality used for de-duplication.
fn eq_slug(a_owner: &str, a_repo: &str, b_owner: &str, b_repo: &str) -> bool {
    a_owner.eq_ignore_ascii_case(b_owner) && a_repo.eq_ignore_ascii_case(b_repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let toml = r#"
            [[project]]
            owner = "a"
            repo = "b"
        "#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.projects.len(), 1);
        let projects = cfg.projects();
        assert_eq!(projects[0].slug(), "a/b");
        assert_eq!(cfg.settings.runs_per_project, 5); // default applied
    }

    #[test]
    fn env_token_overrides_config() {
        let mut cfg = Config::default();
        cfg.settings.token = Some("from-file".into());
        std::env::set_var("GITHUB_TOKEN", "from-env");
        assert_eq!(cfg.resolve_token().as_deref(), Some("from-env"));
        std::env::remove_var("GITHUB_TOKEN");
        // With no env vars set, the config file token wins over the gh CLI.
        let (tok, src) = cfg.resolve_token_source().unwrap();
        assert_eq!(tok, "from-file");
        assert_eq!(src, TokenSource::ConfigFile);
    }

    #[test]
    fn scaffold_is_empty_no_injected_projects() {
        let dir = std::env::temp_dir().join(format!("ghs-scaffold-{}", std::process::id()));
        let path = dir.join("config.toml");
        Config::write_scaffold(&path).unwrap();
        let (cfg, _) = Config::load(Some(&path)).unwrap();
        assert!(
            cfg.projects.is_empty(),
            "init must not inject repos the user didn't choose"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_project_dedupes_case_insensitively() {
        let mut cfg = Config::default();
        assert!(cfg.add_project(Project::new("Rust-Lang", "Rust")));
        assert!(!cfg.add_project(Project::new("rust-lang", "rust")));
        assert_eq!(cfg.projects.len(), 1);
    }

    #[test]
    fn remove_project_reports_hit() {
        let mut cfg = Config::default();
        cfg.add_project(Project::new("a", "b"));
        assert!(cfg.remove_project("A", "B"));
        assert!(!cfg.remove_project("a", "b"));
        assert!(cfg.projects.is_empty());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = std::env::temp_dir().join(format!("ghs-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut cfg = Config::default();
        cfg.add_project(Project::parse("owner/repo@main").unwrap());
        cfg.save(&path).unwrap();
        let (loaded, _) = Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.projects()[0].slug(), "owner/repo");
        assert_eq!(loaded.projects()[0].branch.as_deref(), Some("main"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
