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

pub mod config;
pub mod keymap;
pub mod mapper;
pub mod actions;
pub mod conflict;
pub mod error;
pub mod log;
pub mod os;

// Re-export os items at crate root for convenience.
pub use os::{OsAdapter, OsPlatform};
// Re-export Error at crate root for convenience.
pub use error::Error;
