//! Error types for the addon core library.
//!
/// Defines the [`Error`] enum used throughout the crate for consistent
/// error handling.
use thiserror::Error;

/// The primary error type for the addon core.
///
/// All public-facing functions in `addon-core` should return
/// `Result<T, Error>` to provide clear, actionable error messages.
#[derive(Debug, Error)]
pub enum Error {
    /// Configuration parsing failed (invalid YAML or schema violation).
    #[error("config parse error: {0}")]
    Parse(String),

    /// A key binding conflict was detected.
    #[error("conflict detected: {0}")]
    Conflict(String),

    /// The OS adapter for the current platform is not available.
    #[error("OS adapter not available: {0}")]
    AdapterNotAvailable(String),

    /// An invalid key or key stroke was encountered.
    #[error("invalid key: {0}")]
    InvalidKey(String),
}

/// Convenience alias for `Result<T, Error>`.
pub type Result<T> = std::result::Result<T, Error>;
