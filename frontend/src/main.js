import { getSessions, getRequests, getRequestDetail, pollEvents } from './api.js'
import { projectColor, fmtTime, fmtTokens, fmtDuration, esc, prettyJson, statusIcon } from './utils.js'

// ── State ─────────────────────────────────────────────────────────────────────
let sessions = []
let requests = []
let selectedSession = null   // null = all
let selectedRequest = null   // id string

// ── DOM ───────────────────────────────────────────────────────────────────────
document.querySelector('#app').innerHTML = `
<header class="header">
  <div class="status-dot"></div>
  <span class="header-title">Claude Code Inspector</span>
  <span class="header-meta" id="hMeta">Loading…</span>
</header>
<div class="layout">
  <nav class="sidebar">
    <div class="sidebar-section">Sessions</div>
    <div id="sessionList"></div>
  </nav>
  <div class="req-panel">
    <div class="req-panel-header">
      Requests <span id="reqCount" style="font-weight:400;color:var(--text-muted)"></span>
    </div>
    <div class="req-list" id="reqList"></div>
  </div>
  <div class="detail" id="detail">
    <div class="detail-empty">Select a request to inspect</div>
  </div>
</div>
`

const $hMeta       = document.getElementById('hMeta')
const $sessionList = document.getElementById('sessionList')
const $reqCount    = document.getElementById('reqCount')
const $reqList     = document.getElementById('reqList')
const $detail      = document.getElementById('detail')

// ── Render ────────────────────────────────────────────────────────────────────
function renderSessions() {
  const active = sessions.filter(s => s.pending_count > 0).length
  $hMeta.textContent = `${sessions.length} session${sessions.length !== 1 ? 's' : ''}${active ? ` · ${active} active` : ''}`

  const allClass = selectedSession === null ? 'session-item selected' : 'session-item'
  let html = `<div class="${allClass}" data-sid="">
    <div class="session-name"><span class="sdot live"></span>All sessions</div>
  </div>`

  for (const s of sessions) {
    const live = s.pending_count > 0
    const sel  = selectedSession === s.id
    const tok  = s.total_input_tokens + s.total_output_tokens
    const tokS = tok > 0 ? ` · ${tok >= 1000 ? (tok/1000).toFixed(1)+'k' : tok} tok` : ''
    html += `<div class="${sel ? 'session-item selected' : 'session-item'}" data-sid="${s.id}">
      <div class="session-name">
        <span class="sdot ${live ? 'live' : 'idle'}"></span>
        ${esc(s.project_name || 'unknown')}
      </div>
      <div class="session-cwd">${esc(s.cwd || '')}</div>
      <div class="session-stats">${s.request_count} req${tokS}</div>
    </div>`
  }

  $sessionList.innerHTML = html
  $sessionList.querySelectorAll('[data-sid]').forEach(el => {
    el.addEventListener('click', () => {
      selectedSession = el.dataset.sid || null
      renderSessions()
      loadRequests()
    })
  })
}

function renderRequests() {
  $reqCount.textContent = requests.length ? `(${requests.length})` : ''
  if (!requests.length) {
    $reqList.innerHTML = '<div class="empty-msg">No requests yet</div>'
    return
  }
  let html = ''
  for (const r of requests) {
    const proj  = sessions.find(s => s.id === r.session_id)?.project_name
    const color = projectColor(proj || 'unknown')
    const tok   = fmtTokens(r.input_tokens, r.output_tokens)
    const dur   = fmtDuration(r.duration_ms)
    const sel   = r.id === selectedRequest
    html += `<div class="${sel ? 'req-item selected' : 'req-item'}" data-rid="${r.id}">
      <div class="req-top">
        <span class="badge ${color}">${esc(proj || 'unknown')}</span>
        <span class="req-time">${fmtTime(r.timestamp)}</span>
      </div>
      <div class="req-bottom">
        ${statusIcon(r.status)}
        ${tok ? `<span>${tok}</span>` : ''}
        ${dur ? `<span>${dur}</span>` : ''}
        ${r.is_streaming ? '<span title="streaming" style="color:var(--text-muted)">~sse</span>' : ''}
      </div>
    </div>`
  }
  $reqList.innerHTML = html
  $reqList.querySelectorAll('[data-rid]').forEach(el => {
    el.addEventListener('click', () => {
      selectedRequest = el.dataset.rid
      renderRequests()
      loadDetail(selectedRequest)
    })
  })
}

