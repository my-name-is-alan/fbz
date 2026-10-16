// Official marker detect plugin — chapter-title + silence/black heuristics.
//
// Core worker gathers ffprobe chapters and ffmpeg silencedetect/blackdetect
// windows into request.probeSummary (no bare paths). This process maps them
// into intro_*/credits_* (and optional chapter) candidates for core persistence.
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const { createHttpPluginServer, listen, loadPluginConfig, parsePort } = await loadSdk()

const port = parsePort(process.env.PORT, 19106)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'
const DETECTOR_VERSION = 'marker-detect-0.2.0'
const TICKS_PER_MS = 10000

const INTRO_KEYS = ['intro', 'opening', 'op', 'cold open', 'title sequence', '片头', '開場', '开场', 'op曲']
const CREDITS_KEYS = ['credit', 'credits', 'ending credits', 'end credits', 'closing credits', '字幕', '演职员', '演職人員', '制作人员']
const OUTRO_KEYS = ['outro', 'ending', 'ed', 'epilogue', '片尾', '結尾', '结尾', 'ed曲']

async function loadSdk() {
  const here = dirname(fileURLToPath(import.meta.url))
  for (const candidate of [
    join(here, 'fbz-plugin-http.mjs'),
    join(here, '../../examples/plugins/_shared/fbz-plugin-http.mjs'),
  ]) {
    try {
      return await import(candidate)
    } catch {
      // try next
    }
  }
  throw new Error('fbz-plugin-http.mjs not found (package plugin or run from fbz-api tree)')
}

function classifyTitle(title) {
  const t = String(title ?? '').trim().toLowerCase()
  if (!t) return null
  if (INTRO_KEYS.some((key) => t.includes(key))) return 'intro'
  if (CREDITS_KEYS.some((key) => t.includes(key))) return 'credits'
  if (OUTRO_KEYS.some((key) => t.includes(key))) return 'outro'
  return null
}

function msToTicks(ms) {
  return Math.round(Number(ms) * TICKS_PER_MS)
}

function pushUnique(markers, seen, marker) {
  const key = `${marker.markerType}:${marker.startTicks}`
  if (seen.has(key)) return
  seen.add(key)
  markers.push(marker)
}

function detectFromChapters(chapters, emitPlain, markers, seen) {
  for (const chapter of chapters ?? []) {
    const title = chapter.title ?? chapter.Title ?? ''
    const startMs = Number(chapter.startMs ?? chapter.start_ms ?? 0)
    const endMsRaw = chapter.endMs ?? chapter.end_ms
    const endMs = endMsRaw == null ? undefined : Number(endMsRaw)
    const kind = classifyTitle(title)
    const startTicks = msToTicks(startMs)
    const endTicks = endMs == null ? startTicks + 90 * 10_000_000 : msToTicks(endMs)

    if (kind === 'intro') {
      pushUnique(markers, seen, { markerType: 'intro_start', startTicks, confidence: 0.88, title })
      pushUnique(markers, seen, { markerType: 'intro_end', startTicks: endTicks, confidence: 0.88, title })
      continue
    }
    if (kind === 'credits' || kind === 'outro') {
      pushUnique(markers, seen, { markerType: 'credits_start', startTicks, confidence: 0.88, title })
      pushUnique(markers, seen, { markerType: 'credits_end', startTicks: endTicks, confidence: 0.82, title })
      continue
    }
    if (emitPlain) {
      pushUnique(markers, seen, {
        markerType: 'chapter',
        startTicks,
        ...(endMs == null ? {} : { endTicks: msToTicks(endMs) }),
        confidence: 0.55,
        title,
      })
    }
  }
}

function longestInterval(intervals, predicate) {
  let best = null
  for (const interval of intervals ?? []) {
    const start = Number(interval.startMs ?? interval.start_ms ?? 0)
    const endRaw = interval.endMs ?? interval.end_ms
    const end = endRaw == null ? start : Number(endRaw)
    const duration = end - start
    if (!predicate(start, end, duration)) continue
    if (!best || duration > best.duration) {
      best = { start, end, duration }
    }
  }
  return best
}

