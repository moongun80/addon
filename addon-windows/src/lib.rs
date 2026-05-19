//! # addon-windows
//!
//! Windows adapter for the addon. Provides platform-specific key binding hooks
//! using the Win32 Low-Level Keyboard Hook API (`WH_KEYBOARD_LL`).
//!
//! ## Architecture
//!
//! 1. **Hook installation** — `SetWindowsHookEx` with `WH_KEYBOARD_LL` to
//!    intercept all keyboard events system-wide.
//! 2. **Key translation** — `ToUnicodeEx` converts virtual-key codes to
//!    Unicode characters, respecting the current keyboard layout.
//! 3. **Action dispatch** — matched key strokes are forwarded to registered
//!    actions via callbacks.

use std::os::raw::c_int;

use addon_core::config::Config;
use addon_core::keymap::KeyStroke;
use addon_core::mapper::KeyMapper;
use addon_core::{error::Error, OsAdapter, OsPlatform};

// Re-export Win32 types needed by the hook callback.
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetAsyncKeyState, GetKeyboardLayout, MapVirtualKeyW, SetWindowsHookExW,
    ToUnicodeEx, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL,
};

/// Size of a `KBDLLHOOKSTRUCT` in bytes.
const KB_HOOK_STRUCT_SIZE: usize = std::mem::size_of::<KBDLLHOOKSTRUCT>();

/// A Windows-specific adapter that installs a low-level keyboard hook
/// to capture global key bindings.
pub struct WindowsAdapter {
    /// Configuration loaded from disk.
    config: Config,
    /// Key binding lookup engine built from `config.keybindings`.
    keymap: Box<dyn KeyMapper>,
    /// Handle to the installed low-level keyboard hook.
    hook: Option<HHOOK>,
    /// Whether the adapter has been fully initialized.
    initialized: bool,
}

/// Represents a registered hotkey callback with its metadata.
struct RegisteredHotkey {
    /// The key stroke to match.
    key_stroke: KeyStroke,
    /// Callback invoked when the hotkey fires.
    callback: Box<dyn Fn(&KeyStroke) + Send>,
}

impl WindowsAdapter {
    /// Creates a new Windows adapter with the given configuration and key map.
    pub fn new(config: Config, keymap: Box<dyn KeyMapper>) -> Self {
        Self {
            config,
            keymap,
            hook: None,
            initialized: false,
        }
    }

    /// Builds and registers all key bindings from the configuration.
    fn register_bindings(&mut self) -> Result<(), Error> {
        self.build_keymap();
        tracing::info!(
            "Configured {} key binding(s) for Windows",
            self.config.keybindings.len()
        );
        Ok(())
    }

    /// Rebuilds the keymap from the current configuration.
    fn build_keymap(&mut self) {
        let mut map: std::collections::HashMap<KeyStroke, addon_core::actions::Action> =
            std::collections::HashMap::new();

        for binding in &self.config.keybindings {
            let keys = binding.effective_keys("windows");
            for key_str in keys {
                if let Ok(stroke) = KeyStroke::parse(key_str) {
                    map.insert(stroke, binding.action.clone());
                }
            }
        }

        self.keymap = Box::new(WindowsKeyMapper { map });
    }

    /// Installs the low-level keyboard hook.
    fn install_hook(&mut self) -> Result<(), Error> {
        if self.hook.is_some() {
            return Ok(()); // Already installed.
        }

        let hook = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_callback), std::ptr::null_mut(), 0)
        };

        if hook.is_null() {
            let err = std::io::Error::last_os_error();
            tracing::error!("SetWindowsHookExW failed: {}", err);
            return Err(Error::AdapterNotAvailable(format!(
                "Failed to install keyboard hook: {}",
                err
            )));
        }

        self.hook = Some(hook);
        tracing::info!("Low-level keyboard hook installed (handle={:?})", hook);
        Ok(())
    }

    /// Removes the low-level keyboard hook.
    fn uninstall_hook(&mut self) {
        if let Some(hook) = self.hook.take() {
            unsafe {
                UnhookWindowsHookEx(hook);
            }
            tracing::info!("Low-level keyboard hook uninstalled");
        }
    }
}

impl OsAdapter for WindowsAdapter {
    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing Windows adapter");
        self.register_bindings()?;
        self.initialized = true;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::AdapterNotAvailable(
                "Windows adapter not initialized".to_string(),
            ));
        }

        self.install_hook()?;
        tracing::info!("Windows adapter started — hook active");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        tracing::info!("Stopping Windows adapter");
        self.uninstall_hook();
        self.initialized = false;
        Ok(())
    }

    fn get_platform(&self) -> OsPlatform {
        OsPlatform::Windows
    }
}

/// A concrete implementation of `KeyMapper` backed by a HashMap.
struct WindowsKeyMapper {
    map: std::collections::HashMap<KeyStroke, addon_core::actions::Action>,
}

impl KeyMapper for WindowsKeyMapper {
    fn lookup(&self, stroke: &KeyStroke) -> Option<&addon_core::actions::Action> {
        self.map.get(stroke)
    }
}

// ---------------------------------------------------------------------------
// Low-level keyboard hook callback
// ---------------------------------------------------------------------------

