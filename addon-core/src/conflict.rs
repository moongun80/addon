//! Key binding conflict detection.
//!
/// Provides [`detect_conflicts`] which scans a list of key bindings and
/// returns any overlapping key strokes per platform.
use crate::config::KeyBinding;
use crate::os::OsPlatform;

/// A detected conflict between two key bindings.
///
/// If `platform` is `None`, the conflict applies to all platforms.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// ID of the first conflicting binding.
    pub binding1: String,
    /// ID of the second conflicting binding.
    pub binding2: String,
    /// The platform affected, or `None` for all platforms.
    pub platform: Option<OsPlatform>,
}

/// Detect conflicts among the given key bindings.
///
/// A conflict occurs when two or more bindings use the same key stroke
/// on the same platform.
///
/// # Algorithm
///
/// 1. For each binding, expand its key strokes (applying platform overrides
///    if a platform is specified).
/// 2. Track all key strokes in a map from `(platform, keys)` → binding IDs.
/// 3. Where the map value has more than one binding ID, emit a conflict.
pub fn detect_conflicts(keybindings: &[KeyBinding]) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    // Build a map: (platform, key_vector) -> list of binding IDs
    let mut lookup: std::collections::HashMap<(OsPlatform, String), Vec<String>> =
        std::collections::HashMap::new();

    for binding in keybindings {
        // Add bindings for all platforms (default keys)
        for key in &binding.keys {
            let entry = lookup
                .entry((OsPlatform::platform_all(), key.clone()))
                .or_default();
            entry.push(binding.id.clone());
        }

        // Add per-platform overrides
        if let Some(ref overrides) = binding.overrides {
            if let Some(ref macos_keys) = overrides.macos {
                for key in macos_keys {
                    let entry = lookup
                        .entry((OsPlatform::Macos, key.clone()))
                        .or_default();
                    entry.push(binding.id.clone());
                }
            }
            if let Some(ref windows_keys) = overrides.windows {
                for key in windows_keys {
                    let entry = lookup
                        .entry((OsPlatform::Windows, key.clone()))
                        .or_default();
                    entry.push(binding.id.clone());
                }
            }
        }
    }

    // Collect conflicts where multiple bindings share the same key
    for ((platform, _keys), ids) in &lookup {
        if ids.len() > 1 {
            // Generate pairwise conflicts
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    conflicts.push(Conflict {
                        binding1: ids[i].clone(),
                        binding2: ids[j].clone(),
                        platform: Some(*platform),
                    });
                }
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
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].binding1, "paste");
        assert_eq!(conflicts[0].binding2, "paste2");
        assert_eq!(conflicts[0].platform, Some(OsPlatform::Linux));
    }
}
