# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
