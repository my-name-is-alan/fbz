/**
 * TVDB v4 upstream client for metadata.provider.query.
 * Aligned with core src/metadata/provider/tvdb.rs (login → search → extended).
 */
import {
  buildUrl,
  contributionIsEmpty,
  fetchJson,
  namedList,
  normalizeDate,
  pushExternalId,
  safeHttpUrl,
  trimText,
  yearFromDate,
  MAX_ARTWORK,
  MAX_PEOPLE,
} from './utils.mjs'

const DEFAULT_API = 'https://api4.thetvdb.com/v4'
const TOKEN_TTL_MS = 25 * 24 * 60 * 60 * 1000

let cachedToken = null // { apiKey, token, expiresAt }

function searchKind(itemType) {
  if (itemType === 'movie') return 'movie'
  if (itemType === 'series' || itemType === 'season' || itemType === 'episode') return 'series'
  return null
}

function remoteProvider(sourceName) {
  const s = String(sourceName ?? '').trim().toLowerCase()
  if (s.includes('imdb')) return 'imdb'
  if (s.includes('tmdb') || s.includes('themoviedb') || s.includes('the movie db')) return 'tmdb'
  if (s.includes('eidr')) return 'eidr'
  return null
}

function normalizeTvdbId(value) {
  const text = trimText(value, 64)
  if (!text) return null
  if (/^\d+$/.test(text) && Number(text) > 0) return text
  const tail = text.split(/[-/]/).pop()
  if (tail && /^\d+$/.test(tail) && Number(tail) > 0) return tail
  return text
}

function numericId(externalId) {
  const text = trimText(externalId, 64)
  if (!text) return null
  if (/^\d+$/.test(text)) {
    const n = Number(text)
    return n > 0 ? n : null
  }
  const tail = text.split(/[-/]/).pop()
  const n = Number(tail)
  return Number.isFinite(n) && n > 0 ? n : null
}

function mapArtwork(...urls) {
  const artwork = []
  const seen = new Set()
  for (const [type, url] of [
    ['poster', urls[0]],
    ['poster', urls[1]],
    ['thumb', urls[2]],
  ]) {
    const remote = safeHttpUrl(url)
    if (!remote || seen.has(remote)) continue
    seen.add(remote)
    artwork.push({
      artworkType: type,
      remoteUrl: remote,
      isPrimary: !artwork.some((a) => a.artworkType === type),
    })
    if (artwork.length >= MAX_ARTWORK) break
  }
  return artwork
}

function mapRemoteIds(remoteIds) {
  const externalIds = []
  for (const remote of remoteIds ?? []) {
    const provider = remoteProvider(remote.sourceName ?? remote.source_name)
    if (!provider) continue
    pushExternalId(externalIds, provider, remote.id)
  }
  return externalIds
}

function mapPeople(characters) {
  const people = []
  const seen = new Set()
  for (const ch of characters ?? []) {
    if (people.length >= MAX_PEOPLE) break
    const name = trimText(ch.personName ?? ch.person_name, 256)
    if (!name) continue
    const roleName = trimText(ch.name, 128) ?? ''
    const key = `${name.toLowerCase()}|${roleName.toLowerCase()}`
    if (seen.has(key)) continue
    seen.add(key)
    const person = { name, roleType: 'Actor' }
    if (roleName) person.roleName = roleName
    people.push(person)
  }
  return people
}

async function login(apiBase, apiKey) {
  const key = trimText(apiKey, 256)
  if (!key) throw new Error('TVDB API key is empty')
  if (cachedToken && cachedToken.apiKey === key && cachedToken.expiresAt > Date.now()) {
    return cachedToken.token
  }
  const url = buildUrl(apiBase, '/login')
  const body = await fetchJson(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ apikey: key }),
  })
  const token = trimText(body?.data?.token, 4096)
  if (!token) throw new Error('TVDB login response missing token')
  cachedToken = { apiKey: key, token, expiresAt: Date.now() + TOKEN_TTL_MS }
  return token
}

async function tvdbGet(apiBase, token, path, query = {}) {
  return fetchJson(buildUrl(apiBase, path, query), {
    headers: { authorization: `Bearer ${token}` },
  })
}

