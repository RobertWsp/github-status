//! Domain layer — pure types and business rules.
//!
//! This layer has **zero** dependencies on octocrab, ratatui, or any I/O. It is
//! the stable core that every outer layer (adapters, app, tui) depends on,
//! never the reverse (dependency-inversion / hexagonal architecture).

pub mod project;
pub mod project_status;
pub mod run;
pub mod status;

pub use project::{ParseProjectError, Project};
pub use project_status::{LoadState, ProjectStatus};
pub use run::WorkflowRun;
pub use status::RunState;
