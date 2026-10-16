// Official OpenSubtitles remote subtitle provider (subtitle.provider.query).
import { createHttpPluginServer, listen, loadPluginConfig, parsePort } from './fbz-plugin-http.mjs'
import { download, search } from './provider.mjs'

const port = parsePort(process.env.PORT, 19107)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY,
  async handleDispatch(dispatch, context) {
    if (dispatch.hookEvent !== 'subtitle.provider.query') {
      return { ignored: true, reason: `unsupported dispatch ${dispatch.hookEvent}` }
    }
    const request = dispatch.request ?? {}
    const config = {
      api_key: process.env.OPENSUBTITLES_API_KEY,
      user_agent: process.env.OPENSUBTITLES_USER_AGENT,
      ...(await loadPluginConfig(context, {})),
    }
    if (request.action === 'download') return download(request, config)
    return search(request, config)
  },
})

listen(server, { name: 'opensubtitles provider', host: process.env.HOST ?? '127.0.0.1', port, path })
