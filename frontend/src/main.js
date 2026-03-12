import { getSessions, getRequests, getRequestDetail, connectEvents, deleteSession, toggleStar, setMemo, getInterceptStatus, toggleIntercept, forwardOriginal, forwardModified, rejectRequest } from './api.js'
import { projectColor, fmtTime, fmtTokens, fmtDuration, fmtBytes, esc, prettyJson, statusIcon } from './utils.js'

// ── State ─────────────────────────────────────────────────────────────────────
let sessions = []
let requests = []
let selectedSession = null        // null = all, '__starred__' = starred filter
let selectedRequest = null        // id string
let interceptEnabled = false
let searchQuery = ''
let searchDebounceTimer = null

// ── DOM ───────────────────────────────────────────────────────────────────────
document.querySelector('#app').innerHTML = `
<header class="header">
  <div class="status-dot"></div>
  <span class="header-title">Claude Code Inspector</span>
  <button class="intercept-toggle" id="interceptToggle" title="Toggle request interception">
    <span class="intercept-dot" id="interceptDot"></span>
    <span id="interceptLabel">Intercept</span>
  </button>
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
    <div class="search-bar">
      <input type="text" id="searchInput" class="search-input" placeholder="Search requests…" />
    </div>
    <div class="req-list" id="reqList"></div>
  </div>
  <div class="detail" id="detail">
    <div class="detail-empty">Select a request to inspect</div>
  </div>
</div>
`

const $interceptToggle = document.getElementById('interceptToggle')
const $interceptDot    = document.getElementById('interceptDot')
const $interceptLabel  = document.getElementById('interceptLabel')
const $hMeta       = document.getElementById('hMeta')
const $sessionList = document.getElementById('sessionList')
const $reqCount    = document.getElementById('reqCount')
const $reqList     = document.getElementById('reqList')
const $detail      = document.getElementById('detail')
const $searchInput = document.getElementById('searchInput')

$searchInput.addEventListener('input', () => {
  clearTimeout(searchDebounceTimer)
  searchDebounceTimer = setTimeout(() => {
    searchQuery = $searchInput.value.trim()
    loadRequests()
  }, 300)
})

// ── Copy button helper ─────────────────────────────────────────────────────────
function codeBlock(content, extraClass = 'cb-code') {
  return `<div class="code-wrap"><button class="copy-btn">Copy</button><pre class="code ${extraClass}">${content}</pre></div>`
}

$detail.addEventListener('click', e => {
  const btn = e.target.closest('.copy-btn')
  if (!btn) return
  const pre = btn.closest('.code-wrap')?.querySelector('pre')
  if (!pre) return
  const text = pre.textContent
  navigator.clipboard.writeText(text).catch(() => {
    const ta = document.createElement('textarea')
    ta.value = text
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  })
  btn.textContent = 'Copied!'
  btn.classList.add('copied')
  setTimeout(() => { btn.textContent = 'Copy'; btn.classList.remove('copied') }, 1500)
})

// ── Intercept toggle ──────────────────────────────────────────────────────────
function renderInterceptToggle() {
  $interceptDot.className = `intercept-dot ${interceptEnabled ? 'on' : 'off'}`
  $interceptLabel.textContent = interceptEnabled ? 'Intercept ON' : 'Intercept'
  $interceptToggle.classList.toggle('active', interceptEnabled)
}

$interceptToggle.addEventListener('click', async () => {
  const res = await toggleIntercept()
  interceptEnabled = res.enabled
  renderInterceptToggle()
})

