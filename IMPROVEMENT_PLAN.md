# 🏗️ addon — 개선 설계 계획 (Architecture Improvement Plan)

> **작성일**: 2026-05-26  
> **작성자**: Senior Rust Architect  
> **기준**: Analyst Report (치명적 → 중요 → 코드 냄새 순)  
> **워크스페이스**: `/home/mg/.openclaw/workspace-worker/addon`

---

## 📋 우선순위 매핑 요약

| Priority | 작업 ID | 설명 | 예상 기간 |
|----------|---------|------|-----------|
| **P0** (즉시) | IMP-001 | IPC 인증 시스템 | 2일 |
| **P0** (즉시) | IMP-004 | Tauri CSP 보안 강화 | 0.5일 |
| **P1** (중요) | IMP-002 | Linux process_events() 구현 | 3일 |
| **P1** (중요) | IMP-003 | macOS event_handler 안전성 | 1일 |
| **P1** (중요) | IMP-005 | IPC 프로토콜 버전 관리 | 1일 |
| **P1** (중요) | IMP-006 | Windows 타이밍/대소문자 버그 | 1일 |
| **P1** (중요) | IMP-007 | 동시 파일 쓰기 Race 조건 | 1일 |
| **P1** (중요) | IMP-008 | 테스트 커버리지 확장 | 3일 |
| **P2** (개선) | IMP-009 | 코드 중복 제거 | 1일 |

---

## 🔴 P0 — 치명적 (즉시 조치 필요)

---

### IMP-001: IPC 인증 시스템 설계 및 구현

**설명**: 현재 데몬 소켓에 연결된 모든 로컬 프로세스가 명령어 실행 가능. Unix domain socket 의 `0o600` 권한만으로는 충분하지 않음 (같은 UID 의 모든 프로세스가 접근 가능). 인증 토큰 기반의 challenge-response 프로토콜 도입.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-core/src/ipc.rs` | `AuthChallenge`, `AuthToken` 메시지 타입 추가, `IpcRequest`에 `Auth` variant 추가 |
| `addon-daemon/src/ipc.rs` | 인증 핸들러, 토큰 생성/검증 로직, socket 경로에 PID 포함 |
| `addon-gui/src/commands.rs` | 인증 플로우 클라이언트 측 구현 |
| `addon-daemon/src/main.rs` | 토큰 파일 생성/로드 로직 |

**구현 세부 사항**:

```rust
// addon-core/src/ipc.rs — 새 메시지 타입

/// 인증 챌린지 — 클라이언트가 연결 직후 이 챌린지에 서명해야 함
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthChallenge {
    pub nonce: String,       // 32-byte hex random nonce
    pub daemon_pid: u32,     // 데몬 PID (재플레이 방지)
    pub timestamp: u64,      // Unix timestamp (nanosecond precision)
}

/// 인증 토큰 — 클라이언트의 응답
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub client_pid: u32,
    pub nonce: String,       // 챌린지의 nonce echo
    pub signature: String,   // HMAC-SHA256(secret, nonce + daemon_pid)
}

/// IpcRequest에 Auth variant 추가
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    /// 인증 — 연결 직후 첫 번째 메시지로 전송
    Auth {
        token: AuthToken,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    // ... 기존 variants 유지
}

/// IpcResponse에 AuthResult 추가
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    AuthResult {
        accepted: bool,
        reason: Option<String>, // "expired", "invalid_signature", etc.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    // ... 기존 variants 유지
}
```

**데몬 측 (addon-daemon/src/ipc.rs)**:

```rust
// 토큰 저장 경로 — /tmp/addon/.daemon_token
pub fn get_token_path() -> PathBuf {
    let dir = get_socket_path().parent().unwrap();
    dir.join(".daemon_token")
}