function renderDetail(req) {
  const proj  = sessions.find(s => s.id === req.session_id)?.project_name
  const color = projectColor(proj || 'unknown')

  let reqBody = {}
  let messages = []
  let model = ''
  let systemMsg = null
  try {
    reqBody = JSON.parse(req.request_body)
    messages = reqBody.messages || []
    model = reqBody.model || ''
    systemMsg = reqBody.system || null
  } catch (_) {}

  let respText = ''
  if (req.response_body) {
    try {
      const rb = JSON.parse(req.response_body)
      if (rb.accumulated_content) respText = rb.accumulated_content
      else if (Array.isArray(rb.content)) respText = rb.content.map(c => c.text || '').join('')
    } catch (_) { respText = req.response_body }
  }

  // curl command
  let hdrs = {}
  try { hdrs = JSON.parse(req.request_headers) } catch (_) {}
  const hArgs = Object.entries(hdrs).map(([k, v]) => `  -H "${k}: ${v}"`).join(' \\\n')
  const curl  = `curl -X ${req.method} http://localhost:7878${req.path} \\\n${hArgs} \\\n  -H "x-api-key: $ANTHROPIC_API_KEY" \\\n  -d '${req.request_body.replace(/'/g, "'\\''")}'`

  const statusClass = req.status === 'complete' ? 'status-ok' : req.status === 'error' ? 'status-err' : 'status-pending'

  // Build system message block if present
  const systemBlock = systemMsg ? `<div class="msg-block">
    <div class="msg-role system">system</div>
    <pre class="code">${esc(typeof systemMsg === 'string' ? systemMsg : JSON.stringify(systemMsg, null, 2))}</pre>
  </div>` : ''

  // Build conversation messages
  const msgBlocks = messages.map(m => `<div class="msg-block">
    <div class="msg-role ${m.role}">${m.role}</div>
    <pre class="code">${esc(typeof m.content === 'string' ? m.content : JSON.stringify(m.content, null, 2))}</pre>
  </div>`).join('')

  $detail.innerHTML = `
    <div class="detail-topbar">
      <span class="badge ${color}">${esc(proj || 'unknown')}</span>
      <span class="detail-method">${req.method} ${req.path}</span>
      <span class="detail-time">${fmtTime(req.timestamp)}</span>
      <button class="btn btn-sm" id="copyCurl">Copy curl</button>
    </div>

    <div class="split-pane">
      <div class="split-col">
        <div class="split-header">Request</div>
        <div class="split-body">
          <div class="meta-row">
            <span class="meta-label">Model</span>
            <span class="meta-val">${esc(model || '-')}</span>
          </div>
          <div class="meta-row">
            <span class="meta-label">Input tokens</span>
            <span class="meta-val">${req.input_tokens ?? '-'}</span>
          </div>
          ${systemBlock}
          ${msgBlocks || '<div class="empty-msg" style="padding:12px 0">No messages</div>'}
        </div>
      </div>

      <div class="split-divider"></div>

      <div class="split-col">
        <div class="split-header">Response</div>
        <div class="split-body">
          <div class="meta-row">
            <span class="meta-label">Status</span>
            <span class="${statusClass}" style="font-weight:500">${req.response_status ?? '-'} ${req.status}</span>
          </div>
          <div class="meta-row">
            <span class="meta-label">Output tokens</span>
            <span class="meta-val">${req.output_tokens ?? '-'}</span>
          </div>
          <div class="meta-row">
            <span class="meta-label">Duration</span>
            <span class="meta-val">${fmtDuration(req.duration_ms) || '-'}</span>
          </div>
          <div class="meta-row">
            <span class="meta-label">Streaming</span>
            <span class="meta-val">${req.is_streaming ? 'yes' : 'no'}</span>
          </div>
          ${respText ? `<div class="msg-block" style="margin-top:12px">
            <div class="msg-role assistant">assistant</div>
            <pre class="code resp-code">${esc(respText)}</pre>
          </div>` : '<div class="empty-msg" style="padding:12px 0">No response yet</div>'}
        </div>
      </div>
    </div>
  `
  document.getElementById('copyCurl').addEventListener('click', () => {
    navigator.clipboard.writeText(curl).catch(() => {
      const ta = document.createElement('textarea')
      ta.value = curl
      document.body.appendChild(ta)
      ta.select()
      document.execCommand('copy')
      document.body.removeChild(ta)
    })
  })
}

// ── Data loading ──────────────────────────────────────────────────────────────
async function loadSessions() {
  sessions = await getSessions()
  renderSessions()
}

async function loadRequests() {
  requests = await getRequests(selectedSession)
  renderRequests()
}

async function loadDetail(id) {
  const req = await getRequestDetail(id)
  renderDetail(req)
}

// ── Init ──────────────────────────────────────────────────────────────────────
async function init() {
  await loadSessions()
  await loadRequests()

  pollEvents(() => {
    loadSessions()
    loadRequests()
    if (selectedRequest) loadDetail(selectedRequest)
  })
}

init()
