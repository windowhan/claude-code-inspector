const BASE = ''  // same origin (vite proxy in dev, rust server in prod)

export async function getSessions() {
  const r = await fetch(`${BASE}/api/sessions`)
  return r.json()
}

export async function getRequests(sessionId, { starred = false, search = '', limit = 100, offset = 0 } = {}) {
  const params = new URLSearchParams({ limit, offset })
  if (starred) {
    params.set('starred', '1')
  } else if (sessionId) {
    params.set('session_id', sessionId)
  }
  if (search) params.set('search', search)
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
