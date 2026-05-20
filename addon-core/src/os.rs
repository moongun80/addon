//! OS adapter trait and platform identification.
//!
//! Defines the [`OsAdapter`] trait that each platform-specific crate
//! (`addon-macos`, `addon-windows`, `addon-linux`) implements,
//! and the [`OsPlatform`] enum identifying the operating system.

use crate::error::Error;

/// Identifies the target operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsPlatform {
    /// macOS
    Macos,
    /// Windows
    Windows,
    /// Linux
    Linux,
}

impl OsPlatform {
    /// Returns an iterator over all known platforms.
    ///
    /// Use this instead of the removed `platform_all()` when you need to
    /// iterate every platform (e.g. for cross-platform conflict detection).
    pub fn all() -> impl Iterator<Item = OsPlatform> {
        [OsPlatform::Macos, OsPlatform::Windows, OsPlatform::Linux].into_iter()
    }
}

/// Trait for OS-specific key binding adapters.
///
/// Each platform crate implements this trait to provide platform-specific
/// key event monitoring and action dispatch.
///
/// **Note:** This trait requires `Send + Sync` to allow sharing the
/// adapter across async task boundaries (e.g., the IPC server).
pub trait OsAdapter: Send + Sync {
    /// Initialize the OS adapter.
    ///
    /// Performs any required setup (e.g., requesting accessibility permissions,
    /// creating event taps or hooks).
    fn init(&mut self) -> Result<(), Error>;

    /// Start listening for key events.
    ///
    /// Must be called after [`init`][Self::init].
    fn start(&mut self) -> Result<(), Error>;

    /// Stop listening for key events.
    ///
    /// Tears down the event tap / hook and releases resources.
    fn stop(&mut self) -> Result<(), Error>;

    /// Update the adapter's internal configuration.
    ///
    /// Called during hot-reload so the adapter sees the latest config
    /// before being re-initialized. Default implementation is a no-op
    /// for adapters that don't store their own config copy.
    fn set_config(&mut self, _config: &crate::config::Config) {}

    /// Returns the platform this adapter targets.
    fn get_platform(&self) -> OsPlatform;
}
