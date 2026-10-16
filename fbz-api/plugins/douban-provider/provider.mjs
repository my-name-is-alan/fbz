/**
 * Douban (豆瓣) metadata provider — minimal search + detail.
 *
 * Upstream strategy (see README):
 * 1. Search: movie.douban.com/j/subject_suggest (no key; anti-crawl possible).
 * 2. Detail: optional Douban v2 API when `api_key` set
 *    (`https://api.douban.com/v2/movie/subject/{id}?apikey=`),
 *    else rexxar mobile API best-effort, else suggest fields only.
 *
 * Douban has no maintained public open API. Expect 403/rate-limit; plugin
 * returns metadata:null on failure (never throws into host scrape).
 */
import {
  buildUrl,
  clampRating,
  contributionIsEmpty,
  fetchJson,
  namedList,
  pushExternalId,
  safeHttpUrl,
  trimText,
  yearFromDate,
  MAX_PEOPLE,
} from './utils.mjs'

const SUGGEST_BASE = 'https://movie.douban.com/j/subject_suggest'
const DOUBAN_V2_BASE = 'https://api.douban.com/v2/movie'
const REXXAR_BASE = 'https://m.douban.com/rexxar/api/v2/subject'

const BROWSER_UA = 'Mozilla/5.0 (compatible; fbz-douban-provider/0.1; +https://github.com/fbz)'

function itemIsMovieOrTv(subtype, itemType) {
  const s = String(subtype ?? '').toLowerCase()
  if (itemType === 'movie') return !s || s === 'movie'
  if (itemType === 'series' || itemType === 'season' || itemType === 'episode') {
    return !s || s === 'tv' || s === 'series' || s === 'drama'
  }
  return true
}

function mapSuggest(entry, itemType) {
  if (!entry?.id) return null
  if (!itemIsMovieOrTv(entry.subtype ?? entry.type, itemType)) return null
  const title = trimText(entry.title, 512)
  if (!title) return null
  const year = entry.year && /^\d{4}$/.test(String(entry.year))
    ? Number(entry.year)
    : null
  const poster = safeHttpUrl(entry.img ?? entry.cover_url)
  const externalIds = []
  pushExternalId(externalIds, 'douban', String(entry.id))
  return {
    title,
    productionYear: year,
    externalIds,
    artwork: poster ? [{ artworkType: 'poster', remoteUrl: poster, isPrimary: true }] : [],
    genres: [],
    studios: [],
    people: [],
  }
}

async function suggestSearch(lookup) {
  const title = trimText(lookup.title, 512) ?? trimText(lookup.originalTitle, 512)
  if (!title) return null
  const suggestUrl = `${SUGGEST_BASE}?q=${encodeURIComponent(title)}`
  const data = await fetchJson(suggestUrl, {
    userAgent: BROWSER_UA,
    headers: {
      referer: 'https://movie.douban.com/',
    },
  })
  const list = Array.isArray(data) ? data : []
  const wantYear = lookup.productionYear
  const mapped = list
    .map((e) => mapSuggest(e, lookup.itemType))
    .filter(Boolean)
    .sort((a, b) => {
      if (wantYear == null) return 0
      const da = a.productionYear == null ? 99 : Math.abs(a.productionYear - wantYear)
      const db = b.productionYear == null ? 99 : Math.abs(b.productionYear - wantYear)
      return da - db
    })
  return mapped[0] ?? null
}