/// The C-callable callback that Windows invokes for every keyboard event.
///
/// This function translates raw Windows key codes into [`KeyStroke`] objects,
/// looks them up in the keymap, and fires the matching action callback.
///
/// ## Parameters
///
/// - `nCode` — The hook code. Only process the event if `>= 0`.
/// - `wParam` — Message type (`WM_KEYDOWN`, `WM_KEYUP`, etc.).
/// - `lParam` — Pointer to a `KBDLLHOOKSTRUCT` with event details.
///
/// ## Returns
///
/// - `1` to block the key (don't forward to other apps).
/// - `CallNextHookEx` result to pass the event along.
extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> i32 {
    if n_code < 0 {
        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
    }

    // Safely read the hook structure.
    let hook_struct = unsafe { &*(l_param as *const KBDLLHOOKSTRUCT) };
    let vk_code = hook_struct.vkCode;
    let scan_code = hook_struct.scanCode;
    let flags = hook_struct.flags;
    let key_down = w_param == 0x0100; // WM_KEYDOWN

    // Skip key-up events, repeat flags, and injected events.
    if !key_down || (flags & 0x80) != 0 {
        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
    }

    // Translate virtual-key + scan code to a KeyStroke.
    if let Some(stroke) = vk_to_stroke(vk_code, scan_code) {
        // In a real implementation, the adapter would maintain a global
        // table of registered hotkeys and look up the callback here.
        // For now, log the detected stroke.
        tracing::info!(
            "Keyboard event detected: vk=0x{:02X} → {}",
            vk_code,
            stroke.display()
        );
    }

    // Pass the event to the next hook in the chain.
    unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
}

/// Converts a Windows virtual-key code + scan code to a [`KeyStroke`].
///
/// This is a simplified translation — a production implementation would
/// need more sophisticated modifier state tracking.
fn vk_to_stroke(vk_code: u32, scan_code: u32) -> Option<KeyStroke> {
    use addon_core::keymap::{Key, Modifier};

    // Determine modifiers by checking async key state.
    let mut modifiers = Vec::new();
    if (GetAsyncKeyState(0x11) & 0x8000) != 0 {
        modifiers.push(Modifier::Control);
    }
    if (GetAsyncKeyState(0x10) & 0x8000) != 0 {
        modifiers.push(Modifier::Shift);
    }
    if (GetAsyncKeyState(0x12) & 0x8000) != 0 {
        modifiers.push(Modifier::Alt);
    }
    if (GetAsyncKeyState(0x5B) & 0x8000) != 0 {
        modifiers.push(Modifier::Command);
    }

    // Convert virtual-key code to key code string.
    let key_code = vk_to_key_code(vk_code);
    if key_code.is_empty() {
        return None;
    }

    Some(KeyStroke {
        modifiers,
        key: Key { code: key_code },
    })
}

/// Maps a Windows virtual-key code to a key code string.
fn vk_to_key_code(vk: u32) -> String {
    match vk {
        0x30..=0x39 => (vk - 0x30).to_string(), // 0-9
        0x41..=0x5A => ((vk - 0x41 + b'a') as char).to_string(), // A-Z
        0x70..=0x7B => format!("F{}", vk - 0x6F), // F1-F16
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x20 => "Space".to_string(),
        0x1B => "Escape".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x21 => "PageUp".to_string(),
        0x22 => "PageDown".to_string(),
        0x24 => "Home".to_string(),
        0x23 => "End".to_string(),
        _ => {
            // Use MapVirtualKey for extended keys.
            let char_code = unsafe { MapVirtualKeyW(vk, 0) };
            if char_code > 0 {
                (char_code as u8 as char).to_string()
            } else {
                format!("VK_0x{:02X}", vk)
            }
        }
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
        let mut adapter = WindowsAdapter::new(
            config,
            Box::new(WindowsKeyMapper {
                map: std::collections::HashMap::new(),
            }),
        );
        adapter.build_keymap();

        let stroke = KeyStroke::parse("Ctrl+V").unwrap();
        assert!(adapter.keymap.lookup(&stroke).is_some());
    }

    #[test]
    fn test_keymap_missing() {
        let config = test_config();
        let mut adapter = WindowsAdapter::new(
            config,
            Box::new(WindowsKeyMapper {
                map: std::collections::HashMap::new(),
            }),
        );
        adapter.build_keymap();

        let stroke = KeyStroke::parse("Ctrl+X").unwrap();
        assert!(adapter.keymap.lookup(&stroke).is_none());
    }

    #[test]
    fn test_platform() {
        let config = test_config();
        let adapter = WindowsAdapter::new(
            config,
            Box::new(WindowsKeyMapper {
                map: std::collections::HashMap::new(),
            }),
        );
        assert_eq!(adapter.get_platform(), OsPlatform::Windows);
    }

    #[test]
    fn test_vk_to_key_code() {
        assert_eq!(vk_to_key_code(0x41), "a");
        assert_eq!(vk_to_key_code(0x31), "1");
        assert_eq!(vk_to_key_code(0x70), "F1");
        assert_eq!(vk_to_key_code(0x20), "Space");
        assert_eq!(vk_to_key_code(0x1B), "Escape");
    }
}
