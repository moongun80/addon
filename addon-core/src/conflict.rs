//! Key binding conflict detection.
//!
/// Provides [`detect_conflicts`] which scans a list of key bindings and
/// returns any overlapping key strokes per platform.
use crate::config::KeyBinding;
use crate::keymap::KeyStroke;
use crate::os::OsPlatform;

/// A detected conflict between two or more key bindings.
///
/// Multiple bindings share the same key stroke on the given platform.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The canonical key string involved in the conflict.
    pub key: String,
    /// IDs of all bindings that share this key (at least 2).
    pub bindings: Vec<String>,
}

/// Detect conflicts among the given key bindings.
///
/// A conflict occurs when two or more bindings use the same key stroke
/// on the same platform.
///
/// # Algorithm
///
/// 1. For each platform, iterate all bindings and resolve their effective
///    keys (including platform overrides) via `KeyBinding::effective_keys()`.
/// 2. Parse each key string into a KeyStroke to normalize modifier ordering
///    (e.g. "Ctrl+Shift+V" == "Shift+Ctrl+V"), then use the canonical
///    display string as the lookup key.
/// 3. Track all key strokes in a map from `(platform, canonical_key)` → binding IDs.
/// 4. Where the map value has more than one binding ID, emit a conflict.
pub fn detect_conflicts(keybindings: &[KeyBinding]) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let mut lookup: std::collections::HashMap<(OsPlatform, String), Vec<String>> =
        std::collections::HashMap::new();

    for platform in OsPlatform::all() {
        for binding in keybindings {
            for key in binding.effective_keys(platform) {
                let canonical = match KeyStroke::parse(key) {
                    Ok(stroke) => stroke.display(),
                    Err(e) => {
                        tracing::warn!(
                            "skipping unparseable key {:?} for binding {} on {:?}: {}",
                            key,
                            binding.id,
                            platform,
                            e
                        );
                        continue;
                    }
                };
                let entry = lookup.entry((platform, canonical)).or_default();
                entry.push(binding.id.clone());
            }
        }
    }

    // Generate pairwise conflicts from lookup
    for ((_platform, key), binding_ids) in lookup {
        if binding_ids.len() < 2 {
            continue;
        }
        for i in 0..binding_ids.len() {
            for j in (i + 1)..binding_ids.len() {
                conflicts.push(Conflict {
                    key: key.clone(),
                    bindings: vec![binding_ids[i].clone(), binding_ids[j].clone()],
                });
            }
        }
    }

    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::config::{Config, GlobalSettings, KeyBinding};

    fn sample_binding(id: &str, keys: &[&str]) -> KeyBinding {
        KeyBinding {
            id: id.to_string(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
            action: Action::Paste {
                text: "test".to_string(),
            },
            overrides: None,
        }
    }

    #[test]
    fn test_detect_no_conflicts() {
        let config = Config {
            version: "1.0".to_string(),
            global: GlobalSettings::default(),
            keybindings: vec![
                sample_binding("paste", &["Ctrl+V"]),
                sample_binding("launch_vs", &["Ctrl+Shift+V"]),
            ],
        };
        let conflicts = detect_conflicts(&config.keybindings);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_detect_conflict_same_keys() {
        let keybindings = vec![
            sample_binding("paste", &["Ctrl+V"]),
            sample_binding("paste2", &["Ctrl+V"]),
        ];
        let conflicts = detect_conflicts(&keybindings);
        // Should detect conflicts on all 3 platforms
        assert_eq!(conflicts.len(), 3);
    }

    #[test]
    fn test_detect_conflict_different_modifier_order() {
        // "Ctrl+Shift+V" and "Shift+Ctrl+V" should be treated as the same key
        let keybindings = vec![
            sample_binding("bind1", &["Ctrl+Shift+V"]),
            sample_binding("bind2", &["Shift+Ctrl+V"]),
        ];
        let conflicts = detect_conflicts(&keybindings);
        // Should detect conflicts on all 3 platforms
        assert_eq!(conflicts.len(), 3);
    }
}
