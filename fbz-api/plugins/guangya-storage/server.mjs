import http from 'node:http';
import { timingSafeEqual } from 'node:crypto';
import { createProvider } from './provider.mjs';
const key = process.env.FBZ_STORAGE_PLUGIN_KEY ?? '';
if (key.length < 32) throw new Error('FBZ_STORAGE_PLUGIN_KEY must contain at least 32 characters');
const provider = createProvider();
const server = http.createServer(async (req, res) => {
  const supplied = Buffer.from(String(req.headers.authorization ?? ''));
  const expected = Buffer.from(`Bearer ${key}`);
  res.setHeader('Content-Type', 'application/json'); res.setHeader('Cache-Control', 'no-store');
  if (supplied.length !== expected.length || !timingSafeEqual(supplied, expected)) { res.writeHead(401); res.end('{}'); return; }
  if (req.method !== 'POST' || req.url !== '/rpc') { res.writeHead(404); res.end('{}'); return; }
  try {
    const chunks = []; let size = 0;
    for await (const chunk of req) { size += chunk.length; if (size > 65536) throw new Error('large request'); chunks.push(chunk); }
    const input = JSON.parse(Buffer.concat(chunks));
    if (!input.accountId || !input.deviceId) throw new Error('missing account');
    res.end(JSON.stringify(await provider.dispatch(input)));
  } catch { res.writeHead(400); res.end(JSON.stringify({error:{code:'invalid_request',message:'插件请求无效'}})); }
});
server.requestTimeout = 60000; server.headersTimeout = 10000;
server.listen(Number(process.env.PORT || 8098), process.env.HOST || '127.0.0.1', () => console.log('Guangya storage plugin ready'));
