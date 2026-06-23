import { createHash, createHmac } from 'node:crypto'
import { createServer } from 'node:http'
import { after, describe, it } from 'node:test'
import assert from 'node:assert/strict'

import {
  createHttpPluginServer,
  hostJson,
  loadPluginConfig,
  parsePort,
  verifyDispatchSignature
} from './fbz-plugin-http.mjs'

const servers = []

after(async () => {
  await Promise.all(servers.map(closeServer))
})

describe('fbz-plugin-http helper', () => {
  it('validates signed dispatches and rejects tampered bodies', () => {
    const secret = 'test-secret-with-at-least-32-characters'
    const body = Buffer.from(JSON.stringify({ hookEvent: 'user.login' }))
    const headers = signedHeaders({ body, secret, idempotencyKey: 'dispatch-1' })

    assert.doesNotThrow(() => verifyDispatchSignature(headers, body, { secret }))
    assert.throws(
      () => verifyDispatchSignature(headers, Buffer.from('{"changed":true}'), { secret }),
      /plugin body sha256 mismatch/
    )
  })

  it('runs dispatch handler once per idempotency key and exposes Host API context', async () => {
    const secret = 'test-secret-with-at-least-32-characters'
    const hostApi = await startHostApiServer()
    const handled = []
    const plugin = createHttpPluginServer({
      path: '/fbz-plugin',
      signatureSecret: secret,
      handleDispatch: async (dispatch, context) => {
        const config = await loadPluginConfig(context, {})
        const pong = await hostJson(context, '/api/plugin/ping')
        handled.push({ dispatch, context, config, pong })
        return { channel: config.channel, pong: pong.ok }
      }
    })
    const pluginBaseUrl = await listenOnLoopback(plugin)

    const body = Buffer.from(JSON.stringify({ hookEvent: 'library.scan.completed' }))
    const headers = signedHeaders({
      body,
      secret,
      idempotencyKey: 'dispatch-once',
      hostBaseUrl: hostApi.baseUrl,
      token: 'host-token-1'
    })

    const first = await postJson(`${pluginBaseUrl}/fbz-plugin`, body, headers)
    const second = await postJson(`${pluginBaseUrl}/fbz-plugin`, body, headers)

    assert.deepEqual(first, {
      ok: true,
      result: {
        channel: 'ops',
        pong: true
      }
    })
    assert.deepEqual(second, { deduped: true })
    assert.equal(handled.length, 1)
    assert.equal(handled[0].dispatch.idempotencyKey, 'dispatch-once')
    assert.equal(handled[0].context.token, 'host-token-1')
    assert.equal(handled[0].context.hostBaseUrl, hostApi.baseUrl)
    assert.deepEqual(hostApi.seenTokens, ['host-token-1', 'host-token-1'])
  })

  it('keeps port parsing conservative', () => {
    assert.equal(parsePort('19091', 1), 19091)
    assert.equal(parsePort('0', 1), 1)
    assert.equal(parsePort('70000', 1), 1)
    assert.equal(parsePort('not-a-port', 1), 1)
  })
})

function signedHeaders({
  body,
  secret,
  idempotencyKey,
  pluginId = 'dev.fbz.test',
  hostBaseUrl = 'http://127.0.0.1',
  token = 'host-token'
}) {
  const version = 'v1'
  const timestamp = String(Math.floor(Date.now() / 1000))
  const bodySha256 = createHash('sha256').update(body).digest('hex')
  const canonical = `${version}\n${timestamp}\n${pluginId}\n${idempotencyKey}\n${bodySha256}`
  const signature = createHmac('sha256', secret).update(canonical).digest('hex')
  return {
    'content-type': 'application/json',
    'x-fbz-plugin-id': pluginId,
    'x-fbz-plugin-token': token,
    'x-fbz-plugin-idempotency-key': idempotencyKey,
    'x-fbz-host-base-url': hostBaseUrl,
    'x-fbz-plugin-signature-version': version,
    'x-fbz-plugin-signature-timestamp': timestamp,
    'x-fbz-plugin-body-sha256': bodySha256,
    'x-fbz-plugin-signature': `sha256=${signature}`
  }
}

async function startHostApiServer() {
  const seenTokens = []
  const server = createServer((request, response) => {
    seenTokens.push(request.headers['x-fbz-plugin-token'])
    if (request.url === '/api/plugin/config') {
      sendJson(response, 200, { values: { channel: 'ops' } })
      return
    }
    if (request.url === '/api/plugin/ping') {
      sendJson(response, 200, { ok: true })
      return
    }
    sendJson(response, 404, { error: 'not found' })
  })
  const baseUrl = await listenOnLoopback(server)
  return { baseUrl, seenTokens }
}

async function listenOnLoopback(server) {
  const url = await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      resolve(`http://127.0.0.1:${address.port}`)
    })
  })
  servers.push(server)
  return url
}

async function closeServer(server) {
  if (!server.listening) {
    return
  }
  await new Promise((resolve, reject) => {
    server.close((error) => {
      if (error) {
        reject(error)
        return
      }
      resolve()
    })
  })
}

async function postJson(url, body, headers) {
  const response = await fetch(url, {
    method: 'POST',
    headers,
    body
  })
  assert.equal(response.headers.get('content-type'), 'application/json')
  return response.json()
}

function sendJson(response, statusCode, payload) {
  response
    .writeHead(statusCode, { 'content-type': 'application/json' })
    .end(JSON.stringify(payload))
}
