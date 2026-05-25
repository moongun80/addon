//! # addon-linux
//!
//! Linux adapter for the addon. Provides platform-specific key binding hooks
//! using the X11 Xlib, XInput2 (XI2), and XTest extensions.
//!
//! ## Architecture
//!
//! 1. **X11 connection** — `XOpenDisplay` opens a connection to the X server.
//! 2. **XInput2 event grab** — Selects for `KeyPress`/`KeyRelease` events
//!    to capture global keyboard input.
//! 3. **XTest simulation** — `XTestFakeKeyEvent` generates synthetic key
//!    events for action execution (paste, shortcut, etc.).
//! 4. **Event dispatch** — captured key events are matched against the
//!    keymap and dispatched to registered actions.

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_uint, c_ulong};
use std::ptr::NonNull;

use addon_core::config::Config;
use addon_core::{error::Error, OsAdapter, OsPlatform};

// ---------------------------------------------------------------------------
// X11 FFI — opaque pointer types and core functions
// ---------------------------------------------------------------------------

/// Opaque X11 Display handle (non-null).
pub type XDisplay = *mut c_void;

/// Opaque X11 Window handle.
pub type XWindow = c_ulong;

/// Opaque X11 Keysym.
pub type XKeysym = c_ulong;

/// Minimal XEvent layout (we only inspect `type_`).
#[repr(C)]
pub struct XEvent {
    pub type_: c_int,
    pub serial: c_ulong,
    pub send_event: c_int,
    pub display: XDisplay,
    pub window: XWindow,
}

/// XInput2 event mask.
#[repr(C)]
pub struct XIEventMask {
    pub deviceid: c_int,
    pub mask: *mut u8,
}

// X11 core
// FIX-020: The #[allow(dead_code)] attribute suppresses warnings for unused FFI
// declarations. The following functions are reserved for future use:
//   - XTestQueryExtension: XTest extension capability query
//   - XkbKeycodeToKeysym: keycode-to-keysym translation (XKB)
//   - XKeysymToString: keysym-to-string conversion (XKB)
//   - XTestFakeMotionEvent: mouse motion simulation
//   - XTestFakeButtonEvent: mouse button simulation
//   - XQueryKeymap: query current modifier state
// TODO: Remove these declarations when they are actually used or move to a dedicated FFI module.
#[link(name = "X11", kind = "dylib")]
#[allow(dead_code)]
extern "C" {
    fn XOpenDisplay(name: *const c_char) -> XDisplay;
    fn XCloseDisplay(dpy: XDisplay) -> c_int;
    fn XNextEvent(dpy: XDisplay, event: *mut XEvent) -> c_int;
    fn XPending(dpy: XDisplay) -> c_int;
    fn XGrabKeyboard(
        dpy: XDisplay,
        grab_window: XWindow,
        owner_events: c_int,
        pointer_mode: c_int,
        keyboard_mode: c_int,
        redirect_key: c_ulong,
    ) -> c_int;
    fn XUngrabKeyboard(dpy: XDisplay, time: c_ulong) -> c_int;
    fn XQueryKeymap(dpy: XDisplay, keys: *mut [u8; 32]) -> c_int;
    fn XDefaultRootWindow(dpy: XDisplay) -> XWindow;

    // XTest extension (key simulation)
    fn XTestFakeKeyEvent(dpy: XDisplay, keycode: c_uint, is_press: c_int, current_time: c_ulong);
    fn XTestFakeMotionEvent(
        dpy: XDisplay,
        screen: c_int,
        x: f64,
        y: f64,
        current_time: c_ulong,
    ) -> c_int;
    fn XTestFakeButtonEvent(
        dpy: XDisplay,
        button: c_uint,
        is_press: c_int,
        current_time: c_ulong,
    ) -> c_int;

    // XTest query extension
    fn XTestQueryExtension(
        dpy: XDisplay,
        event_base: *mut c_int,
        error_base: *mut c_int,
        major: *mut c_int,
        minor: *mut c_int,
    ) -> c_int;

    // XInput2 extension
    fn XIQueryVersion(dpy: XDisplay, major: *mut c_int, minor: *mut c_int) -> c_int;
    fn XISelectEvents(
        dpy: XDisplay,
        grab_window: XWindow,
        masks: *mut XIEventMask,
        count: c_int,
    ) -> c_int;

    // XKB (keycode → keysym translation)
    fn XkbKeycodeToKeysym(dpy: XDisplay, keycode: c_uint, group: c_int, level: c_int) -> XKeysym;
    fn XFlush(dpy: XDisplay) -> c_int;
    fn XKeysymToString(keysym: XKeysym) -> *const c_char;
}

// X11 constants (uppercase per Rust convention)
const FALSE: c_int = 0;
const TRUE: c_int = 1;
const CURRENT_TIME: c_ulong = 0;
const GRAB_MODE_ASYNC: c_int = 2;

