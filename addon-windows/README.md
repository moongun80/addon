# addon-windows

Windows adapter for the addon. Provides platform-specific key binding hooks
using the `OsAdapter` trait defined in `addon-core`.

## Platform Details

On Windows, key bindings are installed using `SetWindowsHookEx` with
`WH_KEYBOARD_LL` (low-level keyboard hook) to intercept global keyboard
events.

## Usage

```rust
use addon_windows::{WindowsAdapter, OsAdapter};

let mut adapter = WindowsAdapter::new();
adapter.init()?;
adapter.start()?;
```
