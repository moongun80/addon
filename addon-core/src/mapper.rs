//! Key mapping engine trait.
//!
/// Defines the [`KeyMapper`] trait that OS-specific adapters implement
/// to provide key-stroke lookups and binding checks.

use crate::actions::Action;
use crate::keymap::KeyStroke;

/// Trait for key binding lookup engines.
///
/// Implementations forward key strokes to actions by consulting
/// a binding table built from the configuration file.
pub trait KeyMapper {
    /// Look up the action for a given key stroke.
    ///
    /// Returns `Some(&Action)` if the stroke is bound, or `None` otherwise.
    fn lookup(&self, stroke: &KeyStroke) -> Option<&Action>;

    /// Check if a key stroke is bound.
    fn is_bound(&self, stroke: &KeyStroke) -> bool {
        self.lookup(stroke).is_some()
    }
}