/// 토큰 생성 (daemon startup 시 한 번)
fn generate_auth_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// HMAC-SHA256 서명 검증
fn verify_signature(secret: &str, nonce: &str, daemon_pid: u32) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(nonce.as_bytes());
    mac.update(daemon_pid.to_le_bytes());
    mac.verify_slice(hex::decode(token.signature).unwrap().as_slice()).is_ok()
}
```

**GUI 측 (addon-gui/src/commands.rs)**:

```rust
/// 인증 플러그인 — send_async의 첫 단계
async fn authenticate(socket: &mut UnixStream, secret: &str) -> Result<(), anyhow::Error> {
    // 1. 챌린지 요청
    let challenge_req = IpcMessage::request(IpcRequest::AuthRequest);
    write_message(socket, &challenge_req).await?;
    
    // 2. 챌린지 수신
    let challenge_raw = read_message(socket).await?;
    let challenge: AuthChallenge = match challenge_raw {
        IpcMessage::Response(IpcResponse::AuthChallenge { nonce, daemon_pid, timestamp, .. }) => {
            // 타임스탬프 체크 (±30초 이내)
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?.as_secs();
            if (now as i64 - timestamp as i64).abs() > 30 {
                return Err(anyhow::anyhow!("Auth challenge expired"));
            }
            challenge
        }
        _ => return Err(anyhow::anyhow!("Expected AuthChallenge response")),
    };
    
    // 3. 서명 생성 및 전송
    let signature = compute_hmac(secret, &challenge.nonce, challenge.daemon_pid);
    let token = AuthToken {
        client_pid: std::process::id(),
        nonce: challenge.nonce,
        signature,
    };
    let auth_msg = IpcMessage::request(IpcRequest::Auth { token });
    write_message(socket, &auth_msg).await?;
    
    // 4. 결과 확인
    let result_raw = read_message(socket).await?;
    match result_raw {
        IpcMessage::Response(IpcResponse::AuthResult { accepted, reason, .. }) => {
            if !accepted {
                return Err(anyhow::anyhow!("Auth rejected: {:?}", reason));
            }
            Ok(())
        }
        _ => Err(anyhow::anyhow!("Expected AuthResult response")),
    }
}
```

**예상 영향도**:
- **위험도**: 🟡 중간 — 기존 클라이언트/서버 양쪽 수정 필요. 하위 호환성 위해 `Auth` 미검증 연결은 `READ_ONLY` 모드로 제한
- **테스트 필요**: ✅ 필수 — 인증 성공/실패/만료/잘못된 서명 케이스全覆盖
- **하위 호환성**: 인증 실패 시 연결을 즉시 끊지 않고 `unauthenticated` 플래그로 제한된 명령어만 허용

**추천 우선순위**: **P0** — 외부에서 데몬을 제어할 수 있는 것은 치명적 보안 취약점

---

### IMP-004: Tauri CSP 보안 강화 ('unsafe-inline' 제거)

**설명**: `tauri.conf.json`의 `style-src 'self' 'unsafe-inline'`은 인라인 CSS 삽입을 허용하여 XSS 공격 벡터가 됨. Tauri v2의 CSP는 엄격하게 적용되어야 함.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-gui/src-tauri/tauri.conf.json` | CSP 정책 수정 |
| `addon-gui/src-tauri/capabilities/default.json` | Capability 권한 검토 |
| `addon-gui/index.html` (또는 frontend) | 인라인 스타일을 외부 CSS 파일로 분리 |

**구현 세부 사항**:

```json
// addon-gui/src-tauri/tauri.conf.json — 수정 후
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'sha256-<hash>'; img-src 'self' data:; connect-src 'self' http://localhost:*; worker-src 'self' blob:"
    }
  }
}
```

**구현 단계**:
1. **단계 1**: 모든 인라인 `<style>` 태그를 `assets/styles.css` 같은 외부 파일로 이동
2. **단계 2**: `style-src`에서 `'unsafe-inline'` 제거
3. **단계 3**: 필요한 경우 `style-src-elem`로 세분화
4. **단계 4**: `connect-src`에서 `ws://localhost:*` 제거 (WebSocket 미사용 시)

**주의사항**:
- Tauri v2의 CSP는 빌드 시 검증됨. `'unsafe-inline'` 제거 전 반드시 모든 스타일이 외부 파일에 있는지 확인
- 인라인 스크립트도 `'unsafe-inline'` 문제. `nonce` 또는 `hash` 기반 접근 권장

**예상 영향도**:
- **위험도**: 🟢 낮음 — frontend 리팩토링만 필요
- **테스트 필요**: ✅ GUI 렌더링 확인 (스타일 깨짐 방지)

**추천 우선순위**: **P0** — XSS 취약점은 직접적인 데이터 유출 가능성

---

## 🟡 P1 — 중요 (차순위)

---

### IMP-002: Linux process_events() 실제 구현

**설명**: `addon-linux/src/lib.rs`의 `process_events()`는 X11 이벤트를 단순히 드레인만 하고 실제 키 매핑/액션 디스패치를 수행하지 않음. 핵심 기능인 글로벌 키 캡처 → 매핑 → 액션 실행 파이프라인 완성 필요.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-linux/src/lib.rs` | `process_events()`에 키 매핑 로직 추가 |
| `addon-linux/src/lib.rs` | `KBDLLHOOKSTRUCT` 대체: `XIDeviceEvent` 구조체 정의 |
| `addon-linux/src/lib.rs` | 모디파이어 상태 추적 (`ModifierTracker`) |
| `addon-linux/src/lib.rs` | `simulate_key` → 액션 실행기 통합 |

**구현 세부 사항**:

```rust
// addon-linux/src/lib.rs — XInput2 Raw Event 구조체

/// XInput2 raw key event (XI_RawKeyPress / XI_RawKeyRelease)
#[repr(C)]
pub struct XIValuatorMask {
    mask_len: c_int,
    // ... valuator mask details
}

#[repr(C)]
pub struct XIEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: XDisplay,
    event_type: c_int,
    deviceid: c_int,
    source: c_int,
    // Raw event data follows...
}

/// 모디파이어 상태 추적기
#[derive(Debug, Default)]
struct ModifierTracker {
    control: bool,
    shift: bool,
    alt: bool,
    meta: bool,
}

impl ModifierTracker {
    fn update(&mut self, event: &XIDeviceEvent) {
        match event.keycode {
            37 | 105 => self.control = event.u.key.event_state & 0x4 != 0, // Ctrl_L/R
            50 | 62 => self.shift = event.u.key.event_state & 0x1 != 0,    // Shift_L/R
            64 | 108 => self.alt = event.u.key.event_state & 0x8 != 0,    // Alt_L/Meta_L
            133 | 134 => self.meta = event.u.key.event_state & 0x40 != 0,  // Super_L/R
            _ => {}
        }
    }
    
