// Official book metadata provider (metadata.provider.query, itemType=book only).
import { createHttpPluginServer, listen, loadPluginConfig, parsePort } from './fbz-plugin-http.mjs'
import { queryBooks } from './provider.mjs'
import { resolvePluginConfig } from './utils.mjs'

const port = parsePort(process.env.PORT, 19108)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY,
  async handleDispatch(dispatch, context) {
    if (dispatch.invocation !== 'sync' || dispatch.hookEvent !== 'metadata.provider.query') {
      return { ignored: true, reason: `unsupported dispatch ${dispatch.hookEvent}` }
    }
    const lookup = dispatch.request?.lookup ?? {}
    const config = await resolvePluginConfig(dispatch, context, loadPluginConfig, {
      google_books_api_key: 'GOOGLE_BOOKS_API_KEY',
      sources: 'BOOK_PROVIDER_SOURCES',
    })
    try {
      return { metadata: await queryBooks(config, lookup) }
    } catch (err) {
      console.error(JSON.stringify({ plugin: 'fbz.provider.books', error: String(err?.message ?? err) }))
      return { metadata: null }
    }
  },
})

listen(server, { name: 'book metadata provider', host: process.env.HOST ?? '127.0.0.1', port, path })
