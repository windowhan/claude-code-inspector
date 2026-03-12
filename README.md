# Claude Code LLM API Inspector

A single-binary tool that **transparently intercepts all requests/responses** between Claude Code and the Anthropic API, stores them in SQLite, and lets you inspect them in real-time via a web dashboard.

```
Claude Code ──HTTP──▶ Proxy :7878 ──HTTPS──▶ api.anthropic.com
                           │
                      SQLite DB
               (~/Library/Application Support/claude-code-hook/logs.db)
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
     Dashboard :7879              MCP Server (stdio)
    (Web UI + REST API)       (query logs from within Claude Code)
```

---

## Roadmap

| # | Feature | Status |
|---|---------|--------|
| 1 | **Request interceptor & editor** — Intercept a request mid-flight, modify the payload (model, messages, parameters), then forward the edited request to the upstream API | 🔲 Planned |
| 2 | **Multi-provider routing** — Route specific requests to a different LLM provider (OpenAI, Gemini, Mistral, etc.) based on rules such as model name, session, or request content | 🔲 Planned |

---

## Quick Install (macOS)

```bash
git clone https://github.com/windowhan/claude-code-hook.git
cd claude-code-hook
bash install.sh
```

The script handles everything automatically:

1. Frontend build (`npm install && npm run build`)
2. Rust binary build (`cargo build --release`)
3. Binary installation (`~/.local/bin/claude-code-hook`)
4. Permanent `ANTHROPIC_BASE_URL` setting (`~/.zshrc` or `~/.bash_profile`)
5. macOS LaunchAgent registration (auto-start on login, KeepAlive)
6. Claude Code MCP server registration

---

## Prerequisites

| Tool | How to install |
|------|----------------|
| **Rust** (1.75+) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| **Node.js** (18+) | https://nodejs.org or `brew install node` |
| **Claude Code** | `npm install -g @anthropic-ai/claude-code` |

> Node.js is optional. If absent, the build falls back to a single-file HTML UI.

---

## Manual Install (Step by Step)

### 1. Clone the repo

```bash
git clone https://github.com/windowhan/claude-code-hook.git
cd claude-code-hook
```

### 2. Build the frontend

```bash
cd frontend
npm install
npm run build    # output → ../src/assets/dist/
cd ..
```

### 3. Build the Rust binary

```bash
cargo build --release
# output: ./target/release/claude-code-hook
```

### 4. Install the binary to PATH

```bash
mkdir -p ~/.local/bin
cp target/release/claude-code-hook ~/.local/bin/

# Add ~/.local/bin to PATH if not already there
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

### 5. Set ANTHROPIC_BASE_URL permanently

Add the environment variable to your shell config so Claude Code always routes through the proxy.

```bash
echo 'export ANTHROPIC_BASE_URL=http://127.0.0.1:7878' >> ~/.zshrc
source ~/.zshrc
```

> bash users: use `~/.bash_profile` or `~/.bashrc` instead of `~/.zshrc`.

### 6. Start the server

**Manual:**
```bash
claude-code-hook
```

**Auto-start via macOS LaunchAgent (on login):**
```bash
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

launchctl load ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

### 7. Register the MCP server (optional)

Register if you want to query logs directly as an MCP tool during Claude Code sessions.

```bash
claude mcp add claude-inspector -- ~/.local/bin/claude-code-hook mcp
```

Available MCP tools after registration:

| Tool | Description |
|------|-------------|
| `list_sessions` | List tracked sessions (request count, token totals) |
| `list_requests` | List requests (filter by session_id, pagination) |
| `get_request` | Full detail for a specific request (messages, response, usage, timing) |

---

## Dashboard

Open in your browser after starting the server:

```bash
open http://127.0.0.1:7879
```

### Layout

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
│                │    pending              │  "what file..."   {"file_path"│
│                │    0.9k→…               │                   :"/foo/bar"}│
└────────────────┴─────────────────────────┴───────────────────────────────┘
```

### Features

| Feature | Description |
|---------|-------------|
| **Live updates** | SSE polling — new requests appear instantly |
| **Session auto-grouping** | Subagents and background tasks sharing the same CWD are folded into one session |
| **Request/Response split view** | Text responses, tool_use calls (name + input JSON), and tool_results are all parsed and displayed |
| **Cache token stats** | `cache_read_input_tokens` and `cache_creation_input_tokens` shown per request |
| **Request/response sizes** | Each request row shows `req size → resp size` (e.g. `2.3k→8.7k`) |
| **Copy buttons** | Hover any code block to reveal a Copy button — copies content to clipboard |
| **Star bookmarks** | Star important requests with ★, view them all in the "Starred" tab |
| **Session delete** | Click ✕ on a session to delete it and all its request data |
| **Copy curl** | Copies the selected request as a ready-to-run curl command |

---

## How It Works

### Session identification (macOS)

Multiple Claude Code processes running simultaneously are automatically separated into distinct sessions.

1. TCP connection arrives at proxy → record client **source port**
2. `lsof -i :<port>` → find the **PID** using that port
3. `lsof -a -p <PID> -d cwd` → get the process **working directory**
4. If an existing session with the same CWD exists, reuse it → subagents/background tasks are grouped into the same session
5. `basename(CWD)` → extract project name (e.g. `/Users/foo/my-app` → `my-app`)

### Streaming handling

```
Request received → insert DB pending → forward to upstream
    → SseTeeStream: forward chunks to Claude Code in real-time + accumulate buffer
    → Stream ends → parse SSE (content blocks, tokens, cache stats)
    → update DB complete → publish dashboard SSE event
