// Official LRCLIB lyrics provider (lyrics.provider.query).
import { createHttpPluginServer, listen, loadPluginConfig, parsePort } from './fbz-plugin-http.mjs'
import { download, search } from './provider.mjs'

const port = parsePort(process.env.PORT, 19109)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY,
  async handleDispatch(dispatch, context) {
    if (dispatch.hookEvent !== 'lyrics.provider.query') {
      return { ignored: true, reason: `unsupported dispatch ${dispatch.hookEvent}` }
    }
    const request = dispatch.request ?? {}
    let config = { base_url: process.env.LRCLIB_BASE_URL }
    try { config = { ...config, ...(await loadPluginConfig(context, {})) } } catch { /* host config optional */ }
    if (request.action === 'download') return download(request, config)
    return search(request, config)
  },
})

listen(server, { name: 'lrclib lyrics provider', host: process.env.HOST ?? '127.0.0.1', port, path })
