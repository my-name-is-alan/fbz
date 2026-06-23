const {
  createHttpPluginServer,
  hostJson,
  listen,
  loadPluginConfig,
  parsePort
} = await loadSdk()

const port = parsePort(process.env.PORT, 19091)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'

async function loadSdk() {
  try {
    return await import('./fbz-plugin-http.mjs')
  }
  catch {
    return import('../_shared/fbz-plugin-http.mjs')
  }
}

async function resolveChannel(context) {
  const fallback = process.env.NOTIFICATION_CHANNEL || undefined
  const values = await loadPluginConfig(context, {})
  const channel = values.channel
  return typeof channel === 'string' && channel.trim() ? channel.trim() : fallback
}

function notificationForDispatch(dispatch, channel) {
  const hookEvent = dispatch.hookEvent ?? 'unknown'
  const payload = dispatch.source?.payload ?? dispatch.source ?? {}
  const level = hookEvent.endsWith('.failed') ? 'error' : 'info'
  const title = `FBZ ${hookEvent}`
  const message = JSON.stringify(payload, null, 2).slice(0, 3500)

  return {
    title,
    message: message || `Received ${hookEvent}`,
    level,
    channel,
    metadata: {
      hookEvent,
      handler: dispatch.handler,
      idempotencyKey: dispatch.idempotencyKey
    }
  }
}

async function forwardNotification(dispatch, context) {
  const channel = await resolveChannel(context)
  await hostJson(context, '/api/plugin/notifications', {
    method: 'POST',
    body: JSON.stringify(notificationForDispatch(dispatch, channel))
  })

  return { forwarded: true, channel }
}

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY ?? '',
  idempotencyCacheLimit: Number.parseInt(process.env.PROCESSED_KEY_CACHE_LIMIT ?? '10000', 10),
  handleDispatch: forwardNotification
})

listen(server, {
  name: 'FBZ HTTP notification bridge',
  port,
  path
})
