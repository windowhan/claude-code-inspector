import { getSessions, getRequests, getRequestDetail, connectEvents, deleteSession, toggleStar, setMemo, getInterceptStatus, toggleIntercept, forwardOriginal, forwardModified, rejectRequest, getRoutingConfig, saveRoutingConfig, getRoutingRules, createRoutingRule, updateRoutingRule, deleteRoutingRule, reorderRoutingRules, testRoutingClassifier, getSessionSummary, getFileCoverage, getDetectPatterns, getFileTree, getFileContent, getFileRequests, getSupervisorConfig, saveSupervisorConfig, getSessionGoal, setSessionGoal, deleteSessionGoal, refineGoal, getSupervisorAnalyses } from './api.js'
import { projectColor, fmtTime, fmtTokens, fmtDuration, fmtBytes, esc, prettyJson, statusIcon } from './utils.js'

// ── State ─────────────────────────────────────────────────────────────────────
let sessions = []
let requests = []
let selectedSession = null        // null = all, '__starred__' = starred filter
let selectedRequest = null        // id string
let interceptEnabled = false
let searchQuery = ''
let sourceFilter = ''             // '' = all, 'claude_code', 'cursor'
let searchDebounceTimer = null
let routingPanelOpen = false
let routingConfig = null
let routingRules = []
let editingRuleId = null          // null = not editing, 'new' = adding new
let supervisorSessionId = null    // session ID being supervised (null = not showing)
let supervisorRefreshTimer = null
const debouncedSupervisorRefresh = (sid) => {
  clearTimeout(supervisorRefreshTimer)
  supervisorRefreshTimer = setTimeout(() => renderSupervisorPanel(sid), 2000)
}