    fn build_keymap_key(&self) -> String {
        let mut parts = Vec::new();
        if self.control { parts.push("Ctrl"); }
        if self.shift { parts.push("Shift"); }
        if self.alt { parts.push("Alt"); }
        if self.meta { parts.push("Meta"); }
        parts
    }
}
```

**핵심 `process_events()` 재구현**:

```rust
pub fn process_events(&mut self) -> Result<(), Error> {
    let dpy = self.display.as_ref()
        .map(|h| h.as_ptr())
        .ok_or_else(|| Error::AdapterNotAvailable("X11 display not open".to_string()))?;

    let mut modifier_tracker = ModifierTracker::default();
    
    // root window에 XI2 이벤트 구독 (init 시 설정)
    let root_window = unsafe { XDefaultRootWindow(dpy) };

    while unsafe { XPending(dpy) } > 0 {
        let mut xev = unsafe { std::mem::zeroed::<XEvent>() };
        unsafe { XNextEvent(dpy, &mut xev as *mut _) };

        // XIRawKeyPress (238) 또는 XIRawKeyRelease (239) 확인
        if xev.type_ != XI_RAW_KEY_PRESS && xev.type_ != XI_RAW_KEY_RELEASE {
            continue;
        }

        // XGenericEventCookie로 파싱
        let cookie = unsafe { xev.cookie };
        let mut data_len: c_int = 0;
        let data_ptr = unsafe { XGetEventData(dpy, &mut cookie) };
        
        if data_ptr.is_null() {
            continue;
        }

        let raw_event = unsafe { &*(data_ptr as *const XIEvent) };
        
        if raw_event.type_ != XI_RawKeyPress && raw_event.type_ != XI_RawKeyRelease {
            unsafe { XFreeEventData(dpy, &mut cookie) };
            continue;
        }

        // 모디파이어 상태 업데이트
        modifier_tracker.update(raw_event);

        // keycode → keysym → KeyStroke
        let keycode = raw_event.u.key.detail as c_uint;
        let is_press = raw_event.type_ == XI_RawKeyPress;

        let keysym = unsafe { XkbKeycodeToKeysym(dpy, keycode, 0, 0) };
        let key_code = keysym_to_key_code(keysym);
        
        if key_code.is_empty() {
            unsafe { XFreeEventData(dpy, &mut cookie) };
            continue;
        }

        // KeyStroke 구성
        let modifiers = modifier_tracker.build_modifiers();
        let stroke = KeyStroke {
            modifiers,
            key: Key { code: key_code },
        };

        // keymap에서 액션 조회
        if is_press {
            if let Some(action) = self.keymap.lookup(&stroke) {
                self.dispatch_action(action)?;
            }
        }

        unsafe { XFreeEventData(dpy, &mut cookie) };
    }

    Ok(())
}

fn dispatch_action(&self, action: &Action) -> Result<(), Error> {
    match action {
        Action::Paste { text } | Action::TextInsert { text } => {
            self.simulate_text(text)?;
        }
        Action::Shortcut { shortcut } => {
            for key_str in shortcut {
                let stroke = KeyStroke::parse(key_str)?;
                self.simulate_stroke(&stroke)?;
            }
        }
        // ... 다른 액션 처리
    }
    Ok(())
}
```

**예상 영향도**:
- **위험도**: 🟡 중간 — X11/XI2 API는 복잡하고 여러 X server 버전과 호환성 테스트 필요
- **테스트 필요**: ✅ X11 없는 환경 격리 테스트, 다양한 키보드 레이아웃 테스트
- **블로킹 요인**: X11 development libraries (`libx11-dev`, `libxi-dev`, `libxtst-dev`) 필요

**추천 우선순위**: **P1** — Linux 플랫폼의 핵심 기능이 미구현

---

### IMP-003: macOS event_handler 안전성 개선

**설명**: `addon-macos/src/hotkey.rs`의 `event_handler`에서 `*event as c_uint`는 안전하지 않은 포인터 캐스팅. Carbon 이벤트 구조체의 실제 레이아웃을 정확히 해석해야 함.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-macos/src/hotkey.rs` | `event_handler`의 포인터 캐스팅 안전한 FFI로 교체 |
| `addon-macos/src/hotkey.rs` | Carbon 이벤트 구조체 정밀 정의 |

**구현 세부 사항**:

```rust
// addon-macos/src/hotkey.rs — 안전한 FFI 구조체 정의

/// Carbon EventRef의 안전한 래퍼
#[repr(C)]
pub struct EventRef {
    // EventRef는 opaque이지만, EventHotKeyID는 첫 필드에 있음
    // 구조체는 Carbon.framework에서 정의:
    // struct EventRef {
    //     UInt32 refCon;          // offset 0
    //     EventClass eventClass;  // offset 4
    //     EventID eventID;        // offset 8
    //     ...
    // }
}

/// EventHotKeyID — Carbon이 hotkey callback에 전달하는 식별자
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EventHotKeyID {
    pub signature: u32,  // creator code ("adda")
    pub id: u32,         // hotkey instance ID
}

/// 안전한 이벤트 핸들러
extern "C" fn event_handler(event: *mut c_void, _userData: *mut c_void) {
    if event.is_null() {
        return;
    }

    // ❌ 안전하지 않음: unsafe { *event as c_uint }
    // ✅ 안전함: EventHotKeyID 구조체로 해석
    let hotkey_id = unsafe {
        // EventRef의 첫 필드는 refCon (UInt32)이며, 
        // EventHotKeyID가 여기에 저장됨
        *(event as *const EventHotKeyID)
    };

    // signature 검증 (replay attack 방지)
    if hotkey_id.signature != CREATOR {
        tracing::warn!(
            "Invalid hotkey signature: expected {}, got {}",
            CREATOR,
            hotkey_id.signature
        );
        return;
    }

    let id = HotKeyId {
        creator: hotkey_id.signature,
        id: hotkey_id.id,
    };

    // ... 기존 콜백 디스패치 로직 유지
}
```