function resultToMeta(result) {
  const externalId = normalizeTvdbId(result.tvdb_id ?? result.tvdbId ?? result.id)
  if (!externalId) return null
  const title = trimText(result.title ?? result.name_translated ?? result.name, 512)
  if (!title) return null
  const premiere = normalizeDate(result.first_air_time ?? result.firstAirTime)
  const yearRaw = trimText(result.year, 8)
  const productionYear = yearFromDate(premiere)
    ?? (yearRaw && /^\d{4}$/.test(yearRaw) ? Number(yearRaw) : null)
  const externalIds = mapRemoteIds(result.remote_ids ?? result.remoteIds)
  pushExternalId(externalIds, 'tvdb', externalId)
  return {
    title,
    overview: trimText(result.overview, 8000),
    productionYear,
    premiereDate: premiere,
    externalIds,
    artwork: mapArtwork(result.image_url ?? result.imageUrl, result.poster, result.thumbnail),
    genres: [],
    studios: [],
    people: [],
  }
}

function applyExtended(meta, extended) {
  const data = extended?.data
  if (!data) return
  if (!meta.overview) {
    const overview = trimText(data.overview, 8000)
    if (overview) meta.overview = overview
  }
  const genres = namedList((data.genres ?? []).map((g) => g.name))
  if (genres.length) meta.genres = genres
  const studios = namedList((data.companies ?? []).map((c) => c.name))
  if (studios.length) meta.studios = studios
  const people = mapPeople(data.characters ?? [])
  if (people.length) meta.people = people
}

export async function queryTvdb(cfg, lookup) {
  const apiKey = trimText(cfg.api_key, 256)
  if (!apiKey) return null
  const kind = searchKind(lookup.itemType)
  if (!kind) return null
  const title = trimText(lookup.title, 512) ?? trimText(lookup.originalTitle, 512)
  const tvdbHint = normalizeTvdbId(lookup.tvdbId)
  if (!title && !tvdbHint) return null

  const apiBase = trimText(cfg.api_base_url, 256) ?? DEFAULT_API

  try {
    const token = await login(apiBase, apiKey)

    // Direct-by-id path when recognition provided tvdbId.
    if (tvdbHint) {
      const id = numericId(tvdbHint)
      if (id) {
        try {
          const extended = await tvdbGet(
            apiBase,
            token,
            `/${kind === 'movie' ? 'movies' : 'series'}/${id}/extended`,
            { meta: 'translations' },
          )
          const data = extended?.data
          const meta = {
            title: trimText(data?.name ?? data?.nameTranslations?.[0] ?? title, 512) ?? String(id),
            overview: trimText(data?.overview, 8000),
            productionYear: yearFromDate(normalizeDate(data?.firstAired ?? data?.year))
              ?? lookup.productionYear
              ?? null,
            premiereDate: normalizeDate(data?.firstAired),
            externalIds: [{ provider: 'tvdb', externalId: String(id) }],
            artwork: mapArtwork(data?.image, data?.poster),
            genres: namedList((data?.genres ?? []).map((g) => g.name)),
            studios: namedList((data?.companies ?? []).map((c) => c.name)),
            people: mapPeople(data?.characters ?? []),
          }
          if (!contributionIsEmpty(meta) && trimText(meta.title)) return meta
        } catch {
          // fall through to search
        }
      }
    }

    if (!title) return null

    const query = { query: title, type: kind, limit: '10' }
    if (lookup.productionYear) query.year = String(lookup.productionYear)
    const response = await tvdbGet(apiBase, token, '/search', query)
    const results = response?.data ?? []
    const match = results
      .filter((r) => {
        const et = String(r.entity_type ?? r.type ?? '').trim().toLowerCase()
        return !et || et === kind
      })
      .map(resultToMeta)
      .find(Boolean)

    if (!match) return null

    const id = numericId(match.externalIds.find((x) => x.provider === 'tvdb')?.externalId)
    if (id) {
      try {
        const extended = await tvdbGet(
          apiBase,
          token,
          `/${kind === 'movie' ? 'movies' : 'series'}/${id}/extended`,
          { meta: 'translations' },
        )
        applyExtended(match, extended)
      } catch {
        // keep search-level
      }
    }

    return contributionIsEmpty(match) ? null : match
  } catch (err) {
    if (process.env.FBZ_PROVIDER_STUB_LOG === '1') {
      console.error(JSON.stringify({ provider: 'tvdb', error: String(err.message ?? err) }))
    }
    return null
  }
}
