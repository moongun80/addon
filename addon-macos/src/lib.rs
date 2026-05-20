//! # addon-macos
//!
//! macOS adapter for the addon. Provides platform-specific key binding hooks
//! using Carbon EventHotKey API and Cocoa Event Taps.
//!
//! ## Architecture
//!
//! 1. **HotKey registration** — `HotKey` wrapper around Carbon
//!    `RegisterEventHotKey` for global shortcut detection.
//! 2. **Event dispatch** — when a registered hotkey fires, the adapter
//!    looks up the corresponding `Action` from the keymap and executes it
//!    via the `action_dispatcher` closure stored in the adapter.

use std::sync::Arc;

use addon_core::actions::Action;
use addon_core::config::Config;
use addon_core::keymap::KeyStroke;
use addon_core::mapper::KeyMapper;
use addon_core::{error::Error, OsAdapter, OsPlatform};

mod hotkey;
pub use hotkey::HotKey;

/// Closure type for dispatching actions when a hotkey fires.
///
/// Receives the matched key stroke and the associated action.
pub type ActionDispatcher = dyn Fn(&KeyStroke, &Action) + Send + Sync;

/// A macOS-specific adapter that installs global key bindings via Carbon
/// EventHotKey API and Cocoa EventTap for key simulation.
pub struct MacOsAdapter {
    /// Configuration loaded from disk.
    config: Config,
    /// Key binding lookup engine built from `config.keybindings`.
    keymap: Box<dyn KeyMapper>,
    /// Registered hotkeys.
    hotkeys: Vec<HotKey>,
    /// Whether the adapter has been fully initialized.
    initialized: bool,
    /// Closure invoked when a hotkey fires. Dispatches the matched action.
    ///
    /// This allows the daemon (which owns the adapter) to inject the actual
    /// action-execution logic without coupling the adapter crate to any
    /// specific execution mechanism.
    action_dispatcher: Arc<ActionDispatcher>,
}

impl MacOsAdapter {
    /// Creates a new macOS adapter with the given configuration and key map.
    ///
    /// The `action_dispatcher` closure is called whenever a registered hotkey
    /// fires, receiving the matched [`KeyStroke`] and the associated [`Action`].
    pub fn new(
        config: Config,
        keymap: Box<dyn KeyMapper>,
        action_dispatcher: Arc<ActionDispatcher>,
    ) -> Self {
        Self {
            config,
            keymap,
            hotkeys: Vec::new(),
            initialized: false,
            action_dispatcher,
        }
    }

    /// Builds and registers all key bindings from the configuration.
    fn register_bindings(&mut self) -> Result<(), Error> {
        // Rebuild keymap from config.
        self.build_keymap();

        let dispatcher = Arc::clone(&self.action_dispatcher);

        for binding in &self.config.keybindings {
            let keys = binding.effective_keys(OsPlatform::Macos);
            for key_str in keys {
                match KeyStroke::parse(key_str) {
                    Ok(stroke) => {
                        let binding_id = binding.id.clone();
                        let action = binding.action.clone();

                        let hotkey = HotKey::new(
                            stroke.clone(),
                            Box::new(move |s: &KeyStroke| {
                                tracing::info!(
                                    "Hotkey fired: {} (binding: {})",
                                    s.display(),
                                    binding_id
                                );
                                // Dispatch the action via the adapter's dispatcher.
                                dispatcher(s, &action);
                            }),
                        )
                        .ok_or_else(|| {
                            Error::AdapterNotAvailable(format!(
                                "Failed to register hotkey: {}",
                                key_str
                            ))
                        })?;

                        self.hotkeys.push(hotkey);
                    }
                    Err(e) => {
                        tracing::warn!("Skipping invalid key stroke {:?}: {}", key_str, e);
                    }
                }
            }
        }

        tracing::info!("Registered {} hotkey(s)", self.hotkeys.len());
        Ok(())
    }

    /// Rebuilds the keymap from the current configuration.
    fn build_keymap(&mut self) {
        self.keymap = self.config.build_keymapper(OsPlatform::Macos);
    }
}

impl OsAdapter for MacOsAdapter {
    fn init(&mut self) -> Result<(), Error> {
        tracing::info!("Initializing macOS adapter");

        // Rebuild keymap from current config (ensures consistency between
        // config and keymap when re-initializing after SetConfig).
        self.build_keymap();

        // Register all key bindings.
        self.register_bindings()?;

        self.initialized = true;
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        if !self.initialized {
            return Err(Error::AdapterNotAvailable(
                "macOS adapter not initialized".to_string(),
            ));
        }

        tracing::info!("Starting macOS adapter — running event loop");

        // The Carbon event hotkeys are registered globally and will fire
        // automatically. We return Ok here — the event loop is managed
        // by the OS event dispatcher.
        Ok(())
    }

    fn stop(&mut self) -> Result<(), Error> {
        tracing::info!("Stopping macOS adapter — unloading hotkeys");

        // Drop all hotkeys to unregister them.
        self.hotkeys.clear();

        self.initialized = false;
        Ok(())
    }

    fn get_platform(&self) -> OsPlatform {
        OsPlatform::Macos
    }
}

/// A concrete implementation of `KeyMapper` backed by a HashMap built
/// from the configuration file.
struct MacOsKeyMapper {
    map: std::collections::HashMap<KeyStroke, Action>,
}

impl KeyMapper for MacOsKeyMapper {
    fn lookup(&self, stroke: &KeyStroke) -> Option<&Action> {
        self.map.get(stroke)
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
        let dispatcher: Arc<ActionDispatcher> = Arc::new(|_, _| {});
        let mut adapter = MacOsAdapter::new(
            config,
            Box::new(MacOsKeyMapper {
                map: std::collections::HashMap::new(),
            }),
            dispatcher,
        );
        adapter.build_keymap();

        let stroke = KeyStroke::parse("Ctrl+V").unwrap();
        assert!(adapter.keymap.lookup(&stroke).is_some());
    }

    #[test]
    fn test_keymap_missing() {
        let config = test_config();
        let dispatcher: Arc<ActionDispatcher> = Arc::new(|_, _| {});
        let mut adapter = MacOsAdapter::new(
            config,
            Box::new(MacOsKeyMapper {
                map: std::collections::HashMap::new(),
            }),
            dispatcher,
        );
        adapter.build_keymap();

        let stroke = KeyStroke::parse("Ctrl+X").unwrap();
        assert!(adapter.keymap.lookup(&stroke).is_none());
    }

    #[test]
    fn test_platform() {
        let config = test_config();
        let dispatcher: Arc<ActionDispatcher> = Arc::new(|_, _| {});
        let adapter = MacOsAdapter::new(
            config,
            Box::new(MacOsKeyMapper {
                map: std::collections::HashMap::new(),
            }),
            dispatcher,
        );
        assert_eq!(adapter.get_platform(), OsPlatform::Macos);
    }
}
