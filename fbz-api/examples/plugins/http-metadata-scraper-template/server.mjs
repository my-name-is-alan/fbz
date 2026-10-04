// First-party example: a synchronous metadata scraper plugin.
//
// FBZ invokes subscribers of `metadata.provider.query` inline during metadata
// refresh jobs (see docs/plugin-system.md "Synchronous Provider Invocation").
// The request body carries `invocation: "sync"` plus the recognition-layer
// lookup; the plugin answers with a metadata contribution. Merge rules are
// host-side: built-in providers win, the plugin fills empty fields, and it
// becomes the fallback base match only when no built-in provider matched.
//
// This template answers from a local JSON fixture keyed by normalized title.
// A real scraper would call its upstream (Douban, Bangumi, a private NFO
// service, ...) here instead.
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const { createHttpPluginServer, listen, parsePort } = await loadSdk()

const port = parsePort(process.env.PORT, 19093)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'
const fixturePath = process.env.SCRAPER_FIXTURE_PATH
  ?? join(dirname(fileURLToPath(import.meta.url)), 'scraper-fixture.json')

async function loadSdk() {
  try {
    return await import('./fbz-plugin-http.mjs')
  }
  catch {
    return import('../_shared/fbz-plugin-http.mjs')
  }
}

/** Lowercase, collapse whitespace and strip punctuation for title matching. */
function normalizeTitle(value) {
  return String(value ?? '')
    .toLowerCase()
    .replace(/[._\-:!?,()[\]{}]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

let fixtureCache
async function fixture() {
  if (!fixtureCache) {
    fixtureCache = JSON.parse(await readFile(fixturePath, 'utf8'))
  }
  return fixtureCache
}

/**
 * Sync provider query handler. `dispatch.request.lookup` carries the
 * recognition output: { itemType, title, originalTitle, productionYear,
 * season, episode, tmdbId, imdbId, tvdbId, language, country }.
 * Returning `{}` (or `{ metadata: null }`) means "no contribution".
 */
async function handleProviderQuery(dispatch) {
  const lookup = dispatch.request?.lookup ?? {}
  const titles = [lookup.title, lookup.originalTitle].filter(Boolean)

  const database = await fixture()
  for (const title of titles) {
    const entry = database[normalizeTitle(title)]
    if (!entry) continue
    // Year sanity check when both sides know it (±1 tolerates region skew).
    if (
      entry.productionYear
      && lookup.productionYear
      && Math.abs(entry.productionYear - lookup.productionYear) > 1
    ) {
      continue
    }
    return { metadata: entry }
  }

  return { metadata: null }
}

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY,
  async handleDispatch(dispatch) {
    // One server can serve both async hook dispatches and sync queries;
    // sync provider queries are marked with `invocation: "sync"`.
    if (dispatch.invocation === 'sync' && dispatch.hookEvent === 'metadata.provider.query') {
      return handleProviderQuery(dispatch)
    }
    return { ignored: true, reason: `unsupported dispatch ${dispatch.hookEvent}` }
  }
})

listen(server, { name: 'demo metadata scraper', host: process.env.HOST ?? '127.0.0.1', port, path })
