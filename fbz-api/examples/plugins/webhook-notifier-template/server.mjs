const {
  createHttpPluginServer,
  hostJson,
  listen,
  loadPluginConfig,
  parsePort
} = await loadSdk()

const port = parsePort(process.env.PORT, 19095)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'
const defaultChannel = 'webhook'
const defaultTitlePrefix = 'FBZ Webhook'

async function loadSdk() {
  try {
    return await import('./fbz-plugin-http.mjs')
  }
  catch {
    return import('../_shared/fbz-plugin-http.mjs')
  }
}

function configText(value, fallback) {
  return typeof value === 'string' && value.trim() ? value.trim() : fallback
}

function configBoolean(value, fallback) {
  if (typeof value === 'boolean') {
    return value
  }
  if (typeof value === 'string') {
    return value.toLowerCase() === 'true'
  }
  return fallback
}

function payloadForDispatch(dispatch) {
  return dispatch.source?.payload ?? dispatch.source ?? {}
}

function levelForHook(hookEvent) {
  return String(hookEvent).endsWith('.failed') ? 'error' : 'info'
}

function summaryForDispatch(hookEvent, payload) {
  const libraryName = payload.libraryName ?? payload.libraryId
  const itemName = payload.itemName ?? payload.itemId
  const username = payload.username ?? payload.userId

  if (hookEvent.startsWith('library.scan') && libraryName) {
    return `Library scan event for ${libraryName}.`
  }
  if (hookEvent.startsWith('metadata.refresh') && itemName) {
    return `Metadata refresh event for ${itemName}.`
  }
  if (hookEvent.startsWith('transcode') && itemName) {
    return `Transcode event for ${itemName}.`
  }
  if (hookEvent === 'user.login' && username) {
    return `User login event for ${username}.`
  }
  return `Received ${hookEvent}.`
}

function notificationForDispatch(dispatch, config) {
  const hookEvent = dispatch.hookEvent ?? 'unknown'
  const payload = payloadForDispatch(dispatch)
  const channel = configText(config.channel, process.env.NOTIFICATION_CHANNEL || defaultChannel)
  const titlePrefix = configText(config.title_prefix ?? config.titlePrefix, defaultTitlePrefix)
  const includePayload = configBoolean(config.include_payload ?? config.includePayload, false)
  const payloadText = includePayload
    ? `\n\n${JSON.stringify(payload, null, 2).slice(0, 2800)}`
    : ''

  return {
    title: `${titlePrefix} ${hookEvent}`.slice(0, 180),
    message: `${summaryForDispatch(hookEvent, payload)}${payloadText}`.slice(0, 3500),
    level: levelForHook(hookEvent),
    channel,
    metadata: {
      template: 'webhook',
      hookEvent,
      handler: dispatch.handler,
      idempotencyKey: dispatch.idempotencyKey
    }
  }
}

async function notify(dispatch, context) {
  const config = await loadPluginConfig(context, {})
  const notification = notificationForDispatch(dispatch, config)
  await hostJson(context, '/api/plugin/notifications', {
    method: 'POST',
    body: JSON.stringify(notification)
  })

  return { notified: true, channel: notification.channel }
}

const server = createHttpPluginServer({
  path,
  signatureSecret: process.env.PLUGIN_SECRET_KEY ?? '',
  idempotencyCacheLimit: Number.parseInt(process.env.PROCESSED_KEY_CACHE_LIMIT ?? '10000', 10),
  handleDispatch: notify
})

listen(server, {
  name: 'FBZ webhook notification template',
  port,
  path
})