function mapV2Subject(data) {
  if (!data?.id) return null
  const title = trimText(data.title, 512)
  if (!title) return null
  const original = trimText(data.original_title, 512)
  const overview = trimText(data.summary, 8000)
  const year = data.year && /^\d{4}$/.test(String(data.year)) ? Number(data.year) : null
  const rating = clampRating(data.rating?.average)
  const genres = namedList(data.genres ?? [])
  const people = []
  for (const d of data.directors ?? []) {
    const n = trimText(d.name, 256)
    if (n && people.length < MAX_PEOPLE) people.push({ name: n, roleType: 'Director' })
  }
  for (const w of data.writers ?? []) {
    const n = trimText(w.name, 256)
    if (n && people.length < MAX_PEOPLE) people.push({ name: n, roleType: 'Writer' })
  }
  for (const a of data.casts ?? []) {
    const n = trimText(a.name, 256)
    if (n && people.length < MAX_PEOPLE) {
      people.push({ name: n, roleType: 'Actor', roleName: trimText(a.roles?.[0], 128) ?? undefined })
    }
  }
  const poster = safeHttpUrl(data.images?.large ?? data.images?.medium ?? data.image)
  const externalIds = []
  pushExternalId(externalIds, 'douban', String(data.id))
  if (data.alt && /imdb/i.test(String(data.alt))) {
    /* ignore */
  }
  // attrs sometimes carry IMDb
  const attrs = data.attrs ?? {}
  for (const id of attrs.imdb ?? []) {
    const m = String(id).match(/tt\d+/)
    if (m) pushExternalId(externalIds, 'imdb', m[0])
  }
  return {
    title,
    originalTitle: original,
    overview,
    productionYear: year,
    premiereDate: null,
    communityRating: rating,
    externalIds,
    artwork: poster ? [{ artworkType: 'poster', remoteUrl: poster, isPrimary: true }] : [],
    genres,
    studios: namedList(data.tags?.map?.((t) => t.name) ?? []),
    people,
  }
}

function mapRexxar(data) {
  if (!data?.id) return null
  const title = trimText(data.title, 512)
  if (!title) return null
  const overview = trimText(data.intro ?? data.desc ?? data.short_comment?.content, 8000)
  const year = yearFromDate(data.pubdate?.[0]) 
    ?? (data.year && /^\d{4}$/.test(String(data.year)) ? Number(data.year) : null)
  const rating = clampRating(data.rating?.value ?? data.rating?.average)
  const genres = namedList(data.genres ?? [])
  const people = []
  for (const d of data.directors ?? []) {
    const n = trimText(d.name, 256)
    if (n) people.push({ name: n, roleType: 'Director' })
  }
  for (const a of data.actors ?? data.casts ?? []) {
    const n = trimText(a.name, 256)
    if (n && people.length < MAX_PEOPLE) people.push({ name: n, roleType: 'Actor' })
  }
  const poster = safeHttpUrl(data.pic?.large ?? data.cover_url ?? data.image)
  const externalIds = []
  pushExternalId(externalIds, 'douban', String(data.id))
  return {
    title,
    originalTitle: trimText(data.original_title, 512),
    overview,
    productionYear: year,
    communityRating: rating,
    externalIds,
    artwork: poster ? [{ artworkType: 'poster', remoteUrl: poster, isPrimary: true }] : [],
    genres,
    studios: [],
    people,
  }
}

async function fetchDetail(apiKey, doubanId) {
  if (apiKey) {
    try {
      const url = buildUrl(DOUBAN_V2_BASE, `/subject/${doubanId}`, { apikey: apiKey })
      const data = await fetchJson(url, { userAgent: BROWSER_UA })
      const mapped = mapV2Subject(data)
      if (mapped) return mapped
    } catch {
      // fall through
    }
  }
  try {
    const url = `${REXXAR_BASE}/${doubanId}?ck=&for_mobile=1`
    const data = await fetchJson(url, {
      userAgent: BROWSER_UA,
      headers: {
        referer: `https://m.douban.com/movie/subject/${doubanId}/`,
      },
    })
    return mapRexxar(data)
  } catch {
    return null
  }
}

export async function queryDouban(cfg, lookup) {
  const apiKey = trimText(cfg.api_key, 256)
  if (!trimText(lookup.title) && !trimText(lookup.originalTitle)) return null

  try {
    const base = await suggestSearch(lookup)
    if (!base) return null
    const doubanId = base.externalIds?.find((x) => x.provider === 'douban')?.externalId
    if (!doubanId) return base

    const detail = await fetchDetail(apiKey, doubanId)
    if (!detail) return base

    // Prefer detail fields; keep suggest poster if detail lacks artwork
    if (!detail.artwork?.length && base.artwork?.length) detail.artwork = base.artwork
    return contributionIsEmpty(detail) ? base : detail
  } catch (err) {
    if (process.env.FBZ_PROVIDER_STUB_LOG === '1') {
      console.error(JSON.stringify({ provider: 'douban', error: String(err.message ?? err) }))
    }
    return null
  }
}

