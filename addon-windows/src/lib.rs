//! # addon-windows
//!
//! Windows adapter for the addon. Provides platform-specific key binding hooks
//! using the [`OsAdapter`] trait defined in `addon-core`.

pub use addon_core::{OsAdapter, OsPlatform, Error};

/// A Windows-specific adapter that installs global key bindings.
pub struct WindowsAdapter {
    /// Whether the adapter has been initialized.
    initialized: bool,
}

impl WindowsAdapter {
    /// Creates a new, uninitialized Windows adapter.
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl OsAdapter for WindowsAdapter {
    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing Windows adapter");
        // Placeholder: actual Windows SetWindowsHookEx / raw input goes here.
        self.initialized = true;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::AdapterNotAvailable("Windows adapter not initialized".to_string()));
        }
        tracing::info!("Starting Windows adapter");
        // Placeholder: begin monitoring events.
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        tracing::info!("Stopping Windows adapter");
        // Placeholder: tear down hook.
        self.initialized = false;
        Ok(())
    }

    fn get_platform(&self) -> OsPlatform {
        OsPlatform::Windows
    }
}
