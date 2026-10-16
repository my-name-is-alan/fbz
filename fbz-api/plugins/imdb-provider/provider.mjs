/**
 * IMDb metadata provider (minimal usable).
 *
 * Upstream strategy (documented in README):
 * 1. If config/env `api_key` is set → OMDb (https://www.omdbapi.com) search + detail.
 * 2. Else → public IMDb suggestion endpoint (no key) for title search + light fields.
 * 3. Always normalize tt ids from lookup/current when present.
 *
 * There is no official public IMDb JSON API; OMDb is the compliant paid/free-key
 * bridge. Suggestion endpoint is best-effort and may change or rate-limit.
 */
import {
  buildUrl,
  clampRating,
  contributionIsEmpty,
  fetchJson,
  namedList,
  normalizeDate,
  pushExternalId,
  resolveExternalId,
  safeHttpUrl,
  trimText,
  yearFromDate,
} from './utils.mjs'

const OMDB_BASE = 'https://www.omdbapi.com/'
const IMDB_SUGGEST_BASE = 'https://v2.sg.media-imdb.com/suggestion'

export function normalizeImdbId(value) {
  const trimmed = trimText(value, 32)
  if (!trimmed) return null
  const digits = trimmed.toLowerCase().startsWith('tt') ? trimmed.slice(2) : trimmed
  if (!digits || !/^\d+$/.test(digits)) return null
  return digits.length < 7 ? `tt${digits.padStart(7, '0')}` : `tt${digits}`
}

function mapOmdb(data) {
  if (!data || data.Response === 'False') return null
  const imdbId = normalizeImdbId(data.imdbID)
  const title = trimText(data.Title, 512)
  if (!title && !imdbId) return null
  const premiere = normalizeDate(data.Released)
  const year = yearFromDate(premiere)
    ?? (trimText(data.Year, 8)?.match(/\d{4}/)?.[0]
      ? Number(trimText(data.Year, 8).match(/\d{4}/)[0])
      : null)
  const rating = clampRating(data.imdbRating)
  const genres = namedList(String(data.Genre ?? '').split(',').map((s) => s.trim()))
  const people = []
  for (const name of String(data.Director ?? '').split(',')) {
    const n = trimText(name, 256)
    if (n && n !== 'N/A') people.push({ name: n, roleType: 'Director' })
  }
  for (const name of String(data.Writer ?? '').split(',')) {
    const n = trimText(name, 256)
    if (n && n !== 'N/A') people.push({ name: n, roleType: 'Writer' })
  }
  for (const name of String(data.Actors ?? '').split(',')) {
    const n = trimText(name, 256)
    if (n && n !== 'N/A') people.push({ name: n, roleType: 'Actor' })
  }
  const externalIds = []
  if (imdbId) pushExternalId(externalIds, 'imdb', imdbId)
  const poster = data.Poster && data.Poster !== 'N/A' ? safeHttpUrl(data.Poster) : null
  const artwork = poster
    ? [{ artworkType: 'poster', remoteUrl: poster, isPrimary: true }]
    : []
  const overview = trimText(data.Plot, 8000)
  const official = trimText(data.Rated, 32)
  return {
    title,
    overview: overview && overview !== 'N/A' ? overview : null,
    productionYear: year,
    premiereDate: premiere,
    communityRating: rating,
    officialRating: official && official !== 'N/A' ? official : null,
    externalIds,
    artwork,
    genres,
    studios: [],
    people,
  }
}

async function omdbById(apiKey, imdbId) {
  const url = buildUrl(OMDB_BASE, '/', { apikey: apiKey, i: imdbId, plot: 'full' })
  return mapOmdb(await fetchJson(url))
}

async function omdbSearch(apiKey, lookup) {
  const title = trimText(lookup.title, 512) ?? trimText(lookup.originalTitle, 512)
  if (!title) return null
  const query = { apikey: apiKey, t: title, plot: 'full' }
  if (lookup.productionYear) query.y = String(lookup.productionYear)
  const type = lookup.itemType === 'movie' ? 'movie'
    : (lookup.itemType === 'series' || lookup.itemType === 'season' || lookup.itemType === 'episode')
      ? 'series'
      : null
  if (type) query.type = type
  const direct = mapOmdb(await fetchJson(buildUrl(OMDB_BASE, '/', query)))
  if (direct) return direct

  // Fallback: s= search then detail by id
  const searchQuery = { apikey: apiKey, s: title }
  if (lookup.productionYear) searchQuery.y = String(lookup.productionYear)
  if (type) searchQuery.type = type
  const search = await fetchJson(buildUrl(OMDB_BASE, '/', searchQuery))
  const first = (search?.Search ?? [])[0]
  const id = normalizeImdbId(first?.imdbID)
  if (!id) return null
  return omdbById(apiKey, id)
}

