//! Project identity — a repository the user wants to monitor.

use std::fmt;

/// A monitored repository. Pure domain value: no config/serialization concerns
/// (those live in [`crate::config`], which maps its DTO into this type).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Project {
    /// Repository owner (user or organization).
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Optional branch filter; `None` means "any branch".
    pub branch: Option<String>,
    /// Human-friendly display label; falls back to `owner/repo`.
    pub label: Option<String>,
}

impl Project {
    /// Construct a minimal project from `owner/repo`.
    pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            branch: None,
            label: None,
        }
    }

    /// Parse a `owner/repo` or `owner/repo@branch` spec into a [`Project`].
    ///
    /// Accepts full GitHub URLs too (`https://github.com/owner/repo`), making
    /// it forgiving of whatever the user pastes. Branch may be appended with
    /// `@`, e.g. `rust-lang/rust@master`.
    pub fn parse(spec: &str) -> Result<Self, ParseProjectError> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err(ParseProjectError::Empty);
        }

        // Strip a GitHub URL prefix if present.
        let cleaned = spec
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("github.com/")
            .trim_start_matches("www.github.com/")
            .trim_end_matches(".git")
            .trim_end_matches('/');

        // Split off an optional `@branch` suffix.
        let (path, branch) = match cleaned.split_once('@') {
            Some((p, b)) if !b.trim().is_empty() => (p, Some(b.trim().to_string())),
            _ => (cleaned, None),
        };

        let mut parts = path.split('/').filter(|s| !s.is_empty());
        let owner = parts
            .next()
            .ok_or_else(|| ParseProjectError::Malformed(spec.to_string()))?;
        let repo = parts
            .next()
            .ok_or_else(|| ParseProjectError::Malformed(spec.to_string()))?;
        if parts.next().is_some() {
            return Err(ParseProjectError::Malformed(spec.to_string()));
        }

        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch,
            label: None,
        })
    }

    /// Canonical `owner/repo` slug — stable identity used as a map key.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// What to show in the UI: explicit label, or the slug.
    pub fn display_name(&self) -> String {
        self.label.clone().unwrap_or_else(|| self.slug())
    }

    /// Case-insensitive substring match against the owner, repo, and label.
    /// Empty query matches everything. Drives the interactive search.
    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.trim();
        if q.is_empty() {
            return true;
        }
        let q = q.to_ascii_lowercase();
        contains_ci(&self.owner, &q)
            || contains_ci(&self.repo, &q)
            || self.label.as_deref().is_some_and(|l| contains_ci(l, &q))
    }

    /// Whether this project belongs to `owner` (case-insensitive). Used by the
    /// "show only this company/org" filter.
    pub fn belongs_to(&self, owner: &str) -> bool {
        self.owner.eq_ignore_ascii_case(owner.trim())
    }
}

/// Case-insensitive substring test. `needle` must already be lowercased.
fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    haystack.to_ascii_lowercase().contains(needle_lower)
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.slug())
    }
}

/// Errors from [`Project::parse`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseProjectError {
    #[error("empty project spec")]
    Empty,
    #[error("expected 'owner/repo' (optionally '@branch'), got: {0}")]
    Malformed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_owner_repo() {
        assert_eq!(Project::new("a", "b").slug(), "a/b");
    }

    #[test]
    fn display_name_prefers_label() {
        let mut p = Project::new("a", "b");
        assert_eq!(p.display_name(), "a/b");
        p.label = Some("My API".into());
        assert_eq!(p.display_name(), "My API");
    }

    #[test]
    fn parse_owner_repo() {
        let p = Project::parse("rust-lang/rust").unwrap();
        assert_eq!(p.owner, "rust-lang");
        assert_eq!(p.repo, "rust");
        assert_eq!(p.branch, None);
    }

    #[test]
    fn parse_with_branch() {
        let p = Project::parse("rust-lang/rust@master").unwrap();
        assert_eq!(p.slug(), "rust-lang/rust");
        assert_eq!(p.branch.as_deref(), Some("master"));
    }

    #[test]
    fn parse_github_url() {
        let p = Project::parse("https://github.com/ratatui/ratatui.git").unwrap();
        assert_eq!(p.slug(), "ratatui/ratatui");
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(Project::parse("").is_err());
        assert!(Project::parse("justone").is_err());
        assert!(Project::parse("a/b/c").is_err());
    }

    #[test]
    fn matches_query_is_case_insensitive_substring() {
        let p = Project::new("Acme", "web-api");
        assert!(p.matches_query(""));
        assert!(p.matches_query("acme"));
        assert!(p.matches_query("API"));
        assert!(p.matches_query("web"));
        assert!(!p.matches_query("zzz"));
    }

    #[test]
    fn matches_query_includes_label() {
        let mut p = Project::new("o", "r");
        p.label = Some("Billing Service".into());
        assert!(p.matches_query("billing"));
    }

    #[test]
    fn belongs_to_owner_case_insensitive() {
        let p = Project::new("Acme", "r");
        assert!(p.belongs_to("acme"));
        assert!(!p.belongs_to("other"));
    }
}
