// OpenSubtitles provider core (pure helpers + HTTP calls). Used by server.mjs and tests.
import { gunzipSync } from 'node:zlib'

const TWO_LETTER = {
  eng: 'en', chi: 'zh-cn', zho: 'zh-cn', zht: 'zh-tw', jpn: 'ja', kor: 'ko', fre: 'fr', fra: 'fr',
  ger: 'de', deu: 'de', spa: 'es', ita: 'it', por: 'pt-pt', pob: 'pt-br', rus: 'ru', ara: 'ar',
  dut: 'nl', nld: 'nl', pol: 'pl', tur: 'tr', swe: 'sv', nor: 'no', dan: 'da', fin: 'fi',
  gre: 'el', ell: 'el', heb: 'he', hin: 'hi', tha: 'th', vie: 'vi', ind: 'id', cze: 'cs', ces: 'cs',
  hun: 'hu', rum: 'ro', ron: 'ro', ukr: 'uk',
}

export function toTwoLetter(lang) {
  const l = String(lang ?? '').trim().toLowerCase()
  if (l.length === 2) return l
  return TWO_LETTER[l] ?? l
}

export function toThreeLetter(lang) {
  const l = String(lang ?? '').trim().toLowerCase()
  if (l.length === 3) return l
  const hit = Object.entries(TWO_LETTER).find(([, two]) => two === l || two.startsWith(`${l}-`))
  return hit ? hit[0] : l || 'und'
}

export function normalizeImdb(id) {
  const digits = String(id ?? '').replace(/^tt/i, '').replace(/\D/g, '')
  return digits ? String(Number(digits)) : ''
}

export function encodeId(obj) {
  return Buffer.from(JSON.stringify(obj), 'utf8').toString('base64url')
}

export function decodeId(id) {
  try {
    return JSON.parse(Buffer.from(String(id), 'base64url').toString('utf8'))
  }
  catch {
    return null
  }
}

/** Legacy REST path: segments must be alphabetical. */
export function legacySearchUrl(request) {
  const lang = toThreeLetter(request.language)
  const segs = []
  const imdb = normalizeImdb(request.imdbId)
  const isEpisode = String(request.itemType ?? '').toLowerCase() === 'episode'
  if (isEpisode && request.episode != null) segs.push(`episode-${request.episode}`)
  if (imdb && !isEpisode) segs.push(`imdbid-${imdb}`)
  else {
    const q = String((isEpisode ? request.seriesName : null) ?? request.title ?? '').trim().toLowerCase()
    if (!q) return null
    segs.push(`query-${encodeURIComponent(q)}`)
  }
  if (isEpisode && request.season != null) segs.push(`season-${request.season}`)
  segs.push(`sublanguageid-${lang}`)
  return `https://rest.opensubtitles.org/search/${segs.join('/')}`
}

/** moviehash 是 16 位十六进制（core 按 OpenSubtitles 定义计算：size + 首尾 64KiB u64 和）。 */
export function validHash(request) {
  const hash = String(request.movieHash ?? '').toLowerCase()
  const size = Number(request.movieByteSize)
  return /^[0-9a-f]{16}$/.test(hash) && Number.isSafeInteger(size) && size > 0 ? { hash, size } : null
}

/** Legacy 哈希搜索：`moviebytesize-N/moviehash-H/sublanguageid-xxx`（段按字母序）。 */
export function legacyHashSearchUrl(request) {
  const h = validHash(request)
  if (!h) return null
  return `https://rest.opensubtitles.org/search/moviebytesize-${h.size}/moviehash-${h.hash}/sublanguageid-${toThreeLetter(request.language)}`
}

/** 合并：哈希命中在前，按 id 去重。 */
export function mergeResults(first, second, max = 30) {
  const seen = new Set()
  const out = []
  for (const r of [...first, ...second]) {
    if (seen.has(r.id)) continue
    seen.add(r.id)
    out.push(r)
  }
  out.sort((a, b) => Number(b.isHashMatch) - Number(a.isHashMatch))
  return out.slice(0, max)
}

export function mapLegacyResults(rows, max = 30) {
  if (!Array.isArray(rows)) return []
  return rows.slice(0, max).filter((r) => r && r.SubDownloadLink).map((r) => ({
    id: encodeId({ m: 'legacy', u: r.SubDownloadLink, f: r.SubFormat || 'srt', l: r.SubLanguageID || 'und', e: r.SubEncoding || '', hi: r.SubHearingImpaired === '1' }),
    name: r.SubFileName || r.MovieReleaseName || 'subtitle',
    format: r.SubFormat || 'srt',
    language: r.ISO639 || r.SubLanguageID,
    threeLetterIsoLanguageName: r.SubLanguageID || 'und',
    author: r.UserNickName || '',
    comment: r.SubAuthorComment || '',
    dateCreated: r.SubAddDate ? `${String(r.SubAddDate).replace(' ', 'T')}Z` : null,
    communityRating: Number(r.SubRating) || null,
    downloadCount: Number(r.SubDownloadsCnt) || 0,
    isHashMatch: r.MatchedBy === 'moviehash',
    isForced: r.SubForeignPartsOnly === '1',
    isHearingImpaired: r.SubHearingImpaired === '1',
    providerName: 'OpenSubtitles',
  }))
}

