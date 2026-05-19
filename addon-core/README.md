# addon-core

Core library for the addon project. Provides shared types, configuration parsing, key mapping logic, and platform-agnostic utilities.

## Modules

- `config` — YAML configuration data model
- `keymap` — `KeyStroke` and modifier definitions
- `mapper` — Key mapping engine trait
- `actions` — Action type definitions
- `conflict` — Key binding conflict detection
- `error` — Error types
- `log` — Common logging initialization

## Usage

```toml
[dependencies]
addon-core = { path = "addon-core" }
```
