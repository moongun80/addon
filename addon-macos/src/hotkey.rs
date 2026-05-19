//! Carbon HotKey API FFI bindings for macOS global key bindings.
//!
//! This module wraps the Carbon `EventHotKeyRegister`/`EventHotKeyUnregister`
//! family of functions to install, manage, and tear down global hotkey
//! registrations on macOS.

use std::os::raw::{c_int, c_uint, c_ulong, c_void};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::keymap::KeyStroke;

/// Opaque handle returned by the Carbon HotKey Manager.
pub type HotKeyRef = *mut c_void;

/// Unique identifier for a registered hotkey.
///
/// The high-order word is the creator code; the low-order word is
/// a sequence number.  These are passed to the OS to identify
/// which hotkey fired in the callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotKeyId {
    creator: c_uint,
    id: c_uint,
}

/// A single registered hotkey with its callback.
pub struct HotKey {
    /// Opaque reference to the Carbon hotkey.
    ref_: HotKeyRef,
    /// Key stroke that triggered this hotkey.
    key_stroke: KeyStroke,
    /// Callback invoked when the hotkey fires.
    callback: Box<dyn Fn(&KeyStroke) + Send>,
}

/// Creator code used as part of every hotkey ID.
/// Any four-character string is valid; we use a fixed value.
const CREATOR: u32 = 0x61646461; // "adda"

/// Monotonically increasing counter for hotkey IDs.
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

// ---------------------------------------------------------------------------
// Carbon FFI
// ---------------------------------------------------------------------------

type EventHandlerUPP = extern "C" fn(event: *mut c_void, userData: *mut c_void);

type EventHotKeyID = c_ulong;

type EventHotKeyRef = *mut c_void;

/// Registers a global hotkey with the Carbon event system.
///
/// Returns a reference that must be freed by [`unregister_event_hot_key`].
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn RegisterEventHotKey(
        hotKeyID: c_uint,
        modifiers: c_uint,
        targetHotKeyID: *mut c_void,
        handler: EventHandlerUPP,
        eventHotKeyRef: *mut EventHotKeyRef,
    ) -> c_int;

    fn UnregisterEventHotKey(eventHotKeyRef: EventHotKeyRef) -> c_int;
}

// ---------------------------------------------------------------------------
// Implementation helpers
// ---------------------------------------------------------------------------

/// Encode a `HotKeyId` into the `c_uint` expected by Carbon.
fn hot_key_id_to_uint(id: HotKeyId) -> c_uint {
    // Carbon packs creator (high) and id (low) into a single UInt32
    ((id.creator as u32) << 16) | (id.id & 0xFFFF)
}

/// Decode a `c_uint` from the Carbon callback back into a `HotKeyId`.
fn uint_to_hot_key_id(val: c_uint) -> HotKeyId {
    HotKeyId {
        creator: (val >> 16) as c_uint,
        id: val as c_uint & 0xFFFF,
    }
}

/// Convert a `Modifier` bitmask to Carbon event modifier flags.
fn modifiers_to_cocoa(modifiers: &[addon_core::keymap::Modifier]) -> c_uint {
    let mut flags: c_uint = 0;
    for m in modifiers {
        match m {
            addon_core::keymap::Modifier::Control  => flags |= 0x0010,
            addon_core::keymap::Modifier::Shift    => flags |= 0x0002,
            addon_core::keymap::Modifier::Alt      => flags |= 0x0008,
            addon_core::keymap::Modifier::Option   => flags |= 0x0008,
            addon_core::keymap::Modifier::Command  => flags |= 0x0001,
            addon_core::keymap::Modifier::CapsLock => flags |= 0x0004,
        }
    }
    flags
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl HotKey {
    /// Creates a new hotkey and registers it with the Carbon event system.
    pub fn new(key_stroke: KeyStroke, callback: Box<dyn Fn(&KeyStroke) + Send>) -> Option<Self> {
        let id = HotKeyId {
            creator: CREATOR,
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        };

        let modifiers = modifiers_to_cocoa(&key_stroke.modifiers);
        let hot_key_id = hot_key_id_to_uint(id);

        let mut ref_: HotKeyRef = std::ptr::null_mut();

        let result = unsafe {
            RegisterEventHotKey(
                hot_key_id,
                modifiers,
                std::ptr::null_mut(),
                event_handler,
                &mut ref_ as *mut _,
            )
        };

        if result != 0 {
            tracing::error!(
                "RegisterEventHotKey failed for {} (code={})",
                key_stroke.display(),
                result
            );
            return None;
        }

        Some(Self {
            ref_,
            key_stroke,
            callback,
        })
    }

    /// Returns the key stroke associated with this hotkey.
    pub fn key_stroke(&self) -> &KeyStroke {
        &self.key_stroke
    }

    /// Unregisters this hotkey from the OS.
    pub fn unregister(self) {
        if !self.ref_.is_null() {
            unsafe { UnregisterEventHotKey(self.ref_) };
        }
    }
}

impl Drop for HotKey {
    fn drop(&mut self) {
        if !self.ref_.is_null() {
            unsafe { UnregisterEventHotKey(self.ref_) };
        }
    }
}

// ---------------------------------------------------------------------------
// Event handler
// ---------------------------------------------------------------------------

/// Global table mapping HotKeyId → KeyStroke for callback dispatch.
static KEY_STROKES: std::sync::Mutex<std::collections::HashMap<HotKeyId, KeyStroke>> =
    std::sync::Mutex::new(std::collections::HashMap::new());

/// Callback table for hotkey IDs.
static CALLBACKS: std::sync::Mutex<
    std::collections::HashMap<HotKeyId, Box<dyn Fn(&KeyStroke) + Send>>,
> = std::sync::Mutex::new(std::collections::HashMap::new());

/// Registers a callback for the given hotkey ID and key stroke.
fn register_callback(
    id: HotKeyId,
    key_stroke: KeyStroke,
    callback: Box<dyn Fn(&KeyStroke) + Send>,
) {
    KEY_STROKES.lock().unwrap().insert(id, key_stroke);
    CALLBACKS.lock().unwrap().insert(id, callback);
}

/// The C-function entry point called by Carbon when a hotkey fires.
///
/// Forwards to Rust callback via the global tables.
extern "C" fn event_handler(event: *mut c_void, _userData: *mut c_void) {
    // Decode the hotkey ID from the event data (first field).
    let val = if !event.is_null() {
        unsafe { *event as c_uint }
    } else {
        return;
    };

    let id = uint_to_hot_key_id(val);

    if let Some(stroke) = KEY_STROKES.lock().unwrap().get(&id).cloned() {
        if let Some(callback) = CALLBACKS.lock().unwrap().get(&id) {
            callback(&stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_roundtrip() {
        let id = HotKeyId {
            creator: 0x61646461,
            id: 42,
        };
        let encoded = hot_key_id_to_uint(id);
        let decoded = uint_to_hot_key_id(encoded);
        assert_eq!(id, decoded);
    }

    #[test]
    fn test_creator_decode() {
        let id = HotKeyId {
            creator: CREATOR,
            id: 1,
        };
        let encoded = hot_key_id_to_uint(id);
        let decoded = uint_to_hot_key_id(encoded);
        assert_eq!(decoded.creator, CREATOR);
    }
}