```

**Accept-Encoding filtering**: The proxy strips the `Accept-Encoding` header before forwarding upstream. If Anthropic returns a gzip-compressed response, the SSE stream becomes unparseable.

### Security

- The `x-api-key` header is forwarded to upstream but **never stored in the DB**
- The proxy binds to **localhost only** — no external exposure

---

## SQLite Schema

```sql
CREATE TABLE sessions (
    id           TEXT PRIMARY KEY,  -- UUID
    pid          INTEGER,           -- Claude Code PID
    cwd          TEXT,              -- working directory
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
    request_headers  TEXT NOT NULL,   -- JSON (x-api-key excluded)
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

DB path: `~/Library/Application Support/claude-code-hook/logs.db` (macOS)

Direct query example:
```bash
sqlite3 ~/Library/Application\ Support/claude-code-hook/logs.db \
  "SELECT timestamp, input_tokens, output_tokens, status FROM requests ORDER BY timestamp DESC LIMIT 10;"
```

---

## CLI Options

```
claude-code-hook [OPTIONS] [COMMAND]

Commands:
  serve  Run proxy + dashboard server (default)
  mcp    Run as stdio MCP server

Options:
  --proxy-addr <ADDR>      Proxy bind address [default: 127.0.0.1:7878]
  --dashboard-addr <ADDR>  Dashboard bind address [default: 127.0.0.1:7879]
  --db-path <PATH>         SQLite DB path (default: platform data directory)
  -h, --help               Show help
```

---

## Development

### Run tests

```bash
cargo test
```

| Module | Tests | Coverage |
|--------|-------|----------|
| `db` | 14 | Schema, CRUD, pagination, token aggregation, star, session delete |
| `sse_tee` | 8 | Stream tee behavior, all SSE parse cases |
| `session` | 5 | Cache hit/miss, lsof integration |
| `mcp` | 14 | JSON-RPC protocol, all tools, error cases |
| `proxy` | 5 | Non-streaming/streaming, x-api-key filter, upstream failure |
| `dashboard` | 17 | All API routes, SSE, star, session delete, MIME types |
| `types` | 5 | Serialization, AppState constructor |
| `main` | 2 | DB path resolution |

### Frontend dev server

```bash
cd frontend
npm run dev    # http://localhost:5173 (API proxied to :7879)
```

### Rebuild and redeploy

```bash
cd frontend && npm run build && cd ..
cargo build --release
cp target/release/claude-code-hook ~/.local/bin/claude-code-hook

# Restart LaunchAgent
launchctl unload ~/Library/LaunchAgents/com.claude-code-inspector.plist
launchctl load  ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

### File structure

```
claude-code-hook/
├── install.sh                   # automated install script
├── Cargo.toml
├── CLAUDE.md                    # development guidelines
├── frontend/
│   ├── src/
│   │   ├── main.js              # app entry, rendering, state management
│   │   ├── api.js               # REST API + SSE client
│   │   ├── utils.js             # formatting helpers
│   │   └── style.css            # dark theme
│   └── vite.config.js
└── src/
    ├── main.rs                  # CLI entry point
    ├── types.rs                 # shared types
    ├── db.rs                    # SQLite CRUD
    ├── proxy.rs                 # core proxy logic
    ├── sse_tee.rs               # SSE stream tee
    ├── session.rs               # PID/CWD backtracking
    ├── dashboard.rs             # HTTP API server
    ├── mcp.rs                   # MCP stdio server
    └── assets/
        ├── dashboard.html       # fallback UI
        └── dist/                # Vite build output (embedded in binary)
```

---

## Troubleshooting

**Server won't start:**
```bash
# Check logs
tail -f /tmp/claude-inspector.log

# Check port availability
lsof -i :7878
lsof -i :7879
```

**Restart LaunchAgent:**
```bash
launchctl unload ~/Library/LaunchAgents/com.claude-code-inspector.plist
launchctl load   ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

**Claude Code not routing through proxy:**
```bash
# Check env var
echo $ANTHROPIC_BASE_URL
# Expected: http://127.0.0.1:7878

# If not set
export ANTHROPIC_BASE_URL=http://127.0.0.1:7878
```
