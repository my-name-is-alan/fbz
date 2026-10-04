// API profile verified from api_map live samples on 2026-08-01.
// Keep this boundary independent from the HTTP server so Web and desktop
// implementations can follow the same upstream contract.
export const WINDOWS_CLIENT_ID = 'aMe_SVSlkrbQXpUT';
export const WINDOWS_CLIENT_SECRET = 'FNAfp5IFEfCn5MYsIUTewg';
export const WINDOWS_DEVICE_TYPE = '5';
export const WINDOWS_APP_VERSION = '1.0.2';
export const WINDOWS_VERSION_CODE = '1002';
export const AUTH_EXPIRED_CODES = Object.freeze([110, 117, 118]);

export function resolveGuangyaProfile(environment = process.env) {
  const clientId = String(environment.GUANGYA_OAUTH_CLIENT_ID || WINDOWS_CLIENT_ID).trim();
  const clientSecret = String(environment.GUANGYA_OAUTH_CLIENT_SECRET || WINDOWS_CLIENT_SECRET).trim();
  const appVersion = String(environment.GUANGYA_APP_VERSION || WINDOWS_APP_VERSION).trim();
  const versionCode = String(environment.GUANGYA_VERSION_CODE || WINDOWS_VERSION_CODE).trim();
  if (!clientId || !clientSecret || !appVersion || !versionCode) {
    throw new Error('光鸭客户端协议配置不完整');
  }
  return Object.freeze({
    clientId,
    clientSecret,
    appVersion,
    versionCode,
    deviceType: WINDOWS_DEVICE_TYPE,
    userAgent: `GuangyapanPC/${appVersion}`,
  });
}

export function buildBusinessHeaders({ token, deviceId, profile, traceparent }) {
  const accessToken = String(token || '').trim();
  const normalizedDeviceId = String(deviceId || '').trim();
  if (!accessToken) throw new Error('尚未设置光鸭会话令牌');
  if (!normalizedDeviceId) throw new Error('光鸭设备标识为空');
  return {
    accept: 'application/json',
    'content-type': 'application/json',
    authorization: `Bearer ${accessToken}`,
    dt: profile.deviceType,
    av: profile.appVersion,
    vc: profile.versionCode,
    'x-client-id': profile.clientId,
    'x-device-id': normalizedDeviceId,
    'user-agent': profile.userAgent,
    // Older desktop traffic used this alias. Keeping it avoids breaking a
    // gateway that still reads did while the canonical header is x-device-id.
    did: normalizedDeviceId,
    ...(traceparent ? { traceparent } : {}),
  };
}

export function buildAccountHeaders({ deviceId, profile, token }) {
  const normalizedDeviceId = String(deviceId || '').trim();
  if (!normalizedDeviceId) throw new Error('光鸭设备标识为空');
  const accessToken = String(token || '').trim();
  return {
    accept: 'application/json',
    'content-type': 'application/json',
    'x-client-id': profile.clientId,
    'x-device-id': normalizedDeviceId,
    'x-client-version': profile.appVersion,
    'x-sdk-version': '9.0.2',
    'x-protocol-version': '301',
    'accept-language': 'zh-CN',
    'user-agent': profile.userAgent,
    ...(accessToken ? { authorization: `Bearer ${accessToken}` } : {}),
  };
}

export function isSuccessfulBusinessMessage(value) {
  return /^(?:success|ok)$/i.test(String(value || '').trim());
}

export function businessResponseCode(payload) {
  if (!payload || typeof payload !== 'object') return null;
  const hasCode = Object.prototype.hasOwnProperty.call(payload, 'code')
    && payload.code !== null
    && payload.code !== '';
  if (!hasCode) {
    const message = String(payload.msg || '').trim();
    if (message) return isSuccessfulBusinessMessage(message) ? 0 : null;
    // Older production responses occasionally omitted both code and msg.
    // Retain that compatibility only when there is no contradictory message.
    return Object.prototype.hasOwnProperty.call(payload, 'data') && payload.data != null ? 0 : null;
  }
  const code = Number(payload.code);
  if (!Number.isInteger(code)) return null;
  const message = String(payload.msg || '').trim();
  if (code === 0 && message && !isSuccessfulBusinessMessage(message)) return null;
  return code;
}

export function isAuthExpiredBusinessCode(value) {
  return AUTH_EXPIRED_CODES.includes(Number(value));
}

export function uploadCredentialsExpired(params, now = Date.now()) {
  const expiration = Date.parse(String(params?.creds?.expiration || ''));
  return !Number.isFinite(expiration) || expiration <= Number(now);
}

export function isUploadSecurityTokenExpired(error) {
  const values = [
    error?.code,
    error?.name,
    error?.message,
    error?.cause?.code,
    error?.cause?.message,
    error?.data?.Code,
    error?.response?.data?.Code,
  ];
  return values.some((value) => String(value || '').includes('SecurityTokenExpired'));
}

export function cloudCollectionResourceType(value) {
  const source = String(value || '').trim().toLowerCase();
  if (source.startsWith('magnet:')) return 1;
  if (source.startsWith('ed2k://')) return 3;
  return 0;
}
