# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security
- Set restrictive Content Security Policy in Tauri config (was null, XSS risk)
- Added shell injection validation for `system_command` in GUI add_keybinding
- Added shell injection validation for `system_command` in daemon SetConfig IPC handler

### Fixed
- Propagate `request_id` in all IPC responses (was hardcoded to `None`)
- Mutex → RwLock conversion for concurrent IPC client handling
- Early lock release for GetStatus (read lock) and TestShortcut (no lock)
- Consistent key code normalization (always uppercase, was mixed case)
- Correct error type for logging initialization (`Error::Parse` → `Error::LogInit`)
- Pin Vue.js CDN version (3 → 3.5.13) for supply chain reproducibility

### Added
- `linux` field to `PlatformOverrides` struct for Linux-specific key overrides

### Changed
- Updated `actions/cache@v3` → `@v4` in CI/CD pipeline

### Added
- Cargo workspace structure (`addon-core`, `addon-macos`, `addon-windows`, `addon-linux`, `addon-daemon`, `addon-gui`)
- YAML configuration file parsing and validation
- Key stroke definition and parsing (`KeyStroke`, `Modifier`)
- Action type definitions (`Paste`, `Launch`, `Remap`, `Shortcut`, `SystemCommand`, `TextInsert`)
- Key binding conflict detection (`detect_conflicts`)
- macOS adapter (Carbon HotKey FFI)
- Windows adapter (`WH_KEYBOARD_LL` low-level keyboard hook)
- Linux adapter (X11 XInput2 + XTest)
- Background daemon (tokio-based, IPC server via flume)
- Tauri GUI (Vue 3-based)
- System tray integration
- CI/CD pipeline (GitHub Actions: CI build matrix + Release)
- Logging initialization (`tracing` + `EnvFilter`)
- `.gitignore` with platform-agnostic rules

### Known Issues
- Wayland not supported (Phase 2 target)
- macOS Carbon API is deprecated — `NSEvent` fallback needed
- Config file encryption not yet implemented
- Mouse shortcuts not yet implemented
