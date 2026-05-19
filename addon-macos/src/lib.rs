//! # addon-macos
//!
//! macOS adapter for the addon. Provides platform-specific key binding hooks
//! using the [`OsAdapter`] trait defined in `addon-core`.

pub use addon_core::{OsAdapter, OsPlatform, Error};

/// A macOS-specific adapter that installs global key bindings.
pub struct MacOsAdapter {
    /// Whether the adapter has been initialized.
    initialized: bool,
}

impl MacOsAdapter {
    /// Creates a new, uninitialized macOS adapter.
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl OsAdapter for MacOsAdapter {
    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing macOS adapter");
        // Placeholder: actual macOS CGEvent tap registration goes here.
        self.initialized = true;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::AdapterNotAvailable("macOS adapter not initialized".to_string()));
        }
        tracing::info!("Starting macOS adapter");
        // Placeholder: begin monitoring events.
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        tracing::info!("Stopping macOS adapter");
        // Placeholder: tear down event tap.
        self.initialized = false;
        Ok(())
    }

    fn get_platform(&self) -> OsPlatform {
        OsPlatform::Macos
    }
}
