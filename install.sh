#!/usr/bin/env bash
# install.sh — Claude Code LLM API Inspector 자동 설치 스크립트
# 사용법: bash install.sh

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DEST="$HOME/.local/bin/claude-code-hook"
OS="$(uname -s)"
SHELL_RC=""

# ── 색상 출력 헬퍼 ─────────────────────────────────────────────────────────────
green()  { echo -e "\033[32m$*\033[0m"; }
yellow() { echo -e "\033[33m$*\033[0m"; }
red()    { echo -e "\033[31m$*\033[0m"; }
step()   { echo; green "▶ $*"; }

# ── 전제 조건 확인 ─────────────────────────────────────────────────────────────
step "전제 조건 확인"

if ! command -v cargo &>/dev/null; then
  red "Rust가 설치되어 있지 않습니다."
  echo "  설치: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  exit 1
fi
green "  ✓ Rust $(rustc --version | awk '{print $2}')"

if ! command -v cc &>/dev/null; then
  red "C 컴파일러가 없습니다."
  if [[ "$OS" == "Linux" ]]; then
    echo "  설치: sudo apt-get install build-essential"
  else
    echo "  설치: xcode-select --install"
  fi
  exit 1
fi
green "  ✓ C compiler ($(cc --version | head -1))"

if [[ "$OS" == "Linux" ]]; then
  # Check for required Linux packages
  if ! pkg-config --exists openssl 2>/dev/null; then
    red "libssl-dev가 없습니다."
    echo "  설치: sudo apt-get install pkg-config libssl-dev"
    exit 1
  fi
  green "  ✓ OpenSSL dev"
fi

if ! command -v node &>/dev/null; then
  yellow "  ⚠ Node.js가 없습니다. 프론트엔드 빌드를 건너뜁니다 (폴백 UI 사용)."
  BUILD_FRONTEND=false
else
  green "  ✓ Node.js $(node --version)"
  BUILD_FRONTEND=true
fi

# ── 프론트엔드 빌드 ───────────────────────────────────────────────────────────
if [ "$BUILD_FRONTEND" = true ]; then
  step "프론트엔드 빌드 (Vite)"
  cd "$REPO_DIR/frontend"
  npm install --silent
  npm run build --silent
  cd "$REPO_DIR"
  green "  ✓ 빌드 완료 → src/assets/dist/"
fi

# ── Rust 바이너리 빌드 ────────────────────────────────────────────────────────
step "Rust 바이너리 빌드 (release)"
cd "$REPO_DIR"
cargo build --release --quiet
green "  ✓ 빌드 완료 → target/release/claude-code-hook"

# ── 바이너리 설치 ─────────────────────────────────────────────────────────────
step "바이너리 설치 → $BIN_DEST"
mkdir -p "$HOME/.local/bin"
cp "$REPO_DIR/target/release/claude-code-hook" "$BIN_DEST"
chmod +x "$BIN_DEST"
green "  ✓ 설치 완료"

# ── 쉘 설정 파일 결정 ──────────────────────────────────────────────────────────
detect_shell_rc() {
  if [ -f "$HOME/.zshrc" ]; then
    echo "$HOME/.zshrc"
  elif [ -f "$HOME/.bashrc" ]; then
    echo "$HOME/.bashrc"
  elif [ -f "$HOME/.bash_profile" ]; then
    echo "$HOME/.bash_profile"
  else
    echo "$HOME/.bashrc"
  fi
}

# PATH에 ~/.local/bin 추가 (없는 경우)
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  SHELL_RC="$(detect_shell_rc)"
  yellow "  ⚠ $HOME/.local/bin 이 PATH에 없습니다. 쉘 설정 파일에 추가합니다."
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_RC"
  green "  ✓ PATH 추가 → $SHELL_RC"
fi

# ── ANTHROPIC_BASE_URL 영구 설정 ──────────────────────────────────────────────
step "ANTHROPIC_BASE_URL 영구 설정"
if [ -z "$SHELL_RC" ]; then
  SHELL_RC="$(detect_shell_rc)"
fi

if grep -q "ANTHROPIC_BASE_URL" "$SHELL_RC" 2>/dev/null; then
  yellow "  ⚠ ANTHROPIC_BASE_URL 이미 설정되어 있음 (건너뜀)"
else
  echo 'export ANTHROPIC_BASE_URL=http://127.0.0.1:7878' >> "$SHELL_RC"
  green "  ✓ ANTHROPIC_BASE_URL=http://127.0.0.1:7878 → $SHELL_RC"
fi

# ── 자동 시작 등록 ─────────────────────────────────────────────────────────────
if [[ "$OS" == "Darwin" ]]; then
  # macOS: LaunchAgent
  step "macOS LaunchAgent 등록 (로그인 시 자동 시작)"
  PLIST="$HOME/Library/LaunchAgents/com.claude-code-inspector.plist"
  mkdir -p "$HOME/Library/LaunchAgents"
  cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.claude-code-inspector</string>
    <key>ProgramArguments</key>
    <array>
        <string>$BIN_DEST</string>
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

  launchctl unload "$PLIST" 2>/dev/null || true
  launchctl load "$PLIST"
  green "  ✓ LaunchAgent 등록 및 시작"

elif [[ "$OS" == "Linux" ]]; then
  # Linux: systemd user service
  step "systemd user service 등록 (자동 시작)"
  SYSTEMD_DIR="$HOME/.config/systemd/user"
  SERVICE_FILE="$SYSTEMD_DIR/claude-code-inspector.service"
  mkdir -p "$SYSTEMD_DIR"
  cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=Claude Code LLM API Inspector
After=network.target

[Service]
ExecStart=$BIN_DEST
Restart=always
RestartSec=5
Environment=RUST_LOG=claude_code_hook=info,warn

[Install]
WantedBy=default.target
EOF

  systemctl --user daemon-reload
  systemctl --user enable claude-code-inspector.service
  systemctl --user restart claude-code-inspector.service
  green "  ✓ systemd service 등록 및 시작"
  echo "    상태 확인: systemctl --user status claude-code-inspector"
  echo "    로그 확인: journalctl --user -u claude-code-inspector -f"
fi

# ── MCP 서버 등록 ─────────────────────────────────────────────────────────────
step "Claude Code MCP 서버 등록"
if command -v claude &>/dev/null; then
  claude mcp add claude-inspector -- "$BIN_DEST" mcp 2>/dev/null && \
    green "  ✓ MCP 등록 완료 (claude-inspector)" || \
    yellow "  ⚠ MCP 등록 실패 (이미 등록돼 있거나 claude CLI 버전 문제)"
else
  yellow "  ⚠ claude CLI를 찾을 수 없습니다. 아래 명령으로 수동 등록하세요:"
  echo "     claude mcp add claude-inspector -- $BIN_DEST mcp"
fi

# ── 완료 ──────────────────────────────────────────────────────────────────────
echo
green "══════════════════════════════════════════════"
green "  설치 완료!"
green "══════════════════════════════════════════════"
echo
echo "  Proxy:     http://127.0.0.1:7878"
echo "  Dashboard: http://127.0.0.1:7879"
echo
yellow "  새 터미널을 열거나 아래를 실행하세요:"
echo "    source $SHELL_RC"
echo
if [[ "$OS" == "Darwin" ]]; then
  echo "  대시보드 열기:"
  echo "    open http://127.0.0.1:7879"
elif [[ "$OS" == "Linux" ]]; then
  echo "  대시보드 열기:"
  echo "    xdg-open http://127.0.0.1:7879  # 또는 브라우저에서 직접 접속"
fi
echo
