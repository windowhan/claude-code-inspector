const PALETTE = ['c0','c1','c2','c3','c4','c5','c6','c7']
const colorMap = {}
let colorIdx = 0

export function projectColor(name) {
  if (!name) return 'c0'
  if (!colorMap[name]) colorMap[name] = PALETTE[colorIdx++ % PALETTE.length]
  return colorMap[name]
}

export function fmtTime(iso) {
  return new Date(iso).toLocaleTimeString('en-US', { hour12: false })
}

export function fmtTokens(inp, out) {
  if (inp == null && out == null) return ''
  const f = n => n >= 1000 ? (n / 1000).toFixed(1) + 'k' : String(n)
  return `${inp != null ? f(inp) : '-'}↑ ${out != null ? f(out) : '-'}↓`
}

export function fmtBytes(n) {
  if (!n) return ''
  if (n < 1024) return `${n}b`
  return `${(n / 1024).toFixed(1)}k`
}

export function fmtDuration(ms) {
  if (ms == null) return ''
  return ms >= 1000 ? (ms / 1000).toFixed(2) + 's' : ms + 'ms'
}

export function esc(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

export function prettyJson(s) {
  try { return JSON.stringify(JSON.parse(s), null, 2) } catch { return s }
}

export function statusIcon(status) {
  if (status === 'complete')    return '<span class="status-ok">✓</span>'
  if (status === 'error')       return '<span class="status-err">✗</span>'
  if (status === 'intercepted') return '<span class="status-intercept">⏸</span>'
  if (status === 'rejected')    return '<span class="status-err">⊘</span>'
  return '<span class="status-pending">⏳</span>'
}
