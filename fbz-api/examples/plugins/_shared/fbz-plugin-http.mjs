import { createHash, createHmac, timingSafeEqual } from 'node:crypto'
import { createServer } from 'node:http'

const DEFAULT_BODY_LIMIT_BYTES = 1024 * 1024
const DEFAULT_RESPONSE_LIMIT_BYTES = 1024 * 1024
const DEFAULT_SIGNATURE_TOLERANCE_SECONDS = 300

export function parsePort(value, fallback) {
  const port = Number.parseInt(value ?? '', 10)
  return Number.isInteger(port) && port > 0 && port <= 65535 ? port : fallback
}

export function readBody(request, limit = DEFAULT_BODY_LIMIT_BYTES) {
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

export async function readLimitedResponse(response, limit = DEFAULT_RESPONSE_LIMIT_BYTES) {
  const reader = response.body?.getReader()
  if (!reader) {
    return ''
  }

  const chunks = []
  let size = 0
  while (true) {
    const { done, value } = await reader.read()
    if (done) {
      break
    }
    size += value.byteLength
    if (size > limit) {
      throw new Error(`response body exceeded ${limit} bytes`)
    }
    chunks.push(value)
  }
  return Buffer.concat(chunks).toString('utf8')
}

export function dispatchContextFromHeaders(headers) {
  return {
    pluginId: headerValue(headers, 'x-fbz-plugin-id'),
    token: headerValue(headers, 'x-fbz-plugin-token'),
    idempotencyKey: headerValue(headers, 'x-fbz-plugin-idempotency-key'),
    hostBaseUrl: headerValue(headers, 'x-fbz-host-base-url')
  }
}

export function verifyDispatchSignature(headers, body, options = {}) {
  const secret = options.secret ?? process.env.PLUGIN_SECRET_KEY ?? ''
  if (!secret) {
    return
  }

  const version = requiredHeader(headers, 'x-fbz-plugin-signature-version')
  const timestamp = requiredHeader(headers, 'x-fbz-plugin-signature-timestamp')
  const pluginId = requiredHeader(headers, 'x-fbz-plugin-id')
  const idempotencyKey = requiredHeader(headers, 'x-fbz-plugin-idempotency-key')
  const bodySha256 = requiredHeader(headers, 'x-fbz-plugin-body-sha256')
  const signature = requiredHeader(headers, 'x-fbz-plugin-signature')

  const timestampValue = Number.parseInt(timestamp, 10)
  const ageSeconds = Math.abs(Math.floor(Date.now() / 1000) - timestampValue)
  const toleranceSeconds = options.toleranceSeconds ?? DEFAULT_SIGNATURE_TOLERANCE_SECONDS
  if (!Number.isFinite(ageSeconds) || ageSeconds > toleranceSeconds) {
    throw new Error('stale plugin signature timestamp')
  }

  const computedBodySha256 = createHash('sha256').update(body).digest('hex')
  if (!safeEqualHex(bodySha256, computedBodySha256)) {
    throw new Error('plugin body sha256 mismatch')
  }

  const canonical = `${version}\n${timestamp}\n${pluginId}\n${idempotencyKey}\n${computedBodySha256}`
  const expected = createHmac('sha256', secret).update(canonical).digest('hex')
  const actual = signature.replace(/^sha256=/, '')
  if (!safeEqualHex(actual, expected)) {
    throw new Error('plugin signature mismatch')
  }
}

export function createIdempotencyCache(limit = 10000) {
  const keys = new Set()
  return {
    has(key) {
      return Boolean(key) && keys.has(key)
    },
    remember(key) {
      if (!key) {
        return
      }
      keys.add(key)
      if (Number.isFinite(limit) && limit > 0 && keys.size > limit) {
        const oldest = keys.values().next().value
        if (oldest) {
          keys.delete(oldest)
        }
      }
    }
  }
}

export async function hostJson(context, route, options = {}) {
  requireHostContext(context)
  const response = await fetch(new URL(route, context.hostBaseUrl), {
    ...options,
    headers: {
      'content-type': 'application/json',
      'x-fbz-plugin-token': context.token,
      ...(options.headers ?? {})
    }
  })

  const body = await readLimitedResponse(response, options.responseLimitBytes)
  if (!response.ok) {
    throw new Error(`Host API ${route} failed: ${response.status} ${body}`)
  }

  return body ? JSON.parse(body) : {}
}

export async function loadPluginConfig(context, fallback = {}) {
  try {
    const { values } = await hostJson(context, '/api/plugin/config')
    return values && typeof values === 'object' ? values : fallback
  }
  catch {
    return fallback
  }
}

export function createHttpPluginServer(options) {
  const routePath = options.path ?? '/fbz-plugin'
  const idempotency = createIdempotencyCache(options.idempotencyCacheLimit ?? 10000)

  return createServer(async (request, response) => {
    try {
      if (request.method !== 'POST' || requestPath(request.url) !== routePath) {
        response.writeHead(404).end()
        return
      }

      const body = await readBody(request, options.bodyLimitBytes ?? DEFAULT_BODY_LIMIT_BYTES)
      verifyDispatchSignature(request.headers, body, {
        secret: options.signatureSecret,
        toleranceSeconds: options.signatureToleranceSeconds
      })

      const context = dispatchContextFromHeaders(request.headers)
      const dispatch = JSON.parse(body.toString('utf8'))
      dispatch.idempotencyKey = context.idempotencyKey

      if (idempotency.has(context.idempotencyKey)) {
        sendJson(response, 200, { deduped: true })
        return
      }

      const result = await options.handleDispatch(dispatch, context, request)
      idempotency.remember(context.idempotencyKey)
      sendJson(response, 200, { ok: true, result })
    }
    catch (error) {
      sendJson(response, 500, { error: String(error.message ?? error) })
    }
  })
}

export function listen(server, options) {
  const host = options.host ?? '127.0.0.1'
  server.listen(options.port, host, () => {
    console.log(`${options.name} listening on http://${host}:${options.port}${options.path}`)
  })
}

function sendJson(response, statusCode, payload) {
  response
    .writeHead(statusCode, { 'content-type': 'application/json' })
    .end(JSON.stringify(payload))
}

function requestPath(url) {
  return new URL(url, 'http://127.0.0.1').pathname
}

function requireHostContext(context) {
  if (!context.hostBaseUrl || !context.token) {
    throw new Error('missing host base URL or plugin token')
  }
}

function requiredHeader(headers, name) {
  const value = headerValue(headers, name)
  if (!value) {
    throw new Error(`missing plugin signature header: ${name}`)
  }
  return value
}

function headerValue(headers, name) {
  const value = headers[name]
  return Array.isArray(value) ? value[0] : value
}

function safeEqualHex(actual, expected) {
  if (!isLowerHex(actual) || !isLowerHex(expected)) {
    return false
  }
  const actualBuffer = Buffer.from(actual, 'hex')
  const expectedBuffer = Buffer.from(expected, 'hex')
  return actualBuffer.length === expectedBuffer.length && timingSafeEqual(actualBuffer, expectedBuffer)
}

function isLowerHex(value) {
  return typeof value === 'string' && value.length % 2 === 0 && /^[0-9a-f]+$/.test(value)
}