// ── DOM ───────────────────────────────────────────────────────────────────────
document.querySelector('#app').innerHTML = `
<header class="header">
  <div class="status-dot"></div>
  <span class="header-title">Claude Code Inspector</span>
  <button class="intercept-toggle" id="interceptToggle" title="Toggle request interception">
    <span class="intercept-dot" id="interceptDot"></span>
    <span id="interceptLabel">Intercept</span>
  </button>
  <button class="routing-btn" id="routingBtn" title="Configure multi-provider routing">
    <span id="routingLabel">⇄ Routes</span>
  </button>
  <button class="supervisor-btn" id="supervisorBtn" title="Supervisor analysis for selected session">
    <span id="supervisorLabel">&#x1F50D; Supervisor</span>
  </button>
  <button class="code-btn" id="codeBtn" title="Code Viewer - browse files with request annotations">
    <span id="codeLabel">&#x1F4C4; Code</span>
  </button>
  <button class="prompt-view-btn" id="promptViewBtn" title="Switch to Request/Response view" style="display:none">
    <span>&#x1F4AC; Prompt View</span>
  </button>
  <button class="settings-btn" id="settingsBtn" title="Configure Summarizer LLM">&#x2699; Settings</button>
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
      <div class="source-filter" id="sourceFilter">
        <button class="src-btn active" data-src="">All</button>
        <button class="src-btn" data-src="claude_code">Claude Code</button>
        <button class="src-btn" data-src="cursor">Cursor</button>
      </div>
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
const $routingBtn      = document.getElementById('routingBtn')
const $routingLabel    = document.getElementById('routingLabel')
const $supervisorBtn   = document.getElementById('supervisorBtn')
const $supervisorLabel = document.getElementById('supervisorLabel')
const $codeBtn         = document.getElementById('codeBtn')
const $promptViewBtn   = document.getElementById('promptViewBtn')
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

document.getElementById('sourceFilter').addEventListener('click', e => {
  const btn = e.target.closest('.src-btn')
  if (!btn) return
  sourceFilter = btn.dataset.src
  document.querySelectorAll('.src-btn').forEach(b => b.classList.toggle('active', b.dataset.src === sourceFilter))
  loadRequests()
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

// ── Routing UI ────────────────────────────────────────────────────────────────
function renderRoutingBtn() {
  const enabledRuleCount = routingRules.filter(r => r.enabled).length
  const label = `⇄ Routes${enabledRuleCount > 0 ? ' · ' + enabledRuleCount : ''}`
  $routingLabel.textContent = label
  const isActive = routingConfig && routingConfig.enabled && routingRules.length > 0
  $routingBtn.classList.toggle('active', isActive)
}

$routingBtn.addEventListener('click', () => {
  routingPanelOpen = !routingPanelOpen
  if (routingPanelOpen) {
    renderRoutingPanel()
  } else {
    if (selectedRequest) {
      loadDetail(selectedRequest)
    } else {
      $detail.innerHTML = '<div class="detail-empty">Select a request to inspect</div>'
    }
  }
})

document.getElementById('settingsBtn').addEventListener('click', async () => {
  routingPanelOpen = false
  const [sumResp, supResp] = await Promise.all([
    fetch('/api/summarizer/config'),
    fetch('/api/supervisor/config'),
  ])
  const sumConfig = await sumResp.json()
  const supConfig = await supResp.json()

  const providers = {
    anthropic: { url: 'https://api.anthropic.com', models: [
      'claude-haiku-4-5-20251001', 'claude-sonnet-4-20250514', 'claude-opus-4-20250514',
      'claude-3-5-haiku-20241022', 'claude-3-5-sonnet-20241022',
    ]},
    openai: { url: 'https://api.openai.com', models: [
      'gpt-4o-mini', 'gpt-4o', 'gpt-4.1-mini', 'gpt-4.1', 'gpt-4.1-nano',
      'o4-mini', 'o3', 'o3-mini',
    ]},
    deepseek: { url: 'https://api.deepseek.com', models: [
      'deepseek-chat', 'deepseek-reasoner',
    ]},
    kimi: { url: 'https://api.moonshot.cn', models: [
      'moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k',
    ]},
  }
  const curSumProvider = sumConfig.provider || 'anthropic'
  const curSupProvider = supConfig.provider || 'anthropic'

  $detail.innerHTML = `
    <div class="sv-panel">
      <div class="sv-header">
        <span>Settings</span>
        <button class="btn btn-sm" id="settingsClose">Close</button>
      </div>
      <div style="display:flex;gap:8px;padding:8px 12px;border-bottom:1px solid var(--border)">
        <button class="btn btn-sm settings-tab-btn active" data-tab="summarizer" id="tabSummarizer">Summarizer LLM</button>
        <button class="btn btn-sm settings-tab-btn" data-tab="supervisor" id="tabSupervisor">Supervisor LLM</button>
      </div>

      <!-- Summarizer Tab -->
      <div id="tabContentSummarizer" class="sv-section settings-tab-content">
        <div class="meta-row"><span class="meta-label">Provider</span>
          <select class="memo-input" id="sumProvider" style="flex:1">
            <option value="anthropic" ${curSumProvider === 'anthropic' ? 'selected' : ''}>Anthropic (Claude)</option>
            <option value="openai" ${curSumProvider === 'openai' ? 'selected' : ''}>OpenAI (GPT)</option>
            <option value="deepseek" ${curSumProvider === 'deepseek' ? 'selected' : ''}>DeepSeek</option>
            <option value="kimi" ${curSumProvider === 'kimi' ? 'selected' : ''}>Kimi (Moonshot)</option>
          </select>
        </div>
        <div class="meta-row"><span class="meta-label">API Endpoint</span><input type="text" class="memo-input" id="sumBaseUrl" value="${esc(sumConfig.base_url || providers[curSumProvider].url)}" style="flex:1"></div>
        <div class="meta-row"><span class="meta-label">API Key</span><input type="text" class="memo-input" id="sumApiKey" value="${esc(sumConfig.api_key || '')}" placeholder="Enter API key" style="flex:1"></div>
        <div class="meta-row"><span class="meta-label">Model</span>
          <select class="memo-input" id="sumModel" style="flex:1">
            ${providers[curSumProvider].models.map(m => `<option value="${m}" ${m === (sumConfig.model || '') ? 'selected' : ''}>${m}</option>`).join('')}
          </select>
          <button class="btn btn-sm" id="sumFetchModels" style="margin-left:6px">Fetch</button>
        </div>
        <div class="meta-row"><span class="meta-label">Language</span>
          <select class="memo-input" id="sumLanguage" style="flex:1">
            ${['English','Korean','Japanese','Chinese','Spanish','German','French','Russian','Portuguese'].map(l =>
              `<option value="${l}" ${l === (sumConfig.language || 'English') ? 'selected' : ''}>${l}</option>`
            ).join('')}
          </select>
        </div>
        <div style="margin-top:12px"><button class="btn btn-primary" id="sumSave">Save</button> <span id="sumStatus" style="font-size:12px;color:var(--text-muted)"></span></div>
      </div>

      <!-- Supervisor Tab -->
      <div id="tabContentSupervisor" class="sv-section settings-tab-content" style="display:none">
        <div class="meta-row" style="margin-bottom:8px">
          <span class="meta-label">Enabled</span>
          <label style="display:flex;align-items:center;gap:6px;cursor:pointer">
            <input type="checkbox" id="supEnabled" ${supConfig.enabled ? 'checked' : ''}>
            <span id="supStatusLabel" style="font-size:12px;color:${supConfig.enabled && supConfig.api_key ? 'var(--green)' : 'var(--text-muted)'}">
              ${supConfig.enabled && supConfig.api_key ? 'Active' : 'Inactive'}
            </span>
          </label>
        </div>
        <div class="meta-row"><span class="meta-label">Provider</span>
          <select class="memo-input" id="supProvider" style="flex:1">
            <option value="anthropic" ${curSupProvider === 'anthropic' ? 'selected' : ''}>Anthropic (Claude)</option>
            <option value="openai" ${curSupProvider === 'openai' ? 'selected' : ''}>OpenAI (GPT)</option>
            <option value="deepseek" ${curSupProvider === 'deepseek' ? 'selected' : ''}>DeepSeek</option>
            <option value="kimi" ${curSupProvider === 'kimi' ? 'selected' : ''}>Kimi (Moonshot)</option>
          </select>
        </div>
        <div class="meta-row"><span class="meta-label">API Endpoint</span><input type="text" class="memo-input" id="supBaseUrl" value="${esc(supConfig.base_url || 'https://api.anthropic.com')}" style="flex:1"></div>
        <div class="meta-row"><span class="meta-label">API Key</span><input type="text" class="memo-input" id="supApiKey" value="${esc(supConfig.api_key || '')}" placeholder="Required to enable supervisor" style="flex:1"></div>
        <div class="meta-row"><span class="meta-label">Model</span>
          <select class="memo-input" id="supModel" style="flex:1">
            ${providers[curSupProvider].models.map(m => `<option value="${m}" ${m === (supConfig.model || '') ? 'selected' : ''}>${m}</option>`).join('')}
          </select>
        </div>
        <div class="meta-row"><span class="meta-label">Interval (min)</span>
          <input type="number" class="memo-input" id="supInterval" value="${supConfig.interval_minutes || 10}" min="1" max="60" style="width:80px">
        </div>
        <div class="meta-row"><span class="meta-label">Discord Webhook</span>
          <input type="text" class="memo-input" id="supDiscord" value="${esc(supConfig.discord_webhook_url || '')}" placeholder="https://discord.com/api/webhooks/... (optional)" style="flex:1">
        </div>
        <div style="margin-top:4px;font-size:11px;color:var(--text-muted)">If not set, analysis results are shown only in the Supervisor panel.</div>
        <div style="margin-top:12px"><button class="btn btn-primary" id="supSave">Save</button> <span id="supStatus" style="font-size:12px;color:var(--text-muted)"></span></div>
      </div>
    </div>`

  // Tab switching
  document.querySelectorAll('.settings-tab-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.settings-tab-btn').forEach(b => b.classList.remove('active'))
      document.querySelectorAll('.settings-tab-content').forEach(c => c.style.display = 'none')
      btn.classList.add('active')
      document.getElementById('tabContent' + btn.dataset.tab.charAt(0).toUpperCase() + btn.dataset.tab.slice(1)).style.display = ''
    })
  })

  // Summarizer: provider change
  document.getElementById('sumProvider').addEventListener('change', (e) => {
    const p = providers[e.target.value]
    if (p) {
      document.getElementById('sumBaseUrl').value = p.url
      document.getElementById('sumModel').innerHTML = p.models.map(m => `<option value="${m}">${m}</option>`).join('')
    }
  })

  // Summarizer: fetch models
  document.getElementById('sumFetchModels').addEventListener('click', async () => {
    const btn = document.getElementById('sumFetchModels')
    btn.disabled = true; btn.textContent = '...'
    try {
      const resp = await fetch('/api/summarizer/models', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          provider: document.getElementById('sumProvider').value,
          base_url: document.getElementById('sumBaseUrl').value.trim(),
          api_key: document.getElementById('sumApiKey').value.trim(),
        })
      })
      const data = await resp.json()
      if (data.models?.length > 0) {
        const sel = document.getElementById('sumModel')
        const cur = sel.value
        sel.innerHTML = data.models.map(m => `<option value="${m}" ${m === cur ? 'selected' : ''}>${m}</option>`).join('')
        btn.textContent = `${data.models.length} models`
      } else { btn.textContent = data.error ? 'Error' : '0 models' }
    } catch { btn.textContent = 'Error' }
    btn.disabled = false
    setTimeout(() => { btn.textContent = 'Fetch' }, 3000)
  })

  // Summarizer: save
  document.getElementById('sumSave').addEventListener('click', async () => {
    const status = document.getElementById('sumStatus')
    status.textContent = 'Saving…'
    await fetch('/api/summarizer/config', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        provider: document.getElementById('sumProvider').value,
        base_url: document.getElementById('sumBaseUrl').value.trim(),
        api_key: document.getElementById('sumApiKey').value.trim(),
        model: document.getElementById('sumModel').value,
        language: document.getElementById('sumLanguage').value,
      })
    })
    status.textContent = 'Saved!'
    status.style.color = 'var(--green)'
    setTimeout(() => { status.textContent = '' }, 2000)
  })

  // Supervisor: provider change
  document.getElementById('supProvider').addEventListener('change', (e) => {
    const p = providers[e.target.value]
    if (p) {
      document.getElementById('supBaseUrl').value = p.url
      document.getElementById('supModel').innerHTML = p.models.map(m => `<option value="${m}">${m}</option>`).join('')
    }
  })

  // Supervisor: enabled checkbox → update status label
  document.getElementById('supEnabled').addEventListener('change', () => {
    const enabled = document.getElementById('supEnabled').checked
    const hasKey = document.getElementById('supApiKey').value.trim().length > 0
    const label = document.getElementById('supStatusLabel')
    label.textContent = (enabled && hasKey) ? 'Active' : 'Inactive'
    label.style.color = (enabled && hasKey) ? 'var(--green)' : 'var(--text-muted)'
  })
  document.getElementById('supApiKey').addEventListener('input', () => {
    const enabled = document.getElementById('supEnabled').checked
    const hasKey = document.getElementById('supApiKey').value.trim().length > 0
    const label = document.getElementById('supStatusLabel')
    label.textContent = (enabled && hasKey) ? 'Active' : 'Inactive'
    label.style.color = (enabled && hasKey) ? 'var(--green)' : 'var(--text-muted)'
  })

  // Supervisor: save
  document.getElementById('supSave').addEventListener('click', async () => {
    const status = document.getElementById('supStatus')
    status.textContent = 'Saving…'
    const result = await saveSupervisorConfig({
      enabled: document.getElementById('supEnabled').checked,
      provider: document.getElementById('supProvider').value,
      base_url: document.getElementById('supBaseUrl').value.trim(),
      api_key: document.getElementById('supApiKey').value.trim(),
      model: document.getElementById('supModel').value,
      interval_minutes: parseInt(document.getElementById('supInterval').value, 10) || 10,
      discord_webhook_url: document.getElementById('supDiscord').value.trim(),
    })
    if (result.ok) {
      status.textContent = 'Saved!'
      status.style.color = 'var(--green)'
    } else {
      status.textContent = result.error || 'Error'
      status.style.color = 'var(--red, #e55)'
    }
    setTimeout(() => { status.textContent = '' }, 2000)
  })

  document.getElementById('settingsClose').addEventListener('click', () => {
    if (selectedRequest) loadDetail(selectedRequest)
    else $detail.innerHTML = '<div class="detail-empty">Select a request to inspect</div>'
  })
})

$supervisorBtn.addEventListener('click', () => {
  // Use selected session, or first session if none selected
  const sid = selectedSession && selectedSession !== '__starred__'
    ? selectedSession
    : sessions.length > 0 ? sessions[0].id : null
  if (!sid) {
    $detail.innerHTML = '<div class="detail-empty">No session to analyze</div>'
    return
  }
  routingPanelOpen = false
  renderSupervisorPanel(sid)
})

// ── Code Viewer Mode ─────────────────────────────────────────────────────────

let codeViewerActive = false
let codeViewerSessionId = null

$codeBtn.addEventListener('click', () => {
  const sid = selectedSession && selectedSession !== '__starred__'
    ? selectedSession
    : sessions.length > 0 ? sessions[0].id : null
  if (!sid) return
  enterCodeViewer(sid)
})

async function enterCodeViewer(sessionId, preloadPath, scrollToLine) {
  codeViewerActive = true
  codeViewerSessionId = sessionId
  pushState()
  $promptViewBtn.style.display = ''
  $codeBtn.style.display = 'none'
  const sess = sessions.find(s => s.id === sessionId)
  const projName = sess?.project_name || 'unknown'

  document.querySelector('.layout').style.display = 'none'

  let cvRoot = document.getElementById('codeViewerRoot')
  if (!cvRoot) {
    cvRoot = document.createElement('div')
    cvRoot.id = 'codeViewerRoot'
    cvRoot.className = 'cv-layout'
    document.querySelector('.layout').after(cvRoot)
  }

  cvRoot.style.display = 'flex'
  cvRoot.innerHTML = `
    <div class="cv-body">
      <nav class="sidebar" id="cvSidebar">
        <div class="sidebar-section">Sessions</div>
        <div id="cvSessionList"></div>
        <div class="cv-sidebar-actions">
          <button class="btn btn-sm cv-sidebar-btn" id="cvBack">&larr; Back</button>
        </div>
      </nav>
      <div class="cv-tree" id="cvTree"><div class="cv-loading">Loading tree…</div></div>
      <div class="cv-resize-handle" data-resize="tree"></div>
      <div class="cv-code" id="cvCode"><div class="cv-empty">Select a file from the tree</div></div>
      <div class="cv-resize-handle" data-resize="timeline"></div>
      <div class="cv-timeline" id="cvTimeline"><div class="cv-empty">Click an annotation to see request details</div></div>
    </div>
  `

  // Render sessions in Code Viewer sidebar (same format as main dashboard)
  const cvSessionList = document.getElementById('cvSessionList')
  let sessHtml = ''
  for (const s of sessions) {
    const sel = s.id === sessionId
    const live = s.pending_count > 0
    const tok = s.total_input_tokens + s.total_output_tokens
    const tokS = tok > 0 ? ` · ${tok >= 1000 ? (tok/1000).toFixed(1)+'k' : tok} tok` : ''
    sessHtml += `<div class="${sel ? 'session-item selected' : 'session-item'}" data-cv-sid="${s.id}">
      <div class="session-name"><span class="sdot ${live ? 'live' : 'idle'}"></span>${esc(s.project_name || 'unknown')}</div>
      <div class="session-id" title="${s.id}">${s.id.slice(0, 8)}</div>
      <div class="session-cwd">${esc(s.cwd || '')}</div>
      <div class="session-stats">${s.request_count} req${tokS}</div>
    </div>`
  }
  cvSessionList.innerHTML = sessHtml
  cvSessionList.querySelectorAll('[data-cv-sid]').forEach(el => {
    el.addEventListener('click', () => {
      enterCodeViewer(el.dataset.cvSid)
    })
  })

  document.getElementById('cvBack').addEventListener('click', exitCodeViewer)

  // Resize handles for panels
  document.querySelectorAll('.cv-resize-handle').forEach(handle => {
    handle.addEventListener('mousedown', (e) => {
      e.preventDefault()
      const target = handle.dataset.resize
      const startX = e.clientX
      const tree = document.getElementById('cvTree')
      const timeline = document.getElementById('cvTimeline')
      const startWidth = target === 'tree' ? tree.offsetWidth : timeline.offsetWidth

      const onMove = (ev) => {
        const dx = ev.clientX - startX
        if (target === 'tree') {
          tree.style.width = Math.max(100, startWidth + dx) + 'px'
        } else {
          timeline.style.width = Math.max(150, startWidth - dx) + 'px'
        }
      }
      const onUp = () => {
        document.removeEventListener('mousemove', onMove)
        document.removeEventListener('mouseup', onUp)
      }
      document.addEventListener('mousemove', onMove)
      document.addEventListener('mouseup', onUp)
    })
  })

  // Load file tree + coverage data
  let tree, coverageData
  try {
    ;[tree, coverageData] = await Promise.all([
      getFileTree(sessionId),
      getFileCoverage(sessionId),
    ])
  } catch (err) {
    document.getElementById('cvTree').innerHTML = `<div class="cv-empty">Error: ${esc(String(err))}</div>`
    return
  }
  // Build coverage map: file_path → {lines_read, total_lines, has_full_read}
  const coverageMap = {}
  for (const f of (coverageData.files || [])) {
    coverageMap[f.file_path] = { lines_read: f.lines_read || 0, total_lines: f.total_lines || 0, has_full_read: f.has_full_read || false }
  }
  document.getElementById('cvTree').innerHTML = renderFileTree(tree, sessionId, 0, coverageMap)
  bindTreeClicks(sessionId)

  if (preloadPath) {
    await loadCodeFile(sessionId, preloadPath, scrollToLine)
  }
}

function exitCodeViewer() {
  codeViewerActive = false
  codeViewerSessionId = null
  pushState()
  $promptViewBtn.style.display = 'none'
  $codeBtn.style.display = ''
  const cvRoot = document.getElementById('codeViewerRoot')
  if (cvRoot) { cvRoot.style.display = 'none'; cvRoot.innerHTML = '' }
  document.querySelector('.layout').style.display = 'flex'
}

$promptViewBtn.addEventListener('click', () => {
  const sid = codeViewerSessionId
  exitCodeViewer()
  if (sid) {
    selectedSession = sid
    renderSessions()
    loadRequests()
  }
})

function renderFileTree(nodes, sessionId, depth = 0, coverageMap = {}) {
  if (!Array.isArray(nodes) || nodes.length === 0) return '<div class="cv-empty">No files</div>'
  let html = '<ul class="cv-tree-list">'
  for (const node of nodes) {
    if (node.type === 'dir') {
      // Aggregate coverage for all files in this directory (recursive)
      const dirCov = aggregateDirCoverage(node, coverageMap)
      const dirCovLabel = dirCov.total > 0 ? `(${dirCov.covered}/${dirCov.total})` : ''
      const dirFullCls = dirCov.total > 0 && dirCov.covered === dirCov.total ? ' cv-tree-full' : ''
      html += `<li class="cv-tree-dir" style="padding-left:${depth * 12}px">
        <span class="cv-tree-toggle${dirFullCls}" data-expanded="false">▶ ${dirCovLabel ? `<span class="cv-tree-cov">${dirCovLabel}</span> ` : ''}${esc(node.name)}</span>
        <div class="cv-tree-children" style="display:none">${renderFileTree(node.children || [], sessionId, depth + 1, coverageMap)}</div>
      </li>`
    } else {
      const cov = coverageMap[node.path]
      const totalLines = cov ? cov.total_lines : (node.total_lines > 0 ? node.total_lines : 0)
      const linesRead = cov ? cov.lines_read : 0
      let fullCls = ''
      if (totalLines > 0 && linesRead >= totalLines) fullCls = ' cv-tree-full'
      const covLabel = totalLines > 0 ? `<span class="cv-tree-cov">(${linesRead}/${totalLines})</span> ` : ''
      html += `<li class="cv-tree-file${fullCls}" style="padding-left:${(depth * 12) + 16}px" data-path="${esc(node.path)}" data-sid="${sessionId}">
        ${covLabel}${esc(node.name)}
      </li>`
    }
  }
  html += '</ul>'
  return html
}

function aggregateDirCoverage(dirNode, coverageMap) {
  let covered = 0, total = 0
  const children = dirNode.children || []
  for (const child of children) {
    if (child.type === 'dir') {
      const sub = aggregateDirCoverage(child, coverageMap)
      covered += sub.covered
      total += sub.total
    } else {
      const cov = coverageMap[child.path]
      const fileTotal = cov ? cov.total_lines : (child.total_lines > 0 ? child.total_lines : 0)
      const fileRead = cov ? cov.lines_read : 0
      covered += fileRead
      total += fileTotal
    }
  }
  return { covered, total }
}

function bindTreeClicks(sessionId) {
  // Dir toggle
  document.querySelectorAll('.cv-tree-toggle').forEach(toggle => {
    toggle.addEventListener('click', (e) => {
      e.stopPropagation()
      const li = toggle.closest('.cv-tree-dir')
      const children = li.querySelector('.cv-tree-children')
      const expanded = toggle.dataset.expanded === 'true'
      children.style.display = expanded ? 'none' : 'block'
      const dirName = toggle.textContent.replace(/^[▶▼]\s*/, '')
      toggle.textContent = expanded ? `▶ ${dirName}` : `▼ ${dirName}`
      toggle.dataset.expanded = expanded ? 'false' : 'true'
    })
  })
  // File click
  document.querySelectorAll('.cv-tree-file').forEach(file => {
    file.addEventListener('click', () => {
      document.querySelectorAll('.cv-tree-file.selected').forEach(f => f.classList.remove('selected'))
      file.classList.add('selected')
      loadCodeFile(sessionId, file.dataset.path)
    })
  })
}

async function loadCodeFile(sessionId, filePath, scrollToLine) {
  const $code = document.getElementById('cvCode')
  $code.innerHTML = '<div class="cv-loading">Loading…</div>'
  document.getElementById('cvTimeline').innerHTML = '<div class="cv-empty">Click an annotation to see request details</div>'

  let fileData, reqData
  try {
    ;[fileData, reqData] = await Promise.all([
      getFileContent(sessionId, filePath),
      getFileRequests(sessionId, filePath),
    ])
  } catch (err) {
    $code.innerHTML = `<div class="cv-empty">Failed to load: ${esc(String(err))}</div>`
    return
  }

  if (fileData.error) {
    $code.innerHTML = `<div class="cv-empty">${esc(fileData.error)}</div>`
    return
  }

  const lines = fileData.lines || []
  const requests = reqData.requests || []
  const funcs = fileData.functions || []
  const language = fileData.language || null

  // Build line → requests mapping
  const lineReqMap = buildLineRequestMap(lines.length, requests)

  // Build function coverage: which functions were touched by which requests
  const funcCoverage = funcs.map(f => {
    const touchedReqs = new Map() // request_id → {access_type, agent_type, timestamp}
    for (let line = f.start_line; line <= f.end_line; line++) {
      for (const r of (lineReqMap[line] || [])) {
        if (!touchedReqs.has(r.request_id)) {
          touchedReqs.set(r.request_id, r)
        }
      }
    }
    return { ...f, requests: [...touchedReqs.values()], covered: touchedReqs.size > 0 }
  })

  const coveredCount = funcCoverage.filter(f => f.covered).length
  const totalFuncs = funcCoverage.length

  // File summary + function coverage table
  let html = ''
  if (funcs.length > 0) {
    const uniqueReqIds = new Set()
    requests.forEach(r => uniqueReqIds.add(r.request_id))
    html += `<div class="cv-file-summary">
      <div class="cv-summary-header">
        <span class="cv-summary-title">${esc(filePath.split('/').pop())}${language ? ` <span class="cv-lang">${language}</span>` : ''}</span>
        <span class="cv-summary-stats">${coveredCount}/${totalFuncs} functions covered · ${uniqueReqIds.size} requests · ${lines.length} lines</span>
      </div>
      <table class="cv-func-table">
        <tr><th>Function</th><th>Lines</th><th>Coverage</th><th>Requests</th></tr>
        ${funcCoverage.map(f => {
          const lineRange = `L${f.start_line}-${f.end_line}`
          const covCls = f.covered ? 'cv-func-covered' : 'cv-func-uncovered'
          const reqBadges = f.requests.slice(0, 5).map(r => {
            const cls = r.access_type === 'edit' || r.access_type === 'write' ? 'cv-edit' : r.access_type === 'read' ? 'cv-read' : 'cv-search'
            return `<span class="cv-func-req ${cls}" title="#${r.request_id.slice(0,8)} ${r.agent_type} ${r.access_type}">${r.agent_type}</span>`
          }).join('')
          const more = f.requests.length > 5 ? `<span class="cv-func-more">+${f.requests.length - 5}</span>` : ''
          return `<tr class="cv-func-row" data-start="${f.start_line}">
            <td><span class="cv-func-name">${esc(f.name)}</span> <span class="cv-func-kind">${f.kind}</span></td>
            <td class="cv-func-lines">${lineRange}</td>
            <td><span class="${covCls}">${f.covered ? 'covered' : 'NOT covered'}</span></td>
            <td>${reqBadges}${more}</td>
          </tr>`
        }).join('')}
      </table>
    </div>`
  }

  // Build set of function start lines for markers
  const funcStartLines = new Set(funcs.map(f => f.start_line))
  const funcByStartLine = {}
  for (const f of funcCoverage) { funcByStartLine[f.start_line] = f }

  html += '<div class="cv-code-inner">'
  for (let i = 0; i < lines.length; i++) {
    const lineNum = i + 1
    const reqs = lineReqMap[lineNum] || []
    // Build inline annotation (right side, git-blame style)
    // Show all request IDs for this line: #id1, #id2, #id3
    const annotationHtml = reqs.length > 0
      ? (() => {
          // Deduplicate by request_id, keep order
          const seen = new Set()
          const uniqueReqs = reqs.filter(r => { if (seen.has(r.request_id)) return false; seen.add(r.request_id); return true })
          const tags = uniqueReqs.map(r => {
            const cls = r.access_type === 'edit' || r.access_type === 'write' ? 'cv-ann-edit' : r.access_type === 'read' ? 'cv-ann-read' : 'cv-ann-search'
            return `<span class="cv-ann-tag ${cls}" title="#${r.request_id.slice(0,8)} ${r.agent_type} ${r.access_type}">#${r.request_id.slice(0,6)}</span>`
          }).join(' ')
          return `<span class="cv-annotation" data-line="${lineNum}">${tags}</span>`
        })()
      : ''

    // Function boundary marker
    const func = funcByStartLine[lineNum]
    const funcMarker = func
      ? `<div class="cv-func-marker ${func.covered ? 'cv-func-marker-covered' : 'cv-func-marker-uncovered'}">${esc(func.kind)}: ${esc(func.name)} (L${func.start_line}-${func.end_line}) ${func.covered ? `— ${func.requests.length} req` : '— NOT COVERED'}</div>`
      : ''

    html += `${funcMarker}<div class="cv-line${scrollToLine === lineNum ? ' cv-highlight' : ''}${reqs.length > 0 ? ' cv-line-touched' : ''}" data-line="${lineNum}">
      <span class="cv-linenum">${lineNum}</span>
      <span class="cv-text">${esc(lines[i])}</span>
      ${annotationHtml}
    </div>`
  }
  html += '</div>'
  $code.innerHTML = html

  // Scroll to line if requested
  if (scrollToLine) {
    const targetLine = $code.querySelector(`[data-line="${scrollToLine}"]`)
    if (targetLine) targetLine.scrollIntoView({ block: 'center' })
  }

  // Bind annotation clicks
  $code.querySelectorAll('.cv-annotation').forEach(ann => {
    ann.addEventListener('click', (e) => {
      e.stopPropagation()
      const lineNum = parseInt(ann.dataset.line)
      const lineReqs = lineReqMap[lineNum] || []
      renderTimeline(lineNum, lineReqs)
      // Highlight selected annotation
      $code.querySelectorAll('.cv-annotation.active').forEach(a => a.classList.remove('active'))
      ann.classList.add('active')
    })
  })

  // Also bind line click
  $code.querySelectorAll('.cv-line-touched').forEach(line => {
    line.addEventListener('click', () => {
      const lineNum = parseInt(line.dataset.line)
      const lineReqs = lineReqMap[lineNum] || []
      if (lineReqs.length > 0) renderTimeline(lineNum, lineReqs)
    })
  })

  // Function table row click → scroll to function
  document.querySelectorAll('.cv-func-row').forEach(row => {
    row.addEventListener('click', () => {
      const startLine = parseInt(row.dataset.start)
      const target = $code.querySelector(`[data-line="${startLine}"]`)
      if (target) target.scrollIntoView({ block: 'center', behavior: 'smooth' })
    })
  })
}

