# Claude Code LLM API Inspector

Claude Code가 Anthropic API로 보내는 **모든 요청/응답을 투명하게 가로채어** SQLite에 저장하고, 웹 대시보드에서 실시간으로 열람할 수 있는 단일 바이너리 도구입니다.

```
Claude Code ──HTTP──▶ Proxy :7878 ──HTTPS──▶ api.anthropic.com
                           │
                      SQLite DB
               (~/Library/Application Support/claude-code-hook/logs.db)
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
     Dashboard :7879              MCP Server (stdio)
    (Web UI + REST API)       (Claude Code 내부 쿼리)
```

---

## 빠른 설치 (macOS)

```bash
git clone https://github.com/windowhan/claude-code-hook.git
cd claude-code-hook
bash install.sh
```

스크립트가 아래를 자동으로 처리합니다:

1. 프론트엔드 빌드 (`npm install && npm run build`)
2. Rust 바이너리 빌드 (`cargo build --release`)
3. 바이너리 설치 (`~/.local/bin/claude-code-hook`)
4. `ANTHROPIC_BASE_URL` 영구 설정 (`~/.zshrc` 또는 `~/.bash_profile`)
5. macOS LaunchAgent 등록 (로그인 시 자동 시작, KeepAlive)
6. Claude Code MCP 서버 등록

---

## 전제 조건

| 도구 | 설치 방법 |
|------|-----------|
| **Rust** (1.75+) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| **Node.js** (18+) | https://nodejs.org 또는 `brew install node` |
| **Claude Code** | `npm install -g @anthropic-ai/claude-code` |

> Node.js 없이도 빌드 가능합니다. 이 경우 폴백 UI(단일 HTML)가 사용됩니다.

---

## 수동 설치 (단계별)

### 1. 저장소 클론

```bash
git clone https://github.com/windowhan/claude-code-hook.git
cd claude-code-hook
```

### 2. 프론트엔드 빌드

```bash
cd frontend
npm install
npm run build    # ../src/assets/dist/ 에 빌드 결과물 생성
cd ..
```

### 3. Rust 바이너리 빌드

```bash
cargo build --release
# 결과물: ./target/release/claude-code-hook
```

### 4. 바이너리를 PATH에 설치

```bash
mkdir -p ~/.local/bin
cp target/release/claude-code-hook ~/.local/bin/

# ~/.local/bin 이 PATH에 없는 경우 추가
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

### 5. ANTHROPIC_BASE_URL 영구 설정

Claude Code가 항상 프록시를 통하도록 환경변수를 쉘 설정에 추가합니다.

```bash
echo 'export ANTHROPIC_BASE_URL=http://127.0.0.1:7878' >> ~/.zshrc
source ~/.zshrc
```

> bash 사용자는 `~/.zshrc` 대신 `~/.bash_profile` 또는 `~/.bashrc`를 사용하세요.

### 6. 서버 시작

**수동 실행:**
```bash
claude-code-hook
```

**macOS LaunchAgent로 자동 시작 (로그인 시):**
```bash
# plist 파일 생성
cat > ~/Library/LaunchAgents/com.claude-code-inspector.plist <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.claude-code-inspector</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/YOUR_USERNAME/.local/bin/claude-code-hook</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/claude-inspector.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/claude-inspector.log</string>
</dict>
</plist>
EOF

# 등록 및 즉시 시작
launchctl load ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

### 7. Claude Code MCP 서버 등록 (선택)

Claude Code 대화 중에 MCP 도구로 직접 로그를 조회하고 싶을 때 등록합니다.

```bash
claude mcp add claude-inspector -- ~/.local/bin/claude-code-hook mcp
```

등록 후 사용 가능한 MCP 도구:

| 도구 | 설명 |
|------|------|
| `list_sessions` | 추적 중인 세션 목록 (요청 수, 토큰 합계) |
| `list_requests` | 요청 목록 (session_id 필터, 페이지네이션) |
| `get_request`   | 특정 요청의 전체 상세 (messages, response, usage, timing) |

---

## 대시보드

서버 시작 후 브라우저에서 열기:

```bash
open http://127.0.0.1:7879
```

### 화면 구성

