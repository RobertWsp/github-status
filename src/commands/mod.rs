//! Headless command handlers — the non-TUI entry points.
//!
//! Each submodule owns one concern. They all reuse the same app core
//! (`StatusService`) and ports as the TUI, proving the core is UI-agnostic.
//! Output is plain text suitable for pipelines.

mod doctor;
mod import;
mod manage;
mod status;

pub use doctor::doctor;
pub use import::{import, require_token_for_remote};
pub use manage::{add, init, remove};
pub use status::{check, list};