function buildLineRequestMap(totalLines, requests) {
  const map = {} // lineNum -> [{request_id, access_type, ...}]
  for (const r of requests) {
    let startLine = 1, endLine = totalLines
    if (r.access_type === 'search') {
      // Search: file-level, no specific lines — skip line mapping
      continue
    }
    if (r.read_range && r.read_range !== 'full' && r.read_range !== '') {
      // Parse "offset:N,limit:M"
      const parts = {}
      r.read_range.split(',').forEach(p => { const [k, v] = p.split(':'); parts[k] = parseInt(v) })
      if (parts.offset !== undefined) startLine = parts.offset + 1
      if (parts.limit !== undefined) endLine = startLine + parts.limit - 1
    }
    // Cap to actual file size
    if (r.read_range === 'full' || r.read_range === '' || r.read_range === 'default') {
      // Default read limit is 2000
      endLine = Math.min(totalLines, 2000)
    }
    endLine = Math.min(endLine, totalLines)
    for (let line = startLine; line <= endLine; line++) {
      if (!map[line]) map[line] = []
      // Avoid duplicates
      if (!map[line].some(x => x.request_id === r.request_id)) {
        map[line].push(r)
      }
    }
  }
  // Sort each line's requests by timestamp (oldest first = leftmost layer)
  for (const line in map) {
    map[line].sort((a, b) => a.timestamp.localeCompare(b.timestamp))
  }
  return map
}