```
┌──────────────────────────────────────────────────────────────────────────┐
│  Claude Code Inspector                              3 sessions · 1 active │
├────────────────┬─────────────────────────┬───────────────────────────────┤
│  SESSIONS      │  REQUESTS               │  DETAIL                       │
│                │                         │                               │
│  ★ Starred     │  [proj] 10:23 ✓ ☆      │  POST /v1/messages            │
│  ○ All         │    1.2k↑ 0.3k↓ 4.2s    │  ──────────────────────────── │
│                │    2.3k→8.7k            │  REQUEST          RESPONSE    │
│  ● my-app      │                         │  ─────────        ─────────   │
│    3 requests  │  [my-app] 10:21 ✓ ★    │  Model: ...       200 complete│
│                │    0.8k↑ 0.1k↓ 2.1s    │  Input: 1.2k tok  Out: 0.3k   │
│  ○ api-server  │    1.1k→3.2k            │  Cache: 72k read  4.2s        │
│    5 requests  │                         │                               │
│                │  [api-s] 10:19 ⏳ ☆    │  user             call·Read   │
│                │    pending              │  "어떤 파일..."   {"file_path" │
│                │    0.9k→…               │                   :"/foo/bar"}│
└────────────────┴─────────────────────────┴───────────────────────────────┘
```

### 주요 기능

| 기능 | 설명 |
|------|------|
| **실시간 업데이트** | SSE 폴링으로 새 요청이 즉시 반영됨 |
| **Session 자동 그룹화** | 같은 CWD의 subagent/background task는 하나의 세션으로 묶임 |
| **Request/Response 분할** | text 응답, tool_use call(이름+input JSON), tool_result 모두 파싱해서 표시 |
| **캐시 토큰 통계** | `cache_read_input_tokens`, `cache_creation_input_tokens` 표시 |
| **요청/응답 크기** | 각 요청 항목에 `요청크기→응답크기` 표시 (예: `2.3k→8.7k`) |
| **별표 북마크** | 중요한 요청에 ★ 표시, "Starred" 탭에서 모아보기 |
| **세션 삭제** | 세션 항목의 ✕ 버튼으로 세션과 모든 요청 데이터 삭제 |
| **Copy curl** | 선택한 요청을 curl 명령어로 복사 |

---

## 동작 원리

### Session 식별 (macOS)

여러 Claude Code 프로세스가 동시에 실행될 때 자동으로 구분합니다.

1. 프록시에 TCP 연결 → 클라이언트 **소스 포트** 기록
2. `lsof -i :<port>` → 해당 포트를 사용하는 **PID** 조회
3. `lsof -a -p <PID> -d cwd` → 프로세스의 **작업 디렉토리** 조회
4. 같은 CWD의 기존 세션이 있으면 재사용 → subagent/background task가 같은 세션으로 묶임
5. `basename(CWD)` → 프로젝트명 추출 (예: `/Users/foo/my-app` → `my-app`)

### 스트리밍 처리

```
요청 수신 → DB pending 기록 → upstream 포워딩
    → SseTeeStream: Claude Code에 실시간 포워딩 + 버퍼 동시 축적
    → 스트림 종료 → SSE 파싱 (content blocks, tokens, cache stats)
    → DB complete 업데이트 → 대시보드 SSE 이벤트 발행
```

**Accept-Encoding 필터링**: 프록시는 `Accept-Encoding` 헤더를 upstream으로 전달하지 않습니다. Anthropic이 gzip 압축 응답을 보내면 SSE 파싱이 불가능하기 때문입니다.

### 보안

- `x-api-key` 헤더는 upstream으로 전달되지만 **DB에는 저장하지 않습니다**
- 프록시는 **localhost에만 바인딩**됩니다 (외부 노출 없음)

---

## SQLite 스키마

```sql
CREATE TABLE sessions (
    id           TEXT PRIMARY KEY,  -- UUID
    pid          INTEGER,           -- Claude Code PID
    cwd          TEXT,              -- 작업 디렉토리
    project_name TEXT,              -- basename(cwd)
    started_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL
);

CREATE TABLE requests (
    id               TEXT PRIMARY KEY,
    session_id       TEXT,
    timestamp        TEXT NOT NULL,
    method           TEXT NOT NULL,
    path             TEXT NOT NULL,
    request_headers  TEXT NOT NULL,   -- JSON (x-api-key 제외)
    request_body     TEXT NOT NULL,   -- JSON
    response_status  INTEGER,
    response_headers TEXT,            -- JSON
    response_body    TEXT,            -- streaming: {"accumulated_content":"...","raw_sse":"..."}
    is_streaming     INTEGER NOT NULL DEFAULT 0,
    input_tokens     INTEGER,
    output_tokens    INTEGER,
    duration_ms      INTEGER,
    status           TEXT NOT NULL DEFAULT 'pending',  -- pending | complete | error
    starred          INTEGER NOT NULL DEFAULT 0
);
```

