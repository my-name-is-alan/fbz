/**
 * Shared helpers for official metadata.provider.query plugins.
 * Packaged plugins should ship a copy as ./utils.mjs beside server.mjs.
 */

export const FETCH_TIMEOUT_MS = 12_000
export const MAX_PEOPLE = 100
export const MAX_ARTWORK = 16
export const MAX_EXTERNAL_IDS = 32
export const MAX_NAMED = 32

export function trimText(value, max = 4096) {
  if (value == null) return null
  const text = String(value).trim()
  if (!text) return null
  return text.length > max ? text.slice(0, max) : text
}

export function normalizeLanguage(value) {
  const text = trimText(value, 32)
  if (!text) return null
  const primary = text.replace('_', '-').split('-')[0]?.toLowerCase()
  if (!primary || !/^[a-z]{2,3}$/.test(primary)) return null
  const region = text.includes('-') || text.includes('_')
    ? text.replace('_', '-').split('-')[1]?.toUpperCase()
    : null
  return region && /^[A-Z]{2}$/.test(region) ? `${primary}-${region}` : primary
}

export function normalizeCountry(value) {
  const text = trimText(value, 8)?.toUpperCase()
  return text && /^[A-Z]{2}$/.test(text) ? text : null
}

export function normalizeDate(value) {
  const text = trimText(value, 32)
  if (!text) return null
  const m = text.match(/^(\d{4})-(\d{2})-(\d{2})/)
  return m ? `${m[1]}-${m[2]}-${m[3]}` : null
}

export function yearFromDate(date) {
  if (!date || date.length < 4) return null
  const year = Number.parseInt(date.slice(0, 4), 10)
  return Number.isFinite(year) && year > 1800 && year < 2100 ? year : null
}

export function safeHttpUrl(value) {
  const text = trimText(value, 2048)
  if (!text) return null
  try {
    const url = new URL(text)
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return null
    if (text.includes('..') || /\s/.test(text)) return null
    return url.toString()
  } catch {
    return null
  }
}

export function clampRating(value) {
  const n = typeof value === 'number' ? value : Number.parseFloat(value)
  if (!Number.isFinite(n)) return null
  return Math.min(10, Math.max(0, n))
}

export function namedList(values, max = MAX_NAMED) {
  const out = []
  const seen = new Set()
  for (const raw of values ?? []) {
    const name = trimText(typeof raw === 'string' ? raw : raw?.name, 128)
    if (!name) continue
    const key = name.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    out.push(name)
    if (out.length >= max) break
  }
  return out
}

export function pushExternalId(list, provider, externalId) {
  const p = trimText(provider, 32)?.toLowerCase()
  const id = trimText(externalId, 128)
  if (!p || !id) return
  if (list.some((x) => x.provider === p && x.externalId === id)) return
  if (list.length >= MAX_EXTERNAL_IDS) return
  list.push({ provider: p, externalId: id })
}

export function resolveExternalId(sources, provider) {
  const want = provider.toLowerCase()
  for (const src of sources) {
    if (!src) continue
    if (typeof src === 'string') {
      const id = trimText(src, 128)
      if (id) return id
      continue
    }
    if (Array.isArray(src)) {
      const hit = src.find((x) => String(x?.provider ?? '').toLowerCase() === want)
      const id = trimText(hit?.externalId ?? hit?.external_id, 128)
      if (id) return id
      continue
    }
    if (typeof src === 'object') {
      if (String(src.provider ?? '').toLowerCase() === want) {
        const id = trimText(src.externalId ?? src.external_id, 128)
        if (id) return id
      }
    }
  }
  return null
}

/**
 * Resolve secrets/settings: host config API → dispatch.config → process.env.
 * envKeys maps configSchema key → process.env name, e.g. { access_token: 'TMDB_ACCESS_TOKEN' }.
 */
export async function resolvePluginConfig(dispatch, context, loadPluginConfig, envKeys = {}) {
  let hostConfig = {}
  if (typeof loadPluginConfig === 'function' && context?.token && context?.hostBaseUrl) {
    try {
      hostConfig = await loadPluginConfig(context, {})
    } catch {
      hostConfig = {}
    }
  }
  const dispatchConfig = dispatch?.config && typeof dispatch.config === 'object' ? dispatch.config : {}
  const merged = { ...dispatchConfig, ...hostConfig }
  for (const [configKey, envName] of Object.entries(envKeys)) {
    if ((merged[configKey] == null || merged[configKey] === '') && process.env[envName]) {
      merged[configKey] = process.env[envName]
    }
  }
  return merged
}

export async function fetchJson(url, options = {}) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), options.timeoutMs ?? FETCH_TIMEOUT_MS)
  try {
    const response = await fetch(url, {
      ...options,
      signal: controller.signal,
      headers: {
        accept: 'application/json',
        'user-agent': options.userAgent ?? 'fbz-official-metadata-provider/0.1',
        ...(options.headers ?? {}),
      },
    })
    const text = await response.text()
    if (!response.ok) {
      const err = new Error(`HTTP ${response.status}: ${text.slice(0, 200)}`)
      err.status = response.status
      throw err
    }
    if (!text) return null
    return JSON.parse(text)
  } finally {
    clearTimeout(timer)
  }
}

export function buildUrl(base, path, query = {}) {
  const root = String(base ?? '').replace(/\/+$/, '')
  const suffix = path.startsWith('/') ? path : `/${path}`
  const url = new URL(`${root}${suffix}`)
  for (const [key, value] of Object.entries(query)) {
    if (value == null || value === '') continue
    url.searchParams.set(key, String(value))
  }
  return url.toString()
}

export function contributionIsEmpty(meta) {
  if (!meta || typeof meta !== 'object') return true
  return !(
    meta.title
    || meta.originalTitle
    || meta.overview
    || meta.productionYear
    || meta.premiereDate
    || meta.officialRating
    || meta.communityRating
    || (meta.externalIds?.length)
    || (meta.artwork?.length)
    || (meta.genres?.length)
    || (meta.studios?.length)
    || (meta.people?.length)
  )
}