// XInput2 event types
#[allow(dead_code)]
const XI_ALL_DEVICES: c_int = -1;
#[allow(dead_code)]
const XI_RAW_KEY_PRESS: c_int = 238;
#[allow(dead_code)]
const XI_RAW_KEY_RELEASE: c_int = 239;
#[allow(dead_code)]
const XI_HIERARCHY_CHANGED: c_int = 17;

// ---------------------------------------------------------------------------
// Thread-safe X11 display wrapper
// ---------------------------------------------------------------------------

/// Thread-safe wrapper around a raw X11 Display pointer.
///
/// X11 connections are thread-safe (see the Xlib manual), so we can
/// safely mark this as `Send + Sync`.
pub struct XDisplayHandle {
    inner: NonNull<c_void>,
}

unsafe impl Send for XDisplayHandle {}
unsafe impl Sync for XDisplayHandle {}

impl XDisplayHandle {
    /// Create from a raw non-null display pointer.
    pub fn new(dpy: XDisplay) -> Result<Self, Error> {
        NonNull::new(dpy).map(|inner| Self { inner })
            .ok_or_else(|| Error::AdapterNotAvailable(
                "X11 display pointer is null".to_string()
            ))
    }

    /// Borrow the raw pointer.
    pub fn as_ptr(&self) -> XDisplay {
        self.inner.as_ptr()
    }
}

impl Drop for XDisplayHandle {
    fn drop(&mut self) {
        unsafe { XCloseDisplay(self.inner.as_ptr()) };
    }
}

/// A Linux X11-specific adapter that uses X11 for global key capture
/// and XTest for key simulation.
pub struct LinuxX11Adapter {
    /// Configuration loaded from disk.
    config: Config,
    /// X11 display connection (None if not yet opened).
    display: Option<XDisplayHandle>,
    /// Whether the adapter has been fully initialized.
    initialized: bool,
}

impl LinuxX11Adapter {
    /// Creates a new Linux X11 adapter with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            config,
            display: None,
            initialized: false,
        }
    }

    /// Opens the X11 display connection.
    fn open_display(&mut self) -> Result<(), Error> {
        if self.display.is_some() {
            return Ok(());
        }

        let display_name = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".to_string());
        let display_name = std::ffi::CString::new(display_name)
            .map_err(|e| Error::AdapterNotAvailable(format!("Invalid DISPLAY name: {}", e)))?;

        let dpy = unsafe { XOpenDisplay(display_name.as_ptr()) };

        if dpy.is_null() {
            return Err(Error::AdapterNotAvailable(
                "Failed to open X11 display. Is DISPLAY set?".to_string(),
            ));
        }

        tracing::info!("Opened X11 display");
        self.display = Some(XDisplayHandle::new(dpy)?);
        Ok(())
    }

    /// Registers XI2 raw keyboard events for all devices.
    /// This enables global key capture across all applications.
    fn register_xi2(&mut self) -> Result<(), Error> {
        let dpy = self
            .display
            .as_ref()
            .map(|h| h.as_ptr())
            .ok_or_else(|| Error::AdapterNotAvailable("X11 display not open".to_string()))?;

        // Check XI2 version support
        let mut major = 2;
        let mut minor = 2;
        let result = unsafe { XIQueryVersion(dpy, &mut major, &mut minor) };
        if result == FALSE {
            return Err(Error::AdapterNotAvailable(
                "XInput2 extension not available".to_string(),
            ));
        }
        tracing::info!("XInput2 version supported: {}.{}", major, minor);

        // Get the root window for XISelectEvents
        let root_window = unsafe { XDefaultRootWindow(dpy) };

        // Setup event mask for raw key press/release events
        let mut mask = [0u8; 32];
        mask[(XI_RAW_KEY_PRESS / 8) as usize] |= 1 << (XI_RAW_KEY_PRESS % 8) as u8;
        mask[(XI_RAW_KEY_RELEASE / 8) as usize] |= 1 << (XI_RAW_KEY_RELEASE % 8) as u8;

        let xi_mask = XIEventMask {
            deviceid: XI_ALL_DEVICES,
            mask: mask.as_mut_ptr(),
        };

        let result = unsafe {
            XISelectEvents(
                dpy,
                root_window,
                &xi_mask as *const XIEventMask as *mut XIEventMask,
                1,
            )
        };

        if result == FALSE {
            return Err(Error::AdapterNotAvailable(
                "Failed to select XI2 raw events".to_string(),
            ));
        }

        tracing::info!("XI2 raw keyboard events registered for all devices");
        Ok(())
    }

    /// Closes the X11 display connection.
    fn close_display(&mut self) {
        self.display.take();
    }

    /// Register key bindings (no-op; keymap is built by `Config::build_keymapper`).
    fn register_bindings(&mut self) -> Result<(), Error> {
        tracing::info!(
            "Configured {} key binding(s) for Linux X11",
            self.config.keybindings.len()
        );
        Ok(())
    }

    /// Simulates a key press or release via the XTest extension.
    pub fn simulate_key(&self, keycode: c_uint, press: bool) -> Result<(), Error> {
        let dpy = self
            .display
            .as_ref()
            .map(|h| h.as_ptr())
            .ok_or_else(|| Error::AdapterNotAvailable("X11 display not open".to_string()))?;

        unsafe { XTestFakeKeyEvent(dpy, keycode, if press { TRUE } else { FALSE }, CURRENT_TIME) };

        unsafe { XFlush(dpy) };
        Ok(())
    }

    /// Grabs the keyboard to capture all key events globally.
    fn grab_keyboard(&mut self) -> Result<(), Error> {
        let dpy = self
            .display
            .as_ref()
            .map(|h| h.as_ptr())
            .ok_or_else(|| Error::AdapterNotAvailable("X11 display not open".to_string()))?;

        let result = unsafe {
            XGrabKeyboard(
                dpy,
                XDefaultRootWindow(dpy), // FIX-006: Use root window instead of REVERT_TO_NONE (invalid grab_window)
                TRUE,                    // owner_events: allow pointer events to pass through
                GRAB_MODE_ASYNC,
                GRAB_MODE_ASYNC,
                CURRENT_TIME as c_ulong,
            )
        };

        if result == FALSE {
            return Err(Error::AdapterNotAvailable(
                "Failed to grab keyboard. Is another app holding it?".to_string(),
            ));
        }

        tracing::info!("Keyboard grabbed");
        Ok(())
    }

    /// Releases the keyboard grab.
    fn ungrab_keyboard(&mut self) {
        if let Some(dpy) = self.display.as_ref().map(|h| h.as_ptr()) {
            unsafe { XUngrabKeyboard(dpy, CURRENT_TIME) };
            tracing::info!("Keyboard ungrabbed");
        }
    }

    /// Processes pending X11 events.
    ///
    /// Drains the X11 event queue and processes any key events that are
    /// available. This is needed because the Linux adapter captures events
    /// via X11 grabs but doesn't have its own event loop — the caller
    /// must invoke this method periodically to process captured events.
    ///
    /// Returns `Ok(())` on success, or an error if the display is not open.
    pub fn process_events(&mut self) -> Result<(), Error> {
        let dpy = self
            .display
            .as_ref()
            .map(|h| h.as_ptr())
            .ok_or_else(|| Error::AdapterNotAvailable("X11 display not open".to_string()))?;

        // Process all pending events in a loop.
        while unsafe { XPending(dpy) } > 0 {
            let mut event = unsafe { std::mem::zeroed::<XEvent>() };
            unsafe { XNextEvent(dpy, &mut event as *mut _) };

            // We only care about key-related events here.
            // In a full implementation, this would dispatch to the keymap.
            tracing::trace!("Processed X11 event type: {}", event.type_);
        }

        Ok(())
    }
}

