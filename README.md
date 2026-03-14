# Claude Code LLM API Inspector

A single-binary tool that **transparently intercepts all requests/responses** between Claude Code and the Anthropic API, stores them in SQLite, and lets you inspect them in real-time via a web dashboard.

```
Claude Code ──HTTP──▶ Proxy :7878 ──HTTPS──▶ api.anthropic.com
                           │
                      SQLite DB
               (~/.local/share/claude-code-hook/logs.db)
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
| 1 | **Request interceptor & editor** — Intercept a request mid-flight, modify the payload (model, messages, parameters), then forward the edited request to the upstream API | ✅ Done |
| 2 | **Multi-provider routing** — Route specific requests to a different LLM provider (OpenAI, Gemini, Mistral, etc.) based on rules such as model name, session, or request content | 🔲 Planned |
| 3 | **Supervisor agent** — A separate management agent that continuously reads the recorded request/response history and evaluates whether the ongoing session is heading in the right direction, flags missing steps, detects loops or regressions, and surfaces actionable feedback in real time | 🔲 Planned |

---

## Quick Install

```bash
git clone https://github.com/windowhan/claude-code-hook.git
cd claude-code-hook
bash install.sh
```

The script auto-detects your OS (macOS / Linux) and handles everything:

1. Frontend build (`npm install && npm run build`)
2. Rust binary build (`cargo build --release`)
3. Binary installation (`~/.local/bin/claude-code-hook`)
4. Permanent `ANTHROPIC_BASE_URL` setting (`~/.bashrc` or `~/.zshrc`)
5. Auto-start registration:
   - **Linux**: systemd user service (`systemctl --user`)
   - **macOS**: LaunchAgent (`launchctl`)
6. Claude Code MCP server registration

---

## Prerequisites

### Ubuntu / Debian

```bash
# Build essentials + OpenSSL dev
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev lsof curl

# Rust (1.75+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Node.js (18+) — optional, fallback UI used if absent
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt-get install -y nodejs

# Claude Code
npm install -g @anthropic-ai/claude-code
```

### macOS

| Tool | How to install |
|------|----------------|
| **Rust** (1.75+) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| **Node.js** (18+) | https://nodejs.org or `brew install node` |
| **Claude Code** | `npm install -g @anthropic-ai/claude-code` |

> Node.js is optional on both platforms. If absent, the build falls back to a single-file HTML UI.

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
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### 5. Set ANTHROPIC_BASE_URL permanently

```bash
echo 'export ANTHROPIC_BASE_URL=http://127.0.0.1:7878' >> ~/.bashrc
source ~/.bashrc
```

> zsh users: use `~/.zshrc` instead of `~/.bashrc`.

### 6. Start the server

**Manual:**
```bash
claude-code-hook
```

**Auto-start via systemd (Linux):**
```bash
mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/claude-code-inspector.service <<EOF
[Unit]
Description=Claude Code LLM API Inspector
After=network.target

[Service]
ExecStart=$HOME/.local/bin/claude-code-hook
Restart=always
RestartSec=5
Environment=RUST_LOG=claude_code_hook=info,warn

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable claude-code-inspector.service
systemctl --user start claude-code-inspector.service
```

Useful commands:
```bash
systemctl --user status claude-code-inspector    # 상태 확인
systemctl --user restart claude-code-inspector   # 재시작
journalctl --user -u claude-code-inspector -f    # 로그 확인
```

<details>
<summary><b>Auto-start via macOS LaunchAgent</b></summary>

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
</details>

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
# Linux
xdg-open http://127.0.0.1:7879

# macOS
open http://127.0.0.1:7879
```

### Layout

<img width="1725" height="1024" alt="image" src="https://github.com/user-attachments/assets/2a614bc3-7b97-46e3-bc23-4442b8dfe356" />


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

### Session identification

Multiple Claude Code processes running simultaneously are automatically separated into distinct sessions.

**Linux:**
1. TCP connection arrives at proxy → record client **source port**
2. `lsof -i :<port>` → find the **PID** using that port
3. `/proc/<PID>/cwd` (readlink) → get the process **working directory**
4. If an existing session with the same CWD exists, reuse it → subagents/background tasks are grouped
5. `basename(CWD)` → extract project name

**macOS:**
1. TCP connection → source port → `lsof` → PID
2. `lsof -a -p <PID> -d cwd` → working directory
3. Same CWD grouping + project name extraction

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

DB path:
- **Linux**: `~/.local/share/claude-code-hook/logs.db`
- **macOS**: `~/Library/Application Support/claude-code-hook/logs.db`

Direct query example:
```bash
sqlite3 ~/.local/share/claude-code-hook/logs.db \
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

# Linux: restart systemd service
systemctl --user restart claude-code-inspector

# macOS: restart LaunchAgent
launchctl unload ~/Library/LaunchAgents/com.claude-code-inspector.plist
launchctl load  ~/Library/LaunchAgents/com.claude-code-inspector.plist
```

### File structure

```
claude-code-hook/
├── install.sh                   # automated install script (macOS + Linux)
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
    ├── session.rs               # PID/CWD backtracking (Linux /proc + lsof)
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
# Linux: check systemd logs
journalctl --user -u claude-code-inspector --no-pager -n 50

# macOS: check logs
tail -f /tmp/claude-inspector.log

# Check port availability
ss -tlnp | grep -E '7878|7879'   # Linux
lsof -i :7878                     # macOS
```

**Restart service:**
```bash
# Linux
systemctl --user restart claude-code-inspector

# macOS
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
