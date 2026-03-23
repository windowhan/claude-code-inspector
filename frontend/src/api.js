const BASE = ''  // same origin (vite proxy in dev, rust server in prod)

export async function getSessions() {
  const r = await fetch(`${BASE}/api/sessions`)
  return r.json()
}

export async function getRequests(sessionId, { starred = false, search = '', source = '', limit = 100, offset = 0 } = {}) {
  const params = new URLSearchParams({ limit, offset })
  if (starred) {
    params.set('starred', '1')
  } else if (sessionId) {
    params.set('session_id', sessionId)
  }
  if (search) params.set('search', search)
  if (source) params.set('source', source)
  const r = await fetch(`${BASE}/api/requests?${params}`)
  return r.json()
}

export async function getRequestDetail(id) {
  const r = await fetch(`${BASE}/api/requests/${id}`)
  return r.json()
}

export async function deleteSession(id) {
  const r = await fetch(`${BASE}/api/sessions/${id}`, { method: 'DELETE' })
  return r.json()
}

export async function toggleStar(id) {
  const r = await fetch(`${BASE}/api/requests/${id}/star`, { method: 'POST' })
  return r.json()
}

export async function setMemo(id, memo) {
  const r = await fetch(`${BASE}/api/requests/${id}/memo`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ memo }),
  })
  return r.json()
}

// ── Intercept API ─────────────────────────────────────────────────────────────

export async function getInterceptStatus() {
  const r = await fetch(`${BASE}/api/intercept/status`)
  return r.json()
}

export async function toggleIntercept() {
  const r = await fetch(`${BASE}/api/intercept/toggle`, { method: 'POST' })
  return r.json()
}

export async function getInterceptPending() {
  const r = await fetch(`${BASE}/api/intercept/pending`)
  return r.json()
}

export async function forwardOriginal(id) {
  const r = await fetch(`${BASE}/api/intercept/${id}/forward`, { method: 'POST' })
  return r.json()
}

export async function forwardModified(id, body) {
  const r = await fetch(`${BASE}/api/intercept/${id}/forward-modified`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: typeof body === 'string' ? body : JSON.stringify(body),
  })
  return r.json()
}

export async function rejectRequest(id) {
  const r = await fetch(`${BASE}/api/intercept/${id}/reject`, { method: 'POST' })
  return r.json()
}

// ── Routing API ───────────────────────────────────────────────────────────────

export async function getRoutingConfig() {
  const r = await fetch(`${BASE}/api/routing/config`)
  return r.json()
}

export async function saveRoutingConfig(config) {
  const r = await fetch(`${BASE}/api/routing/config`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(config),
  })
  return r.json()
}

export async function getRoutingRules() {
  const r = await fetch(`${BASE}/api/routing/rules`)
  return r.json()
}

export async function createRoutingRule(rule) {
  const r = await fetch(`${BASE}/api/routing/rules`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(rule),
  })
  return r.json()
}

export async function updateRoutingRule(id, rule) {
  const r = await fetch(`${BASE}/api/routing/rules/${id}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(rule),
  })
  return r.json()
}

export async function deleteRoutingRule(id) {
  const r = await fetch(`${BASE}/api/routing/rules/${id}`, { method: 'DELETE' })
  return r.json()
}

export async function reorderRoutingRules(ids) {
  const r = await fetch(`${BASE}/api/routing/reorder`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ ids }),
  })
  return r.json()
}

export async function testRoutingClassifier(prompt, system = '') {
  const r = await fetch(`${BASE}/api/routing/test`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ prompt, system }),
  })
  return r.json()
}

// ── Supervisor API ───────────────────────────────────────────────────────────

export async function getSessionSummary(sessionId) {
  const r = await fetch(`${BASE}/api/supervisor/summary/${sessionId}`)
  return r.json()
}

export async function getFileCoverage(sessionId) {
  const r = await fetch(`${BASE}/api/supervisor/coverage/${sessionId}`)
  return r.json()
}

export async function getDetectPatterns(sessionId) {
  const r = await fetch(`${BASE}/api/supervisor/patterns/${sessionId}`)
  return r.json()
}

// ── Files API (Code Viewer) ──────────────────────────────────────────────────

export async function getFileTree(sessionId) {
  const r = await fetch(`${BASE}/api/files/tree/${sessionId}`)
  return r.json()
}

export async function getFileContent(sessionId, path) {
  const r = await fetch(`${BASE}/api/files/content/${sessionId}?path=${encodeURIComponent(path)}`)
  return r.json()
}

export async function getFileRequests(sessionId, path) {
  const r = await fetch(`${BASE}/api/files/requests/${sessionId}?path=${encodeURIComponent(path)}`)
  return r.json()
}

/** Connect to SSE stream and call cb on each event */
export function connectEvents(cb) {
  let es
  function connect() {
    es = new EventSource(`${BASE}/events`)
    es.addEventListener('request_update', e => cb(e))
    es.addEventListener('request_intercepted', e => cb(e))
    es.addEventListener('session_update', e => cb(e))
    es.onerror = () => {
      es.close()
      setTimeout(connect, 2000)
    }
  }
  connect()
  return () => es?.close()
}

// ── Supervisor LLM API ───────────────────────────────────────────────────────

export async function getSupervisorConfig() {
  const r = await fetch(`${BASE}/api/supervisor/config`)
  return r.json()
}

export async function saveSupervisorConfig(config) {
  const r = await fetch(`${BASE}/api/supervisor/config`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(config),
  })
  return r.json()
}

export async function getSessionGoal(sessionId) {
  const r = await fetch(`${BASE}/api/supervisor/goals/${sessionId}`)
  if (r.status === 404) return null
  return r.json()
}

export async function setSessionGoal(sessionId, goal, refinedGoal) {
  const body = { goal }
  if (refinedGoal) body.refined_goal = refinedGoal
  const r = await fetch(`${BASE}/api/supervisor/goals/${sessionId}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  return r.json()
}

export async function deleteSessionGoal(sessionId) {
  const r = await fetch(`${BASE}/api/supervisor/goals/${sessionId}`, { method: 'DELETE' })
  return r.json()
}

export async function refineGoal(goalText) {
  const r = await fetch(`${BASE}/api/supervisor/refine-goal`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ goal: goalText }),
  })
  return r.json()
}

export async function getSupervisorAnalyses(sessionId, limit = 10) {
  const r = await fetch(`${BASE}/api/supervisor/analyses/${sessionId}?limit=${limit}`)
  return r.json()
}