DB 경로: `~/Library/Application Support/claude-code-hook/logs.db` (macOS)

직접 쿼리 예시:
```bash
sqlite3 ~/Library/Application\ Support/claude-code-hook/logs.db \
  "SELECT timestamp, input_tokens, output_tokens, status FROM requests ORDER BY timestamp DESC LIMIT 10;"
```

---

## CLI 옵션

```
claude-code-hook [OPTIONS] [COMMAND]

Commands:
  serve  프록시 + 대시보드 서버 실행 (기본값)
  mcp    stdio MCP 서버로 실행

Options:
  --proxy-addr <ADDR>      프록시 바인딩 주소 [default: 127.0.0.1:7878]
  --dashboard-addr <ADDR>  대시보드 바인딩 주소 [default: 127.0.0.1:7879]
  --db-path <PATH>         SQLite DB 경로 (기본: 플랫폼 데이터 디렉토리)
  -h, --help               도움말
```

---

## 개발

### 테스트 실행

```bash
cargo test
```

| 모듈 | 테스트 수 | 내용 |
|------|-----------|------|
| `db` | 14 | 스키마, CRUD, 페이지네이션, 토큰 집계, 별표, 세션 삭제 |
| `sse_tee` | 8 | 스트림 tee 동작, SSE 파싱 전 케이스 |
| `session` | 5 | 캐시 hit/miss, lsof 통합 테스트 |
| `mcp` | 14 | JSON-RPC 프로토콜, 모든 도구, 에러 케이스 |
| `proxy` | 5 | Non-streaming/streaming, x-api-key 필터, upstream 실패 |
| `dashboard` | 17 | 모든 API 라우트, SSE, 별표, 세션 삭제, MIME 타입 |
| `types` | 5 | 직렬화, AppState 생성자 |
| `main` | 2 | DB 경로 해석 |

### 프론트엔드 개발 서버

```bash
cd frontend
npm run dev    # http://localhost:5173 (API는 :7879로 프록시)
```

### 바이너리 재빌드 및 배포

```bash
cd frontend && npm run build && cd ..
cargo build --release
cp target/release/claude-code-hook ~/.local/bin/claude-code-hook

# LaunchAgent 재시작
launchctl unload ~/Library/LaunchAgents/com.claude-code-inspector.plist
launchctl load  ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

### 파일 구조

```
claude-code-hook/
├── install.sh                   # 자동 설치 스크립트
├── Cargo.toml
├── CLAUDE.md                    # 개발 지침
├── frontend/
│   ├── src/
│   │   ├── main.js              # 앱 진입점, 렌더링, 상태 관리
│   │   ├── api.js               # REST API + SSE 클라이언트
│   │   ├── utils.js             # 포맷팅 헬퍼
│   │   └── style.css            # 다크 테마
│   └── vite.config.js
└── src/
    ├── main.rs                  # CLI 진입점
    ├── types.rs                 # 공유 타입
    ├── db.rs                    # SQLite CRUD
    ├── proxy.rs                 # 프록시 핵심 로직
    ├── sse_tee.rs               # SSE 스트림 tee
    ├── session.rs               # PID/CWD 역추적
    ├── dashboard.rs             # HTTP API 서버
    ├── mcp.rs                   # MCP stdio 서버
    └── assets/
        ├── dashboard.html       # 폴백 UI
        └── dist/                # Vite 빌드 결과 (바이너리에 임베드)
```

---

## 문제 해결

**서버가 시작되지 않을 때:**
```bash
# 로그 확인
tail -f /tmp/claude-inspector.log

# 포트 사용 여부 확인
lsof -i :7878
lsof -i :7879
```

**LaunchAgent 재시작:**
```bash
launchctl unload ~/Library/LaunchAgents/com.claude-code-inspector.plist
launchctl load   ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

**Claude Code가 프록시를 통하지 않을 때:**
```bash
# 환경변수 확인
echo $ANTHROPIC_BASE_URL
# 출력: http://127.0.0.1:7878

# 설정 안 돼 있으면
export ANTHROPIC_BASE_URL=http://127.0.0.1:7878
```
