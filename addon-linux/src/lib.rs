//! # addon-linux
//!
//! Linux adapter for the addon. Provides platform-specific key binding hooks
//! using the [`OsAdapter`] trait defined in `addon-core`.

pub use addon_core::{OsAdapter, OsPlatform, Error};

/// A Linux-specific adapter that installs global key bindings.
pub struct LinuxAdapter {
    /// Whether the adapter has been initialized.
    initialized: bool,
}

impl LinuxAdapter {
    /// Creates a new, uninitialized Linux adapter.
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl OsAdapter for LinuxAdapter {
    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing Linux adapter");
        // Placeholder: actual X11 / Wayland key grab goes here.
        self.initialized = true;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::AdapterNotAvailable("Linux adapter not initialized".to_string()));
        }
        tracing::info!("Starting Linux adapter");
        // Placeholder: begin monitoring events.
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        tracing::info!("Stopping Linux adapter");
        // Placeholder: ungrab keys.
        self.initialized = false;
        Ok(())
    }

    fn get_platform(&self) -> OsPlatform {
        OsPlatform::Linux
    }
}
