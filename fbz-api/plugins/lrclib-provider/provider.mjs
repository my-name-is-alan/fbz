// LRCLIB 远程歌词 provider（免 key）。
// search：有 title(+artist) 时先精确 /api/get（带时长更准），再 /api/search 补充；只有 searchTerm 时 q= 搜索。
// download：/api/get/{id}。
const UA = 'fbz-lrclib-provider/0.1 (+https://github.com/fbz)'

export async function getJson(url, fetcher = fetch) {
  const res = await fetcher(url, { headers: { 'user-agent': UA, accept: 'application/json' } })
  if (res.status === 404) return null
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}

export function mapRecord(r) {
  if (!r || r.id == null || r.instrumental) return null
  const synced = typeof r.syncedLyrics === 'string' && r.syncedLyrics.trim() ? r.syncedLyrics : null
  const plain = typeof r.plainLyrics === 'string' && r.plainLyrics.trim() ? r.plainLyrics : null
  if (!synced && !plain) return null
  return {
    id: String(r.id),
    providerName: 'LrcLib',
    trackName: r.trackName ?? null,
    artistName: r.artistName ?? null,
    albumName: r.albumName ?? null,
    duration: r.duration ?? null,
    syncedLyrics: synced,
    plainLyrics: plain,
  }
}

const norm = (s) => String(s ?? '').toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim()

/** 排序：同步歌词 > 时长接近 > 标题/歌手匹配。 */
export function rank(results, request) {
  const dur = Number(request.durationSeconds) || null
  const score = (r) => (r.syncedLyrics ? 4 : 0)
    + (dur && r.duration && Math.abs(r.duration - dur) <= 3 ? 3 : 0)
    + (request.title && norm(r.trackName) === norm(request.title) ? 2 : 0)
    + (request.artist && norm(r.artistName) === norm(request.artist) ? 1 : 0)
  return [...results].sort((a, b) => score(b) - score(a))
}

export async function search(request, config = {}, fetcher = fetch) {
  const base = String(config.base_url || 'https://lrclib.net').replace(/\/+$/, '')
  const max = Number(config.max_results) || 10
  const out = []
  const push = (r) => { const m = mapRecord(r); if (m && !out.some((x) => x.id === m.id)) out.push(m) }
  const title = request.searchTerm ? null : request.title
  if (title && request.artist) {
    const q = new URLSearchParams({ track_name: title, artist_name: request.artist })
    if (request.album) q.set('album_name', request.album)
    if (request.durationSeconds) q.set('duration', String(request.durationSeconds))
    try { push(await getJson(`${base}/api/get?${q}`, fetcher)) } catch { /* fall through to search */ }
  }
  const q = new URLSearchParams()
  if (title) {
    q.set('track_name', title)
    if (request.artist) q.set('artist_name', request.artist)
  } else {
    const term = request.searchTerm || [request.artist, request.title].filter(Boolean).join(' ')
    if (!term) return { results: out }
    q.set('q', term)
  }
  const list = await getJson(`${base}/api/search?${q}`, fetcher)
  for (const r of Array.isArray(list) ? list : []) push(r)
  return { results: rank(out, request).slice(0, max) }
}

export async function download(request, config = {}, fetcher = fetch) {
  const base = String(config.base_url || 'https://lrclib.net').replace(/\/+$/, '')
  const id = String(request.lyricId ?? '')
  if (!/^\d+$/.test(id)) return { error: 'invalid lyricId' }
  return mapRecord(await getJson(`${base}/api/get/${id}`, fetcher)) ?? { error: 'not found' }
}