// ── Render ────────────────────────────────────────────────────────────────────
function renderSessions() {
  const active = sessions.filter(s => s.pending_count > 0).length
  $hMeta.textContent = `${sessions.length} session${sessions.length !== 1 ? 's' : ''}${active ? ` · ${active} active` : ''}`

  const starredClass = selectedSession === '__starred__' ? 'session-item selected' : 'session-item'
  const allClass     = selectedSession === null ? 'session-item selected' : 'session-item'
  let html = `
  <div class="${starredClass}" data-sid="__starred__">
    <div class="session-name"><span style="color:var(--yellow)">★</span> Starred</div>
  </div>
  <div class="${allClass}" data-sid="">
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
        <button class="session-del-btn" data-del-sid="${s.id}" title="Delete session">✕</button>
      </div>
      <div class="session-cwd">${esc(s.cwd || '')}</div>
      <div class="session-stats">${s.request_count} req${tokS}</div>
    </div>`
  }

  const sessionScroll = $sessionList.scrollTop
  $sessionList.innerHTML = html
  $sessionList.scrollTop = sessionScroll

  // Session select
  $sessionList.querySelectorAll('[data-sid]').forEach(el => {
    el.addEventListener('click', (e) => {
      if (e.target.closest('[data-del-sid]')) return  // handled below
      selectedSession = el.dataset.sid || null
      if (selectedSession === '') selectedSession = null
      renderSessions()
      loadRequests()
    })
  })

  // Session delete buttons
  $sessionList.querySelectorAll('[data-del-sid]').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation()
      const sid = btn.dataset.delSid
      if (!confirm('Delete this session and all its requests?')) return
      await deleteSession(sid)
      if (selectedSession === sid) { selectedSession = null }
      await loadSessions()
      await loadRequests()
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
    const shortId = r.id.slice(0, 8)
    html += `<div class="${sel ? 'req-item selected' : 'req-item'}" data-rid="${r.id}">
      <div class="req-top">
        <span class="req-id" title="${r.id}">#${shortId}</span>
        ${r.agent_type && r.agent_type !== 'main' ? `<span class="agent-badge agent-${r.agent_type}">${r.agent_type}</span>` : ''}
        <span class="badge ${color}">${esc(proj || 'unknown')}</span>
        <span class="req-time">${fmtTime(r.timestamp)}</span>
        <button class="star-btn ${r.starred ? 'starred' : ''}" data-star-rid="${r.id}" title="${r.starred ? 'Unstar' : 'Star'}">${r.starred ? '★' : '☆'}</button>
      </div>
      <div class="req-bottom">
        ${statusIcon(r.status)}
        ${tok ? `<span>${tok}</span>` : ''}
        ${dur ? `<span title="Response time">${dur}</span>` : ''}
        ${r.is_streaming ? '<span class="req-tag streaming" title="Server-Sent Events streaming response">stream</span>' : '<span class="req-tag" title="Single JSON response">json</span>'}
        ${r.memo ? `<span class="req-memo" title="${esc(r.memo)}">${esc(r.memo)}</span>` : ''}
      </div>
    </div>`
  }
  const reqScroll = $reqList.scrollTop
  $reqList.innerHTML = html
  $reqList.scrollTop = reqScroll
  $reqList.querySelectorAll('[data-rid]').forEach(el => {
    el.addEventListener('click', (e) => {
      if (e.target.closest('[data-star-rid]')) return
      selectedRequest = el.dataset.rid
      renderRequests()
      loadDetail(selectedRequest)
    })
  })
  $reqList.querySelectorAll('[data-star-rid]').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation()
      const rid = btn.dataset.starRid
      const res = await toggleStar(rid)
      const req = requests.find(r => r.id === rid)
      if (req) req.starred = res.starred
      renderRequests()
    })
  })
}

