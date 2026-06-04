//! Adapters — concrete implementations of the ports.
//!
//! Each adapter is a driven (outbound) adapter in hexagonal terms. They depend
//! on the domain and ports, never the other way around.

pub mod clock;
pub mod file_cache;
pub mod file_store;
pub mod github;
pub mod local_git;

pub use clock::{FixedClock, SystemClock};
pub use file_cache::FileCacheStore;
pub use file_store::FileProjectStore;
pub use github::GithubProvider;
pub use local_git::{resolve_root, LocalGitScanner};