**추가 개선**:
```rust
// event_handlerUPP 타입을 더 안전하게 정의
type EventHandlerProc = extern "C" fn(
    event_class: u32,
    event_id: u32,
    user_data: *mut c_void,
) -> OSStatus;

// RegisterEventHotKey도 안전하게:
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn RegisterEventHotKey(
        hot_key_id: u32,
        modifiers: u32,
        handler: EventHandlerUPP,
        event_hot_key_ref: *mut EventHotKeyRef,
    ) -> i32;
}
```

**예상 영향도**:
- **위험도**: 🟢 낮음 — 구조체 재정의는 컴파일 타임 검증 받음
- **테스트 필요**: ✅ HotKeyId 라운드트립 테스트 (이미 존재), 이벤트 핸들러 호출 테스트

**추천 우선순위**: **P1** — 메모리 안전성은 플랫폼 안정성의 기초

---

### IMP-005: IPC 프로토콜 버전 관리

**설명**: `IpcMessage`에 `version` 필드 추가. 새로운 request/response variant 추가 시 구 버전 클라이언트/서버 간 상호 운용성 보장.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-core/src/ipc.rs` | `IpcMessage`에 `version` 필드 추가, 버전 검증 로직 |
| `addon-daemon/src/ipc.rs` | 서버 측 버전 체크 |
| `addon-gui/src/commands.rs` | 클라이언트 측 버전 첨부 |

**구현 세부 사항**:

```rust
// addon-core/src/ipc.rs

/// 현재 프로토콜 버전
pub const PROTOCOL_VERSION: u16 = 2;

/// 지원하는 최소/최대 버전 범위
pub const PROTOCOL_MIN_VERSION: u16 = 1;
pub const PROTOCOL_MAX_VERSION: u16 = 2;

/// IpcMessage에 version 필드 추가
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub version: u16,
    #[serde(flatten)]
    pub inner: IpcMessageInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IpcMessageInner {
    Request(IpcRequest),
    Response(IpcResponse),
}

impl IpcMessage {
    pub fn new_request(req: IpcRequest) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            inner: IpcMessageInner::Request(req),
        }
    }

    pub fn new_response(resp: IpcResponse) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            inner: IpcMessageInner::Response(resp),
        }
    }

    /// 버전 호환성 체크
    pub fn is_compatible(&self) -> bool {
        self.version >= PROTOCOL_MIN_VERSION && self.version <= PROTOCOL_MAX_VERSION
    }
}

/// 버전 불일치 응답
impl IpcResponse {
    pub fn protocol_error(incoming_version: u16, expected_range: (u16, u16)) -> Self {
        Self::Error {
            code: "PROTOCOL_VERSION_MISMATCH".to_string(),
            details: format!(
                "Incoming version {} not in supported range [{}, {}]",
                incoming_version, expected_range.0, expected_range.1
            ),
            request_id: None,
        }
    }
}
```

**서버 측 검증** (`addon-daemon/src/ipc.rs`):

```rust
async fn handle_client(stream: UnixStream, daemon_state: Arc<RwLock<DaemonState>>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = tokio::io::BufReader::new(reader);

    loop {
        let mut line = String::new();
        buf_reader.read_line(&mut line).await?;
        
        if line.is_empty() { break; }

        let msg: IpcMessage = serde_json::from_str(line.trim())?;

        // 버전 체크
        if !msg.is_compatible() {
            let err = IpcMessage::new_response(IpcResponse::protocol_error(
                msg.version,
                (PROTOCOL_MIN_VERSION, PROTOCOL_MAX_VERSION),
            ));
            write_response(&mut writer, &err).await?;
            continue;
        }

        // ... 기존 처리 로직
    }
}
```

**하위 호환성 전략**:
- **v1 → v2**: v1 클라이언트는 `version` 필드 없이 메시지 전송 → 서버에서 `version=1`으로 간주
- **v2 → v1**: v2 서버가 v1 클라이언트 메시지 수신 시 `version` 필드 누락 감지 → 기본값 적용
- **역방향 호환성**: 새 response variant는 `skip_serializing_if`로 선택적 필드로 추가

**예상 영향도**:
- **위험도**: 🟢 낮음 — serde `flatten` + `default`로 하위 호환성 확보
- **테스트 필요**: ✅ 버전 불일치, v1 호환 메시지 파싱

**추천 우선순위**: **P1** — 프로토콜 진화 없이는 장기적 유지보수 어려움

---

### IMP-006: Windows 타이밍/대소문자 버그 수정

**설명**: 두 가지 별도 버그: (1) `GetAsyncKeyState`가 이벤트 시점이 아닌 현재 상태를 읽음 → 모디파이어 트래킹 필요, (2) `vk_to_key_code`가 소문자 반환 → 파서가 대문자 기대.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-windows/src/lib.rs` | `vk_to_stroke` — 모디파이어 상태 추적기로 교체 |
| `addon-windows/src/lib.rs` | `vk_to_key_code` — 대문자 반환 |