function renderTimeline(lineNum, requests) {
  const $timeline = document.getElementById('cvTimeline')
  if (requests.length === 0) {
    $timeline.innerHTML = '<div class="cv-empty">No requests for this line</div>'
    return
  }

  // Sort by timestamp and deduplicate by request_id
  const seen = new Set()
  const uniqueReqs = requests.filter(r => { if (seen.has(r.request_id)) return false; seen.add(r.request_id); return true })

  // Sort by message count ascending to find correct prev for delta
  const withMsgCount = uniqueReqs.map(r => {
    let msgCount = 0
    try { msgCount = (JSON.parse(r.request_body).messages || []).length } catch {}
    return { ...r, _msgCount: msgCount }
  }).sort((a, b) => a.timestamp.localeCompare(b.timestamp))

  // Store current timeline data for summarize button
  window._currentTimelineData = { lineNum, requests: withMsgCount }

  let html = `<div class="cv-timeline-header">Line ${lineNum} — ${withMsgCount.length} request${withMsgCount.length > 1 ? 's' : ''} <button class="btn btn-sm cv-summarize-btn" id="cvSummarize">Summarize</button></div>`
  let prevSummary = ''
  for (let ri = 0; ri < withMsgCount.length; ri++) {
    const r = withMsgCount[ri]
    // Find the closest previous request by message count (the one with fewer messages)
    const prevReqBody = ri > 0
      ? withMsgCount.slice(0, ri).sort((a, b) => b._msgCount - a._msgCount).find(p => p._msgCount < r._msgCount)?.request_body || null
      : null
    const accessCls = r.access_type === 'edit' || r.access_type === 'write' ? 'cv-access-edit'
      : r.access_type === 'read' ? 'cv-access-read' : 'cv-access-search'
    const promptSummary = extractRequestSummary(r.request_body)
    const respSummary = extractResponseSummary(r.response_body)
    // If prompt is same as previous request, show response summary instead to differentiate
    const summary = (promptSummary === prevSummary && respSummary) ? respSummary : promptSummary
    const summaryLabel = (promptSummary === prevSummary && respSummary) ? 'resp' : 'prompt'
    prevSummary = promptSummary
    html += `<div class="cv-req-card">
      <div class="cv-req-header">
        <span class="cv-req-time">${esc(r.timestamp.slice(11, 19))}</span>
        <span class="cv-req-id">#${esc(r.request_id.slice(0, 8))}</span>
        <span class="cv-req-agent">${esc(r.agent_type)}</span>
        <span class="${accessCls}">${esc(r.access_type)}</span>
      </div>
      <div class="cv-req-meta">${r.input_tokens ?? '-'} in / ${r.output_tokens ?? '-'} out${r.duration_ms ? ' · ' + r.duration_ms + 'ms' : ''}</div>
      <details class="cv-req-details"><summary>Prompt</summary><pre class="cv-req-pre">${esc(formatPrompt(r.request_body, prevReqBody))}</pre></details>
      <details class="cv-req-details"><summary>Raw Prompt</summary><pre class="cv-req-pre cv-req-raw">${esc(formatRawPrompt(r.request_body))}</pre></details>
      <details class="cv-req-details"><summary>Response</summary><pre class="cv-req-pre">${esc(formatResponse(r.response_body))}</pre></details>
    </div>`
  }
  $timeline.innerHTML = html

  // Summarize button handler
  const sumBtn = document.getElementById('cvSummarize')
  if (sumBtn) {
    sumBtn.addEventListener('click', async () => {
      sumBtn.disabled = true
      sumBtn.textContent = 'Summarizing…'
      try {
        const data = window._currentTimelineData
        const resp = await fetch('/api/summarize', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({
            line: data.lineNum,
            requests: data.requests.map(r => ({
              request_id: r.request_id,
              agent_type: r.agent_type,
              access_type: r.access_type,
              read_range: r.read_range,
              timestamp: r.timestamp,
              request_body: r.request_body,
              response_body: r.response_body,
            }))
          })
        })
        const result = await resp.json()
        if (result.error) {
          sumBtn.textContent = 'Error'
          sumBtn.title = result.error
        } else {
          // Insert summary at top of timeline (remove old one if exists)
          const old = $timeline.querySelector('.cv-summary-result')
          if (old) old.remove()
          const summaryDiv = document.createElement('div')
          summaryDiv.className = 'cv-summary-result'
          summaryDiv.innerHTML = `<div class="cv-summary-label">AI Summary <button class="btn btn-sm cv-summary-refresh" id="cvSummaryRefresh">Refresh</button></div><pre class="cv-req-pre">${esc(result.summary)}</pre>`
          $timeline.querySelector('.cv-timeline-header').after(summaryDiv)
          sumBtn.textContent = 'Summarize'
          // Refresh button re-triggers summarize
          document.getElementById('cvSummaryRefresh').addEventListener('click', () => {
            sumBtn.click()
          })
        }
      } catch (e) {
        sumBtn.textContent = 'Error'
        sumBtn.title = e.message
      }
      sumBtn.disabled = false
    })
  }
}

