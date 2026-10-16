/**
 * TMDB upstream client for metadata.provider.query.
 * Request semantics aligned with core src/metadata/provider/tmdb.rs (HTTP only; no core keys).
 */
import {
  buildUrl,
  clampRating,
  contributionIsEmpty,
  fetchJson,
  namedList,
  normalizeCountry,
  normalizeDate,
  normalizeLanguage,
  pushExternalId,
  safeHttpUrl,
  trimText,
  yearFromDate,
  MAX_ARTWORK,
  MAX_PEOPLE,
} from './utils.mjs'

const DEFAULT_API = 'https://api.themoviedb.org/3'
const DEFAULT_IMAGE = 'https://image.tmdb.org/t/p'

function searchKind(itemType) {
  if (itemType === 'movie') return 'movie'
  if (itemType === 'series' || itemType === 'season' || itemType === 'episode') return 'tv'
  return null
}

function yearParam(kind) {
  return kind === 'movie' ? 'year' : 'first_air_date_year'
}

function detailAppends(kind) {
  return kind === 'movie'
    ? 'credits,release_dates,external_ids,videos'
    : 'credits,content_ratings,external_ids,videos'
}

function imageUrl(imageBase, path, size = 'original') {
  const p = trimText(path, 512)
  if (!p || !p.startsWith('/') || p.includes('..') || /\s/.test(p)) return null
  return `${String(imageBase).replace(/\/+$/, '')}/${size}${p}`
}

function profileUrl(imageBase, path) {
  return imageUrl(imageBase, path, 'w185')
}

function artworkList(imageBase, posterPath, backdropPath) {
  const artwork = []
  const poster = imageUrl(imageBase, posterPath)
  if (poster) artwork.push({ artworkType: 'poster', remoteUrl: poster, isPrimary: true })
  const backdrop = imageUrl(imageBase, backdropPath)
  if (backdrop) artwork.push({ artworkType: 'backdrop', remoteUrl: backdrop, isPrimary: true })
  return artwork.slice(0, MAX_ARTWORK)
}

function crewRoleType(job, department) {
  const j = String(job ?? '').trim().toLowerCase()
  const d = String(department ?? '').trim().toLowerCase()
  if (j === 'director') return 'Director'
  if (['writer', 'screenplay', 'story', 'teleplay', 'creator'].includes(j)) return 'Writer'
  if (j === 'producer' || j === 'executive producer') return 'Producer'
  if (['composer', 'original music composer', 'music'].includes(j)) return 'Composer'
  if (d === 'directing') return 'Director'
  if (d === 'writing') return 'Writer'
  if (d === 'production') return 'Producer'
  if (d === 'sound' && j.includes('composer')) return 'Composer'
  return null
}

function mapPeople(credits, imageBase) {
  const people = []
  const seen = new Set()
  const push = (name, roleType, roleName, profilePath) => {
    const n = trimText(name, 256)
    if (!n || people.length >= MAX_PEOPLE) return
    const rn = trimText(roleName, 128) ?? ''
    const key = `${n.toLowerCase()}|${roleType}|${rn.toLowerCase()}`
    if (seen.has(key)) return
    seen.add(key)
    const person = { name: n, roleType }
    if (rn) person.roleName = rn
    const profile = profileUrl(imageBase, profilePath)
    if (profile) person.profileImageUrl = profile
    people.push(person)
  }

  for (const cast of credits?.cast ?? []) {
    push(cast.name, 'Actor', cast.character, cast.profile_path)
  }
  for (const crew of credits?.crew ?? []) {
    const role = crewRoleType(crew.job, crew.department)
    if (!role) continue
    push(crew.name, role, crew.job, crew.profile_path)
  }
  return people
}

function officialRating(detail, country, kind) {
  const preferred = normalizeCountry(country) ?? 'US'
  if (kind === 'movie') {
    const results = detail?.release_dates?.results ?? []
    const pick = (code) => {
      const row = results.find((r) => String(r?.iso_3166_1 ?? '').toUpperCase() === code)
      const dates = row?.release_dates ?? []
      // Prefer theatrical (3) then digital (4) then any with certification.
      const ranked = [...dates].sort((a, b) => {
        const rank = (t) => (t === 3 ? 0 : t === 4 ? 1 : t === 2 ? 2 : 3)
        return rank(a?.type) - rank(b?.type)
      })
      for (const d of ranked) {
        const cert = trimText(d?.certification, 32)
        if (cert) return cert
      }
      return null
    }
    return pick(preferred) ?? pick('US')
  }
  const results = detail?.content_ratings?.results ?? []
  const pick = (code) => {
    const row = results.find((r) => String(r?.iso_3166_1 ?? '').toUpperCase() === code)
    return trimText(row?.rating, 32)
  }
  return pick(preferred) ?? pick('US')
}

