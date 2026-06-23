import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const {
  createHttpPluginServer,
  hostJson,
  listen,
  loadPluginConfig,
  parsePort,
  readLimitedResponse
} = await loadSdk()

const port = parsePort(process.env.PORT, 19092)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'
const markerFixturePath = process.env.MARKER_FIXTURE_PATH
  ?? join(dirname(fileURLToPath(import.meta.url)), 'marker-fixture.json')
const supportedMarkerTypes = new Set([
  'intro_start',
  'intro_end',
  'credits_start',
  'credits_end',
  'commercial',
  'chapter'
])

async function loadSdk() {
  try {
    return await import('./fbz-plugin-http.mjs')
  }
  catch {
    return import('../_shared/fbz-plugin-http.mjs')
  }
}

function itemIdFromDispatch(dispatch) {
  return dispatch.source?.payload?.itemId
    ?? dispatch.source?.aggregateId
    ?? dispatch.itemId
}

function preferredExternalIds(dispatch, item) {
  const ids = []
  const provider = dispatch.source?.payload?.provider
  const externalId = dispatch.source?.payload?.externalId
  if (provider && externalId) {
    ids.push({ provider, externalId })
  }
  for (const value of item.externalIds ?? []) {
    if (value?.provider && value?.externalId) {
      ids.push(value)
    }
  }
  return ids
}

async function loadFixtureMap() {
  const raw = await readFile(markerFixturePath, 'utf8')
  return JSON.parse(raw)
}

async function loadRemoteMarkers(markerSourceUrl, item, externalId) {
  const url = new URL(markerSourceUrl)
  url.searchParams.set('itemId', item.id)
  url.searchParams.set('itemType', item.itemType)
  url.searchParams.set('provider', externalId.provider)
  url.searchParams.set('externalId', externalId.externalId)
  if (item.seasonNumber != null) {
    url.searchParams.set('seasonNumber', String(item.seasonNumber))
  }
  if (item.episodeNumber != null) {
    url.searchParams.set('episodeNumber', String(item.episodeNumber))
  }

  const response = await fetch(url, { headers: { accept: 'application/json' } })
  const body = await readLimitedResponse(response)
  if (!response.ok) {
    throw new Error(`marker source failed: ${response.status} ${body}`)
  }
  return JSON.parse(body)
}

function markerKeys(externalId, item) {
  const provider = String(externalId.provider).trim().toLowerCase()
  const id = String(externalId.externalId).trim()
  const keys = [`${provider}:${id}`]
  if (item.seasonNumber != null && item.episodeNumber != null) {
    keys.push(`${provider}:${id}:season-${item.seasonNumber}:episode-${item.episodeNumber}`)
  }
  return keys
}

async function markerCandidates(config, item, externalId) {
  const markerSourceUrl = config.marker_source_url ?? config.markerSourceUrl
  if (typeof markerSourceUrl === 'string' && markerSourceUrl.trim()) {
    const payload = await loadRemoteMarkers(markerSourceUrl, item, externalId)
    return Array.isArray(payload) ? payload : payload.markers
  }

  const markerMap = await loadFixtureMap()
  for (const key of markerKeys(externalId, item)) {
    if (Array.isArray(markerMap[key])) {
      return markerMap[key]
    }
  }
  return []
}

function normalizeMarkers(candidates, minConfidence) {
  const markers = []
  const seen = new Set()
  for (const candidate of candidates ?? []) {
    const markerType = String(candidate.markerType ?? candidate.marker_type ?? '').trim().toLowerCase()
    const startTicks = Number(candidate.startTicks ?? candidate.start_ticks)
    const endTicksValue = candidate.endTicks ?? candidate.end_ticks
    const endTicks = endTicksValue == null ? undefined : Number(endTicksValue)
    const confidenceValue = candidate.confidence
    const confidence = confidenceValue == null ? undefined : Number(confidenceValue)

    if (!supportedMarkerTypes.has(markerType) || !Number.isSafeInteger(startTicks) || startTicks < 0) {
      continue
    }
    if (endTicks !== undefined && (!Number.isSafeInteger(endTicks) || endTicks < startTicks)) {
      continue
    }
    if (confidence !== undefined && (!Number.isFinite(confidence) || confidence < 0 || confidence > 1)) {
      continue
    }
    if (confidence !== undefined && Number.isFinite(minConfidence) && confidence < minConfidence) {
      continue
    }

    const dedupeKey = `${markerType}:${startTicks}`
    if (seen.has(dedupeKey)) {
      continue
    }
    seen.add(dedupeKey)

    markers.push({
      markerType,
      startTicks,
      ...(endTicks === undefined ? {} : { endTicks }),
      ...(confidence === undefined ? {} : { confidence })
    })
    if (markers.length >= 512) {
      break
    }
  }
  return markers
}

async function importMarkers(dispatch, context) {
  if (dispatch.hookEvent !== 'metadata.refresh.completed') {
    return { skipped: true, reason: 'unsupported hook event' }
  }

  const itemId = itemIdFromDispatch(dispatch)
  if (!itemId) {
    return { skipped: true, reason: 'missing itemId' }
  }

  const config = await loadPluginConfig(context, {})
  const item = await hostJson(context, `/api/plugin/items/${encodeURIComponent(itemId)}`)
  const minConfidence = Number(config.min_confidence ?? config.minConfidence ?? 0)
  const source = typeof config.source === 'string' && config.source.trim() ? config.source.trim() : 'marker-importer'

  for (const externalId of preferredExternalIds(dispatch, item)) {
    const candidates = await markerCandidates(config, item, externalId)
    const markers = normalizeMarkers(candidates, minConfidence)
    if (markers.length === 0 && (config.clear_when_missing ?? config.clearWhenMissing) !== true) {
      continue
    }

    const result = await hostJson(context, `/api/plugin/items/${encodeURIComponent(item.id)}/markers`, {
      method: 'PUT',
      body: JSON.stringify({
        source,
        markers
      })
    })
    return {
      itemId: item.id,
      provider: externalId.provider,
      externalId: externalId.externalId,
      markerCount: result.markerCount ?? markers.length,
      cleared: markers.length === 0
    }
  }

  return { skipped: true, reason: 'no marker candidates' }
}

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY ?? '',
  idempotencyCacheLimit: Number.parseInt(process.env.PROCESSED_KEY_CACHE_LIMIT ?? '10000', 10),
  handleDispatch: importMarkers
})

listen(server, {
  name: 'FBZ HTTP marker importer',
  port,
  path
})