function formatPrompt(requestBody, prevRequestBody) {
  try {
    const body = JSON.parse(requestBody)
    const msgs = body.messages || []

    // Determine which messages are NEW (not in previous request)
    let prevMsgCount = 0
    if (prevRequestBody) {
      try {
        const prevBody = JSON.parse(prevRequestBody)
        prevMsgCount = (prevBody.messages || []).length
      } catch {}
    }
    let newMsgs = msgs.slice(prevMsgCount)
    if (newMsgs.length === 0 && msgs.length > 0) {
      // Fallback: show last 2 messages
      newMsgs = msgs.slice(-2)
    } else if (newMsgs.length > 20) {
      // Sanity check: delta too large means prevMsgCount was likely wrong, show last 4
      newMsgs = msgs.slice(-4)
    }

    // Collect tool_use IDs → tool_result content for merging
    const toolResults = {}
    for (const m of newMsgs) {
      if (m.role !== 'user') continue
      const blocks = Array.isArray(m.content) ? m.content : []
      for (const b of blocks) {
        if (b.type === 'tool_result' && b.tool_use_id) {
          const content = typeof b.content === 'string' ? b.content
            : Array.isArray(b.content) ? b.content.map(x => x.text || '').join('')
            : ''
          const lines = content.split('\n')
          const isFileContent = lines.length > 3 && /^\s*\d+→/.test(lines[0])
          if (isFileContent) {
            const lastLine = [...lines].reverse().find(l => /^\s*\d+→/.test(l))
            const lastMatch = lastLine ? lastLine.match(/^\s*(\d+)→/) : null
            toolResults[b.tool_use_id] = `${lines.length} lines, L1-${lastMatch ? lastMatch[1] : '?'}`
          } else {
            // Check if it looks like file paths (Glob/Grep results)
            const pathLines = lines.filter(l => l.trim().startsWith('/') || l.includes('/'))
            if (pathLines.length > 0) {
              const shortPaths = pathLines.slice(0, 8).map(p => p.trim().split('/').slice(-2).join('/'))
              const suffix = pathLines.length > 8 ? ` +${pathLines.length - 8} more` : ''
              toolResults[b.tool_use_id] = `${pathLines.length} files: ${shortPaths.join(', ')}${suffix}`
            } else {
              toolResults[b.tool_use_id] = content
            }
          }
        }
      }
    }

    const parts = []
    if (prevMsgCount > 0 && msgs.length > prevMsgCount) {
      parts.push(`(${prevMsgCount} previous messages hidden)`)
    }

    for (const m of newMsgs) {
      const role = m.role || '?'
      if (typeof m.content === 'string') {
        if (m.content.startsWith('<system-reminder>')) continue
        parts.push(`[${role}] ${m.content}`)
        continue
      }
      if (!Array.isArray(m.content)) continue
      const lines = []
      for (const b of m.content) {
        if (b.type === 'tool_use') {
          const name = b.name || '?'
          const input = b.input || {}
          const path = input.file_path || input.path || ''
          const short = path ? path.split('/').slice(-2).join('/') : ''
          if (name === 'Read' && short) {
            const o = input.offset, l = input.limit
            const lineCount = toolResults[b.id] || ''
            const countNum = lineCount.match(/^(\d+) lines/)
            if (o != null || l != null) {
              const start = (o||0)+1, len = l || '?'
              lines.push(`[Read: ${short} — L${start}-${typeof len === 'number' ? start+len-1 : '?'} (${len} lines)]`)
            } else {
              lines.push(`[Read: ${short} — ${countNum ? countNum[1]+' lines, full' : 'full'}]`)
            }
          } else if (name === 'Edit' && short) {
            lines.push(`[Edit: ${short}]`)
          } else if (name === 'Write' && short) {
            lines.push(`[Write: ${short}]`)
          } else if (name === 'Glob') {
            const pattern = input.pattern || '*'
            const dir = path ? path.split('/').slice(-2).join('/') : '.'
            const result = toolResults[b.id] || ''
            lines.push(`[Glob: ${dir}/${pattern}${result ? ` → ${result}` : ''}]`)
          } else if (name === 'Grep') {
            const pattern = input.pattern || ''
            const dir = path ? path.split('/').slice(-2).join('/') : '.'
            const result = toolResults[b.id] || ''
            lines.push(`[Grep: "${pattern}" in ${dir}${result ? ` → ${result}` : ''}]`)
          } else if (short) {
            lines.push(`[${name}: ${short}]`)
          } else {
            const result = toolResults[b.id] || ''
            lines.push(`[${name}${result ? `: ${result}` : ''}]`)
          }
        } else if (b.type === 'tool_result') {
          continue
        } else if (b.type === 'text') {
          const t = (b.text || '').trim()
          if (t && !t.startsWith('<system-reminder>')) {
            lines.push(t)
          }
        }
      }
      if (lines.length > 0) parts.push(`[${role}]\n${lines.join('\n')}`)
    }
    return parts.join('\n\n---\n\n')
  } catch { return requestBody || '' }
}

/** Collapse tool_result file contents into compact tags */
function collapseBlock(block) {
  if (typeof block === 'string') return block
  if (!block || typeof block !== 'object') return JSON.stringify(block)

  // tool_result with file content → collapse
  if (block.type === 'tool_result') {
    const content = typeof block.content === 'string' ? block.content
      : Array.isArray(block.content) ? block.content.map(x => x.text || '').join('')
      : ''
    // Detect numbered file content (e.g. "     1→#include..." from Read tool)
    const lines = content.split('\n')
    if (lines.length > 5 && /^\s*\d+→/.test(lines[0])) {
      // Extract first and last line numbers
      const firstMatch = lines[0].match(/^\s*(\d+)→/)
      const lastLine = [...lines].reverse().find(l => /^\s*\d+→/.test(l))
      const lastMatch = lastLine ? lastLine.match(/^\s*(\d+)→/) : null
      const start = firstMatch ? firstMatch[1] : '?'
      const end = lastMatch ? lastMatch[1] : '?'
      return `[tool_result: ${lines.length} lines → L${start}-${end}]`
    }
    // Non-file content, just truncate if long
    if (content.length > 300) {
      return `[tool_result: ${content.length} chars]\n${content.slice(0, 200)}…`
    }
    return block.text || content || JSON.stringify(block)
  }

  // tool_use → show compact form
  if (block.type === 'tool_use') {
    const name = block.name || '?'
    const input = block.input || {}
    const path = input.file_path || input.path || ''
    if (path) {
      const short = path.split('/').slice(-2).join('/')
      const offset = input.offset, limit = input.limit
      if (name === 'Read') {
        if (offset != null || limit != null) {
          const s = (offset || 0) + 1, e = limit ? s + limit - 1 : '?'
          return `[${name}: ${short} L${s}-${e}]`
        }
        return `[${name}: ${short} full]`
      }
      return `[${name}: ${short}]`
    }
    return `[${name}: ${JSON.stringify(input).slice(0, 100)}]`
  }

  // text block
  if (block.text) return block.text
  return JSON.stringify(block)
}

function formatResponse(responseBody) {
  if (!responseBody) return '(no response)'
  try {
    const body = JSON.parse(responseBody)
    if (body.accumulated_content) return body.accumulated_content
    if (body.content) {
      if (Array.isArray(body.content)) {
        return body.content.map(b => collapseBlock(b)).join('\n')
      }
      return typeof body.content === 'string' ? body.content : JSON.stringify(body.content, null, 2)
    }
    return JSON.stringify(body, null, 2).slice(0, 5000)
  } catch { return (responseBody || '').slice(0, 5000) }
}

function formatRawPrompt(requestBody) {
  try {
    const body = JSON.parse(requestBody)
    return JSON.stringify(body, null, 2)
  } catch { return requestBody || '' }
}

function extractRequestSummary(requestBody) {
  // Plain-text bodies (e.g. Cursor CHAT) — return directly
  if (requestBody && !requestBody.trimStart().startsWith('{')) {
    const t = requestBody.trim().replace(/\n+/g, ' ')
    return t.length > 120 ? t.slice(0, 120) + '…' : t
  }
  try {
    const body = JSON.parse(requestBody)
    const msgs = body.messages || []
    // Find the last user message (= the actual prompt for this request)
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role !== 'user') continue
      const content = msgs[i].content
      let text = ''
      if (typeof content === 'string') {
        text = content
      } else if (Array.isArray(content)) {
        // Find last text block that isn't a system-reminder
        for (let j = content.length - 1; j >= 0; j--) {
          const t = content[j].text || ''
          if (t && !t.startsWith('<system-reminder>')) { text = t; break }
        }
      }
      if (text) {
        // Trim and truncate
        text = text.trim().replace(/\n+/g, ' ')
        return text.length > 120 ? text.slice(0, 120) + '…' : text
      }
    }
  } catch {}
  return ''
}

function extractResponseSummary(responseBody) {
  if (!responseBody) return ''
  // Plain-text bodies (e.g. Cursor CHAT)
  if (!responseBody.trimStart().startsWith('{')) {
    const t = responseBody.trim().replace(/\n+/g, ' ')
    return t.length > 150 ? t.slice(0, 150) + '…' : t
  }
  try {
    const body = JSON.parse(responseBody)
    let text = ''
    if (body.accumulated_content) {
      text = body.accumulated_content
    } else if (body.content) {
      if (Array.isArray(body.content)) {
        text = body.content.map(b => b.text || '').filter(Boolean).join(' ')
      } else if (typeof body.content === 'string') {
        text = body.content
      }
    }
    if (text) {
      text = text.trim().replace(/\n+/g, ' ')
      return text.length > 150 ? text.slice(0, 150) + '…' : text
    }
  } catch {}
  return ''
}

// Global function for cross-linking from Supervisor and Request Detail
window.openCodeViewer = function(sessionId, filePath, lineNum) {
  enterCodeViewer(sessionId, filePath, lineNum)
}

