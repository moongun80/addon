# addon-daemon

Daemon binary for the addon. Runs as a background service that loads
configuration, selects the appropriate OS adapter, and installs key bindings.

## Usage

```bash
cargo run -p addon-daemon
```

Configuration is loaded from the user's config directory
(`dirs::config_dir()`) under `addon/addon.yaml`.
