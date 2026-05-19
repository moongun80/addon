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
    /// Returns the platform used for "all platforms" conflict detection.
    pub fn platform_all() -> OsPlatform {
        OsPlatform::Linux
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

    /// Returns the platform this adapter targets.
    fn get_platform(&self) -> OsPlatform;
}
