// Official metadata provider: TMDB
// Contract: SyncInvoker POSTs metadata.provider.query with invocation: "sync".
// Secrets: plugin configSchema / Host /api/plugin/config / process env only.
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { queryTmdb } from './provider.mjs'
import { resolvePluginConfig } from './utils.mjs'

const { createHttpPluginServer, listen, parsePort, loadPluginConfig } = await loadSdk()

const port = parsePort(process.env.PORT, 19101)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'
const pluginId = process.env.FBZ_PLUGIN_ID ?? 'fbz.provider.tmdb'

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

async function handleProviderQuery(dispatch, context) {
  const lookup = dispatch.request?.lookup ?? {}
  const current = dispatch.request?.current ?? null
  const config = await resolvePluginConfig(dispatch, context, loadPluginConfig, { access_token: 'TMDB_ACCESS_TOKEN', api_base_url: 'TMDB_API_BASE_URL', image_base_url: 'TMDB_IMAGE_BASE_URL', language: 'TMDB_LANGUAGE' })

  if (process.env.FBZ_PROVIDER_STUB_LOG === '1') {
    console.log(JSON.stringify({
      pluginId,
      hookEvent: dispatch.hookEvent,
      itemType: lookup.itemType ?? null,
      title: lookup.title ?? null,
      productionYear: lookup.productionYear ?? null,
      hasConfigKeys: Object.keys(config).filter((k) => config[k]).length,
    }))
  }

  try {
    const metadata = await queryTmdb(config, lookup)
    return { metadata: metadata ?? null }
  } catch (err) {
    console.error(JSON.stringify({
      pluginId,
      error: String(err?.message ?? err),
    }))
    return { metadata: null }
  }
}

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY,
  async handleDispatch(dispatch, context) {
    if (dispatch.invocation === 'sync' && dispatch.hookEvent === 'metadata.provider.query') {
      return handleProviderQuery(dispatch, context)
    }
    return { ignored: true, reason: `unsupported dispatch ${dispatch.hookEvent}` }
  }
})

listen(server, {
  name: 'official TMDB provider',
  host: process.env.HOST ?? '127.0.0.1',
  port,
  path,
})
