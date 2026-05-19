//! Common logging initialization.
//!
/// Sets up a [`tracing`] subscriber with reasonable defaults.
use crate::error::Error;

/// Initializes the tracing subscriber.
///
/// This function must be called once at application startup.
///
/// The log level can be controlled via the `RUST_LOG` environment variable.
/// The default level is `"info"` when `RUST_LOG` is not set.
///
/// # Errors
///
/// Returns an error if the tracing subscriber cannot be installed.
pub fn init() -> crate::error::Result<()> {
    use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .map_err(|e| Error::Parse(format!("invalid RUST_LOG: {e}")))?;

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::CLOSE)
        .try_init()
        .map_err(|e| Error::Parse(format!("failed to initialize tracing: {e}")))?;

    Ok(())
}