function applyDetail(meta, detail, imageBase, country, kind) {
  const title = trimText(detail.title ?? detail.name, 512)
  if (title) meta.title = title
  const original = trimText(detail.original_title ?? detail.original_name, 512)
  if (original) meta.originalTitle = original
  const overview = trimText(detail.overview, 8000)
  if (overview) meta.overview = overview
  const premiere = normalizeDate(detail.release_date ?? detail.first_air_date)
  if (premiere) {
    meta.premiereDate = premiere
    meta.productionYear = yearFromDate(premiere)
  }
  const rating = clampRating(detail.vote_average)
  if (rating != null) meta.communityRating = rating
  const official = officialRating(detail, country, kind)
  if (official) meta.officialRating = official

  const art = artworkList(imageBase, detail.poster_path, detail.backdrop_path)
  if (art.length) meta.artwork = art

  meta.genres = namedList((detail.genres ?? []).map((g) => g.name))
  meta.studios = namedList((detail.production_companies ?? []).map((c) => c.name))
  meta.people = mapPeople(detail.credits, imageBase)

  const externalIds = []
  pushExternalId(externalIds, 'tmdb', String(detail.id ?? meta.externalIds?.[0]?.externalId ?? ''))
  if (detail.external_ids?.imdb_id) pushExternalId(externalIds, 'imdb', detail.external_ids.imdb_id)
  if (kind === 'tv' && detail.external_ids?.tvdb_id) {
    pushExternalId(externalIds, 'tvdb', String(detail.external_ids.tvdb_id))
  }
  // Keep only real ids (drop empty tmdb push if id missing)
  meta.externalIds = externalIds.filter((x) => x.externalId && x.externalId !== 'undefined')
}

function applyEpisode(meta, episode, imageBase) {
  const title = trimText(episode.name, 512)
  if (title) meta.title = title
  const overview = trimText(episode.overview, 8000)
  if (overview) meta.overview = overview
  const premiere = normalizeDate(episode.air_date)
  if (premiere) {
    meta.premiereDate = premiere
    meta.productionYear = yearFromDate(premiere)
  }
  const rating = clampRating(episode.vote_average)
  if (rating != null) meta.communityRating = rating
  const still = imageUrl(imageBase, episode.still_path)
  if (still) {
    meta.artwork = [
      { artworkType: 'thumb', remoteUrl: still, isPrimary: true },
      ...(meta.artwork ?? []).map((a) => ({ ...a, isPrimary: false })),
    ].slice(0, MAX_ARTWORK)
  }
}

function applySeason(meta, season, imageBase) {
  const title = trimText(season.name, 512)
  if (title) meta.title = title
  const overview = trimText(season.overview, 8000)
  if (overview) meta.overview = overview
  const premiere = normalizeDate(season.air_date)
  if (premiere) {
    meta.premiereDate = premiere
    meta.productionYear = yearFromDate(premiere)
  }
  const poster = imageUrl(imageBase, season.poster_path)
  if (poster) {
    meta.artwork = [
      { artworkType: 'poster', remoteUrl: poster, isPrimary: true },
      ...(meta.artwork ?? []).map((a) => ({ ...a, isPrimary: false })),
    ].slice(0, MAX_ARTWORK)
  }
}

async function tmdbGet(apiBase, token, path, query = {}) {
  const url = buildUrl(apiBase, path, query)
  return fetchJson(url, {
    headers: { authorization: `Bearer ${token}` },
  })
}