function renderRoutingPanel() {
  if (!routingPanelOpen) return
  const cfg = routingConfig || {}
  const rules = routingRules

  const rulesHtml = rules.map((rule, idx) => `
    <div class="rule-row ${rule.enabled ? '' : 'disabled'}" data-rule-id="${esc(rule.id)}">
      <button class="btn btn-sm" data-rule-up="${idx}" title="Move up" ${idx === 0 ? 'disabled' : ''}>↑</button>
      <button class="btn btn-sm" data-rule-down="${idx}" title="Move down" ${idx === rules.length - 1 ? 'disabled' : ''}>↓</button>
      <input type="checkbox" class="rule-enabled-cb" data-rule-id="${esc(rule.id)}" ${rule.enabled ? 'checked' : ''} title="Enable/Disable">
      <span class="routing-badge">${esc(rule.category)}</span>
      <span style="flex:1;font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${esc(rule.target_url)}">${esc(rule.target_url)}</span>
      ${rule.model_override ? `<span style="font-size:11px;color:var(--text-muted)">${esc(rule.model_override)}</span>` : ''}
      ${rule.label ? `<span style="font-size:11px;color:var(--yellow)">${esc(rule.label)}</span>` : ''}
      <button class="btn btn-sm" data-rule-edit="${esc(rule.id)}">Edit</button>
      <button class="btn btn-sm btn-danger" data-rule-del="${esc(rule.id)}">✕</button>
    </div>
  `).join('')

  $detail.innerHTML = `
    <div class="routing-panel">
      <div class="routing-section">
        <h3>Classifier Settings</h3>
        <div class="meta-row">
          <span class="meta-label">Enabled</span>
          <input type="checkbox" id="rEnabled" ${cfg.enabled ? 'checked' : ''}>
        </div>
        <div class="meta-row">
          <span class="meta-label">Provider URL</span>
          <input type="text" class="memo-input" id="rBaseUrl" value="${esc(cfg.classifier_base_url || 'https://api.anthropic.com')}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">Model</span>
          <input type="text" class="memo-input" id="rModel" value="${esc(cfg.classifier_model || 'claude-haiku-4-5-20251001')}" style="flex:1">
        </div>
        <div class="meta-row">
          <span class="meta-label">API Key</span>
          <input type="password" class="memo-input" id="rApiKey" value="${esc(cfg.classifier_api_key || '')}" placeholder="Leave empty to use proxy key" style="flex:1">
        </div>
        <div class="meta-row" style="align-items:flex-start">
          <span class="meta-label">System Prompt</span>
          <textarea class="intercept-textarea" id="rPrompt" rows="3" style="flex:1;min-height:60px">${esc(cfg.classifier_prompt || '')}</textarea>
        </div>
      </div>

      <div class="routing-section">
        <h3>Routing Rules <button class="btn btn-sm" id="addRuleBtn" style="margin-left:8px">+ Add Rule</button></h3>
        <div id="rulesList">${rulesHtml || '<div class="empty-msg" style="padding:8px 0;font-size:12px">No rules yet</div>'}</div>
        <div id="ruleForm" style="display:none;margin-top:8px;padding:8px;background:var(--bg3);border-radius:6px;border:1px solid var(--border)">
          <div class="meta-row">
            <span class="meta-label">Category</span>
            <input type="text" class="memo-input" id="rfCategory" placeholder="code_gen" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">API Endpoint</span>
            <input type="text" class="memo-input" id="rfTargetUrl" placeholder="https://api.openai.com" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">API Key</span>
            <input type="password" class="memo-input" id="rfApiKey" placeholder="Leave empty to use proxy key" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">Model Override</span>
            <input type="text" class="memo-input" id="rfModelOverride" placeholder="gpt-4 (optional)" style="flex:1">
          </div>
          <div class="meta-row">
            <span class="meta-label">Label</span>
            <input type="text" class="memo-input" id="rfLabel" placeholder="Display name (optional)" style="flex:1">
          </div>
          <div class="meta-row" style="align-items:flex-start">
            <span class="meta-label">Description</span>
            <textarea class="memo-input" id="rfDescription" rows="2" placeholder="Describe when to use this route (helps classifier)" style="flex:1;resize:vertical"></textarea>
          </div>
          <div class="meta-row" style="align-items:flex-start">
            <span class="meta-label">Prompt Override</span>
            <textarea class="memo-input" id="rfPromptOverride" rows="3" placeholder="Optional. Use {original_prompt} to inject the original message. Leave empty to keep original." style="flex:1;resize:vertical"></textarea>
          </div>
          <div class="meta-row">
            <span class="meta-label">Enabled</span>
            <input type="checkbox" id="rfEnabled" checked>
          </div>
          <div style="display:flex;gap:8px;margin-top:8px">
            <button class="btn btn-primary" id="rfSaveBtn">Save Rule</button>
            <button class="btn" id="rfCancelBtn">Cancel</button>
          </div>
        </div>
      </div>

      <div class="routing-section">
        <h3>Test Classifier</h3>
        <div class="meta-row" style="align-items:flex-start">
          <span class="meta-label">Prompt</span>
          <textarea class="intercept-textarea" id="rTestPrompt" rows="2" style="flex:1;min-height:50px" placeholder="Enter a test prompt…"></textarea>
        </div>
        <div style="display:flex;gap:8px;margin-top:8px;align-items:center">
          <button class="btn btn-primary" id="rTestBtn">Test</button>
          <span id="rTestResult" style="font-size:13px;color:var(--text-muted)"></span>
        </div>
      </div>

      <div style="margin-top:16px;display:flex;gap:8px">
        <button class="btn btn-primary" id="rSaveAllBtn">Save All Settings</button>
        <button class="btn" id="rCloseBtn">Close</button>
      </div>
    </div>
  `

  // Save all settings
  document.getElementById('rSaveAllBtn').addEventListener('click', async () => {
    const newCfg = {
      enabled: document.getElementById('rEnabled').checked,
      classifier_base_url: document.getElementById('rBaseUrl').value.trim(),
      classifier_api_key: document.getElementById('rApiKey').value.trim(),
      classifier_model: document.getElementById('rModel').value.trim(),
      classifier_prompt: document.getElementById('rPrompt').value,
    }
    routingConfig = await saveRoutingConfig(newCfg)
    renderRoutingBtn()
    renderRoutingPanel()
  })

  // Close
  document.getElementById('rCloseBtn').addEventListener('click', () => {
    routingPanelOpen = false
    if (selectedRequest) {
      loadDetail(selectedRequest)
    } else {
      $detail.innerHTML = '<div class="detail-empty">Select a request to inspect</div>'
    }
  })

  // Add rule button
  document.getElementById('addRuleBtn').addEventListener('click', () => {
    editingRuleId = 'new'
    document.getElementById('ruleForm').style.display = ''
    document.getElementById('rfCategory').value = ''
    document.getElementById('rfTargetUrl').value = ''
    document.getElementById('rfApiKey').value = ''
    document.getElementById('rfModelOverride').value = ''
    document.getElementById('rfLabel').value = ''
    document.getElementById('rfDescription').value = ''
    document.getElementById('rfPromptOverride').value = ''
    document.getElementById('rfEnabled').checked = true
  })

  // Rule up/down buttons
  document.querySelectorAll('[data-rule-up]').forEach(btn => {
    btn.addEventListener('click', async () => {
      const idx = parseInt(btn.dataset.ruleUp)
      if (idx <= 0) return
      const ids = routingRules.map(r => r.id)
      ;[ids[idx - 1], ids[idx]] = [ids[idx], ids[idx - 1]]
      routingRules = await reorderRoutingRules(ids)
      renderRoutingPanel()
    })
  })

  document.querySelectorAll('[data-rule-down]').forEach(btn => {
    btn.addEventListener('click', async () => {
      const idx = parseInt(btn.dataset.ruleDown)
      if (idx >= routingRules.length - 1) return
      const ids = routingRules.map(r => r.id)
      ;[ids[idx], ids[idx + 1]] = [ids[idx + 1], ids[idx]]
      routingRules = await reorderRoutingRules(ids)
      renderRoutingPanel()
    })
  })

  // Rule enable/disable checkboxes
  document.querySelectorAll('.rule-enabled-cb').forEach(cb => {
    cb.addEventListener('change', async () => {
      const ruleId = cb.dataset.ruleId
      const rule = routingRules.find(r => r.id === ruleId)
      if (!rule) return
      const updated = { ...rule, enabled: cb.checked }
      await updateRoutingRule(ruleId, updated)
      routingRules = await getRoutingRules()
      renderRoutingBtn()
      renderRoutingPanel()
    })
  })

  // Rule edit buttons
  document.querySelectorAll('[data-rule-edit]').forEach(btn => {
    btn.addEventListener('click', () => {
      const ruleId = btn.dataset.ruleEdit
      const rule = routingRules.find(r => r.id === ruleId)
      if (!rule) return
      editingRuleId = ruleId
      document.getElementById('ruleForm').style.display = ''
      document.getElementById('rfCategory').value = rule.category
      document.getElementById('rfTargetUrl').value = rule.target_url
      document.getElementById('rfApiKey').value = rule.api_key || ''
      document.getElementById('rfModelOverride').value = rule.model_override || ''
      document.getElementById('rfLabel').value = rule.label || ''
      document.getElementById('rfDescription').value = rule.description || ''
      document.getElementById('rfPromptOverride').value = rule.prompt_override || ''
      document.getElementById('rfEnabled').checked = rule.enabled
    })
  })

  // Rule delete buttons
  document.querySelectorAll('[data-rule-del]').forEach(btn => {
    btn.addEventListener('click', async () => {
      const ruleId = btn.dataset.ruleDel
      if (!confirm('Delete this routing rule?')) return
      await deleteRoutingRule(ruleId)
      routingRules = await getRoutingRules()
      renderRoutingBtn()
      renderRoutingPanel()
    })
  })

  // Rule form save
  document.getElementById('rfSaveBtn').addEventListener('click', async () => {
    const category = document.getElementById('rfCategory').value.trim()
    const targetUrl = document.getElementById('rfTargetUrl').value.trim()
    if (!category || !targetUrl) { alert('Category and Target URL are required'); return }
    const ruleData = {
      id: editingRuleId || '',
      priority: 0,
      enabled: document.getElementById('rfEnabled').checked,
      category,
      target_url: targetUrl,
      api_key: document.getElementById('rfApiKey').value.trim(),
      prompt_override: document.getElementById('rfPromptOverride').value,
      model_override: document.getElementById('rfModelOverride').value.trim(),
      label: document.getElementById('rfLabel').value.trim(),
      description: document.getElementById('rfDescription').value.trim(),
    }
    if (editingRuleId === 'new') {
      await createRoutingRule(ruleData)
    } else if (editingRuleId) {
      await updateRoutingRule(editingRuleId, { ...ruleData, id: editingRuleId })
    }
    editingRuleId = null
    routingRules = await getRoutingRules()
    renderRoutingBtn()
    renderRoutingPanel()
  })

  // Rule form cancel
  document.getElementById('rfCancelBtn').addEventListener('click', () => {
    editingRuleId = null
    document.getElementById('ruleForm').style.display = 'none'
  })

  // Test classifier
  document.getElementById('rTestBtn').addEventListener('click', async () => {
    const prompt = document.getElementById('rTestPrompt').value.trim()
    const result = document.getElementById('rTestResult')
    result.textContent = 'Testing…'
    const res = await testRoutingClassifier(prompt)
    if (res.error) {
      result.textContent = `Error: ${res.error}`
      result.style.color = 'var(--red)'
    } else {
      result.textContent = `Category: ${res.category}`
      result.style.color = 'var(--green)'
    }
  })
}

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
      <div class="session-id" title="${s.id}">${s.id.slice(0, 8)}</div>
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
      selectedRequest = null
      renderSessions()
      loadRequests()
      pushState()
      // Update supervisor panel if open
      if (supervisorSessionId && selectedSession && selectedSession !== '__starred__') {
        renderSupervisorPanel(selectedSession)
      }
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
        ${r.source === 'cursor' ? '<span class="src-badge cursor-badge" title="Cursor IDE traffic">Cursor</span>' : ''}
        ${r.agent_type && r.agent_type !== 'main' && r.agent_type !== 'cursor' ? `<span class="agent-badge agent-${r.agent_type}" title="${esc(r.agent_task || '')}">${r.agent_type}${r.agent_task ? ': ' + esc(r.agent_task.slice(0, 40)) + (r.agent_task.length > 40 ? '…' : '') : ''}</span>` : ''}
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
        ${r.routing_category ? `<span class="routing-badge" title="Routed: ${esc(r.routing_category)}">${esc(r.routing_category)}</span>` : ''}
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
      pushState()
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
  // Cursor CHAT requests — simple chat view
  if (req.source === 'cursor') {
    const proj  = sessions.find(s => s.id === req.session_id)?.project_name
    const color = projectColor(proj || 'unknown')
    let attachedFiles = [], model = ''
    try {
      const hdrs = JSON.parse(req.request_headers)
      attachedFiles = hdrs.attached_files || []
      model = hdrs.model || ''
    } catch {}
    const filesHtml = attachedFiles.length
      ? `<div class="meta-row"><span class="meta-label">Files</span><span class="meta-val" style="font-size:11px">${attachedFiles.map(f => esc(f)).join(', ')}</span></div>`
      : ''
    const statusClass = req.status === 'complete' ? 'status-ok' : 'status-pending'

    // Parse response_body as timeline array or plain text
    let timeline = []
    if (req.response_body) {
      try {
        const parsed = JSON.parse(req.response_body)
        if (Array.isArray(parsed)) {
          timeline = parsed
        } else {
          timeline = [{ type: 'text', content: req.response_body }]
        }
      } catch {
        timeline = [{ type: 'text', content: req.response_body }]
      }
    }

    const timelineHtml = timeline.map(item => {
      if (item.type === 'tool') {
        const argsStr = item.args ? JSON.stringify(item.args, null, 2) : ''
        const pathVal = item.args?.path || item.args?.query || item.args?.pattern || ''
        return `<div class="msg-block">
          <div class="cb-label tool-use-label">tool · ${esc(item.name)} <span style="opacity:0.6;font-size:10px">${esc(item.status || '')}</span></div>
          ${pathVal ? `<div style="font-size:11px;padding:4px 8px;color:var(--text-muted)">${esc(pathVal)}</div>` : ''}
          <details><summary style="font-size:11px;cursor:pointer;padding:2px 8px;color:var(--text-muted)">args</summary>${codeBlock(esc(argsStr))}</details>
        </div>`
      }
      return `<div class="msg-block">
        <div class="msg-role assistant">assistant</div>
        ${codeBlock(esc(item.content || ''))}
      </div>`
    }).join('') || `<div class="empty-msg" style="padding:12px 0">${req.status === 'pending' ? 'Waiting…' : 'No response'}</div>`

    $detail.innerHTML = `
      <div class="detail-topbar">
        <span class="req-id" title="${req.id}">#${req.id.slice(0, 8)}</span>
        <span class="src-badge cursor-badge">Cursor</span>
        <span class="badge ${color}">${esc(proj || 'unknown')}</span>
        ${model ? `<span style="font-size:11px;color:var(--text-muted)">${esc(model)}</span>` : ''}
        <span class="detail-time">${fmtTime(req.timestamp)}</span>
      </div>
      <div class="split-pane">
        <div class="split-col">
          <div class="split-header">User</div>
          <div class="split-body">
            ${filesHtml}
            <div class="msg-block"><div class="msg-role user">user</div>${codeBlock(esc(req.request_body || ''))}</div>
          </div>
        </div>
        <div class="split-divider"></div>
        <div class="split-col">
          <div class="split-header">Response</div>
          <div class="split-body">
            <div class="meta-row"><span class="meta-label">Status</span><span class="${statusClass}">${req.status}</span></div>
            <div class="meta-row"><span class="meta-label">Output tokens</span><span class="meta-val">${req.output_tokens ?? '-'}</span></div>
            ${timelineHtml}
          </div>
        </div>
      </div>`
    return
  }

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
      ${req.agent_type && req.agent_type !== 'main' ? `<span class="agent-badge agent-${req.agent_type}">${req.agent_type}${req.agent_task ? ': ' + esc(req.agent_task.slice(0, 60)) + (req.agent_task.length > 60 ? '…' : '') : ''}</span>` : ''}
      <span class="badge ${color}">${esc(proj || 'unknown')}</span>
      <span class="detail-method">${req.method} ${req.path}</span>
      <span class="detail-time">${fmtTime(req.timestamp)}</span>
      <button class="btn btn-sm" id="copyCurl">Copy curl</button>
    </div>
    ${req.routing_category ? `<div class="routing-meta">Routing: <span class="routing-badge">${esc(req.routing_category)}</span> → ${esc(req.routed_to_url || 'default upstream')}</div>` : ''}

    <div class="split-pane">
      <div class="split-col">
        <div class="split-header">Request</div>
        <div class="split-body">
          <div class="meta-row"><span class="meta-label">Model</span><span class="meta-val">${esc(model || '-')}</span></div>
          <div class="meta-row"><span class="meta-label">Input tokens</span><span class="meta-val">${req.input_tokens ?? '-'}</span></div>
          ${systemBlock}
          ${prevMessageCount > 0 ? `<div class="prev-messages-toggle" id="prevMsgToggle">▶ ${prevMessageCount} previous messages hidden</div><div id="prevMsgContainer" class="prev-messages hidden">${messages.slice(0, prevMessageCount).map((m, i) => renderMsgBlock(m, i, fmtTime(msgTimestamps[i]))).join('')}</div><div class="new-messages-divider">── New in this request ──</div>` : ''}
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
        ? `▶ ${prevMessageCount} previous messages hidden`
        : `▼ ${prevMessageCount} previous messages shown`
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
// ── Supervisor Panel ──────────────────────────────────────────────────────────

