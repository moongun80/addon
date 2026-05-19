# addon-macos

macOS adapter for the addon. Provides platform-specific key binding hooks
using the `OsAdapter` trait defined in `addon-core`.

## Platform Details

On macOS, key bindings are installed using the Carbon Event Manager
(`CGEventTap` / `NSEvent.addLocalEventHandler`) to intercept global
keyboard events.

## Usage

```rust
use addon_macos::{MacOsAdapter, OsAdapter};

let mut adapter = MacOsAdapter::new();
adapter.init()?;
adapter.start()?;
```
