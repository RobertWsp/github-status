//! github-status (`ghs`) — interactive TUI for GitHub Actions status.
//!
//! # Architecture (Hexagonal / Ports & Adapters)
//!
//! Dependencies always point *inward*; outer layers depend on inner ones,
//! never the reverse:
//!
//! ```text
//!   tui ─┐                       (presentation: ratatui)
//!        ├─▶ app ─▶ ports ◀─ adapters   (orchestration ▶ abstractions ◀ impls)
//!   cli ─┘            ▲
//!                  domain                (pure types — the stable core)
//! ```
//!
//! * [`domain`] — pure types & business rules (no I/O, no octocrab, no ratatui).
//!   Holds the SSoT for status semantics ([`domain::RunState`]).
//! * [`ports`] — the [`ports::StatusProvider`] and [`ports::RepoDiscovery`]
//!   traits the app depends on.
//! * [`adapters`] — concrete port impls: an octocrab-backed provider/discovery
//!   plus a dependency-free local `.git` scanner. Anti-corruption mapping here.
//! * [`app`] — UI-agnostic state machine ([`app::AppState`]) & orchestration.
//! * [`tui`] — ratatui presentation + interactive runtime.
//! * [`cli`] / [`commands`] — argument parsing & headless entry points.
//! * [`config`] — TOML config loading and token resolution.
//!
//! `main.rs` is the composition root: it wires concrete adapters into the app.

pub mod adapters;
pub mod app;
pub mod cli;
pub mod commands;
pub mod config;
pub mod domain;
pub mod ports;
pub mod tui;
