import { createHash } from 'node:crypto';
import { lookup } from 'node:dns/promises';
import https from 'node:https';
import { isIP } from 'node:net';
import { buildBusinessHeaders, buildAccountHeaders, resolveGuangyaProfile, businessResponseCode, isAuthExpiredBusinessCode } from './guangya-protocol.mjs';

export class ProviderError extends Error {
  constructor(code, message, retryAfter = 0) { super(message); this.code = code; this.retryAfter = retryAfter; }
}
// One queue per account, no parallel refresh, requests always spaced including retries.
export class Scheduler {
  constructor() { this.queues = new Map(); }
  async run(key, qps, fn) {
    const state = this.queues.get(key) ?? { tail: Promise.resolve(), next: 0 };
    this.queues.set(key, state);
    const result = state.tail.then(async () => {
      await new Promise(r => setTimeout(r, Math.max(0, state.next - Date.now())));
      try { return await fn(); } finally { state.next = Date.now() + 1000 / Math.min(20, Math.max(1, qps || 10)); }
    });
    state.tail = result.catch(() => {});
    return result;
  }
}
export function publicIp(address) {
  if (address.includes(':')) {
    // Restrict downloads to global IPv6 unicast; reject mapped/private/link-local IPs.
    return /^[23][0-9a-f]{3}:/i.test(address) && !/^2001:db8:/i.test(address);
  }
  const p = address.split('.').map(Number);
  return p.length === 4 && ![0, 10, 127].includes(p[0]) && p[0] < 224 &&
    !(p[0] === 169 && p[1] === 254) && !(p[0] === 172 && p[1] >= 16 && p[1] <= 31) &&
    !(p[0] === 192 && p[1] === 168) && !(p[0] === 100 && p[1] >= 64 && p[1] <= 127);
}
export function checkedUrl(value) {
  const url = new URL(value);
  if (url.protocol !== 'https:' || url.username || url.password || (url.port && url.port !== '443') ||
    url.hostname === 'localhost' || (isIP(url.hostname.replace(/^\[|\]$/g, '')) && !publicIp(url.hostname.replace(/^\[|\]$/g, '')))) {
    throw new ProviderError('unsafe_url', '云端地址不符合安全策略');
  }
  return url;
}
export async function readSmall(url, maxBytes = 10 * 1024 * 1024, redirects = 0) {
  if (redirects > 4) throw new ProviderError('upstream', '云端重定向次数过多');
  const target = checkedUrl(url);
  return new Promise((resolve, reject) => {
    const request = https.get(target, { timeout: 15000, lookup(host, options, callback) {
      lookup(host, { all: true }).then(addresses => {
        if (!addresses.length || addresses.some(a => !publicIp(a.address))) throw new Error('private address');
        if (options.all) callback(null, addresses); else callback(null, addresses[0].address, addresses[0].family);
      }).catch(() => callback(new Error('blocked download host')));
    } }, response => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode)) {
        response.resume();
        try { if (!response.headers.location) throw new Error('missing location'); readSmall(new URL(response.headers.location, target).href, maxBytes, redirects + 1).then(resolve, reject); } catch { reject(new ProviderError('unsafe_url', '云端重定向地址无效')); } return;
      }
      if (response.statusCode !== 200) { response.resume(); reject(new ProviderError('download_failed', '云端小文件读取失败')); return; }
      let size = 0; const chunks = [];
      response.on('data', chunk => { size += chunk.length; if (size > maxBytes) request.destroy(new Error('file too large')); else chunks.push(chunk); });
      response.on('end', () => resolve(Buffer.concat(chunks)));
      response.on('error', reject);
    });
    request.on('timeout', () => request.destroy(new Error('download timed out')));
    request.on('error', () => reject(new ProviderError('download_failed', '云端小文件读取失败或超出大小限制')));
  });
}
export function normalizeEntry(row) {
  // Cloud names are opaque labels, not filesystem paths; '/' is legal upstream.
  // Cached paths are generated exclusively from account/file/version hashes.
  const id = String(row.fileId ?? row.id ?? ''); const name = String(row.fileName ?? row.name ?? '');
  if (!id || !name || name.length > 1024 || /[\x00-\x1f]/.test(name) || ['.', '..'].includes(name)) throw new ProviderError('invalid_response', `云端文件条目无效（id=${!!id},name=${!!name},length=${name.length},separator=${/[\\/]/.test(name)}）`);
  const size = Number(row.fileSize ?? row.size ?? 0);
  return { id, name, directory: Number(row.resType ?? row.type) === 2 || row.isDirectory === true, size: Number.isSafeInteger(size) && size >= 0 ? size : 0,
    version: createHash('sha256').update(JSON.stringify([id, name, size, row.updateTime ?? row.modifyTime ?? row.updatedAt ?? '', row.hash ?? ''])).digest('hex') };
}
export function expiry(url, now = Date.now()) {
  const u = new URL(url); const p = new Map([...u.searchParams].map(([k, v]) => [k.toLowerCase(), v]));
  const candidates = []; const absolute = Number(p.get('expires')); const ossAbsolute = Number(p.get('x-oss-expires')); if(ossAbsolute>10000000) candidates.push(ossAbsolute<1e12?ossAbsolute*1000:ossAbsolute);
  if (absolute > 1e9) candidates.push(absolute < 1e12 ? absolute * 1000 : absolute);
  for (const prefix of ['x-oss', 'x-amz']) {
    const date = p.get(`${prefix}-date`); const seconds = Number(p.get(`${prefix}-expires`));
    const m = date?.match(/^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/);
    if (m && seconds > 0) candidates.push(Date.UTC(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +m[6]) + seconds * 1000);
  }
  return Math.min(now + 60000, ...candidates.map(t => t - 10000));
}