impl OsAdapter for LinuxX11Adapter {
    fn set_config(&mut self, config: &Config) {
        self.config = config.clone();
    }

    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing Linux X11 adapter");
        self.open_display()?;
        self.register_xi2()?;
        self.register_bindings()?;
        self.initialized = true;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::AdapterNotAvailable(
                "Linux X11 adapter not initialized".to_string(),
            ));
        }

        self.grab_keyboard()?;
        tracing::info!("Linux X11 adapter started — keyboard grabbed, event loop pending");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        tracing::info!("Stopping Linux X11 adapter");
        self.ungrab_keyboard();
        self.close_display();
        self.initialized = false;
        Ok(())
    }

    fn get_platform(&self) -> OsPlatform {
        OsPlatform::Linux
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use addon_core::actions::Action;
    use addon_core::config::KeyBinding;

    fn test_config() -> Config {
        Config {
            version: "1.0".to_string(),
            global: addon_core::config::GlobalSettings::default(),
            keybindings: vec![KeyBinding {
                id: "test_paste".to_string(),
                keys: vec!["Ctrl+V".to_string()],
                action: Action::Paste {
                    text: "hello".to_string(),
                },
                overrides: None,
            }],
        }
    }

    #[test]
    fn test_keymap_build() {
        let config = test_config();
        let mut adapter = LinuxX11Adapter::new(config.clone());
        adapter.register_bindings().unwrap();

        let stroke = addon_core::keymap::KeyStroke::parse("Ctrl+V").unwrap();
        let mapper = config.build_keymapper(OsPlatform::Linux);
        assert!(mapper.lookup(&stroke).is_some());
    }

    #[test]
    fn test_keymap_missing() {
        let config = test_config();
        let mut adapter = LinuxX11Adapter::new(config.clone());
        adapter.register_bindings().unwrap();

        let stroke = addon_core::keymap::KeyStroke::parse("Ctrl+X").unwrap();
        let mapper = config.build_keymapper(OsPlatform::Linux);
        assert!(mapper.lookup(&stroke).is_none());
    }

    #[test]
    fn test_platform() {
        let config = test_config();
        let adapter = LinuxX11Adapter::new(config);
        assert_eq!(adapter.get_platform(), OsPlatform::Linux);
    }
}