export function comSearchUrl(request) {
  const params = new URLSearchParams()
  params.set('languages', toTwoLetter(request.language))
  const imdb = normalizeImdb(request.imdbId)
  const isEpisode = String(request.itemType ?? '').toLowerCase() === 'episode'
  const h = validHash(request)
  if (h) params.set('moviehash', h.hash)
  if (imdb) params.set(isEpisode ? 'parent_imdb_id' : 'imdb_id', imdb)
  else if (request.title || request.seriesName) params.set('query', String((isEpisode ? request.seriesName : null) ?? request.title))
  else if (!h) return null
  if (isEpisode && request.season != null) params.set('season_number', String(request.season))
  if (isEpisode && request.episode != null) params.set('episode_number', String(request.episode))
  if (request.isHearingImpaired) params.set('hearing_impaired', 'only')
  if (request.isForced) params.set('foreign_parts_only', 'only')
  return `https://api.opensubtitles.com/api/v1/subtitles?${params}`
}

export function mapComResults(body, request, max = 30) {
  const rows = Array.isArray(body?.data) ? body.data : []
  return rows.slice(0, max).flatMap((row) => {
    const a = row?.attributes ?? {}
    const file = Array.isArray(a.files) ? a.files[0] : null
    if (!file?.file_id) return []
    const three = toThreeLetter(a.language ?? request.language)
    return [{
      id: encodeId({ m: 'com', id: file.file_id, f: 'srt', l: three, hi: !!a.hearing_impaired }),
      name: file.file_name || a.release || 'subtitle',
      format: 'srt',
      language: a.language,
      threeLetterIsoLanguageName: three,
      author: a.uploader?.name || '',
      comment: a.comments || '',
      dateCreated: a.upload_date || null,
      communityRating: Number(a.ratings) || null,
      downloadCount: Number(a.download_count) || 0,
      isHashMatch: !!a.moviehash_match,
      isForced: !!a.foreign_parts_only,
      isHearingImpaired: !!a.hearing_impaired,
      providerName: 'OpenSubtitles',
    }]
  })
}

export function decodeText(buf, encoding) {
  const enc = String(encoding ?? '').trim().toLowerCase()
  if (!enc || enc === 'utf-8' || enc === 'utf8' || enc === 'ascii') return buf
  try {
    return Buffer.from(new TextDecoder(enc).decode(buf), 'utf8')
  }
  catch {
    return buf
  }
}

export async function search(request, config, fetchImpl = fetch) {
  const max = Number(config.max_results) > 0 ? Number(config.max_results) : 30
  const apiKey = String(config.api_key ?? '').trim()
  if (apiKey) {
    const url = comSearchUrl(request)
    if (!url) return { results: [], skipped: true, reason: 'no title or imdb id' }
    const res = await fetchImpl(url, { headers: { 'Api-Key': apiKey, 'User-Agent': config.user_agent || 'fbz v0.1', Accept: 'application/json' } })
    if (!res.ok) throw new Error(`opensubtitles.com search ${res.status}`)
    return { results: mergeResults(mapComResults(await res.json(), request, max), [], max), mode: 'com' }
  }
  const headers = { 'User-Agent': config.user_agent || 'TemporaryUserAgent', Accept: 'application/json' }
  const hashUrl = legacyHashSearchUrl(request)
  let hashResults = []
  if (hashUrl) {
    // 哈希搜索失败不致命：继续走文本搜索。
    try {
      const res = await fetchImpl(hashUrl, { headers })
      if (res.ok) hashResults = mapLegacyResults(await res.json(), max).map((r) => ({ ...r, isHashMatch: true }))
    }
    catch {}
  }
  const url = legacySearchUrl(request)
  if (!url) {
    if (hashUrl) return { results: mergeResults(hashResults, [], max), mode: 'legacy' }
    return { results: [], skipped: true, reason: 'no title or imdb id' }
  }
  const res = await fetchImpl(url, { headers })
  if (!res.ok) {
    if (hashResults.length) return { results: mergeResults(hashResults, [], max), mode: 'legacy' }
    throw new Error(`opensubtitles legacy search ${res.status}`)
  }
  return { results: mergeResults(hashResults, mapLegacyResults(await res.json(), max), max), mode: 'legacy' }
}

export async function download(request, config, fetchImpl = fetch) {
  const ref = decodeId(request.subtitleId)
  if (!ref) throw new Error('invalid subtitle id')
  let bytes
  if (ref.m === 'legacy') {
    if (!/^https:\/\/dl\.opensubtitles\.org\//.test(String(ref.u))) throw new Error('unexpected legacy download host')
    const res = await fetchImpl(ref.u, { headers: { 'User-Agent': config.user_agent || 'TemporaryUserAgent' } })
    if (!res.ok) throw new Error(`legacy download ${res.status}`)
    const raw = Buffer.from(await res.arrayBuffer())
    bytes = decodeText(raw[0] === 0x1f && raw[1] === 0x8b ? gunzipSync(raw) : raw, ref.e)
  }
  else if (ref.m === 'com') {
    const apiKey = String(config.api_key ?? '').trim()
    if (!apiKey) throw new Error('api_key required for opensubtitles.com downloads')
    const ua = config.user_agent || 'fbz v0.1'
    const res = await fetchImpl('https://api.opensubtitles.com/api/v1/download', {
      method: 'POST',
      headers: { 'Api-Key': apiKey, 'User-Agent': ua, 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({ file_id: ref.id }),
    })
    if (!res.ok) throw new Error(`opensubtitles.com download ${res.status}`)
    const { link } = await res.json()
    const file = await fetchImpl(link, { headers: { 'User-Agent': ua } })
    if (!file.ok) throw new Error(`opensubtitles.com file ${file.status}`)
    bytes = Buffer.from(await file.arrayBuffer())
  }
  else throw new Error('unknown subtitle id mode')
  return { format: ref.f || 'srt', language: ref.l || 'und', isHearingImpaired: !!ref.hi, isForced: false, dataBase64: bytes.toString('base64') }
}
