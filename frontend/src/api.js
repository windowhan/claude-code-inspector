const BASE = ''  // same origin (vite proxy in dev, rust server in prod)

export async function getSessions() {
  const r = await fetch(`${BASE}/api/sessions`)
  return r.json()
}

export async function getRequests(sessionId, limit = 100, offset = 0) {
  const params = new URLSearchParams({ limit, offset })
  if (sessionId) params.set('session_id', sessionId)
  const r = await fetch(`${BASE}/api/requests?${params}`)
  return r.json()
}

export async function getRequestDetail(id) {
  const r = await fetch(`${BASE}/api/requests/${id}`)
  return r.json()
}

/** Long-poll /events (100ms window) and call cb when data arrives */
export function pollEvents(cb) {
  async function loop() {
    try {
      const r = await fetch(`${BASE}/events`)
      const text = await r.text()
      if (text.trim()) cb(text)
    } catch (_) {}
    setTimeout(loop, 1500)
  }
  loop()
}
