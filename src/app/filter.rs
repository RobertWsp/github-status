//! Filter — the value object that decides which projects are *visible*.
//!
//! Collects the two filtering dimensions (free-text search + owner/company
//! restriction) into one cohesive type, so the visibility rule lives in exactly
//! one place (SSoT) instead of being scattered as loose fields on `AppState`.
//! Pure and trivially unit-testable.

use crate::domain::Project;

/// Active view filters. Empty query + no owner means "show everything".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Free-text query matched against owner/repo/label (case-insensitive).
    pub query: String,
    /// Restrict to a single owner/company, if set.
    pub owner: Option<String>,
}

impl Filter {
    /// Whether `project` passes all active filters.
    pub fn passes(&self, project: &Project) -> bool {
        let owner_ok = self.owner.as_deref().is_none_or(|o| project.belongs_to(o));
        owner_ok && project.matches_query(&self.query)
    }

    /// Whether any filter is currently narrowing the list.
    pub fn is_active(&self) -> bool {
        self.owner.is_some() || !self.query.trim().is_empty()
    }

    /// The trimmed query, or `None` when blank.
    pub fn query_text(&self) -> Option<&str> {
        let q = self.query.trim();
        (!q.is_empty()).then_some(q)
    }

    /// Reset to "show everything".
    pub fn clear(&mut self) {
        self.query.clear();
        self.owner = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_filter_passes_everything() {
        let f = Filter::default();
        assert!(f.passes(&Project::new("acme", "api")));
        assert!(!f.is_active());
    }

    #[test]
    fn query_matches_substring_case_insensitively() {
        let f = Filter {
            query: "API".into(),
            owner: None,
        };
        assert!(f.passes(&Project::new("acme", "web-api")));
        assert!(!f.passes(&Project::new("acme", "web")));
        assert!(f.is_active());
    }

    #[test]
    fn owner_restricts_to_company() {
        let f = Filter {
            query: String::new(),
            owner: Some("acme".into()),
        };
        assert!(f.passes(&Project::new("Acme", "x")));
        assert!(!f.passes(&Project::new("other", "x")));
    }

    #[test]
    fn query_and_owner_combine() {
        let f = Filter {
            query: "api".into(),
            owner: Some("acme".into()),
        };
        assert!(f.passes(&Project::new("acme", "api")));
        assert!(!f.passes(&Project::new("acme", "web"))); // owner ok, query no
        assert!(!f.passes(&Project::new("other", "api"))); // query ok, owner no
    }

    #[test]
    fn clear_resets() {
        let mut f = Filter {
            query: "x".into(),
            owner: Some("o".into()),
        };
        f.clear();
        assert!(!f.is_active());
    }
}
