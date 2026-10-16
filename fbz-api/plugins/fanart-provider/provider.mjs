/**
 * Fanart.tv enrichment client for metadata.provider.query.
 * Needs TMDB id (movie) or TVDB id (tv). Aligned with core fanart.rs.
 */
import {
  buildUrl,
  contributionIsEmpty,
  fetchJson,
  normalizeLanguage,
  resolveExternalId,
  safeHttpUrl,
  trimText,
  MAX_ARTWORK,
} from './utils.mjs'

const DEFAULT_API = 'https://webservice.fanart.tv/v3'
const MAX_FANART = 64

const MOVIE_FIELDS = [
  ['movieposter', 'poster'],
  ['moviebackground', 'backdrop'],
  ['hdmovielogo', 'logo'],
  ['movielogo', 'logo'],
  ['moviethumb', 'thumb'],
  ['moviebanner', 'banner'],
  ['moviedisc', 'thumb'], // host allowlist has no disc → thumb
  ['hdmovieclearart', 'thumb'],
  ['movieart', 'thumb'],
]

const TV_FIELDS = [
  ['tvposter', 'poster'],
  ['seasonposter', 'poster'],
  ['showbackground', 'backdrop'],
  ['hdtvlogo', 'logo'],
  ['clearlogo', 'logo'],
  ['tvthumb', 'thumb'],
  ['seasonthumb', 'thumb'],
  ['tvbanner', 'banner'],
  ['seasonbanner', 'banner'],
  ['hdclearart', 'thumb'],
  ['characterart', 'thumb'],
]

function fanartKind(itemType) {
  if (itemType === 'movie') return 'movies'
  if (itemType === 'series' || itemType === 'season' || itemType === 'episode') return 'tv'
  return null
}

function preferredLang(value) {
  const lang = normalizeLanguage(value)
  return lang ? lang.split('-')[0].toLowerCase() : null
}

function languageRank(language, preferred) {
  const lang = String(language ?? '').trim().toLowerCase()
  if (preferred && lang === preferred) return 0
  if (lang === 'en') return 1
  if (!lang || lang === '00') return 2
  return 3
}

function mapArtwork(response, kindPath, language) {
  const fields = kindPath === 'movies' ? MOVIE_FIELDS : TV_FIELDS
  const preferred = preferredLang(language)
  const seen = new Set()
  const primaryTypes = new Set()
  const artwork = []

  for (const [field, artworkType] of fields) {
    const values = response?.[field]
    if (!Array.isArray(values)) continue
    const candidates = values
      .map((v) => {
        const url = trimText(v?.url, 2048)
        if (!url) return null
        return {
          url,
          languageRank: languageRank(v?.lang, preferred),
          likes: Number.parseInt(String(v?.likes ?? '0'), 10) || 0,
        }
      })
      .filter(Boolean)
      .sort((a, b) => a.languageRank - b.languageRank || b.likes - a.likes || a.url.localeCompare(b.url))

    for (const c of candidates) {
      if (artwork.length >= Math.min(MAX_ARTWORK, MAX_FANART)) return artwork
      const remoteUrl = safeHttpUrl(c.url)
      if (!remoteUrl || seen.has(remoteUrl)) continue
      seen.add(remoteUrl)
      const isPrimary = !primaryTypes.has(artworkType)
      primaryTypes.add(artworkType)
      artwork.push({ artworkType, remoteUrl, isPrimary })
    }
  }
  return artwork
}

function resolveIds(lookup, current) {
  const tmdb = resolveExternalId(
    [
      lookup?.tmdbId,
      current?.provider?.toLowerCase?.() === 'tmdb' ? current.externalId : null,
      current?.externalIds,
      lookup?.externalIds,
    ],
    'tmdb',
  )
  const tvdb = resolveExternalId(
    [
      lookup?.tvdbId,
      current?.provider?.toLowerCase?.() === 'tvdb' ? current.externalId : null,
      current?.externalIds,
      lookup?.externalIds,
    ],
    'tvdb',
  )
  return { tmdb, tvdb }
}

export async function queryFanart(cfg, lookup, current = null) {
  const apiKey = trimText(cfg.api_key, 256)
  if (!apiKey) return null
  const kindPath = fanartKind(lookup.itemType)
  if (!kindPath) return null

  const { tmdb, tvdb } = resolveIds(lookup, current)
  const externalId = kindPath === 'movies' ? tmdb : tvdb
  if (!externalId) return null

  const apiBase = trimText(cfg.api_base_url, 256) ?? DEFAULT_API
  try {
    const url = buildUrl(apiBase, `/${kindPath}/${externalId}`, { api_key: apiKey })
    const response = await fetchJson(url)
    const artwork = mapArtwork(response, kindPath, lookup.language)
    if (!artwork.length) return null
    const meta = {
      externalIds: [
        {
          provider: kindPath === 'movies' ? 'tmdb' : 'tvdb',
          externalId: String(externalId),
        },
      ],
      artwork,
    }
    return contributionIsEmpty(meta) ? null : meta
  } catch (err) {
    if (err?.status === 404) return null
    if (process.env.FBZ_PROVIDER_STUB_LOG === '1') {
      console.error(JSON.stringify({ provider: 'fanart', error: String(err.message ?? err) }))
    }
    return null
  }
}