async function renderSupervisorPanel(sessionId) {
  supervisorSessionId = sessionId
  $detail.innerHTML = '<div class="detail-empty">Loading supervisor analysis…</div>'

  const [summary, coverage, patterns, existingGoalRaw, analyses] = await Promise.all([
    getSessionSummary(sessionId),
    getFileCoverage(sessionId),
    getDetectPatterns(sessionId),
    getSessionGoal(sessionId),
    getSupervisorAnalyses(sessionId, 5),
  ])

  const existingGoal = existingGoalRaw && !existingGoalRaw.error ? existingGoalRaw : null

  // Patterns section
  const patternsHtml = (patterns.patterns || []).length === 0
    ? '<div class="empty-msg" style="padding:8px 0">No problematic patterns detected</div>'
    : (patterns.patterns || []).map(p => `
        <div class="sv-pattern sv-${p.severity}">
          <span class="sv-severity">${p.severity}</span>
          <span class="sv-type">${esc(p.type)}</span>
          ${esc(p.description)}
        </div>`).join('')

  // Coverage section with stats
  const files = (coverage.files || []).sort((a, b) => (b.access_count || 0) - (a.access_count || 0))
  const readFiles = files.filter(f => (f.access_types||[]).includes('read'))
  const fullReadFiles = files.filter(f => f.has_full_read)
  const partialOnlyFiles = readFiles.filter(f => !f.has_full_read)
  const writeFiles = files.filter(f => (f.access_types||[]).includes('write') || (f.access_types||[]).includes('edit'))
  const searchFiles = files.filter(f => (f.access_types||[]).includes('search'))
  const notReadFiles = files.filter(f => !(f.access_types||[]).includes('read'))

  const formatRange = (ranges) => {
    if (!ranges || ranges.length === 0) return ''
    return ranges.map(r => {
      if (r === 'full') return 'full'
      const parts = {}
      r.split(',').forEach(p => { const [k,v] = p.split(':'); parts[k] = parseInt(v) })
      const start = (parts.offset || 0) + 1
      const end = parts.limit ? start + parts.limit - 1 : '?'
      return `L${start}–${end}`
    }).join(', ')
  }

  const statsHtml = files.length === 0 ? '' : `
    <div class="sv-stats-bar">
      <span class="sv-stat sv-stat-good">${fullReadFiles.length} full read</span>
      <span class="sv-stat sv-stat-warn">${partialOnlyFiles.length} partial</span>
      <span class="sv-stat">${writeFiles.length} written</span>
      <span class="sv-stat">${searchFiles.length} searched</span>
      ${notReadFiles.length > 0 ? `<span class="sv-stat sv-stat-bad">${notReadFiles.length} not read</span>` : ''}
    </div>`

  const coverageHtml = files.length === 0
    ? '<div class="empty-msg" style="padding:8px 0">No file access recorded</div>'
    : `${statsHtml}
      <table class="sv-table">
        <tr><th>File</th><th>Type</th><th>Read Coverage</th><th>Count</th></tr>
        ${files.map(f => {
          const ranges = f.read_ranges || []
          const hasRead = (f.access_types||[]).includes('read')
          const totalLines = f.total_lines > 0 ? f.total_lines : null
          const linesRead = f.lines_read || 0
          let readStatus = '-'
          if (f.has_full_read) {
            readStatus = `<span class="sv-full-read">full${totalLines ? ` (${totalLines}/${totalLines})` : ''}</span>`
          } else if (ranges.length > 0) {
            const pct = totalLines ? ` ${Math.round(linesRead / totalLines * 100)}%` : ''
            readStatus = `<span class="sv-partial-read">${formatRange(ranges)} (${linesRead}/${totalLines || '?'})${pct}</span>`
          } else if (hasRead) {
            readStatus = '<span class="sv-partial-read">partial</span>'
          }
          return `<tr>
          <td class="sv-filepath"><a href="#" class="sv-file-link" data-path="${esc(f.file_path)}">${esc(f.file_path)}</a></td>
          <td>${(f.access_types || []).map(t => `<span class="sv-access-${t}">${t}</span>`).join(' ')}</td>
          <td>${readStatus}</td>
          <td>${f.access_count}</td>
        </tr>`}).join('')}
      </table>`

  // Summary section
  const reqList = (summary.requests || []).map(r => `
    <tr>
      <td>${esc(r.request_id?.slice(0, 8) || '')}</td>
      <td><span class="sv-agent">${esc(r.agent_type || '')}</span></td>
      <td>${esc(r.status || '')}</td>
      <td>${r.input_tokens ?? '-'}/${r.output_tokens ?? '-'}</td>
      <td>${r.duration_ms ? r.duration_ms + 'ms' : '-'}</td>
    </tr>`).join('')

  // Goal section
  const goalHtml = `
    <div class="sv-section" id="svGoalSection">
      <h3>Session Goal</h3>
      <div style="display:flex;gap:6px;align-items:flex-start">
        <textarea id="svGoalInput" class="memo-input" style="flex:1;min-height:48px;resize:vertical" placeholder="Describe the goal of this session (e.g. &quot;Refactor auth module to use JWT&quot;)">${esc(existingGoal?.goal || '')}</textarea>
        <div style="display:flex;flex-direction:column;gap:4px">
          <button class="btn btn-sm btn-primary" id="svGoalSave">Set Goal</button>
          ${existingGoal ? `<button class="btn btn-sm" id="svGoalDelete" style="color:var(--text-muted)">Clear</button>` : ''}
        </div>
      </div>
      <div id="svGoalStatus" style="font-size:12px;color:var(--text-muted);margin-top:4px"></div>
      <div id="svGoalQuestions" style="margin-top:8px"></div>
    </div>`

  // LLM Analyses section
  const analysesHtml = (analyses || []).length === 0
    ? '<div class="empty-msg" style="padding:8px 0">No LLM analyses yet. Set a goal and wait for the next analysis cycle.</div>'
    : (analyses || []).map(a => {
        const align = a.goal_alignment_score != null ? Math.round(a.goal_alignment_score * 100) : null
        const eff = a.efficiency_score != null ? Math.round(a.efficiency_score * 100) : null
        const alignColor = align == null ? 'var(--text-muted)' : align >= 70 ? 'var(--green)' : align >= 40 ? 'var(--yellow, #f5a623)' : 'var(--red, #e55)'
        const effColor = eff == null ? 'var(--text-muted)' : eff >= 70 ? 'var(--green)' : eff >= 40 ? 'var(--yellow, #f5a623)' : 'var(--red, #e55)'
        let issues = []
        try { issues = JSON.parse(a.issues || '[]') } catch {}
        return `
          <div style="border:1px solid var(--border);border-radius:4px;padding:10px;margin-bottom:8px">
            <div style="display:flex;gap:12px;align-items:center;margin-bottom:6px">
              <span style="font-size:11px;color:var(--text-muted)">${esc(a.analyzed_at?.slice(0,19).replace('T',' ') || '')}</span>
              <span style="color:${alignColor};font-weight:600">Goal: ${align != null ? align + '%' : 'N/A'}</span>
              <span style="color:${effColor};font-weight:600">Efficiency: ${eff != null ? eff + '%' : 'N/A'}</span>
            </div>
            ${a.recommendation ? `<div style="margin-bottom:4px"><strong>Recommendation:</strong> ${esc(a.recommendation)}</div>` : ''}
            ${issues.length > 0 ? `<div style="color:var(--yellow, #f5a623)">${issues.map(i => `• ${esc(i)}`).join('<br>')}</div>` : ''}
          </div>`
      }).join('')

  $detail.innerHTML = `
    <div class="sv-panel">
      <div class="sv-header">
        <span>Supervisor Analysis</span>
        <span class="sv-session-id" title="${sessionId}">${sessionId.slice(0, 12)}…</span>
        <button class="btn btn-sm" id="svClose">Close</button>
      </div>

      ${goalHtml}

      <div class="sv-section">
        <h3>LLM Analyses</h3>
        ${analysesHtml}
      </div>

      <div class="sv-section">
        <h3>Patterns (${patterns.pattern_count || 0})</h3>
        ${patternsHtml}
      </div>

      <div class="sv-section">
        <h3>File Coverage (${coverage.file_count || 0} files, ${coverage.total_accesses || 0} accesses)</h3>
        ${coverageHtml}
      </div>

      <div class="sv-section">
        <h3>Request Summary (${summary.request_count || 0} requests, ${summary.total_tokens || 0} tokens)</h3>
        ${summary.error_count > 0 ? `<div class="sv-pattern sv-error">${summary.error_count} errors detected</div>` : ''}
        <table class="sv-table">
          <tr><th>ID</th><th>Agent</th><th>Status</th><th>In/Out</th><th>Duration</th></tr>
          ${reqList || '<tr><td colspan="5">No requests</td></tr>'}
        </table>
      </div>
    </div>
  `

  document.getElementById('svClose').addEventListener('click', () => {
    supervisorSessionId = null
    if (selectedRequest) loadDetail(selectedRequest)
    else $detail.innerHTML = '<div class="detail-empty">Select a request to inspect</div>'
  })

  // Goal save
  document.getElementById('svGoalSave').addEventListener('click', async () => {
    const goalText = document.getElementById('svGoalInput').value.trim()
    const statusEl = document.getElementById('svGoalStatus')
    const questionsEl = document.getElementById('svGoalQuestions')
    if (!goalText) { statusEl.textContent = 'Please enter a goal.'; return }

    statusEl.textContent = 'Saving…'
    questionsEl.innerHTML = ''

    // Save goal first
    await setSessionGoal(sessionId, goalText, null)
    statusEl.textContent = 'Saved. Checking clarity…'

    // Check ambiguity via refine-goal
    try {
      const refinement = await refineGoal(goalText)
      if (refinement.error) {
        statusEl.textContent = 'Goal saved. (Supervisor API key not configured for refinement)'
        return
      }
      if (refinement.ambiguity_score > 0.5 && refinement.questions?.length > 0) {
        statusEl.textContent = `Goal saved. Ambiguity: ${Math.round(refinement.ambiguity_score * 100)}% — clarifying questions:`
        questionsEl.innerHTML = refinement.questions.map((q, i) => `
          <div style="margin-bottom:6px;padding:6px 8px;background:var(--bg-alt,#1e1e1e);border-radius:4px;font-size:12px">
            <strong>Q${i+1}:</strong> ${esc(q)}
          </div>`).join('')
      } else {
        statusEl.textContent = `Goal saved. Clarity: ${Math.round((1 - refinement.ambiguity_score) * 100)}% ✓`
        statusEl.style.color = 'var(--green)'
        setTimeout(() => { statusEl.style.color = ''; statusEl.textContent = '' }, 3000)
      }
    } catch {
      statusEl.textContent = 'Goal saved.'
    }
  })

  // Goal delete
  const deleteBtn = document.getElementById('svGoalDelete')
  if (deleteBtn) {
    deleteBtn.addEventListener('click', async () => {
      await deleteSessionGoal(sessionId)
      renderSupervisorPanel(sessionId)
    })
  }

  // File path links → Code Viewer
  document.querySelectorAll('.sv-file-link').forEach(link => {
    link.addEventListener('click', (e) => {
      e.preventDefault()
      enterCodeViewer(sessionId, link.dataset.path)
    })
  })
}