/** Parse ALL content blocks from raw SSE in stream order. Returns [{type, text?, name?, input?}] */
function parseSseBlocks(rawSse) {
  const blocks = {}
  let usage = {}
  for (const line of rawSse.split('\n')) {
    if (!line.startsWith('data: ')) continue
    try {
      const d = JSON.parse(line.slice(6))
      if (d.type === 'message_start') {
        const u = d.message?.usage || {}
        usage = { ...u }
      }
      if (d.type === 'message_delta' && d.usage) {
        Object.assign(usage, d.usage)
      }
      if (d.type === 'content_block_start') {
        blocks[d.index] = { type: d.content_block.type, name: d.content_block.name, _buf: '' }
      }
      if (d.type === 'content_block_delta') {
        const b = blocks[d.index]
        if (!b) continue
        if (d.delta.type === 'text_delta')       b._buf += d.delta.text || ''
        if (d.delta.type === 'input_json_delta') b._buf += d.delta.partial_json || ''
      }
    } catch (_) {}
  }
  const ordered = Object.entries(blocks)
    .sort(([a], [b]) => Number(a) - Number(b))
    .map(([, b]) => {
      if (b.type === 'text')     return { type: 'text', text: b._buf }
      if (b.type === 'tool_use') {
        let input = {}
        try { input = JSON.parse(b._buf) } catch (_) { input = b._buf }
        return { type: 'tool_use', name: b.name, input }
      }
      return { type: b.type, raw: b._buf }
    })
  return { blocks: ordered, usage }
}

/** Render a single content block item from a message's content array */
function renderContentBlock(c) {
  if (typeof c === 'string') return codeBlock(esc(c))
  switch (c.type) {
    case 'text':
      return codeBlock(esc(c.text || ''))
    case 'tool_use':
      return `<div class="cb-tool-use">
        <div class="cb-label tool-use-label">call · ${esc(c.name)}</div>
        ${codeBlock(esc(JSON.stringify(c.input || {}, null, 2)))}
      </div>`
    case 'tool_result': {
      const body = Array.isArray(c.content)
        ? c.content.map(x => x.text || JSON.stringify(x)).join('\n')
        : (c.content || '')
      return `<div class="cb-tool-result">
        <div class="cb-label tool-result-label">result · ${esc(c.tool_use_id || '')}</div>
        ${codeBlock(esc(body))}
      </div>`
    }
    default:
      return codeBlock(esc(JSON.stringify(c, null, 2)))
  }
}

/** Render one message turn */
function renderMsgBlock(m, idx, time) {
  const contentHtml = typeof m.content === 'string'
    ? codeBlock(esc(m.content))
    : Array.isArray(m.content)
      ? m.content.map(renderContentBlock).join('')
      : codeBlock(esc(JSON.stringify(m.content, null, 2)))
  const meta = idx != null ? `<span class="msg-num">#${idx + 1}${time ? ' · ' + time : ''}</span>` : ''
  return `<div class="msg-block"><div class="msg-role ${m.role}">${meta}${m.role}</div>${contentHtml}</div>`
}

