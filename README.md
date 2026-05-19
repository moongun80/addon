# 🔑 addon

> **단 한 번 설정, 모든 OS에서 동일하게 동작**하는 키보드/마우스 단축키 동기화 도구.

## 특징

- **크로스플랫폼**: macOS, Windows, Linux(X11) 지원
- **단일 설정**: JSON/YAML 기반 설정 파일로 모든 OS에서 공유
- **시스템 데몬**: 백그라운드에서 전역 이벤트 모니터링
- **Tauri GUI**: 가볍고 아름다운 설정 UI
- **시스템 트레이**: 우측 하단에서 빠르게 접근

## 지원 OS

| OS | 상태 |
|----|------|
| macOS | ✅ 지원 |
| Windows | ✅ 지원 |
| Linux (X11) | ✅ 지원 |
| Linux (Wayland) | 🔜 Phase 2 |

## 아키텍처

```
┌──────────┐   ┌──────────┐   ┌──────────────┐
│  GUI     │──▶│ Daemon   │──▶│ OS Adapters  │
│ (Tauri)  │   │ (bg)     │   │ (macos/win/  │
│          │◀──│          │◀──│  linux)      │
│ Settings │   │ IPC      │   │ Native APIs  │
└──────────┘   └──────────┘   └──────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| `addon-core` | Core library — config model, key strokes, actions, conflict detection |
| `addon-macos` | macOS adapter (Carbon HotKey / CGEvent) |
| `addon-windows` | Windows adapter (WH_KEYBOARD_LL hook) |
| `addon-linux` | Linux adapter (X11 XInput2 + XTest) |
| `addon-daemon` | Background daemon with IPC support |
| `addon-gui` | Tauri-based GUI configuration tool |

## 설치

### macOS

1. [Latest Release](https://github.com/moongun80/addon/releases)에서 `.dmg` 다운로드
2. 실행 후 접근성 권한 허용
3. 시스템 트레이에서 실행 확인

### Windows

1. [Latest Release](https://github.com/moongun80/addon/releases)에서 `.msi` 다운로드
2. 설치 마법사 따라 설치
3. 시작 메뉴에서 "addon" 실행

### Linux

```bash
# deb 기반 (Ubuntu/Debian)
wget https://github.com/moongun80/addon/releases/latest/download/addon_x86_64.deb
sudo dpkg -i addon_x86_64.deb

# AppImage
wget https://github.com/moongun80/addon/releases/latest/download/addon-x86_64.AppImage
chmod +x addon-x86_64.AppImage
./addon-x86_64.AppImage
```

## 설정 파일

`~/.config/addon/config.yaml`:

```yaml
version: "1.0"
global:
  modifier_map:
    command: alt_ctrl

keybindings:
  - id: "copy-vim"
    keys: ["Ctrl", "Shift", "V"]
    action:
      type: paste
      text: "Hello!"
    overrides:
      macos: ["Cmd", "Shift", "V"]

  - id: "caps-to-esc"
    keys: ["CapsLock"]
    action:
      type: remap
      to: Escape
```

## 개발

```bash
# 빌드
cargo build --workspace

# 테스트
cargo test --workspace

# 포맷 체크
cargo fmt --all --check

# 데몬 실행
cargo run -p addon-daemon
```

## 라이센스

MIT OR Apache-2.0
