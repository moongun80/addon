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

use std::os::raw::{c_char, c_int, c_ulong, c_uint};

use addon_core::config::Config;
use addon_core::keymap::KeyStroke;
use addon_core::mapper::KeyMapper;
use addon_core::{OsAdapter, OsPlatform, error::Error};

// ---------------------------------------------------------------------------
// X11 FFI — opaque pointer types and core functions
// ---------------------------------------------------------------------------

/// Opaque X11 Display handle.
pub type XDisplay = *mut std::ffi::c_void;

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
#[link(name = "X11", kind = "dylib")]
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

    // XTest extension (key simulation)
    fn XTestFakeKeyEvent(
        dpy: XDisplay,
        keycode: c_uint,
        is_press: c_int,
        current_time: c_ulong,
    ) -> c_int;
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
    fn XISelectEvents(
        dpy: XDisplay,
        grab_window: XWindow,
        masks: *mut XIEventMask,
        count: c_int,
    ) -> c_int;

    // XKB (keycode → keysym translation)
    fn XkbKeycodeToKeysym(
        dpy: XDisplay,
        keycode: c_uint,
        group: c_int,
        level: c_int,
    ) -> XKeysym;
    fn XKeysymToString(keysym: XKeysym) -> *const c_char;
}

// X11 constants
pub const False: c_int = 0;
pub const True: c_int = 1;
pub const PointerModeAsync: c_int = 0;
pub const KeyboardModeAsync: c_int = 0;
pub const RevertToRoot: XWindow = 0;
pub const CurrentTime: c_ulong = 0;

// XInput2 event types
pub const XIAllDevices: c_int = -1;
pub const XI_RawKeyPress: c_int = 238;
pub const XI_RawKeyRelease: c_int = 239;
pub const XI_HierarchyChanged: c_int = 17;

// XEvent type constants
pub const KeyPress: c_int = 2;
pub const KeyRelease: c_int = 3;
pub const GenericEvent: c_int = 34;

/// A Linux X11-specific adapter that uses X11 for global key capture
/// and XTest for key simulation.
pub struct LinuxX11Adapter {
    /// Configuration loaded from disk.
    config: Config,
    /// Key binding lookup engine built from `config.keybindings`.
    keymap: Box<dyn KeyMapper>,
    /// X11 display connection (None if not yet opened).
    display: Option<XDisplay>,
    /// Whether the adapter has been fully initialized.
    initialized: bool,
}

impl LinuxX11Adapter {
    /// Creates a new Linux X11 adapter with the given configuration and key map.
    pub fn new(config: Config, keymap: Box<dyn KeyMapper>) -> Self {
        Self {
            config,
            keymap,
            display: None,
            initialized: false,
        }
    }

    /// Opens the X11 display connection.
    fn open_display(&mut self) -> Result<(), Error> {
        if self.display.is_some() {
            return Ok(());
        }

        let display_name = std::ffi::CString::new(":0").map_err(|e| {
            Error::AdapterNotAvailable(format!("Invalid display name: {}", e))
        })?;

        let dpy = unsafe { XOpenDisplay(display_name.as_ptr()) };

        if dpy.is_null() {
            return Err(Error::AdapterNotAvailable(
                "Failed to open X11 display. Is DISPLAY set?".to_string(),
            ));
        }

        tracing::info!("Opened X11 display");
        self.display = Some(dpy);
        Ok(())
    }

    /// Closes the X11 display connection.
    fn close_display(&mut self) {
        if let Some(dpy) = self.display.take() {
            unsafe { XCloseDisplay(dpy) };
            tracing::info!("Closed X11 display");
        }
    }

    /// Builds and registers all key bindings from the configuration.
    fn register_bindings(&mut self) -> Result<(), Error> {
        self.build_keymap();
        tracing::info!(
            "Configured {} key binding(s) for Linux X11",
            self.config.keybindings.len()
        );
        Ok(())
    }

