# Claude Code LLM API Inspector

Claude Code가 Anthropic API로 보내는 모든 요청/응답을 가로채어 SQLite에 저장하고, 웹 대시보드와 MCP 서버를 통해 열람할 수 있는 단일 바이너리 도구입니다.

## Architecture

```
Claude Code ──HTTP──▶ Proxy :7878 ──HTTPS──▶ api.anthropic.com
                           │
                      SQLite DB
                   (~/.local/share/claude-code-hook/logs.db)
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
     Dashboard :7879              MCP Server (stdio)
    (Web UI + REST API)       (Claude Code integration)
```

### 컴포넌트

| 컴포넌트 | 포트/방식 | 역할 |
|---------|-----------|------|
| **Proxy** | `:7878` HTTP | Claude Code 트래픽을 가로채어 DB 저장 후 업스트림으로 포워딩 |
| **Dashboard** | `:7879` HTTP | 세션/요청 목록 Web UI + REST API + SSE 실시간 업데이트 |
| **MCP Server** | stdio | Claude Code 세션 내에서 inspector 데이터를 쿼리하는 MCP 도구 |

### 요청 처리 흐름

**Non-streaming:**
```
요청 수신 → DB pending 기록 → 업스트림 포워딩 → 응답 수신 → DB complete 업데이트 → SSE 이벤트 발행
```

**Streaming (SSE):**
```
요청 수신 → DB pending 기록 → 업스트림 포워딩 → SseTeeStream으로 래핑
    → 클라이언트에 실시간 포워딩 + 버퍼 동시 축적
    → 스트림 종료 → SSE 파싱 (content/tokens 추출) → DB complete 업데이트
```

## 파일 구조

```
claude-code-hook/
├── Cargo.toml
├── CLAUDE.md                    # 프로젝트 개발 지침 (100% 테스트 커버리지 등)
├── frontend/                    # Vite + vanilla JS 대시보드 프론트엔드
│   ├── index.html
│   ├── package.json
│   ├── vite.config.js           # dev: :5173 → :7879 프록시 / build: src/assets/dist/
│   └── src/
│       ├── main.js              # 앱 진입점, DOM 렌더링, 상태 관리
│       ├── api.js               # REST API + SSE 폴링 클라이언트
│       ├── utils.js             # 포맷팅, 색상 팔레트, HTML 이스케이프
│       └── style.css            # 다크 테마 CSS
└── src/
    ├── main.rs                  # CLI (clap), DB 초기화, 서버 시작
    ├── types.rs                 # 공유 타입: RequestRecord, SessionRecord, AppState
    ├── db.rs                    # SQLite 스키마 초기화, CRUD 헬퍼
    ├── proxy.rs                 # 핵심: 요청 가로채기, 포워딩, SSE tee, DB 저장
    ├── sse_tee.rs               # SSE 스트림 tee + 파서
    ├── session.rs               # macOS lsof: TCP 포트 → PID → CWD 역추적 + 캐시
    ├── dashboard.rs             # HTTP 라우팅: /api/*, /events, 정적 파일 서빙
    ├── mcp.rs                   # MCP 서버 (stdio JSON-RPC)
    └── assets/
        ├── dashboard.html       # 폴백 단일 파일 대시보드 (항상 바이너리에 포함)
        └── dist/                # Vite 빌드 결과물 (include_dir!로 바이너리에 임베드)
```

## 설치 및 실행

```bash
# 백엔드 빌드
cargo build --release

# 프론트엔드 빌드 (생략하면 폴백 HTML 사용)
cd frontend && npm install && npm run build && cd ..

# 실행
./target/release/claude-code-hook
```

```
  Claude Code LLM API Inspector
  ─────────────────────────────────────────────────────
  Proxy:     http://127.0.0.1:7878
  Dashboard: http://127.0.0.1:7879

  Set the environment variable:
    export ANTHROPIC_BASE_URL=http://127.0.0.1:7878

  Claude Code MCP integration:
    claude mcp add claude-inspector -- $(which claude-code-hook) mcp
```

## Claude Code에 적용

```bash
# 현재 세션
export ANTHROPIC_BASE_URL=http://127.0.0.1:7878

# 영구 적용
echo 'export ANTHROPIC_BASE_URL=http://127.0.0.1:7878' >> ~/.zshrc
```

## Claude Code MCP 플러그인

```bash
# 등록 (최초 1회)
claude mcp add claude-inspector -- /path/to/claude-code-hook mcp
```

등록 후 Claude Code 대화 중 사용 가능한 MCP 도구:

| 도구 | 설명 |
|------|------|
| `list_sessions` | 추적 중인 모든 세션 (요청 수, 토큰 합계) |
| `list_requests` | 최근 요청 목록 (session_id 필터, 페이지네이션) |
| `get_request`   | 특정 요청의 전체 상세 (messages, response, usage, timing) |

## Session 식별 (macOS)

동시에 여러 Claude Code 프로세스를 자동으로 구분합니다:

1. 프록시 TCP 연결 → **소스 포트** 기록
2. `lsof -i :<port>` → **PID** 조회 (자신 제외)
3. `lsof -a -p <PID> -d cwd` → **CWD** 조회
4. `basename(CWD)` → **프로젝트명** 추출
5. 조회 결과를 메모리에 캐시

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
    request_body     TEXT NOT NULL,
    response_status  INTEGER,
    response_headers TEXT,
    response_body    TEXT,            -- SSE: {"accumulated_content": "...", "raw_sse": "..."}
    is_streaming     INTEGER NOT NULL DEFAULT 0,
    input_tokens     INTEGER,
    output_tokens    INTEGER,
    duration_ms      INTEGER,
    status           TEXT NOT NULL DEFAULT 'pending'  -- pending | complete | error
);
```

## 테스트

```bash
# 전체 테스트 실행 (72개)
cargo test

# 커버리지 측정 (cargo-llvm-cov 필요)
cargo llvm-cov --all-features --workspace
```

| 모듈 | 테스트 수 | 내용 |
|------|-----------|------|
| `db` | 10 | 스키마 초기화, CRUD, 페이지네이션, 토큰 집계 |
| `sse_tee` | 8 | 스트림 tee 동작, SSE 파싱 전 케이스 |
| `session` | 5 | 캐시 hit/miss, lsof 통합 테스트 |
| `mcp` | 14 | JSON-RPC 프로토콜, 모든 도구, 에러 케이스 |
| `proxy` | 5 | Non-streaming/streaming, x-api-key 필터, 업스트림 실패 |
| `dashboard` | 14 | 모든 API 라우트, SSE, MIME 타입, 정적 파일 |
| `types` | 5 | 직렬화, AppState 생성자 |
| `main` | 2 | DB 경로 해석 |

## 프론트엔드 개발

```bash
cd frontend
npm run dev    # :5173 (API는 :7879로 프록시)
npm run build  # ../src/assets/dist/ → Rust 바이너리에 임베드
```

## CLI 옵션

```
claude-code-hook [OPTIONS] [COMMAND]

Commands:
  serve  프록시 + 대시보드 서버 실행 (기본값)
  mcp    stdio MCP 서버로 실행

Options:
  --proxy-addr <ADDR>      [default: 127.0.0.1:7878]
  --dashboard-addr <ADDR>  [default: 127.0.0.1:7879]
  --db-path <PATH>         SQLite DB 경로 (기본: 플랫폼 데이터 디렉토리)
```

## 보안

- `x-api-key` 헤더는 업스트림으로는 전달되지만 DB에는 **절대 저장하지 않습니다.**
- 프록시는 localhost에만 바인딩됩니다 (외부 노출 없음).