**구현 세부 사항**:

```rust
// addon-windows/src/lib.rs

/// 이벤트 기반 모디파이어 상태 추적기
struct ModifierState {
    control: bool,
    shift: bool,
    alt: bool,
    command: bool,
}

impl ModifierState {
    fn new() -> Self {
        Self {
            control: false,
            shift: false,
            alt: false,
            command: false,
        }
    }

    /// 키 이벤트로 모디파이어 상태 업데이트
    fn update_from_event(&mut self, vk_code: u32, key_down: bool) {
        match vk_code {
            0x11 => self.control = key_down,    // Ctrl
            0x10 => self.shift = key_down,      // Shift
            0x12 => self.alt = key_down,        // Alt
            0x5B => self.command = key_down,    // Win
            _ => {}
        }
    }

    fn build_modifiers(&self) -> Vec<Modifier> {
        let mut mods = Vec::new();
        if self.control { mods.push(Modifier::Control); }
        if self.shift { mods.push(Modifier::Shift); }
        if self.alt { mods.push(Modifier::Alt); }
        if self.command { mods.push(Modifier::Command); }
        mods
    }
}
```

**`hook_callback` 수정**:

```rust
extern "system" fn hook_callback(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> i32 {
    if n_code < 0 {
        return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
    }

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let hook_struct = unsafe { &*(l_param as *const KBDLLHOOKSTRUCT) };
        let vk_code = hook_struct.vkCode;
        let scan_code = hook_struct.scanCode;
        let flags = hook_struct.flags;
        let key_down = w_param == 0x0100; // WM_KEYDOWN

        // Skip key-up, repeat, injected
        if !key_down || (flags & 0x80) != 0 {
            return unsafe { CallNextHookEx(None, n_code, w_param, l_param) };
        }

        // 모디파이어 상태 업데이트 (이벤트 기반)
        MODIFIER_STATE.lock().unwrap().update_from_event(vk_code, key_down);

        if let Some(stroke) = vk_to_stroke_with_modifiers(vk_code, scan_code) {
            tracing::info!(
                "Keyboard event detected: vk=0x{:02X} → {}",
                vk_code,
                stroke.display()
            );
        }

        unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
    }));

    match result {
        Ok(ret) => ret,
        Err(_) => {
            tracing::error!("Keyboard hook callback panicked");
            unsafe { CallNextHookEx(None, n_code, w_param, l_param) }
        }
    }
}

/// 전역 모디파이어 상태 (hook callback 내에서 공유)
static MODIFIER_STATE: std::sync::Mutex<ModifierState> = 
    std::sync::Mutex::new(ModifierState::new());

fn vk_to_stroke_with_modifiers(vk_code: u32, _scan_code: u32) -> Option<KeyStroke> {
    let modifiers = MODIFIER_STATE.lock().unwrap().build_modifiers();
    let key_code = vk_to_key_code(vk_code);
    if key_code.is_empty() { return None; }
    
    Some(KeyStroke {
        modifiers,
        key: Key { code: key_code },
    })
}
```

**`vk_to_key_code` 대문자 수정**:

```rust
fn vk_to_key_code(vk: u32) -> String {
    match vk {
        0x30..=0x39 => (vk - 0x30).to_string(),        // 0-9 (변경 없음)
        0x41..=0x5A => ((vk - 0x41 + b'A') as char).to_string(), // A-Z → 대문자
        0x70..=0x7B => format!("F{}", vk - 0x6F),       // F1-F16 (변경 없음)
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x20 => "Space".to_string(),
        0x1B => "Escape".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x21 => "PageUp".to_string(),
        0x22 => "PageDown".to_string(),
        0x24 => "Home".to_string(),
        0x23 => "End".to_string(),
        _ => {
            let char_code = unsafe { MapVirtualKeyW(vk, 0) };
            if char_code > 0 {
                (char_code as u8 as char).to_uppercase().next().map(String::from)
                    .unwrap_or_else(|| format!("VK_0x{:02X}", vk))
            } else {
                format!("VK_0x{:02X}", vk)
            }
        }
    }
}
```

**테스트 수정**:

```rust
#[test]
fn test_vk_to_key_code_uppercase() {
    assert_eq!(vk_to_key_code(0x41), "A");  // 이전: "a" → "A"
    assert_eq!(vk_to_key_code(0x42), "B");
    assert_eq!(vk_to_key_code(0x5A), "Z");
    assert_eq!(vk_to_key_code(0x31), "1");
    assert_eq!(vk_to_key_code(0x70), "F1");
}
```

**예상 영향도**:
- **위험도**: 🟡 중간 — 모디파이어 트래킹은 전역 상태 관리 필요 (스레드 세이프 필수)
- **테스트 필요**: ✅ 모디파이어 조합 테스트, 대소문자 일관성 확인
- **병렬 안전**: `MODIFIER_STATE`는 `Mutex`로 보호 (hook callback은 다른 스레드에서 호출될 수 있음)

**추천 우선순위**: **P1** — Windows 사용자 경험에 직접적 영향

---

### IMP-007: 동시 파일 쓰기 Race 조건 해결