function mapSuggestItem(item) {
  const imdbId = normalizeImdbId(item?.id)
  const title = trimText(item?.l, 512)
  if (!imdbId || !title) return null
  const year = Number.isFinite(item?.y) ? item.y : null
  const poster = safeHttpUrl(item?.i?.imageUrl)
  const externalIds = []
  pushExternalId(externalIds, 'imdb', imdbId)
  const people = []
  // "s" is often "Actor1, Actor2" cast line
  for (const name of String(item?.s ?? '').split(',')) {
    const n = trimText(name, 256)
    if (n) people.push({ name: n, roleType: 'Actor' })
  }
  return {
    title,
    productionYear: year,
    externalIds,
    artwork: poster ? [{ artworkType: 'poster', remoteUrl: poster, isPrimary: true }] : [],
    genres: [],
    studios: [],
    people,
  }
}

async function suggestSearch(lookup) {
  const title = trimText(lookup.title, 512) ?? trimText(lookup.originalTitle, 512)
  if (!title) return null
  const q = title.toLowerCase().replace(/\s+/g, ' ').trim()
  if (!q) return null
  const first = encodeURIComponent(q[0])
  const encoded = encodeURIComponent(q)
  const url = `${IMDB_SUGGEST_BASE}/${first}/${encoded}.json`
  const data = await fetchJson(url, {
    userAgent: 'Mozilla/5.0 (compatible; fbz-imdb-provider/0.1)',
  })
  const items = data?.d ?? []
  const wantYear = lookup.productionYear
  const ranked = items
    .map(mapSuggestItem)
    .filter(Boolean)
    .filter((m) => {
      // Prefer feature/TV titles (qid: movie/tvSeries/…) when present
      return true
    })
    .sort((a, b) => {
      if (wantYear != null) {
        const da = a.productionYear == null ? 99 : Math.abs(a.productionYear - wantYear)
        const db = b.productionYear == null ? 99 : Math.abs(b.productionYear - wantYear)
        return da - db
      }
      return 0
    })
  return ranked[0] ?? null
}

function normalizeOnly(lookup, current) {
  const raw = normalizeImdbId(
    resolveExternalId(
      [
        lookup?.imdbId,
        current?.provider?.toLowerCase?.() === 'imdb' ? current.externalId : null,
        current?.externalIds,
      ],
      'imdb',
    ) ?? lookup?.imdbId,
  )
  if (!raw) return null
  return {
    externalIds: [{ provider: 'imdb', externalId: raw }],
  }
}

export async function queryImdb(cfg, lookup, current = null) {
  const apiKey = trimText(cfg.api_key, 256)
  const dataSource = trimText(cfg.data_source, 64)?.toLowerCase()

  try {
    const hintId = normalizeImdbId(
      resolveExternalId(
        [
          lookup?.imdbId,
          current?.provider?.toLowerCase?.() === 'imdb' ? current.externalId : null,
          current?.externalIds,
        ],
        'imdb',
      ) ?? lookup?.imdbId,
    )

    if (apiKey && dataSource !== 'suggest') {
      if (hintId) {
        const byId = await omdbById(apiKey, hintId)
        if (byId) return byId
      }
      const searched = await omdbSearch(apiKey, lookup)
      if (searched) return searched
    }

    // Public suggest fallback (or when data_source=suggest)
    if (hintId && !trimText(lookup.title)) {
      // Suggest API needs a query string; without title only return normalized id.
      const only = normalizeOnly(lookup, current)
      return only && !contributionIsEmpty(only) ? only : null
    }
    const suggested = await suggestSearch(lookup)
    if (suggested) {
      if (hintId) {
        // Prefer canonical hint id on the contribution
        suggested.externalIds = [{ provider: 'imdb', externalId: hintId }]
      }
      return suggested
    }

    const only = normalizeOnly(lookup, current)
    return only && !contributionIsEmpty(only) ? only : null
  } catch (err) {
    if (process.env.FBZ_PROVIDER_STUB_LOG === '1') {
      console.error(JSON.stringify({ provider: 'imdb', error: String(err.message ?? err) }))
    }
    const only = normalizeOnly(lookup, current)
    return only && !contributionIsEmpty(only) ? only : null
  }
}
