//! # addon-core
//!
//! Core library for the addon — shared types, configuration, and platform-agnostic logic.
//!
//! ## Modules
//!
//! - [`config`] — YAML configuration data model
//! - [`keymap`] — KeyStroke and modifier definitions
//! - [`mapper`] — Key mapping engine trait
//! - [`actions`] — Action type definitions
//! - [`conflict`] — Key binding conflict detection
//! - [`error`] — Error types
//! - [`log`] — Common logging initialization
//! - [`os`] — OS adapter trait and platform identification
//! - [`ipc`] — IPC message types for GUI ↔ Daemon communication

pub mod actions;
pub mod config;
pub mod conflict;
pub mod error;
pub mod ipc;
pub mod keymap;
pub mod log;
pub mod mapper;
pub mod os;

// Re-export os items at crate root for convenience.
pub use os::{OsAdapter, OsPlatform};
// Re-export Error at crate root for convenience.
pub use error::Error;