**설명**: `commands.rs`의 `add_keybinding`과 `remove_keybinding`이 독립적으로 config 파일을 읽고 수정 → 동시 호출 시 한쪽 변경이 손실됨.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-daemon/src/ipc.rs` | `SetConfig`를 유일한 쓰기 엔트리 포인트로 강제 |
| `addon-gui/src/commands.rs` | `add_keybinding`/`remove_keybinding`을 IPC `SetConfig`로 리팩토링 |
| `addon-core/src/config.rs` | 파일 레벨 락 (선택적) |

**구현 세부 사항**:

```rust
// addon-core/src/config.rs — 파일 레벨 락

use std::sync::atomic::{AtomicBool, Ordering};

static CONFIG_WRITE_LOCK: AtomicBool = AtomicBool::new(false);

fn try_config_write_lock() -> Option<ConfigWriteGuard> {
    if CONFIG_WRITE_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
        Some(ConfigWriteGuard)
    } else {
        None
    }
}

pub struct ConfigWriteGuard;

impl Drop for ConfigWriteGuard {
    fn drop(&mut self) {
        CONFIG_WRITE_LOCK.store(false, Ordering::Release);
    }
}
```

**GUI 측 리팩토링** (`addon-gui/src/commands.rs`):

```rust
// 새 IPC Request 타입 추가 (addon-core/src/ipc.rs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    // ... 기존
    AddKeybinding {
        id: String,
        keys: Vec<String>,
        action: serde_json::Value,  // Action 직렬화
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    RemoveKeybinding {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
}

// commands.rs — add_keybinding 리팩토링
#[tauri::command(async)]
async fn add_keybinding(
    id: String,
    keys: String,
    action_type: String,
    action_data: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // 1. 로컬에서 액션 검증
    let action = build_action(&action_type, &action_data)?;
    let keys_vec: Vec<String> = keys.split(',').map(|s| s.trim().to_string()).collect();
    
    // 2. 액션을 직렬화
    let action_json = serde_json::to_value(&action).map_err(|e| e.to_string())?;
    
    // 3. IPC로 데몬에 위임 (단일 쓰기 포인트)
    let req = IpcMessage::request(IpcRequest::AddKeybinding {
        id,
        keys: keys_vec,
        action: action_json,
    });
    
    match send_async(&req).await {
        Ok(msg) => Ok(serde_json::to_value(&msg).map_err(|e| e.to_string())?),
        Err(e) => Ok(serde_json::json!({"type": "error", "code": "ipc", "details": e.to_string()})),
    }
}
```

**데몬 측 처리** (`addon-daemon/src/ipc.rs`):

```rust
IpcRequest::AddKeybinding { id, keys, action, .. } => {
    // 단일 쓰기 포인트 — 이미 write lock 확보됨
    let new_action: Action = serde_json::from_value(action).map_err(|e| /* ... */)?;
    
    guard.config.keybindings.push(KeyBinding {
        id, keys, action: new_action, overrides: None,
    });
    
    // 파일 저장도 데몬에서 수행 (GUI는 IPC만 사용)
    let config_path = get_config_path()?;
    addon_core::config::save_to_disk(&config_path, &guard.config)?;
    
    // ... adapter 재초기화
}
```

**예상 영향도**:
- **위험도**: 🟡 중간 — GUI → IPC 위임은 아키텍처 변경. 기존 로컬 파일 직접 수정 패턴 제거 필요
- **테스트 필요**: ✅ 동시 add/remove 테스트, IPC 실패 시 롤백

**추천 우선순위**: **P1** — 데이터 무결성 보장 필수

---

### IMP-008: 테스트 커버리지 확장

**설명**: 핵심 IPC 로직, config 파싱/검증, IPC 통합 테스트가 전무함. `addon-daemon`과 `addon-core`에 단위 테스트 추가 필요.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-daemon/src/ipc.rs` | `#[cfg(test)]` — IPC 메시지 파싱/서명 테스트 |
| `addon-core/src/config.rs` | `#[cfg(test)]` — config 로드/검증 테스트 |
| `addon-core/src/keymap.rs` | `#[cfg(test)]` — KeyStroke 파싱 테스트 |
| `addon-daemon/tests/integration.rs` | IPC 통합 테스트 (새 파일) |

**구현 세부 사항**:

```rust
// addon-core/src/config.rs — 테스트 모듈 추가

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::config::KeyBinding;

    fn sample_config() -> Config {
        Config {
            version: "1.0".to_string(),
            global: GlobalSettings::default(),
            keybindings: vec![
                KeyBinding {
                    id: "test1".to_string(),
                    keys: vec!["Ctrl+A".to_string()],
                    action: Action::Paste { text: "hello".to_string() },
                    overrides: None,
                },
            ],
        }
    }

    #[test]
    fn test_validate_no_errors() {
        let config = sample_config();
        assert!(config.validate().is_empty());
    }

    #[test]
    fn test_validate_duplicate_ids() {
        let mut config = sample_config();
        config.keybindings.push(KeyBinding {
            id: "test1".to_string(), // duplicate
            keys: vec!["Ctrl+B".to_string()],
            action: Action::Paste { text: "world".to_string() },
            overrides: None,
        });
        let errors = config.validate();
        assert!(errors.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn test_validate_empty_keys() {
        let mut config = sample_config();
        config.keybindings.push(KeyBinding {
            id: "empty".to_string(),
            keys: vec![],
            action: Action::Paste { text: "".to_string() },
            overrides: None,
        });
        let errors = config.validate();
        assert!(errors.iter().any(|e| e.contains("no keys")));
    }

    #[test]
    fn test_validate_invalid_command() {
        let mut config = sample_config();
        config.keybindings.push(KeyBinding {
            id: "bad_cmd".to_string(),
            keys: vec!["Ctrl+X".to_string()],
            action: Action::SystemCommand { command: "rm -rf /; echo hack".to_string() },
            overrides: None,
        });
        let errors = config.validate();
        assert!(errors.iter().any(|e| e.contains("shell metacharacter")));
    }

    #[test]
    fn test_build_keymapper() {
        let config = sample_config();
        let mapper = config.build_keymapper(OsPlatform::Linux);
        let stroke = KeyStroke::parse("Ctrl+A").unwrap();
        assert!(mapper.lookup(&stroke).is_some());
    }

    #[test]
    fn test_effective_keys_with_overrides() {
        let mut config = sample_config();
        config.keybindings[0].overrides = Some(PlatformOverrides {
            macos: Some(vec!["Cmd+A".to_string()]),
            windows: None,
            linux: None,
        });
        
        let binding = &config.keybindings[0];
        assert_eq!(binding.effective_keys(OsPlatform::Macos), &["Cmd+A"]);
        assert_eq!(binding.effective_keys(OsPlatform::Linux), &["Ctrl+A"]);
    }
}

// addon-daemon/tests/integration.rs — IPC 통합 테스트
#[cfg(test)]
mod integration_tests {
    use addon_core::ipc::{IpcMessage, IpcRequest, IpcResponse};
    use serde_json;

    #[test]
    fn test_serialize_deserialize_setconfig() {
        let config_json = serde_json::json!({
            "version": "1.0",
            "keybindings": [],
            "global": {}
        });
        let req = IpcMessage::request(IpcRequest::SetConfig {
            config: config_json,
            request_id: Some("test-1".to_string()),
        });
        let json = serde_json::to_string(&req).unwrap();
        let parsed: IpcMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, IpcMessage::Request(IpcRequest::SetConfig { .. })));
    }

    #[test]
    fn test_request_response_correlation() {
        let req = IpcMessage::request(IpcRequest::GetStatus {
            request_id: Some("corr-1".to_string()),
        });
        let resp = IpcMessage::response(IpcResponse::DaemonStatus {
            running: true,
            pid: 1234,
            version: "0.1.0".to_string(),
            request_id: Some("corr-1".to_string()),
        });
        // request_id가 응답에 복제되었는지 확인
        if let IpcMessage::Response(IpcResponse::DaemonStatus { request_id, .. }) = resp {
            assert_eq!(request_id, Some("corr-1".to_string()));
        } else {
            panic!("Expected DaemonStatus response");
        }
    }

    #[test]
    fn test_keybinding_json_conversion() {
        use addon_core::ipc::KeyBindingJson;
        let binding = addon_core::config::KeyBinding {
            id: "test".to_string(),
            keys: vec!["Ctrl+V".to_string()],
            action: addon_core::actions::Action::Paste { text: "hi".to_string() },
            overrides: None,
        };
        let json = KeyBindingJson::from(binding);
        assert_eq!(json.id, "test");
        assert_eq!(json.keys, vec!["Ctrl+V"]);
        assert_eq!(json.action_type, "paste");
    }
}
```

**예상 영향도**:
- **위험도**: 🟢 낮음 — 테스트 추가만
- **테스트 필요**: ✅ 테스트 자체를 작성해야 함
- **목표 커버리지**: addon-core ≥ 80%, addon-daemon ≥ 70%

**추천 우선순위**: **P1** — 테스트 없는 코드는 유지보수 불가

---

## 🔵 P2 — 코드 냄새 / 개선 기회

---

### IMP-009: 코드 중복 제거

**설명**: `get_config_path()` 3곳 중복, `send_async()` 반복, `SetConfig`/`ReloadConfig` 어댑터 재초기화 로직 복사.

**수정 대상 파일**:
| 파일 | 변경 내용 |
|------|----------|
| `addon-core/src/config.rs` | `get_config_path()` 이동 + public export |
| `addon-gui/src/commands.rs` | `send_async` 헬퍼 추출, 어댑터 재초기화 공통 함수 |
| `addon-daemon/src/ipc.rs` | `reinitialize_adapter` 공통 함수 |
| `addon-gui/src/config_ops.rs` | `get_config_path()` 제거 (addon-core 사용) |

**구현 세부 사항**:

```rust
// addon-core/src/config.rs — get_config_path() 중앙화

/// config.yaml 찾기 경로:
/// 1. $ADDON_CONFIG env var
/// 2. ~/.config/addon/config.yaml (XDG)
/// 3. ~/.addon/config.yaml
/// 4. ./config.yaml
pub fn get_config_path() -> Result<std::path::PathBuf> {
    if let Ok(path) = std::env::var("ADDON_CONFIG") {
        return Ok(std::path::PathBuf::from(path));
    }
    if let Some(config_dir) = dirs::config_dir() {
        let xdg = config_dir.join("addon").join("config.yaml");
        if xdg.exists() { return Ok(xdg); }
    }
    if let Some(home) = dirs::home_dir() {
        let home_config = home.join(".addon").join("config.yaml");
        if home_config.exists() { return Ok(home_config); }
    }
    let local = std::path::PathBuf::from("config.yaml");
    if local.exists() { return Ok(local); }
    Err(anyhow::anyhow!(
        "Config not found. Searched: ~/.config/addon/config.yaml, ~/.addon/config.yaml, ./config.yaml"
    ))
}

// addon-gui/src/config_ops.rs — get_config_path() 제거 후 re-export
pub use addon_core::config::get_config_path;
```

```rust
// addon-daemon/src/ipc.rs — 어댑터 재초기화 공통 함수

/// SetConfig / ReloadConfig에서 공유하는 어댑터 재초기화 로직
fn reinitialize_adapter(
    state: &Arc<RwLock<DaemonState>>,
    adapter: Option<Box<dyn OsAdapter + Send + Sync>>,
    new_config: &Config,
) -> bool {
    if let Some(mut adapter) = adapter {
        adapter.set_config(new_config);
        let stop_ok = adapter.stop().is_ok();
        let init_ok = if stop_ok { adapter.init().is_ok() } else { false };
        let start_ok = if init_ok { adapter.start().is_ok() } else { true };

        if !stop_ok { tracing::warn!("Adapter stop during reinit"); }
        else if !init_ok { tracing::warn!("Adapter reinit during reinit"); }
        else if !start_ok { tracing::warn!("Adapter start during reinit"); }

        let mut g = state.write().unwrap_or_else(|e| e.into_inner());
        g.adapter = Some(adapter);
        g.initialized = stop_ok && init_ok && start_ok;
        stop_ok && init_ok && start_ok
    } else {
        true
    }
}
```

```rust
// addon-gui/src/commands.rs — send_async 공통 헬퍼 (이미 존재하지만 add/remove에서 복사됨)

// 기존 send_async()를 add_keybinding/remove_keybinding에서 재사용:
// ❌ 기존: 각 함수마다 UnixStream::connect(...) 복사
// ✅ 수정: send_async(&IpcMessage::request(IpcRequest::ReloadConfig))

// add_keybinding에서:
let req = IpcMessage::request(IpcRequest::ReloadConfig);
match send_async(&req).await { ... }

// remove_keybinding에서 동일하게:
let req = IpcMessage::request(IpcRequest::ReloadConfig);
match send_async(&req).await { ... }
```

**예상 영향도**:
- **위험도**: 🟢 매우 낮음 — 단순 리팩토링
- **테스트 필요**: ✅ config path 테스트, reinitialize_adapter 테스트

**추천 우선순위**: **P2** — 코드 가독성/유지보수성 개선

---

## 📊 구현 순서 및 의존성 매트릭스

```
Phase 1 (P0 — 보안, 2-3일):
├── IMP-004  Tauri CSP 보안 강화          ← 의존성 없음, 가장 빠름
└── IMP-001  IPC 인증 시스템              ← IMP-004와 병렬 가능

Phase 2 (P1 — 기능, 5-7일):
├── IMP-005  IPC 프로토콜 버전 관리       ← IMP-001과 함께 진행
├── IMP-003  macOS event_handler 안전성   ← 의존성 없음
├── IMP-006  Windows 타이밍/대소문자 버그 ← 의존성 없음
├── IMP-007  동시 파일 쓰기 Race 조건     ← IMP-005의 IpcRequest 확장 필요
└── IMP-002  Linux process_events() 구현  ← 별도, 가장 큰 작업

Phase 3 (P1+P2 — 테스트/리팩토링, 3-4일):
├── IMP-008  테스트 커버리지 확장         ← Phase 1-2의 변경 반영
└── IMP-009  코드 중복 제거              ← 모든 변경 후 최종 정리
```

---

## 🧪 테스트 전략

| 테스트 유형 | 범위 | 도구 |
|------------|------|------|
| 단위 테스트 | `addon-core/*` config, keymap, actions | `cargo test` |
| IPC 단위 테스트 | `addon-daemon/src/ipc.rs` 메시지 직렬화/파싱 | `cargo test` |
| IPC 통합 테스트 | daemon ↔ GUI 소켓 통신 | `tokio::net::UnixStream` mock |
| 플랫폼 테스트 | Linux/X11, macOS/Carbon, Windows/Hook | 플랫폼별 CI |
| 보안 테스트 | 인증 우회, CSP 우회, shell injection | `cargo test` + manual |

---

## ⚠️ 위험 요소 및 완화 전략

| 위험 | 영향 | 완화 |
|------|------|------|
| X11/XI2 API 복잡성 | IMP-002 지연 | 먼저 basic grab + XTest로 PoC, 이후 XI2로 확장 |
| IPC 인증 하위 호환성 | 기존 클라이언트 차단 | 인증 실패 시 `unauthenticated` 모드 (GET-only) |
| Windows 모디파이어 전역 상태 | 동시성 버그 | `Mutex` + 이벤트 기반 업데이트로 동기화 |
| CSP 인라인 스타일 누락 | 빌드 실패 | CSP strict 모드에서 빌드 검증 |

---

## 📝 결론

이 개선 계획은 **보안 → 기능 → 품질**의 3단계로 구성되어 있습니다:

1. **P0 (2개 작업)**: 보안 취약점 즉시 해결 — IPC 인증 + CSP 강화
2. **P1 (6개 작업)**: 핵심 기능 완성 + 데이터 무결성 — Linux 구현, macOS 안전성, Windows 버그, IPC 버전, Race 조건, 테스트
3. **P2 (1개 작업)**: 코드 품질 — 중복 제거

모든 작업은 작은 청크로 분할되어 독립적으로 구현·테스트·배포 가능합니다.