    /// Rebuilds the keymap from the current configuration.
    fn build_keymap(&mut self) {
        let mut map: std::collections::HashMap<KeyStroke, addon_core::actions::Action> =
            std::collections::HashMap::new();

        for binding in &self.config.keybindings {
            let keys = binding.effective_keys("linux");
            for key_str in keys {
                if let Ok(stroke) = KeyStroke::parse(key_str) {
                    map.insert(stroke, binding.action.clone());
                }
            }
        }

        self.keymap = Box::new(LinuxX11KeyMapper { map });
    }

    /// Simulates a key press or release via the XTest extension.
    pub fn simulate_key(&self, keycode: c_uint, press: bool) -> Result<(), Error> {
        let dpy = self.display.ok_or_else(|| {
            Error::AdapterNotAvailable("X11 display not open".to_string())
        })?;

        let result = unsafe {
            XTestFakeKeyEvent(dpy, keycode, if press { 1 } else { 0 }, 0)
        };

        if result == False {
            return Err(Error::AdapterNotAvailable(
                format!("XTestFakeKeyEvent failed for keycode {}", keycode),
            ));
        }

        Ok(())
    }

    /// Grabs the keyboard to capture all key events globally.
    fn grab_keyboard(&mut self) -> Result<(), Error> {
        let dpy = self.display.ok_or_else(|| {
            Error::AdapterNotAvailable("X11 display not open".to_string())
        })?;

        let result = unsafe {
            XGrabKeyboard(
                dpy,
                RevertToRoot,
                1,    // owner_events: allow pointer events to pass through
                PointerModeAsync,
                KeyboardModeAsync,
                0,
            )
        };

        if result == False {
            return Err(Error::AdapterNotAvailable(
                "Failed to grab keyboard. Is another app holding it?".to_string(),
            ));
        }

        tracing::info!("Keyboard grabbed");
        Ok(())
    }

    /// Releases the keyboard grab.
    fn ungrab_keyboard(&mut self) {
        if let Some(dpy) = self.display {
            unsafe { XUngrabKeyboard(dpy, 0) };
            tracing::info!("Keyboard ungrabbed");
        }
    }
}

impl OsAdapter for LinuxX11Adapter {
    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing Linux X11 adapter");
        self.open_display()?;
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

/// A concrete implementation of `KeyMapper` backed by a HashMap.
struct LinuxX11KeyMapper {
    map: std::collections::HashMap<KeyStroke, addon_core::actions::Action>,
}

impl KeyMapper for LinuxX11KeyMapper {
    fn lookup(&self, stroke: &KeyStroke) -> Option<&addon_core::actions::Action> {
        self.map.get(stroke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use addon_core::config::KeyBinding;
    use addon_core::actions::Action;

    fn test_config() -> Config {
        Config {
            version: "1.0".to_string(),
            global: addon_core::config::GlobalSettings::default(),
            keybindings: vec![
                KeyBinding {
                    id: "test_paste".to_string(),
                    keys: vec!["Ctrl+V".to_string()],
                    action: Action::Paste {
                        text: "hello".to_string(),
                    },
                    overrides: None,
                },
            ],
        }
    }

    #[test]
    fn test_keymap_build() {
        let config = test_config();
        let mut adapter = LinuxX11Adapter::new(config, Box::new(LinuxX11KeyMapper {
            map: std::collections::HashMap::new(),
        }));
        adapter.build_keymap();

        let stroke = KeyStroke::parse("Ctrl+V").unwrap();
        assert!(adapter.keymap.lookup(&stroke).is_some());
    }

    #[test]
    fn test_keymap_missing() {
        let config = test_config();
        let mut adapter = LinuxX11Adapter::new(config, Box::new(LinuxX11KeyMapper {
            map: std::collections::HashMap::new(),
        }));
        adapter.build_keymap();

        let stroke = KeyStroke::parse("Ctrl+X").unwrap();
        assert!(adapter.keymap.lookup(&stroke).is_none());
    }

    #[test]
    fn test_platform() {
        let config = test_config();
        let adapter = LinuxX11Adapter::new(config, Box::new(LinuxX11KeyMapper {
            map: std::collections::HashMap::new(),
        }));
        assert_eq!(adapter.get_platform(), OsPlatform::Linux);
    }
}
