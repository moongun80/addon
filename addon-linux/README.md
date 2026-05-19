# addon-linux

Linux adapter for the addon. Provides platform-specific key binding hooks
using the `OsAdapter` trait defined in `addon-core`.

## Platform Details

On Linux, key bindings can be installed via:
- **X11**: `XGrabKey` for X server-level grabs
- **Wayland**: Depends on the compositor; may require an XWayland fallback
  or a portal-based approach

## Usage

```rust
use addon_linux::{LinuxAdapter, OsAdapter};

let mut adapter = LinuxAdapter::new();
adapter.init()?;
adapter.start()?;
```