function renderDetail(req, prevMessageCount = 0, msgTimestamps = []) {
  const proj  = sessions.find(s => s.id === req.session_id)?.project_name
  const color = projectColor(proj || 'unknown')

  let reqBody = {}, messages = [], model = '', systemMsg = null
  try {
    reqBody = JSON.parse(req.request_body)
    messages = reqBody.messages || []
    model = reqBody.model || ''
    systemMsg = reqBody.system || null
  } catch (_) {}

  // Parse response
  let sseBlocks = [], sseUsage = {}, rawRespJson = null
  if (req.response_body) {
    try {
      const rb = JSON.parse(req.response_body)
      if (rb.raw_sse) {
        const parsed = parseSseBlocks(rb.raw_sse)
        sseBlocks = parsed.blocks
        sseUsage  = parsed.usage
      } else {
        rawRespJson = rb
      }
    } catch (_) {}
  }

  // Build response content HTML from SSE blocks
  let respContentHtml = sseBlocks.map(b => {
    if (b.type === 'text') {
      return `<div class="msg-block">
        <div class="msg-role assistant">text</div>
        ${codeBlock(esc(b.text))}
      </div>`
    }
    if (b.type === 'tool_use') {
      return `<div class="msg-block">
        <div class="cb-label tool-use-label">call · ${esc(b.name)}</div>
        ${codeBlock(esc(JSON.stringify(b.input, null, 2)))}
      </div>`
    }
    return `<div class="msg-block">${codeBlock(esc(JSON.stringify(b, null, 2)))}</div>`
  }).join('')

  if (!respContentHtml && rawRespJson) {
    respContentHtml = codeBlock(esc(JSON.stringify(rawRespJson, null, 2)))
  }
  if (!respContentHtml) {
    respContentHtml = `<div class="empty-msg" style="padding:12px 0">${req.status === 'pending' ? 'Waiting…' : 'No content'}</div>`
  }

  // Usage stats from SSE (may include cache tokens)
  const cacheIn  = sseUsage.cache_read_input_tokens
  const cacheNew = sseUsage.cache_creation_input_tokens
  const cacheHtml = (cacheIn || cacheNew) ? `
    <div class="meta-row">
      <span class="meta-label">Cache read</span>
      <span class="meta-val">${cacheIn ?? 0}</span>
    </div>
    <div class="meta-row">
      <span class="meta-label">Cache write</span>
      <span class="meta-val">${cacheNew ?? 0}</span>
    </div>` : ''

  // curl command
  let hdrs = {}
  try { hdrs = JSON.parse(req.request_headers) } catch (_) {}
  const hArgs = Object.entries(hdrs).map(([k, v]) => `  -H "${k}: ${v}"`).join(' \\\n')
  const curl  = `curl -X ${req.method} http://localhost:7878${req.path} \\\n${hArgs} \\\n  -H "x-api-key: $ANTHROPIC_API_KEY" \\\n  -d '${req.request_body.replace(/'/g, "'\\''")}'`

  const statusClass = req.status === 'complete' ? 'status-ok' : req.status === 'error' ? 'status-err' : req.status === 'intercepted' ? 'status-intercept' : req.status === 'rejected' ? 'status-err' : 'status-pending'
  const systemBlock = systemMsg ? `<div class="msg-block">
    <div class="msg-role system">system</div>
    ${codeBlock(esc(typeof systemMsg === 'string' ? systemMsg : JSON.stringify(systemMsg, null, 2)))}
  </div>` : ''

  // Build response column content based on status
  let responseColumnHtml
  if (req.status === 'intercepted') {
    const prettyBody = (() => {
      try { return JSON.stringify(JSON.parse(req.request_body), null, 2) } catch { return req.request_body }
    })()
    responseColumnHtml = `
      <div class="split-header">Intercepted — Edit & Forward</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="status-intercept" style="font-weight:500">⏸ intercepted</span></div>
        <div class="intercept-editor">
          <textarea id="interceptBody" class="intercept-textarea" spellcheck="false">${esc(prettyBody)}</textarea>
          <div class="intercept-actions">
            <button class="btn" id="btnForwardOriginal">Forward Original</button>
            <button class="btn btn-primary" id="btnForwardModified">Forward Modified</button>
            <button class="btn btn-danger" id="btnReject">Reject</button>
          </div>
        </div>
      </div>`
  } else {
    responseColumnHtml = `
      <div class="split-header">Response</div>
      <div class="split-body">
        <div class="meta-row"><span class="meta-label">Status</span><span class="${statusClass}" style="font-weight:500">${req.response_status ?? '-'} ${req.status}</span></div>
        <div class="meta-row"><span class="meta-label">Output tokens</span><span class="meta-val">${req.output_tokens ?? '-'}</span></div>
        ${cacheHtml}
        <div class="meta-row"><span class="meta-label">Duration</span><span class="meta-val">${fmtDuration(req.duration_ms) || '-'}</span></div>
        ${respContentHtml}
      </div>`
  }

  $detail.innerHTML = `
    <div class="detail-topbar">
      <span class="req-id" title="${req.id}">#${req.id.slice(0, 8)}</span>
      ${req.agent_type && req.agent_type !== 'main' ? `<span class="agent-badge agent-${req.agent_type}">${req.agent_type}</span>` : ''}
      <span class="badge ${color}">${esc(proj || 'unknown')}</span>
      <span class="detail-method">${req.method} ${req.path}</span>
      <span class="detail-time">${fmtTime(req.timestamp)}</span>
      <button class="btn btn-sm" id="copyCurl">Copy curl</button>
    </div>

    <div class="split-pane">
      <div class="split-col">
        <div class="split-header">Request</div>
        <div class="split-body">
          <div class="meta-row"><span class="meta-label">Model</span><span class="meta-val">${esc(model || '-')}</span></div>
          <div class="meta-row"><span class="meta-label">Input tokens</span><span class="meta-val">${req.input_tokens ?? '-'}</span></div>
          ${systemBlock}
          ${prevMessageCount > 0 ? `<div class="prev-messages-toggle" id="prevMsgToggle">${prevMessageCount} previous messages hidden — click to show</div><div id="prevMsgContainer" class="prev-messages hidden">${messages.slice(0, prevMessageCount).map((m, i) => renderMsgBlock(m, i, fmtTime(msgTimestamps[i]))).join('')}</div>` : ''}
          ${(prevMessageCount > 0 ? messages.slice(prevMessageCount) : messages).map((m, i) => { const gi = prevMessageCount > 0 ? prevMessageCount + i : i; return renderMsgBlock(m, gi, fmtTime(msgTimestamps[gi])) }).join('') || '<div class="empty-msg" style="padding:12px 0">No messages</div>'}
        </div>
      </div>

      <div class="split-divider"></div>

      <div class="split-col">
        ${responseColumnHtml}
      </div>
    </div>

    <div class="memo-section">
      <div class="memo-header">Memo</div>
      ${req.memo ? `<div class="memo-display" id="memoDisplay">${esc(req.memo)}<button class="memo-delete-btn" id="memoDeleteBtn" title="Delete memo">&times;</button></div>` : ''}
      <div class="memo-form">
        <input type="text" id="memoInput" class="memo-input" placeholder="Write a memo…" value="${esc(req.memo || '')}" />
        <button class="btn btn-sm" id="memoSaveBtn">Save</button>
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

  // Previous messages toggle
  const prevToggle = document.getElementById('prevMsgToggle')
  if (prevToggle) {
    prevToggle.addEventListener('click', () => {
      const container = document.getElementById('prevMsgContainer')
      const hidden = container.classList.toggle('hidden')
      prevToggle.textContent = hidden
        ? `${prevMessageCount} previous messages hidden — click to show`
        : `${prevMessageCount} previous messages — click to hide`
    })
  }

  // Memo save
  const memoInput = document.getElementById('memoInput')
  const memoSaveBtn = document.getElementById('memoSaveBtn')
  const memoDisplay = document.getElementById('memoDisplay')
  const saveMemo = async () => {
    const val = memoInput.value.trim()
    memoSaveBtn.disabled = true
    memoSaveBtn.textContent = 'Saving…'
    await setMemo(req.id, val)
    memoSaveBtn.disabled = false
    memoSaveBtn.textContent = 'Save'
    memoInput.value = ''
    // Update display
    if (memoDisplay) {
      memoDisplay.textContent = val
      memoDisplay.style.display = val ? '' : 'none'
    } else if (val) {
      const d = document.createElement('div')
      d.className = 'memo-display'
      d.id = 'memoDisplay'
      d.textContent = val
      document.querySelector('.memo-form').before(d)
    }
    // Update request list memo badge
    const r = requests.find(r => r.id === req.id)
    if (r) r.memo = val
    renderRequests()
  }
  memoSaveBtn.addEventListener('click', saveMemo)
  memoInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); saveMemo() }
  })

  // Memo delete
  const memoDeleteBtn = document.getElementById('memoDeleteBtn')
  if (memoDeleteBtn) {
    memoDeleteBtn.addEventListener('click', async () => {
      await setMemo(req.id, '')
      const r = requests.find(r => r.id === req.id)
      if (r) r.memo = ''
      renderRequests()
      const d = document.getElementById('memoDisplay')
      if (d) d.remove()
    })
  }

  // Intercept action buttons
  if (req.status === 'intercepted') {
    const refreshAfterAction = async () => {
      // Wait briefly for proxy to complete upstream round-trip
      await new Promise(r => setTimeout(r, 500))
      await loadRequests()
      await loadDetail(req.id)
    }
    document.getElementById('btnForwardOriginal').addEventListener('click', async (e) => {
      e.target.disabled = true
      e.target.textContent = 'Forwarding…'
      await forwardOriginal(req.id)
      await refreshAfterAction()
    })
    document.getElementById('btnForwardModified').addEventListener('click', async (e) => {
      e.target.disabled = true
      e.target.textContent = 'Forwarding…'
      const body = document.getElementById('interceptBody').value
      await forwardModified(req.id, body)
      await refreshAfterAction()
    })
    document.getElementById('btnReject').addEventListener('click', async (e) => {
      e.target.disabled = true
      e.target.textContent = 'Rejecting…'
      await rejectRequest(req.id)
      await refreshAfterAction()
    })
  }
}

// ── Data loading ──────────────────────────────────────────────────────────────
async function loadSessions() {
  sessions = await getSessions()
  renderSessions()
}

async function loadRequests() {
  if (selectedSession === '__starred__') {
    requests = await getRequests(null, { starred: true })
  } else {
    requests = await getRequests(selectedSession, { search: searchQuery })
  }
  renderRequests()
}

async function loadDetail(id) {
  const req = await getRequestDetail(id)

  // Build per-message timestamp map by tracing session history.
  // Each message index gets the timestamp of the request that first introduced it.
  let prevMessageCount = 0
  const msgTimestamps = [] // msgTimestamps[i] = timestamp string for message i

  let currentMessages = []
  try { currentMessages = JSON.parse(req.request_body).messages || [] } catch {}
  const totalMessages = currentMessages.length

  if (req.session_id) {
    // Collect same-session requests older than (and including) current, sorted oldest→newest
    const idx = requests.findIndex(r => r.id === id)
    if (idx >= 0) {
      const sessionReqs = requests
        .slice(idx)  // idx and older (list is desc by time)
        .filter(r => r.session_id === req.session_id)
        .reverse()   // now oldest first

      // Walk through each request to find when each message index first appeared
      let prevCount = 0
      for (const sr of sessionReqs) {
        let detail, msgCount
        if (sr.id === id) {
          // Current request — no extra fetch needed
          msgCount = totalMessages
          detail = req
        } else {
          try {
            detail = await getRequestDetail(sr.id)
            const body = JSON.parse(detail.request_body)
            msgCount = (body.messages || []).length
          } catch { continue }
        }
        // Messages from prevCount to msgCount-1 were introduced by this request
        for (let i = prevCount; i < msgCount; i++) {
          msgTimestamps[i] = detail.timestamp
        }
        prevCount = msgCount
      }

      // prevMessageCount = messages from the request just before this one
      const prevReq = requests.slice(idx + 1).find(r => r.session_id === req.session_id)
      if (prevReq) {
        try {
          const prevDetail = await getRequestDetail(prevReq.id)
          const prevBody = JSON.parse(prevDetail.request_body)
          prevMessageCount = (prevBody.messages || []).length
        } catch {}
      }
    }
  }

  // Fill any gaps (e.g. first request in session, or no history)
  for (let i = 0; i < totalMessages; i++) {
    if (!msgTimestamps[i]) msgTimestamps[i] = req.timestamp
  }

  renderDetail(req, prevMessageCount, msgTimestamps)
}

// ── Init ──────────────────────────────────────────────────────────────────────
async function init() {
  // Load intercept status
  try {
    const res = await getInterceptStatus()
    interceptEnabled = res.enabled
    renderInterceptToggle()
  } catch (_) {}

  await loadSessions()
  await loadRequests()

  connectEvents((e) => {
    loadSessions()
    loadRequests()

    let eventRequestId = null
    try { eventRequestId = JSON.parse(e.data)?.data?.id } catch (_) {}

    // Auto-select intercepted requests
    if (e.type === 'request_intercepted' && eventRequestId) {
      selectedRequest = eventRequestId
      loadDetail(selectedRequest)
      return
    }
    // Only reload detail if the event is about the selected request
    if (selectedRequest && eventRequestId === selectedRequest) {
      loadDetail(selectedRequest)
    }
  })
}

init()
