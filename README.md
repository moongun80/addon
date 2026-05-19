# addon

Cross-platform global key binding automation toolkit.

## Architecture

```
addon/
├── Cargo.toml              # Workspace root
├── addon-core/             # Shared types, config, logic
├── addon-macos/            # macOS adapter
├── addon-windows/          # Windows adapter
├── addon-linux/            # Linux adapter
├── addon-daemon/           # Background daemon binary
└── addon-gui/              # GUI (planned, not yet in workspace)
```

## Crates

| Crate | Description |
|-------|-------------|
| `addon-core` | Core library — config model, key strokes, actions, conflict detection |
| `addon-macos` | macOS adapter (Carbon Event Manager / CGEventTap) |
| `addon-windows` | Windows adapter (SetWindowsHookEx low-level keyboard hook) |
| `addon-linux` | Linux adapter (X11 XGrabKey / Wayland fallback) |
| `addon-daemon` | Daemon binary — runs in background, applies key bindings |
| `addon-gui` | GUI configuration tool (framework TBD) |

## Quick Start

```bash
# Build the workspace
cargo build --workspace

# Run the daemon
cargo run -p addon-daemon
```

## Configuration

Key bindings are defined in a YAML config file. See `docs/` for schema details.

## License

MIT OR Apache-2.0
