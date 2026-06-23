import { createHash, createHmac, timingSafeEqual } from 'node:crypto'
import { createServer } from 'node:http'

const port = Number.parseInt(process.env.PORT ?? '19091', 10)
const path = process.env.PLUGIN_PATH ?? '/fbz-plugin'
const signatureSecret = process.env.PLUGIN_SECRET_KEY ?? ''
const maxProcessedKeys = Number.parseInt(process.env.PROCESSED_KEY_CACHE_LIMIT ?? '10000', 10)
const processedKeys = new Set()

function readBody(request, limit = 1024 * 1024) {
  return new Promise((resolve, reject) => {
    const chunks = []
    let size = 0

    request.on('data', (chunk) => {
      size += chunk.length
      if (size > limit) {
        reject(new Error(`request body exceeded ${limit} bytes`))
        request.destroy()
        return
      }
      chunks.push(chunk)
    })
    request.on('end', () => resolve(Buffer.concat(chunks)))
    request.on('error', reject)
  })
}

function safeEqualHex(actual, expected) {
  const actualBuffer = Buffer.from(actual, 'hex')
  const expectedBuffer = Buffer.from(expected, 'hex')
  return actualBuffer.length === expectedBuffer.length && timingSafeEqual(actualBuffer, expectedBuffer)
}

function verifySignature(headers, body) {
  if (!signatureSecret) {
    return
  }

  const version = headers['x-fbz-plugin-signature-version']
  const timestamp = headers['x-fbz-plugin-signature-timestamp']
  const pluginId = headers['x-fbz-plugin-id']
  const idempotencyKey = headers['x-fbz-plugin-idempotency-key']
  const bodySha256 = headers['x-fbz-plugin-body-sha256']
  const signature = headers['x-fbz-plugin-signature']
  if (!version || !timestamp || !pluginId || !idempotencyKey || !bodySha256 || !signature) {
    throw new Error('missing plugin signature headers')
  }

  const ageSeconds = Math.abs(Math.floor(Date.now() / 1000) - Number.parseInt(timestamp, 10))
  if (!Number.isFinite(ageSeconds) || ageSeconds > 300) {
    throw new Error('stale plugin signature timestamp')
  }

  const computedBodySha256 = createHash('sha256').update(body).digest('hex')
  if (!safeEqualHex(bodySha256, computedBodySha256)) {
    throw new Error('plugin body sha256 mismatch')
  }

  const canonical = `${version}\n${timestamp}\n${pluginId}\n${idempotencyKey}\n${computedBodySha256}`
  const expected = createHmac('sha256', signatureSecret).update(canonical).digest('hex')
  const actual = signature.replace(/^sha256=/, '')
  if (!safeEqualHex(actual, expected)) {
    throw new Error('plugin signature mismatch')
  }
}

function rememberIdempotencyKey(key) {
  processedKeys.add(key)

  if (Number.isFinite(maxProcessedKeys) && maxProcessedKeys > 0 && processedKeys.size > maxProcessedKeys) {
    const oldest = processedKeys.values().next().value
    if (oldest) {
      processedKeys.delete(oldest)
    }
  }
}

async function resolveChannel(hostBaseUrl, token) {
  const fallback = process.env.NOTIFICATION_CHANNEL || undefined
  const response = await fetch(new URL('/api/plugin/config', hostBaseUrl), {
    headers: {
      'x-fbz-plugin-token': token
    }
  })

  if (!response.ok) {
    return fallback
  }

  const { values } = await response.json()
  const channel = values?.channel
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

async function forwardNotification(dispatch, headers) {
  const hostBaseUrl = headers['x-fbz-host-base-url']
  const token = headers['x-fbz-plugin-token']
  if (!hostBaseUrl || !token) {
    throw new Error('missing host base URL or plugin token')
  }

  const channel = await resolveChannel(hostBaseUrl, token)
  const response = await fetch(new URL('/api/plugin/notifications', hostBaseUrl), {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-fbz-plugin-token': token
    },
    body: JSON.stringify(notificationForDispatch(dispatch, channel))
  })

  if (!response.ok) {
    const body = await response.text()
    throw new Error(`Host API notification request failed: ${response.status} ${body}`)
  }
}

const server = createServer(async (request, response) => {
  try {
    if (request.method !== 'POST' || new URL(request.url, 'http://127.0.0.1').pathname !== path) {
      response.writeHead(404).end()
      return
    }

    const body = await readBody(request)
    verifySignature(request.headers, body)

    const dispatch = JSON.parse(body.toString('utf8'))
    dispatch.idempotencyKey = request.headers['x-fbz-plugin-idempotency-key']
    if (dispatch.idempotencyKey && processedKeys.has(dispatch.idempotencyKey)) {
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ deduped: true }))
      return
    }

    await forwardNotification(dispatch, request.headers)
    if (dispatch.idempotencyKey) {
      rememberIdempotencyKey(dispatch.idempotencyKey)
    }

    response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ ok: true }))
  }
  catch (error) {
    response
      .writeHead(500, { 'content-type': 'application/json' })
      .end(JSON.stringify({ error: String(error.message ?? error) }))
  }
})

server.listen(port, '127.0.0.1', () => {
  console.log(`FBZ HTTP notification bridge listening on http://127.0.0.1:${port}${path}`)
})