export function createProvider({ fetchImpl = fetch, apiBase = 'https://api.guangyapan.com', accountBase = 'https://account.guangyapan.com', profile = resolveGuangyaProfile(), smallReader = readSmall } = {}) {
  const scheduler = new Scheduler(); const urls = new Map();
  async function dispatch(input) {
    const { op, accountId, deviceId, qps = 10 } = input;
    let credentials = input.credentials ?? {}; let changed = false;
    const accountHeaders = () => buildAccountHeaders({ deviceId, profile, token: credentials.access_token });
    async function call(path, body, account = false, method = 'POST') {
      return scheduler.run(accountId, qps, async () => {
        const response = await fetchImpl(`${account ? accountBase : apiBase}${path}`, { method,
          headers: account ? accountHeaders() : buildBusinessHeaders({ token: credentials.access_token, deviceId, profile }),
          body: method === 'POST' ? JSON.stringify(body) : undefined, signal: AbortSignal.timeout(12000), redirect: 'error' });
        if (response.status === 429) throw new ProviderError('rate_limited', '光鸭请求过快，请稍后重试', Math.min(300, Number(response.headers.get('retry-after')) || 5));
        const chunks=[]; let size=0; for await (const chunk of response.body ?? []) { size+=chunk.length; if(size>4*1024*1024) throw new ProviderError('invalid_response','云端响应过大'); chunks.push(chunk); } const text=Buffer.concat(chunks).toString('utf8');
        let payload; try { payload = JSON.parse(text); } catch { throw new ProviderError('invalid_response', '云端未返回有效数据'); }
        if (!account) {
          const code = businessResponseCode(payload);
          if (isAuthExpiredBusinessCode(code) || response.status === 401) throw new ProviderError('auth_expired', '光鸭登录已失效');
          if (code === 120) throw new ProviderError('rate_limited', '光鸭暂时限制请求，请稍后重试', 10);
          if (!response.ok || code !== 0) throw new ProviderError('upstream', `光鸭请求失败（${code ?? response.status}）`);
        }
        return { payload: payload.data ?? payload, status: response.status };
      });
    }
    async function refresh() {
      if (!credentials.refresh_token) throw new ProviderError('auth_expired', '请重新扫码登录');
      const r = await call('/v1/auth/token', { grant_type: 'refresh_token', refresh_token: credentials.refresh_token, client_id: profile.clientId, client_secret: profile.clientSecret }, true);
      if (!r.payload.access_token) throw new ProviderError(r.status >= 500 ? 'upstream' : 'auth_expired', '光鸭会话刷新失败');
      credentials = { ...credentials, ...r.payload }; changed = true;
    }
    async function business(path, body) {
      try { return (await call(path, body)).payload; }
      catch (e) { if (e.code !== 'auth_expired') throw e; await refresh(); return (await call(path, body)).payload; }
    }
    async function download(id, force = false) {
      const key = `${accountId}:${input.credentialVersion ?? 0}:${id}`; const cached = urls.get(key);
      if (!force && cached?.expiresAt > Date.now()) return cached.url;
      const result = await business('/userres/v1/get_res_download_url', { fileId: id });
      const url = checkedUrl(String(result.signedURL ?? result.signedUrl ?? '')).href;
      urls.set(key, { url, expiresAt: expiry(url) });
      if (urls.size > 2048) urls.delete(urls.keys().next().value);
      return url;
    }
    let data;
    try {
      if (op === 'auth.start') {
        const r = await call('/v1/auth/device/code', { scope: 'user', client_id: profile.clientId, meta: { scene: 'pc_login' } }, true);
        if (r.status >= 400 || !r.payload.device_code) throw new ProviderError('upstream', '无法创建光鸭扫码任务');
        data = r.payload;
      } else if (op === 'auth.poll') {
        const r = await call('/v1/auth/token', { grant_type: 'urn:ietf:params:oauth:grant-type:device_code', device_code: input.deviceCode, client_id: profile.clientId, client_secret: profile.clientSecret }, true);
        if (r.payload.access_token) {
          credentials = r.payload;
          const me = await call('/v1/user/me', undefined, true, 'GET');
          const user = me.payload.user ?? me.payload.profile ?? me.payload;
          const uid = user.id ?? user.user_id ?? user.userId ?? user.accountId ?? user.account_id ?? user.uid ?? user.sub;
          if (me.status >= 400 || !uid) throw new ProviderError('identity_failed', '无法确认光鸭账号身份，请重新授权');
          changed = true;
          data = { authenticated: true, userId: String(uid), name: String(user.nickname ?? user.name ?? uid) };
        } else if (['authorization_pending', 'slow_down'].includes(r.payload.error) || [202, 428].includes(r.status)) {
          data = { pending: true, slowDown: r.payload.error === 'slow_down' };
        } else throw new ProviderError('auth_expired', '二维码已失效或授权被拒绝');
      } else if (op === 'list') {
        const page = Math.max(0, Number(input.page) || 0);
        const result = await business('/userres/v1/file/get_file_list', { page, pageSize: 100, parentId: String(input.parentId ?? ''), orderBy: 0, sortType: 0, needSubFolderStat: true });
        if (!Array.isArray(result.list)) throw new ProviderError('invalid_response', '光鸭目录响应缺少文件列表');
        const total = Number(result.total); const complete = result.list.length === 0 || (Number.isFinite(total) && result.total != null ? (page + 1) * 100 >= total : result.list.length < 100);
        data = { entries: result.list.map(normalizeEntry), nextPage: complete ? null : page + 1 };
      } else if (op === 'resolve') {
        const url = await download(String(input.fileId), input.force === true);
        data = { url, expiresAt: expiry(url) };
      } else if (op === 'read') {
        let content;
        try { content = await smallReader(await download(String(input.fileId)), Math.min(10 * 1024 * 1024, input.maxBytes || 2 * 1024 * 1024)); }
        catch { content = await smallReader(await download(String(input.fileId), true), Math.min(10 * 1024 * 1024, input.maxBytes || 2 * 1024 * 1024)); }
        // JSON byte array keeps the host protocol independent of a base64 dependency.
        data = { bytes: [...content] };
      } else throw new ProviderError('unsupported', '不支持的媒体源操作');
      return { data, credentials: changed ? credentials : undefined };
    } catch (e) {
      // Log only typed, locally constructed diagnostics; never credentials, URLs or response bodies.
      console.warn(JSON.stringify({operation:op,code:e.code ?? 'upstream',message:e instanceof ProviderError?e.message:'transport failure'}));
      // A successful refresh must survive a later failed file request.
      return { error: { code: e.code ?? 'upstream', message: e instanceof ProviderError ? e.message : '光鸭请求失败', retryAfter: e.retryAfter ?? 0 }, credentials: changed ? credentials : undefined };
    }
  }
  return { dispatch };
}
