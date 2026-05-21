# addon 사용법 가이드

> **단 한 번 설정, 모든 OS에서 동일하게 동작**하는 키보드 단축키 동기화 도구.

---

## 목차

1. [빠른 시작](#1-빠른-시작)
2. [시스템 요구사항](#2-시스템-요구사항)
3. [설치](#3-설치)
4. [설정 파일](#4-설정-파일)
5. [데몬 실행](#5-데몬-실행)
6. [GUI 사용](#6-gui-사용)
7. [IPC 프로토콜](#7-ipc-프로토콜)
8. [플랫폼별 오버라이드](#8-플랫폼별-오버라이드)
9. [로깅 및 디버깅](#9-로깅-및-디버깅)
10. [개발](#10-개발)
11. [문제 해결](#11-문제-해결)

---

## 1. 빠른 시작

```bash
# 1. 설정 파일 생성
mkdir -p ~/.config/addon
cat > ~/.config/addon/config.yaml << 'EOF'
version: "1.0"
global:
  modifier_map:
    command: alt_ctrl

keybindings:
  - id: "paste-hello"
    keys: ["Ctrl+Shift+V"]
    action:
      type: paste
      text: "Hello!"
EOF

# 2. 데몬 실행 (Linux)
cargo run -p addon-daemon --features linux

# 3. 다른 터미널에서 GUI 실행
cargo run -p addon-gui
```

---

## 2. 시스템 요구사항

| OS | 요구사항 |
|----|---------|
| **macOS** | 10.15+, 접근성 권한 필요 |
| **Windows** | 10+, 관리자 권한 권장 |
| **Linux** | X11 디스플레이 서버 필요 (Wayland 미지원) |
| **공통** | Rust 1.70+ (빌드 시), 256MB RAM |

### Linux 추가 의존성

```bash
# Debian/Ubuntu
sudo apt install libx11-dev libxi-dev

# Fedora
sudo dnf install libX11-devel libXi-devel

# Arch
sudo pacman -S libx11 libxi
```

---

## 3. 설치

### 3.1 릴리스 바이너리

#### macOS
1. [Latest Release](https://github.com/moongun80/addon/releases)에서 `.dmg` 다운로드
2. 실행 후 **시스템 설정 → 개인정보보호 및 보안 → 접근성**에서 권한 허용
3. 시스템 트레이에서 실행 확인

#### Windows
1. [Latest Release](https://github.com/moongun80/addon/releases)에서 `.msi` 다운로드
2. 설치 마법사 따라 설치
3. 시작 메뉴에서 "addon" 실행

#### Linux
```bash
# deb 기반 (Ubuntu/Debian)
wget https://github.com/moongun80/addon/releases/latest/download/addon_x86_64.deb
sudo dpkg -i addon_x86_64.deb

# AppImage
wget https://github.com/moongun80/addon/releases/latest/download/addon-x86_64.AppImage
chmod +x addon-x86_64.AppImage
./addon-x86_64.AppImage
```

### 3.2 소스에서 빌드

```bash
# Rust 설치 (아직 없다면)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 저장소 클론
git clone https://github.com/moongun80/addon.git
cd addon

# Linux용 빌드
cargo build --release --features linux

# 실행 파일 위치
# ./target/release/addon-daemon
# ./target/release/addon-gui
```

---

## 4. 설정 파일

### 4.1 위치

설정 파일은 다음 위치에서 순차적으로 검색됩니다 (먼저 찾은 것 사용):

1. `$ADDON_CONFIG` 환경 변수
2. `~/.config/addon/config.yaml` (XDG 표준)
3. `~/.addon/config.yaml` (홈 디렉토리)
4. `./config.yaml` (현재 디렉토리)

### 4.2 기본 구조

```yaml
version: "1.0"                          # 설정 파일 버전
global:                                 # 전역 설정
  modifier_map:
    command: alt_ctrl                   # Command 키 폴백 (Linux/Windows)

keybindings:                            # 키 바인딩 목록
  - id: "unique-id"                    # 고유 식별자 (필수)
    keys: ["Ctrl+Shift+V"]             # 트리거 키 (필수)
    action:                            # 실행 액션 (필수)
      type: paste
      text: "Hello!"
    overrides:                         # 플랫폼별 오버라이드 (선택)
      macos: ["Cmd+Shift+V"]
      windows: ["Ctrl+Alt+V"]
      linux: ["Ctrl+Shift+V"]
```

### 4.3 키 스트로크 형식

```
[모디파이어+]+키
```

**지원 모디파이어:**
| 이름 | 설명 |
|------|------|
| `Ctrl` / `Control` | Control 키 |
| `Shift` | Shift 키 |
| `Alt` | Alt 키 (Windows/Linux) |
| `Option` | Option 키 (macOS, Alt와 동일 처리) |
| `Cmd` / `Command` / `Super` | Command/Super/Win 키 |
| `CapsLock` / `Caps` | Caps Lock 키 |

**키 조합 예시:**
```
Ctrl+V              # Control + V
Ctrl+Shift+V        # Control + Shift + V
Cmd+Option+Esc      # Command + Option + Escape
CapsLock            # Caps Lock 단일 키
```

> **참고:** 모디파이어 순서는 중요하지 않습니다. `Ctrl+Shift+V`와 `Shift+Ctrl+V`는 동일하게 처리됩니다.

### 4.4 액션 유형

#### Paste — 텍스트 붙여넣기
```yaml
action:
  type: paste
  text: "붙여넣을 텍스트"
```

#### Launch — 애플리케이션 실행
```yaml
action:
  type: launch
  path: "/usr/bin/code"    # 실행 파일 경로
```

#### Remap — 키 재매핑
```yaml
action:
  type: remap
  to: "Escape"            # 재매핑 대상 키
```

#### Shortcut — 키 조합 시뮬레이션
```yaml
action:
  type: shortcut
  shortcut:
    - "Alt+F4"
    - "Ctrl+S"
```

#### SystemCommand — 셸 명령어 실행
```yaml
action:
  type: system_command
  command: "code ."       # 셸 메타문자(; | & $ 등)는 보안상 차단됨
```

> **⚠️ 보안 주의:** `system_command`은 셸 메타문자(`;`, `|`, `&`, `$`, `` ` ``, `(`, `)`, `{`, `}`, `<`, `>`, `\`, `"`, `'`)를 포함할 수 없습니다. 명령어 인젝션을 방지하기 때문입니다.

#### TextInsert — 커서 위치에 텍스트 삽입
```yaml
action:
  type: text_insert
  text: "삽입할 텍스트"
```

### 4.5 전역 설정

```yaml
global:
  modifier_map:
    # Command 키의 플랫폼별 폴백
    # - alt_ctrl: Alt+Ctrl로 대체 (기본값)
    # - alt: Alt만으로 대체
    command: alt_ctrl
```

### 4.6 설정 유효성 검사

다음 항목이 자동으로 검사됩니다:
- 중복된 바인딩 ID
- 빈 키 목록
- 키 문자열 내 빈 문자열
- 플랫폼 오버라이드 내 빈 키 문자열

---

## 5. 데몬 실행

### 5.1 기본 실행

```bash
# Linux
cargo run -p addon-daemon --features linux

# macOS
cargo run -p addon-daemon --features macos

# Windows
cargo run -p addon-daemon --features windows
```

### 5.2 백그라운드 실행

```bash
# nohup으로 백그라운드 실행
nohup cargo run -p addon-daemon --features linux > /tmp/addon-daemon.log 2>&1 &

# systemd 서비스로 등록 (프로덕션 권장)
sudo cp addon-daemon.service /etc/systemd/system/
sudo systemctl enable --now addon-daemon
```

### 5.3 종료

```bash
# Ctrl+C 또는 SIGTERM 전송
kill $(pgrep addon-daemon)

# systemd 사용 시
sudo systemctl stop addon-daemon
```

### 5.4 설정 파일 지정

```bash
# 환경 변수로 설정 파일 경로 지정
ADDON_CONFIG=/path/to/custom/config.yaml cargo run -p addon-daemon --features linux
```

---

## 6. GUI 사용

### 6.1 실행

```bash
cargo run -p addon-gui
```

### 6.2 기능

| 기능 | 설명 |
|------|------|
| **Daemon Status** | 데몬 실행 상태 확인 |
| **Reload Config** | 설정 파일 다시 로드 |
| **Test Shortcut** | 단축키 구문 테스트 |
| **List Keybindings** | 현재 바인딩 목록 조회 |
| **Add Keybinding** | 새 키 바인딩 추가 |
| **Remove Keybinding** | 키 바인딩 삭제 |
| **Export Config** | 설정 파일 내보내기 (YAML/JSON) |

### 6.3 시스템 트레이

GUI 실행 시 시스템 트레이에 아이콘이 표시됩니다:
- **Show**: 창 표시/초점
- **Quit**: 프로그램 종료

---

## 7. IPC 프로토콜

데몬과 GUI는 **Unix 도메인 소켓**을 통해 JSON 메시지로 통신합니다.

### 7.1 소켓 경로

```
/tmp/addon/daemon.sock
```

소켓 파일 권한은 `0o600`(소유자 전용)으로 제한됩니다.

### 7.2 메시지 형식

뉴라인으로 구분된 JSON (Newline-Delimited JSON):

```json
{"type":"get_status","request_id":"abc-123"}\n
{"type":"daemon_status","running":true,"pid":12345,"version":"0.1.0","request_id":"abc-123"}\n
```

### 7.3 지원 명령어

| 명령어 | 방향 | 설명 |
|--------|------|------|
| `get_status` | GUI → Daemon | 데몬 상태 조회 |
| `set_config` | GUI → Daemon | 설정 업데이트 |
| `reload_config` | GUI → Daemon | 설정 파일 재로드 |
| `test_shortcut` | GUI → Daemon | 단축키 테스트 |
| `start_daemon` | GUI → Daemon | 데몬 시작 |
| `stop_daemon` | GUI → Daemon | 데몬 중지 |

### 7.4 수동 테스트

```bash
# 데몬 상태 확인
echo '{"type":"get_status"}' | socat - UNIX-CONNECT:/tmp/addon/daemon.sock

# 설정 재로드
echo '{"type":"reload_config"}' | socat - UNIX-CONNECT:/tmp/addon/daemon.sock
```

---

## 8. 플랫폼별 오버라이드

같은 바인딩을 각 OS에 맞게 다르게 설정할 수 있습니다:

```yaml
keybindings:
  - id: "copy"
    keys: ["Ctrl+C"]                    # 기본 (Linux)
    action:
      type: paste
      text: "Copied!"
    overrides:
      macos: ["Cmd+C"]                  # macOS에서만 Cmd+C
      windows: ["Ctrl+C"]               # Windows에서는 Ctrl+C
      linux: ["Ctrl+Shift+C"]           # Linux에서는 Ctrl+Shift+C
```

> **참고:** 오버라이드가 지정되지 않은 플랫폼은 기본 `keys`를 사용합니다.

---

## 9. 로깅 및 디버깅

### 9.1 로그 레벨 조절

```bash
# 환경 변수로 로그 레벨 설정
RUST_LOG=info cargo run -p addon-daemon --features linux

# 지원 레벨: trace, debug, info, warn, error
RUST_LOG=debug cargo run -p addon-daemon --features linux
RUST_LOG=error cargo run -p addon-daemon --features linux
```

### 9.2 특정 모듈만 디버깅

```bash
# addon-core 모듈만 debug 레벨
RUST_LOG=addon_core=debug,info cargo run -p addon-daemon --features linux

# IPC 관련만 trace
RUST_LOG=addon_daemon::ipc=trace,info cargo run -p addon-daemon --features linux
```

---

## 10. 개발

### 10.1 프로젝트 구조

```
addon/
├── addon-core/src/        # 핵심 라이브러리
│   ├── lib.rs             # 모듈 선언
│   ├── actions.rs         # 액션 유형 정의
│   ├── config.rs          # 설정 데이터 모델
│   ├── conflict.rs        # 충돌 감지
│   ├── error.rs           # 에러 유형
│   ├── ipc.rs             # IPC 메시지 유형
│   ├── keymap.rs          # 키 스트로크 정의
│   ├── log.rs             # 로깅 초기화
│   ├── mapper.rs          # 키 매핑 엔진 트레이트
│   └── os.rs              # OS 어댑터 트레이트
├── addon-daemon/src/      # 백그라운드 데몬
│   ├── main.rs            # 진입점
│   ├── daemon.rs          # 데몬 상태 관리
│   ├── ipc.rs             # IPC 서버
│   └── log.rs             # 데몬 로깅
├── addon-gui/             # Tauri GUI 앱
│   ├── src/               # Rust GUI 소스
│   │   ├── main.rs        # 진입점
│   │   ├── commands.rs    # Tauri 명령어 핸들러
│   │   ├── config_ops.rs  # 설정 파일 조작
│   │   └── tray.rs        # 시스템 트레이
│   └── src-tauri/         # Tauri 설정
├── addon-linux/src/       # Linux 플랫폼 어댑터
│   └── lib.rs             # X11 XInput2 + XTest
├── addon-macos/src/       # macOS 플랫폼 어댑터
├── addon-windows/src/     # Windows 플랫폼 어댑터
├── Cargo.toml             # 워크스페이스 루트
└── Cargo.lock
```

### 10.2 빌드 및 테스트

```bash
# 전체 빌드 (Linux)
cargo build --workspace --features linux

# 타입 체크만 (빠름)
cargo check --features linux

# 테스트 실행
cargo test --features linux

# 코드 포맷
cargo fmt --all

# 린팅
cargo clippy --features linux

# 릴리스 빌드
cargo build --release --features linux
```

### 10.3 아키텍처

```
┌──────────────┐     IPC (Unix Socket)     ┌──────────────┐
│   GUI        │ ◄─────────────────────────► │   Daemon     │
│  (Tauri)     │    JSON newline-delimited   │  (tokio)     │
│              │                             │              │
│ • Settings   │                             │ • Config     │
│ • Status     │                             │ • IPC Server │
│ • Tray       │                             │ • State Mgmt │
└──────────────┘                             └──────┬───────┘
                                                    │
                                          ┌─────────▼─────────┐
                                          │   OS Adapter      │
                                          │                   │
                                          │  macOS: Carbon    │
                                          │  Windows: Hook    │
                                          │  Linux: X11/XI2   │
                                          └───────────────────┘
```

### 10.4 OsAdapter 트레이트 구현

새 플랫폼을 추가하려면 `OsAdapter` 트레이트를 구현하세요:

```rust
use addon_core::OsAdapter;
use addon_core::error::Error;

pub struct MyPlatformAdapter {
    // ...
}

impl OsAdapter for MyPlatformAdapter {
    fn init(&mut self) -> Result<(), Error> {
        // 초기화 (권한 요청, 이벤트 탭 생성 등)
        todo!()
    }

    fn start(&mut self) -> Result<(), Error> {
        // 키 이벤트 리스닝 시작
        todo!()
    }

    fn stop(&mut self) -> Result<(), Error> {
        // 키 이벤트 리스닝 중지
        todo!()
    }

    fn set_config(&mut self, config: &addon_core::config::Config) {
        // 설정 업데이트 (기본값: noop)
    }

    fn get_platform(&self) -> addon_core::OsPlatform {
        // 플랫폼 반환
        todo!()
    }
}
```

---

## 11. 문제 해결

### 11.1 데몬이 시작되지 않음

**증상:** `Config not found` 에러

**해결:**
```bash
# 설정 파일이 있는지 확인
ls -la ~/.config/addon/config.yaml

# 없으면 기본 설정 파일 생성
mkdir -p ~/.config/addon
cat > ~/.config/addon/config.yaml << 'EOF'
version: "1.0"
global:
  modifier_map:
    command: alt_ctrl
keybindings: []
EOF
```

### 11.2 Linux에서 X11 연결 실패

**증상:** `Failed to open X11 display. Is DISPLAY set?`

**해결:**
```bash
# DISPLAY 환경 변수 확인
echo $DISPLAY

# 설정되지 않았다면 설정
export DISPLAY=:0

# Wayland에서 X11 호환 모드 활성화
# ~/.profile 또는 ~/.bashrc에 추가
export GDK_BACKEND=x11
export QT_QPA_PLATFORM=xcb
```

### 11.3 키보드 그랩 실패

**증상:** `Failed to grab keyboard. Is another app holding it?`

**해결:**
1. 다른 키보드 훅 프로그램 종료 (AutoHotkey, Karabiner, etc.)
2. 데몬 재시작
3. 터미널에서 `xdotool key Escape`로 강제 해제 시도

### 11.4 IPC 연결 타임아웃

**증상:** `Daemon not responding (connection timeout)`

**해결:**
```bash
# 데몬 실행 확인
pgrep -a addon-daemon

# 소켓 파일 확인
ls -la /tmp/addon/daemon.sock

# 데몬 재시작
kill $(pgrep addon-daemon)
cargo run -p addon-daemon --features linux
```

### 11.5 설정 변경이 적용되지 않음

**해결:**
1. GUI에서 **Reload Config** 클릭
2. 또는 IPC 명령어 전송:
   ```bash
   echo '{"type":"reload_config"}' | socat - UNIX-CONNECT:/tmp/addon/daemon.sock
   ```
3. 또는 데몬 재시작

### 11.6 충돌 감지 경고

**증상:** `key binding conflict(s) detected` 로그

**해결:**
```bash
# debug 로그로 충돌 세부사항 확인
RUST_LOG=debug cargo run -p addon-daemon --features linux

# 충돌하는 키 바인딩 수정 또는 제거
nano ~/.config/addon/config.yaml
```

---

## 부록

### A. 환경 변수

| 변수 | 설명 | 기본값 |
|------|------|--------|
| `ADDON_CONFIG` | 설정 파일 경로 | `~/.config/addon/config.yaml` |
| `RUST_LOG` | 로그 레벨 | `info` |
| `DISPLAY` | X11 디스플레이 (Linux) | `:0` |

### B. 파일 위치

| 파일 | 위치 |
|------|------|
| 설정 파일 | `~/.config/addon/config.yaml` |
| IPC 소켓 | `/tmp/addon/daemon.sock` |
| 로그 | stderr (또는 syslog) |

### C. 키보드 이벤트 흐름

```
1. 사용자 키 입력
       ↓
2. OS Adapter (X11/XI2) 이벤트 캡처
       ↓
3. KeyStroke::parse() → 정규화
       ↓
4. KeyMapper::lookup() → 액션 조회
       ↓
5. 액션 실행 (Paste/Launch/Remap/etc.)
       ↓
6. XTestFakeKeyEvent() → 시뮬레이션
```