async function enrichFromId(cfg, lookup, kind, id, base = {}) {
  const apiBase = trimText(cfg.api_base_url, 256) ?? DEFAULT_API
  const imageBase = trimText(cfg.image_base_url, 256) ?? DEFAULT_IMAGE
  const token = trimText(cfg.access_token, 4096)
  const language = normalizeLanguage(cfg.language ?? lookup.language)

  const detailQuery = { append_to_response: detailAppends(kind) }
  if (language) detailQuery.language = language

  const detail = await tmdbGet(apiBase, token, `/${kind}/${id}`, detailQuery)
  const meta = {
    title: base.title ?? null,
    originalTitle: base.originalTitle ?? null,
    overview: base.overview ?? null,
    productionYear: base.productionYear ?? null,
    premiereDate: base.premiereDate ?? null,
    communityRating: base.communityRating ?? null,
    officialRating: null,
    externalIds: [{ provider: 'tmdb', externalId: String(id) }],
    artwork: base.artwork ?? [],
    genres: [],
    studios: [],
    people: [],
  }
  applyDetail(meta, detail, imageBase, lookup.country, kind)

  // Localized artwork (best-effort).
  try {
    const include = language ? `${language.split('-')[0]},null` : 'null'
    const images = await tmdbGet(apiBase, token, `/${kind}/${id}/images`, {
      include_image_language: include,
    })
    const posters = images?.posters ?? []
    const backdrops = images?.backdrops ?? []
    const pick = (list) => {
      if (!list.length) return null
      const want = language?.split('-')[0]?.toLowerCase()
      const ranked = [...list].sort((a, b) => {
        const la = String(a.iso_639_1 ?? '').toLowerCase()
        const lb = String(b.iso_639_1 ?? '').toLowerCase()
        const rank = (l) => (want && l === want ? 0 : l === 'en' ? 1 : !l ? 2 : 3)
        return rank(la) - rank(lb) || (b.vote_average ?? 0) - (a.vote_average ?? 0)
      })
      return imageUrl(imageBase, ranked[0]?.file_path)
    }
    const poster = pick(posters)
    const backdrop = pick(backdrops)
    if (poster || backdrop) {
      const artwork = []
      if (poster) artwork.push({ artworkType: 'poster', remoteUrl: poster, isPrimary: true })
      if (backdrop) artwork.push({ artworkType: 'backdrop', remoteUrl: backdrop, isPrimary: true })
      meta.artwork = artwork
    }
  } catch {
    // keep detail artwork
  }

  if (lookup.itemType === 'episode' && lookup.season != null && lookup.episode != null) {
    try {
      const epQuery = language ? { language } : {}
      const ep = await tmdbGet(
        apiBase,
        token,
        `/tv/${id}/season/${lookup.season}/episode/${lookup.episode}`,
        epQuery,
      )
      applyEpisode(meta, ep, imageBase)
    } catch {
      // keep series-level
    }
  }

  if (lookup.itemType === 'season' && lookup.season != null) {
    try {
      const seQuery = language ? { language } : {}
      const se = await tmdbGet(apiBase, token, `/tv/${id}/season/${lookup.season}`, seQuery)
      applySeason(meta, se, imageBase)
    } catch {
      // keep series-level
    }
  }

  if (!trimText(meta.title)) return null
  if (contributionIsEmpty(meta)) return null
  return meta
}

async function searchTmdb(cfg, lookup, kind) {
  const apiBase = trimText(cfg.api_base_url, 256) ?? DEFAULT_API
  const imageBase = trimText(cfg.image_base_url, 256) ?? DEFAULT_IMAGE
  const token = trimText(cfg.access_token, 4096)
  const language = normalizeLanguage(cfg.language ?? lookup.language)
  const includeAdult = cfg.include_adult === true || cfg.include_adult === 'true'

  const titles = [lookup.title, lookup.originalTitle].map((t) => trimText(t, 512)).filter(Boolean)
  for (const title of titles) {
    const query = {
      query: title,
      include_adult: includeAdult ? 'true' : 'false',
    }
    if (lookup.productionYear) query[yearParam(kind)] = String(lookup.productionYear)
    if (language) query.language = language
    const region = normalizeCountry(lookup.country)
    if (region) query.region = region

    const response = await tmdbGet(apiBase, token, `/search/${kind}`, query)
    const result = (response?.results ?? []).find((r) => Number(r?.id) > 0)
    if (!result) continue

    const premiere = normalizeDate(result.release_date ?? result.first_air_date)
    const base = {
      title: trimText(result.title ?? result.name, 512),
      originalTitle: trimText(result.original_title ?? result.original_name, 512),
      overview: trimText(result.overview, 8000),
      premiereDate: premiere,
      productionYear: yearFromDate(premiere),
      communityRating: clampRating(result.vote_average),
      artwork: artworkList(imageBase, result.poster_path, result.backdrop_path),
    }
    const enriched = await enrichFromId(cfg, lookup, kind, result.id, base)
    if (enriched) return enriched
  }
  return null
}

/**
 * @returns {Promise<object|null>} metadata contribution or null
 */
export async function queryTmdb(cfg, lookup) {
  const token = trimText(cfg.access_token, 4096)
  if (!token) return null
  const kind = searchKind(lookup.itemType)
  if (!kind) return null
  if (!trimText(lookup.title) && !trimText(lookup.tmdbId)) return null

  const tmdbId = trimText(lookup.tmdbId, 32)
  if (tmdbId && /^\d+$/.test(tmdbId) && Number(tmdbId) > 0) {
    try {
      const byId = await enrichFromId(cfg, lookup, kind, Number(tmdbId))
      if (byId) return byId
    } catch {
      // fall through to search
    }
  }

  if (!trimText(lookup.title) && !trimText(lookup.originalTitle)) return null
  try {
    return await searchTmdb(cfg, lookup, kind)
  } catch (err) {
    if (process.env.FBZ_PROVIDER_STUB_LOG === '1') {
      console.error(JSON.stringify({ provider: 'tmdb', error: String(err.message ?? err) }))
    }
    return null
  }
}

// silence unused import lint for safeHttpUrl in some bundlers
void safeHttpUrl
