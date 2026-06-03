//! File-backed [`ProjectStore`] — persists the tracked-project list to the
//! TOML config, preserving the user's `[settings]`.
//!
//! When the TUI removes a repo (or a whole company), it hands the new full
//! project list to this adapter, which reloads the on-disk config, swaps the
//! project list, and writes it back. Reloading first means concurrent manual
//! edits to `[settings]` aren't clobbered.

use std::path::PathBuf;

use crate::config::Config;
use crate::domain::Project;
use crate::ports::{ProjectStore, StoreError};

/// A [`ProjectStore`] that writes to a config file at `path`.
pub struct FileProjectStore {
    path: PathBuf,
}

impl FileProjectStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ProjectStore for FileProjectStore {
    fn save(&self, projects: &[Project]) -> Result<(), StoreError> {
        // Reload to preserve [settings] and any out-of-band edits, then swap
        // the project list and write back.
        let (mut config, _) =
            Config::load_or_default(Some(&self.path)).map_err(|e| StoreError(e.to_string()))?;
        config.set_projects(projects.iter().cloned());
        config
            .save(&self.path)
            .map_err(|e| StoreError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_persists_only_the_given_projects() {
        let dir = std::env::temp_dir().join(format!("ghs-store-{}", std::process::id()));
        let path = dir.join("config.toml");
        let store = FileProjectStore::new(path.clone());

        store
            .save(&[Project::new("a", "b"), Project::new("c", "d")])
            .unwrap();
        let (cfg, _) = Config::load(Some(&path)).unwrap();
        assert_eq!(cfg.projects.len(), 2);

        // Removing one and re-saving reflects on disk.
        store.save(&[Project::new("a", "b")]).unwrap();
        let (cfg, _) = Config::load(Some(&path)).unwrap();
        assert_eq!(cfg.projects.len(), 1);
        assert_eq!(cfg.projects()[0].slug(), "a/b");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