function detectFromSilenceBlack(summary, durationMs, markers, seen) {
  const hasIntro = markers.some((marker) => marker.markerType === 'intro_start')
  const hasCredits = markers.some((marker) => marker.markerType === 'credits_start')
  const windowMs = Number(summary.analysisWindowMs ?? summary.analysis_window_ms ?? 180_000)
  const tailWindowMs = Number(summary.tailWindowMs ?? summary.tail_window_ms ?? 240_000)
  const silence = [...(summary.silence ?? []), ...(summary.black ?? [])]

  if (!hasIntro) {
    const intro = longestInterval(silence, (start, end, duration) => (
      start < windowMs && end <= windowMs && end >= 5_000 && duration >= 1_500
    ))
    if (intro) {
      pushUnique(markers, seen, { markerType: 'intro_start', startTicks: 0, confidence: 0.58 })
      pushUnique(markers, seen, {
        markerType: 'intro_end',
        startTicks: msToTicks(intro.end),
        confidence: 0.58,
      })
    }
  }

  if (!hasCredits && durationMs > 0) {
    const tailOrigin = Math.max(0, durationMs - Math.max(1, tailWindowMs))
    const credits = longestInterval(silence, (start, end, duration) => (
      start >= tailOrigin && duration >= 2_000
    ))
    if (credits) {
      pushUnique(markers, seen, {
        markerType: 'credits_start',
        startTicks: msToTicks(credits.start),
        confidence: 0.52,
      })
      pushUnique(markers, seen, {
        markerType: 'credits_end',
        startTicks: msToTicks(Math.min(credits.end, durationMs)),
        confidence: 0.52,
      })
    }
  }
}

function filterByConfidence(markers, minConfidence) {
  if (!Number.isFinite(minConfidence)) return markers
  return markers.filter((marker) => {
    if (marker.confidence == null) return true
    return marker.confidence >= minConfidence
  })
}

async function handleMarkerDetect(dispatch, context) {
  const request = dispatch.request ?? {}
  const config = await loadPluginConfig(context, {})
  const mode = String(config.detect_mode ?? config.detectMode ?? 'all').trim().toLowerCase()
  const minConfidence = Number(config.min_confidence ?? config.minConfidence ?? 0.5)
  const emitPlain = config.emit_plain_chapters === true || config.emitPlainChapters === true
  const probeSummary = request.probeSummary ?? request.probe_summary ?? {}
  const durationMs = Number(request.durationMs ?? request.duration_ms ?? 0)

  if (process.env.FBZ_MARKER_DETECT_LOG === '1') {
    console.log(JSON.stringify({
      hookEvent: dispatch.hookEvent,
      mediaItemId: request.mediaItemId ?? null,
      mediaFileId: request.mediaFileId ?? null,
      durationMs,
      chapterCount: (probeSummary.chapters ?? []).length,
      silenceCount: (probeSummary.silence ?? []).length,
      blackCount: (probeSummary.black ?? []).length,
      mode,
    }))
  }

  const markers = []
  const seen = new Set()

  if (mode === 'chapters' || mode === 'all' || mode === '') {
    detectFromChapters(probeSummary.chapters, emitPlain, markers, seen)
  }
  if (mode === 'silence' || mode === 'all' || mode === '') {
    detectFromSilenceBlack(probeSummary, durationMs, markers, seen)
  }

  const filtered = filterByConfidence(markers, minConfidence).slice(0, 32)
  return {
    markers: filtered,
    detectorVersion: DETECTOR_VERSION,
    skipped: filtered.length === 0,
    reason: filtered.length === 0 ? 'no candidates above threshold' : undefined,
  }
}

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY,
  async handleDispatch(dispatch, context) {
    if (
      dispatch.hookEvent === 'media.marker.detect'
      || dispatch.hookEvent === 'media.marker.detect.draft'
    ) {
      return handleMarkerDetect(dispatch, context)
    }
    return { ignored: true, reason: `unsupported dispatch ${dispatch.hookEvent}` }
  },
})

listen(server, {
  name: 'marker detect',
  host: process.env.HOST ?? '127.0.0.1',
  port,
  path,
})