async function loadSessions() {
  sessions = await getSessions()
  renderSessions()
}

async function loadRequests() {
  if (selectedSession === '__starred__') {
    requests = await getRequests(null, { starred: true, source: sourceFilter })
  } else {
    requests = await getRequests(selectedSession, { search: searchQuery, source: sourceFilter })
  }
  renderRequests()
}

async function loadDetail(id) {
  const req = await getRequestDetail(id)

  let prevMessageCount = 0
  const msgTimestamps = []

  let currentMessages = []
  try { currentMessages = JSON.parse(req.request_body).messages || [] } catch {}
  const totalMessages = currentMessages.length

  if (req.session_id) {
    // Fetch session request list (summary only, no full bodies — lightweight)
    const sessionReqs = await getRequests(req.session_id, { limit: 500 })
    const sorted = [...sessionReqs].sort((a, b) => a.timestamp.localeCompare(b.timestamp))
    const curIdx = sorted.findIndex(r => r.id === id)

    // Use current request's message count to estimate prevMessageCount
    // Only fetch the immediately previous request's detail (not all of them)
    if (curIdx > 0) {
      try {
        const prevDetail = await getRequestDetail(sorted[curIdx - 1].id)
        const prevBody = JSON.parse(prevDetail.request_body)
        prevMessageCount = (prevBody.messages || []).length
      } catch {}
    }

    // Simple timestamp assignment: current request's timestamp for new messages
    for (let i = 0; i < totalMessages; i++) {
      msgTimestamps[i] = i < prevMessageCount && curIdx > 0
        ? sorted[curIdx - 1].timestamp
        : req.timestamp
    }
  }

  // Fill any gaps
  for (let i = 0; i < totalMessages; i++) {
    if (!msgTimestamps[i]) msgTimestamps[i] = req.timestamp
  }

  renderDetail(req, prevMessageCount, msgTimestamps)
}

// ── URL State Persistence ─────────────────────────────────────────────────────

function pushState() {
  const params = new URLSearchParams()
  if (selectedSession) params.set('session', selectedSession)
  if (selectedRequest) params.set('req', selectedRequest)
  if (codeViewerActive && codeViewerSessionId) params.set('view', 'code')
  const hash = params.toString()
  history.replaceState(null, '', hash ? '#' + hash : location.pathname + location.search)
}

async function restoreFromHash() {
  const hash = location.hash.slice(1)
  if (!hash) return
  const params = new URLSearchParams(hash)
  const session = params.get('session')
  const req = params.get('req')
  const view = params.get('view')

  if (session) {
    selectedSession = session
    renderSessions()
    await loadRequests()
  }
  if (req) {
    selectedRequest = req
    renderRequests()
    await loadDetail(req)
  }
  if (view === 'code' && session) {
    await enterCodeViewer(session)
  }
}

// ── Init ──────────────────────────────────────────────────────────────────────
async function init() {
  // Load intercept status
  try {
    const res = await getInterceptStatus()
    interceptEnabled = res.enabled
    renderInterceptToggle()
  } catch (_) {}

  // Load routing config and rules
  try {
    routingConfig = await getRoutingConfig()
    routingRules = await getRoutingRules()
    renderRoutingBtn()
  } catch (_) {}

  await loadSessions()
  await loadRequests()
  await restoreFromHash()

  let sseRefreshTimer = null
  connectEvents((e) => {
    // Debounce session/request list refresh (500ms)
    clearTimeout(sseRefreshTimer)
    sseRefreshTimer = setTimeout(() => { loadSessions(); loadRequests() }, 500)

    let eventRequestId = null
    try { eventRequestId = JSON.parse(e.data)?.data?.id } catch (_) {}

    // Auto-select intercepted requests (immediate, no debounce)
    if (e.type === 'request_intercepted' && eventRequestId) {
      selectedRequest = eventRequestId
      loadDetail(selectedRequest)
      return
    }
    // Only reload detail if the event is about the selected request
    if (selectedRequest && eventRequestId === selectedRequest) {
      loadDetail(selectedRequest)
    }
    // Auto-refresh supervisor panel if open (debounced)
    if (supervisorSessionId) {
      debouncedSupervisorRefresh(supervisorSessionId)
    }
  })
}

init()
