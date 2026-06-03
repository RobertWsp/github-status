//! Local git adapter — discovers GitHub projects by scanning the filesystem.
//!
//! Walks a directory (one level deep by default) looking for git repositories,
//! reads each repo's `origin` remote from `.git/config`, and extracts the
//! `owner/repo` for any GitHub remote. This lets `ghs import --local` pick up
//! every project you've already cloned, with zero typing.
//!
//! It deliberately parses `.git/config` directly rather than shelling out to
//! `git`, so it works even when `git` isn't installed.

use std::path::{Path, PathBuf};

use crate::domain::Project;

/// Scans the filesystem for cloned GitHub repositories.
pub struct LocalGitScanner {
    /// How many directory levels below the root to descend.
    depth: usize,
}

impl Default for LocalGitScanner {
    fn default() -> Self {
        Self { depth: 1 }
    }
}

impl LocalGitScanner {
    /// Create a scanner that descends `depth` levels (0 = only `root` itself).
    pub fn with_depth(depth: usize) -> Self {
        Self { depth }
    }

    /// Discover GitHub projects under `root`. Never errors on individual
    /// unreadable dirs — it skips them and returns whatever it found.
    pub fn scan(&self, root: &Path) -> Vec<Project> {
        let mut found = Vec::new();
        self.walk(root, self.depth, &mut found);
        // De-dup by slug, preserving first occurrence.
        found.sort_by_key(|p| p.slug());
        found.dedup_by_key(|p| p.slug());
        found
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<Project>) {
        if let Some(project) = project_from_git_dir(dir) {
            out.push(project);
            return; // don't descend into a repo's subdirs
        }
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !is_hidden(&path) {
                self.walk(&path, depth - 1, out);
            }
        }
    }
}

/// If `dir` is a git repo with a GitHub `origin`, return its [`Project`].
fn project_from_git_dir(dir: &Path) -> Option<Project> {
    let git_config = dir.join(".git").join("config");
    if !git_config.is_file() {
        return None;
    }
    let contents = std::fs::read_to_string(&git_config).ok()?;
    let url = origin_url(&contents)?;
    parse_github_remote(&url)
}

/// Extract the `url` of the `[remote "origin"]` section from a git config.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_origin = trimmed.replace(' ', "") == "[remote\"origin\"]";
            continue;
        }
        if in_origin {
            if let Some(rest) = trimmed.strip_prefix("url") {
                if let Some((_, value)) = rest.split_once('=') {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    None
}

/// Parse `owner/repo` out of any GitHub remote URL form:
///   * `https://github.com/owner/repo.git`
///   * `git@github.com:owner/repo.git`
///   * `ssh://git@github.com/owner/repo.git`
fn parse_github_remote(url: &str) -> Option<Project> {
    let url = url.trim();
    let path = if let Some(rest) = url.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = url.strip_prefix("ssh://git@github.com/") {
        rest
    } else if let Some(idx) = url.find("github.com/") {
        &url[idx + "github.com/".len()..]
    } else {
        return None;
    };

    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = path.split('/').filter(|s| !s.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    Some(Project::new(owner, repo))
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.') || n == "node_modules" || n == "target")
        .unwrap_or(false)
}

/// Resolve a directory argument, defaulting to the current directory.
pub fn resolve_root(dir: Option<PathBuf>) -> PathBuf {
    dir.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_remote() {
        let p = parse_github_remote("https://github.com/rust-lang/rust.git").unwrap();
        assert_eq!(p.slug(), "rust-lang/rust");
    }

    #[test]
    fn parses_ssh_remote() {
        let p = parse_github_remote("git@github.com:ratatui/ratatui.git").unwrap();
        assert_eq!(p.slug(), "ratatui/ratatui");
    }

    #[test]
    fn ignores_non_github_remote() {
        assert!(parse_github_remote("https://gitlab.com/a/b.git").is_none());
    }

    #[test]
    fn extracts_origin_url_from_config() {
        let cfg = r#"
[core]
    repositoryformatversion = 0
[remote "origin"]
    url = git@github.com:owner/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*
"#;
        assert_eq!(
            origin_url(cfg).as_deref(),
            Some("git@github.com:owner/repo.git")
        );
    }

    #[test]
    fn scans_a_fake_repo_tree() {
        let base = std::env::temp_dir().join(format!("ghs-scan-{}-{}", std::process::id(), "a"));
        let repo = base.join("myproj");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(
            repo.join(".git").join("config"),
            "[remote \"origin\"]\n    url = https://github.com/me/myproj.git\n",
        )
        .unwrap();

        let found = LocalGitScanner::default().scan(&base);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].slug(), "me/myproj");
        let _ = std::fs::remove_dir_all(&base);
    }
}
