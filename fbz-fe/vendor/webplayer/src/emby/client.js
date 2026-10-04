// A thin, dependency-free Emby client. It does exactly what the browse UI needs
// and nothing more: authenticate, list the server's libraries and items, build
// image URLs without extra round-trips (from the ImageTags the item already
// carries), and resolve a DIRECT stream URL so the original container bytes go
// straight into LinWeb's existing HttpSource -> worker -> MSE pipeline. No
// transcode: the DeviceProfile below advertises "I can play everything", which
// is the whole point of this player.
//
// Auth is carried two ways on purpose: API calls use the X-Emby-Token header,
// but the video stream uses ?api_key= in the query string -- a browser cannot
// attach a custom header to a plain Range GET without a preflight, and the
// player fetches the file as a plain URL.
//
// CORS caveat: a browser page on another origin can only read an Emby server
// that returns Access-Control-Allow-Origin (and exposes Accept-Ranges /
// Content-Range). Self-hosters either enable that or put the extractor proxy in
// front; that is a deployment concern, not this module's.

const CLIENT = 'LinWeb';
const VERSION = '0.1.0';

const norm = u => (u || '').trim().replace(/\/+$/, '');

// ---- remembered servers ----------------------------------------------------
// Every server the user has logged into, newest first: address, last user, its
// access token and the line list synced from the server. Deliberately SURVIVES
// logout -- logging out drops the token for that server, not the server itself.
// That is what lets the login page switch between servers/accounts instead of
// facing an empty address box.
const SERVERS_KEY = 'linweb:servers';
const readServers = () => { try { return JSON.parse(localStorage.getItem(SERVERS_KEY)) || []; } catch { return []; } };
const writeServers = l => { try { localStorage.setItem(SERVERS_KEY, JSON.stringify(l)); } catch {} };

export const servers = {
  all: readServers,
  get: url => readServers().find(s => s.url === norm(url)) || null,
  // merge-and-promote: an update keeps the fields it doesn't mention (a line
  // sync must not wipe the token) and moves the server to the front.
  put(rec) {
    const url = norm(rec.url);
    writeServers([{ ...servers.get(url), ...rec, url }, ...readServers().filter(s => s.url !== url)]);
  },
  forget(url) { writeServers(readServers().filter(s => s.url !== norm(url))); },
};

// Candidate icons for a server, best first: the server's own branding image
// from the API, then the signed-in user's avatar. Returned as a list rather
// than probed here on purpose -- the <img> that displays it walks the list with
// onerror, so a missing icon costs no extra request and no CORS exposure.
export function serverIcons(rec) {
  const base = norm(rec?.line || rec?.url);
  if (!base) return [];
  const q = rec.token ? `?api_key=${encodeURIComponent(rec.token)}&maxWidth=96` : '?maxWidth=96';
  const urls = [`${base}/Branding/Splashscreen${q}`];
  if (rec.userId) urls.push(`${base}/Users/${rec.userId}/Images/Primary${q}`);
  return urls;
}

// Re-point a URL built against one domain at another. Used to switch lines
// mid-playback: same Emby, same path, different way in.
export function reline(url, from, to) {
  if (!url || !to || from === to) return url;
  if (from && url.startsWith(from)) return to + url.slice(from.length);
  const u = new URL(url);                       // different host than expected: keep path+query only
  return to + u.pathname + u.search;
}

function deviceId() {
  try {
    let id = localStorage.getItem('linweb:deviceId');
    if (!id) { id = (crypto.randomUUID?.() || Date.now().toString(36) + Math.random().toString(36).slice(2)); localStorage.setItem('linweb:deviceId', id); }
    return id;
  } catch { return 'linweb-' + (Date.now().toString(36)); }
}

// Tell Emby the client can direct-play the containers/codecs LinWeb actually
// hands to the browser. No TranscodingProfiles -> the server has no fallback but
// to give us the original file. maxStreamingBitrate is high so nothing is capped.
const DIRECT_PROFILE = {
  MaxStreamingBitrate: 400_000_000,
  MaxStaticBitrate: 400_000_000,
  DirectPlayProfiles: [
    { Type: 'Video', Container: 'mkv,webm,mp4,m4v,mov,flv,ts,m2ts', VideoCodec: 'h264,hevc,hev1,vp8,vp9,av1', AudioCodec: 'aac,ac3,eac3,mp3,opus,flac,vorbis,dts,truehd,pcm' },
    // A .strm item carries no probed codecs (MediaStreams is empty until the
    // first play), so a codec-constrained profile has nothing to match against
    // and the server can decide it must transcode -- which this profile forbids,
    // leaving "no compatible streams". This entry names the container and
    // deliberately constrains NO codec, so a remote source is always direct-
    // playable. NOTE: matched against a live .strm library, this is the one line
    // here I have not been able to verify end to end.
    { Type: 'Video', Container: 'strm' },
  ],
  // Deliberately empty: force DirectPlay/DirectStream, never transcode.
  TranscodingProfiles: [],
  ContainerProfiles: [],
  CodecProfiles: [],
  SubtitleProfiles: [
    { Format: 'ass', Method: 'Embed' }, { Format: 'ssa', Method: 'Embed' },
    { Format: 'pgssub', Method: 'Embed' }, { Format: 'srt', Method: 'Embed' },
  ],
};

// ---- remote (.strm-backed) sources -----------------------------------------
// A .strm is a one-line text file holding a URL; Emby resolves it server-side,
// so this client never sees the file -- only the SHAPE of the MediaSource it
// produces, which differs from a local file in three ways that matter:
//   Container is "strm" or missing  (the real container was never probed)
//   Size is null                    (nobody asked the remote how long it is)
//   MediaStreams is []              (Emby only ffprobes a .strm on first play)
// Everything downstream has to survive all three. See DEPLOY/README for why the
// stream still comes from Emby rather than the remote URL directly.
// The URL a .strm actually points at, when a BROWSER stands a chance with it.
// Emby returns `Path` verbatim to any user who can see the library, so this
// costs no extra call -- it is the same string the .strm file contains.
//
// Only http(s) qualifies. The other things a .strm may legally hold (a UNC
// share, an absolute local path, rtsp://, mms://) are things a page cannot
// fetch at all, and offering them as a candidate would just spend a failed
// request before the server path runs anyway.
//
// Returning it is NOT a promise that it works: a browser may still be refused
// for want of a CORS header, or by a Referer/User-Agent check it cannot satisfy
// (README, ".strm 与原生兜底"). It is a CANDIDATE -- callers must fall back.
export function directUrl(source) {
  if (!isRemoteSource(source)) return null;
  const p = (source.Path || '').trim();
  return /^https?:\/\//i.test(p) ? p : null;
}

export const isRemoteSource = s =>
  !!s && (s.Protocol === 'Http' || s.IsRemote === true
          || /^(https?|rtsp|rtmp|mms):\/\//i.test(s.Path || '')
          || /\.strm$/i.test(s.Path || ''));

// The extension Emby's /Videos/{id}/stream path ends with. Emby keys the served
// container off it, so "strm" -- which is not a container, it is a pointer file
// -- must never be sent. Fall back to the extension of the URL inside the .strm,
// which IS the real container, and to nothing at all when even that is unknown
// (Emby then picks, and the native <video> leg covers whatever comes out).
const BAD_CONTAINER = /^(strm|m3u|m3u8|)$/i;
function containerExt(source) {
  const c = (source?.Container || '').split(',')[0].trim();
  if (c && !BAD_CONTAINER.test(c)) return '.' + c;
  const path = (source?.Path || '').split(/[?#]/)[0];
  const ext = (path.match(/\.([a-z0-9]{2,5})$/i)?.[1] || '').toLowerCase();
  return ext && !BAD_CONTAINER.test(ext) ? '.' + ext : '';
}

export class EmbyClient {
  // `home` is the address the account belongs to and the key everything is
  // remembered under; `server` is the domain actually being talked to. They
  // differ once the viewer picks another line, and only `server` moves.
  constructor(server, line) {
    this.home = norm(server);
    this.server = norm(line) || this.home;
    this.token = null;
    this.userId = null;
    this.userName = null;
    this.deviceId = deviceId();
  }

  // ---- session persistence -------------------------------------------------
  static restore() {
    try {
      const s = JSON.parse(localStorage.getItem('linweb:emby') || 'null');
      if (!s?.token || !(s.home || s.server)) return null;
      const c = new EmbyClient(s.home || s.server, s.server);
      c.token = s.token; c.userId = s.userId; c.userName = s.userName; c.policy = s.policy || {};
      return c;
    } catch { return null; }
  }
  // Sign in from a remembered server record, no password needed.
  static from(rec) {
    if (!rec?.url || !rec.token) return null;
    const c = new EmbyClient(rec.url, rec.line);
    c.token = rec.token; c.userId = rec.userId; c.userName = rec.userName; c.policy = rec.policy || {};
    c._persist();
    return c;
  }
  _persist() {
    const s = { home: this.home, server: this.server, token: this.token, userId: this.userId, userName: this.userName, policy: this.policy };
    try { localStorage.setItem('linweb:emby', JSON.stringify(s)); } catch {}
    servers.put({ url: this.home, line: this.server, token: this.token, userId: this.userId, userName: this.userName, policy: this.policy });
  }
  // A restored session predates the policy field, and a policy can be revoked
  // server-side between sessions; re-read it so the menus match reality.
  async refreshPolicy() {
    try { this.policy = (await this._get(`/Users/${this.userId}`))?.Policy || this.policy || {}; this._persist(); } catch {}
    return this.policy;
  }
  get isAdmin() { return !!this.policy?.IsAdministrator; }
  logout() {
    servers.put({ url: this.home, token: null });   // keep the server, drop the token
    this.token = this.userId = this.userName = null;
    try { localStorage.removeItem('linweb:emby'); } catch {}
  }

  // ---- lines (uhdnow/emby_ext_domains) -------------------------------------
  // That add-on bolts an endpoint onto the server listing its alternative entry
  // domains -- same Emby and same token, different route in, so a slow or
  // blocked line can be swapped without touching the session. Token-gated;
  // servers without the add-on just 404, which means "no extra lines" and the
  // home address keeps working.
  async lines() {
    for (const path of ['/emby/System/Ext/ServerDomains', '/System/Ext/ServerDomains']) {
      try {
        const r = await this._get(path);
        const list = (r?.data || []).filter(d => d?.url).map(d => ({ name: d.name || d.url, url: norm(d.url) }));
        if (list.length) return list;
      } catch {}
    }
    return [];
  }
  async syncLines() {
    const list = await this.lines();
    servers.put({ url: this.home, lines: list });
    return list;
  }
  // What the viewer can pick from: the home address first, then the synced ones.
  allLines() {
    const out = [{ name: '主线路', url: this.home }];
    for (const l of servers.get(this.home)?.lines || []) if (l.url !== this.home) out.push(l);
    return out;
  }
  useLine(url) { this.server = norm(url) || this.home; this._persist(); return this.server; }
  icons() { return serverIcons({ line: this.server, token: this.token, userId: this.userId }); }

  // ---- low-level -----------------------------------------------------------
  // CORS-critical: use the STANDARD `Authorization` header, never the Emby-
  // specific X-Emby-Authorization / X-Emby-Token. A cross-origin Emby (verified
  // against a real server) only allow-lists `Authorization` in its CORS preflight
  // (Access-Control-Allow-Headers: Content-Type, Authorization, ...), so any
  // X-Emby-* header makes the browser block the request as "not allowed". Emby
  // accepts the same `MediaBrowser Client=...` credential string in either
  // header. The access token rides both here (Token="...") and as ?api_key=
  // (added in _url) so simple GETs work without any custom header at all.
  _authHeader() {
    return `MediaBrowser Client="${CLIENT}", Device="Browser", DeviceId="${this.deviceId}", Version="${VERSION}"`
      + (this.token ? `, Token="${this.token}"` : '');
  }
  _headers(json) {
    const h = { 'Authorization': this._authHeader() };
    if (json) h['Content-Type'] = 'application/json';
    return h;
  }
  _url(path, params) {
    const u = new URL(this.server + path);
    if (params) for (const [k, v] of Object.entries(params)) if (v != null) u.searchParams.set(k, v);
    if (this.token && !u.searchParams.has('api_key')) u.searchParams.set('api_key', this.token);
    return u.toString();
  }
  async _get(path, params) {
    const r = await fetch(this._url(path, params), { headers: this._headers() });
    if (!r.ok) throw new Error(`Emby ${r.status} ${path}`);
    return r.json();
  }
  async _post(path, body, params) {
    const r = await fetch(this._url(path, params), { method: 'POST', headers: this._headers(true), body: body ? JSON.stringify(body) : undefined });
    if (!r.ok) throw new Error(`Emby ${r.status} ${path}`);
    return r.status === 204 ? null : r.json().catch(() => null);
  }
  async _del(path, params) {
    const r = await fetch(this._url(path, params), { method: 'DELETE', headers: this._headers() });
    if (!r.ok) throw new Error(`Emby ${r.status} ${path}`);
    return r.status === 204 ? null : r.json().catch(() => null);
  }

  // ---- auth ----------------------------------------------------------------
  async login(username, password) {
    const r = await fetch(this._url('/Users/AuthenticateByName'), {
      method: 'POST', headers: this._headers(true),
      body: JSON.stringify({ Username: username, Pw: password ?? '' }),
    });
    if (r.status === 401) throw new Error('用户名或密码错误');
    if (!r.ok) throw new Error(`登录失败 (${r.status})`);
    const data = await r.json();
    this.token = data.AccessToken;
    this.userId = data.User?.Id;
    this.userName = data.User?.Name;
    // What this account is ALLOWED to do. Measured, never assumed: two live
    // servers proved the same UI must offer different menus per account
    // (IsAdministrator, EnableContentDeletion, EnableContentDownloading).
    this.policy = data.User?.Policy || {};
    this._persist();
    return data.User;
  }
  publicUsers() { return this._get('/Users/Public').catch(() => []); }

  // ---- library / browse ----------------------------------------------------
  // One Fields set for everything so a list never needs a follow-up call per item.
  static FIELDS = 'PrimaryImageAspectRatio,Overview,Genres,ProductionYear,CommunityRating,OfficialRating,RunTimeTicks,MediaSources,People';

  views() { return this._get(`/Users/${this.userId}/Views`).then(r => r.Items || []); }

  // The Items envelope, unwrapped. items() returns just the array (most callers
  // only want that); itemsPage() keeps TotalRecordCount for the library page's
  // count + pagination. Both funnel through here so the param map lives once.
  _items(opts = {}) {
    return this._get(`/Users/${this.userId}/Items`, {
      ParentId: opts.parentId, IncludeItemTypes: opts.types, Recursive: opts.recursive ?? true,
      SortBy: opts.sortBy || 'SortName', SortOrder: opts.sortOrder || 'Ascending',
      Fields: opts.fields || EmbyClient.FIELDS, StartIndex: opts.start, Limit: opts.limit,
      Filters: opts.filters, Genres: opts.genre, SearchTerm: opts.search,
      // extra library filters (each is a separate Emby query param, not a Filters value)
      IsHD: opts.ishd, Is4K: opts.is4k, HasSubtitles: opts.hasSubtitles,
    });
  }
  items(opts = {}) { return this._items(opts).then(r => r.Items || []); }
  itemsPage(opts = {}) {
    return this._items(opts).then(r => ({ items: r.Items || [], total: r.TotalRecordCount ?? (r.Items || []).length }));
  }
  item(id) { return this._get(`/Users/${this.userId}/Items/${id}`, { Fields: EmbyClient.FIELDS }); }

  // "继续观看". Scoped to the libraries handed in -- normally the ids from
  // views(), i.e. exactly what this account is allowed to SEE. A ParentId-less
  // recursive query is not the same thing: it walks every library the account
  // can read, including the ones Emby hides from this user's home screen, and
  // that is how a hidden library's half-watched episodes ended up on the rail
  // while its "最近添加" row (built from views()) correctly stayed away.
  //
  // IsUnplayed alongside IsResumable is the other half. IsResumable only asks
  // "is PlaybackPositionTicks > 0", so an item marked played that still carries
  // a stale resume point matches it -- a finished show sitting in 继续观看.
  // Filters are ANDed, so the pair means "started and not finished".
  //
  // One request per library rather than one for the server: Emby takes a single
  // ParentId, and libraries are a handful, not a page.
  async resume(parentIds, limit = 24) {
    const ids = (parentIds || []).length ? parentIds : [null];
    const ask = (id, filters) => this._get(`/Users/${this.userId}/Items`, {
      ParentId: id, Filters: filters, Recursive: true,
      SortBy: 'DatePlayed', SortOrder: 'Descending',
      IncludeItemTypes: 'Movie,Episode', Fields: EmbyClient.FIELDS, Limit: limit,
    }).then(r => r.Items || []);
    const pages = await Promise.all(ids.map(id =>
      // A server that will not take the pair falls back to the filter that has
      // always worked, rather than letting the catch turn the whole rail into
      // nothing -- an empty 继续观看 looks like "you have watched everything",
      // which is a worse lie than the bug this filter fixes.
      ask(id, 'IsResumable,IsUnplayed').catch(() => ask(id, 'IsResumable')).catch(() => [])));
    // Merged by hand: each library sorted itself, the rail wants one timeline.
    // Date.parse('') is NaN, not 0 -- an item the server gave no LastPlayedDate
    // has to sort last, not wherever NaN lands.
    const played = it => Date.parse(it.UserData?.LastPlayedDate || '') || 0;
    const seen = new Set();
    return pages.flat()
      // Played is re-checked here and not left to the server: it is free, and it
      // is what makes the fallback above safe to take.
      .filter(it => it?.Id && !it.UserData?.Played && !seen.has(it.Id) && seen.add(it.Id))
      .sort((a, b) => played(b) - played(a))
      .slice(0, limit);
  }
  latest(parentId, limit = 20) {
    return this._get(`/Users/${this.userId}/Items/Latest`, { ParentId: parentId, Limit: limit, Fields: EmbyClient.FIELDS });
  }
  nextUp(limit = 20) {
    return this._get('/Shows/NextUp', { UserId: this.userId, Limit: limit, Fields: EmbyClient.FIELDS }).then(r => r.Items || []).catch(() => []);
  }
  seasons(seriesId) { return this._get(`/Shows/${seriesId}/Seasons`, { UserId: this.userId, Fields: EmbyClient.FIELDS }).then(r => r.Items || []); }
  episodes(seriesId, seasonId) { return this._get(`/Shows/${seriesId}/Episodes`, { UserId: this.userId, SeasonId: seasonId, Fields: EmbyClient.FIELDS }).then(r => r.Items || []); }
  search(term, limit = 40) { return this.items({ search: term, recursive: true, types: 'Movie,Series,Episode,Person', limit }); }

  // ---- images (no extra request: tags come on the item) --------------------
  imageUrl(item, type = 'Primary', { maxWidth = 400, maxHeight, quality = 90 } = {}) {
    if (!item) return null;
    // id and tag MUST come from the same entity. The old code chose them
    // separately: any Episode whose series had a poster got the series' id,
    // but the tag still preferred the episode's OWN Primary tag -- a series-id
    // + episode-tag URL Emby can't resolve, so an episode that genuinely had a
    // cover (e.g. Emby's auto-generated thumbnail) rendered blank. Pair them:
    // the item's own image first, the series poster only as a fallback.
    let id = item.Id, tag;
    if (type === 'Primary') {
      if (item.ImageTags?.Primary) tag = item.ImageTags.Primary;                 // this item's own cover
      else if (item.Type === 'Episode' && item.SeriesPrimaryImageTag) {          // no own cover -> series poster
        id = item.SeriesId; tag = item.SeriesPrimaryImageTag;
      }
    } else {
      tag = item.BackdropImageTags?.[0] || item.ImageTags?.[type];
    }
    if (!tag) return null;
    const p = new URLSearchParams({ tag, quality: String(quality), maxWidth: String(maxWidth) });
    if (maxHeight) p.set('maxHeight', String(maxHeight));
    if (this.token) p.set('api_key', this.token);
    return `${this.server}/Items/${id}/Images/${type}?${p}`;
  }
  backdropUrl(item, opts) {
    if (!item) return null;
    const tag = item.BackdropImageTags?.[0] || item.ParentBackdropImageTags?.[0];
    const id = item.BackdropImageTags?.[0] ? item.Id : (item.ParentBackdropItemId || item.Id);
    if (!tag) return this.imageUrl(item, 'Primary', { maxWidth: 1280 });
    const p = new URLSearchParams({ tag, quality: '85', maxWidth: String(opts?.maxWidth || 1280) });
    if (this.token) p.set('api_key', this.token);
    return `${this.server}/Items/${id}/Images/Backdrop?${p}`;
  }

  // ---- playback ------------------------------------------------------------
  // Ask the server how it will serve this item; with the empty-transcode profile
  // above it can only answer with a direct source. Returns { source, url,
  // playSessionId } ready to hand to the player.
  async playbackInfo(itemId) {
    const data = await this._post(`/Items/${itemId}/PlaybackInfo`, {
      UserId: this.userId, DeviceProfile: DIRECT_PROFILE, MaxStreamingBitrate: 400_000_000,
      AutoOpenLiveStream: true,
    }, { UserId: this.userId });
    const source = data?.MediaSources?.[0];
    if (!source) throw new Error('该条目没有可用的媒体源');
    return { source, sources: data.MediaSources, playSessionId: data.PlaySessionId, url: this.streamUrl(itemId, source) };
  }
  // The untouched original file. Static=true guarantees no server-side remux.
  streamUrl(itemId, source) {
    const p = new URLSearchParams({ Static: 'true', mediaSourceId: source?.Id || itemId });
    if (source?.ETag) p.set('Tag', source.ETag);
    if (this.token) p.set('api_key', this.token);
    return `${this.server}/Videos/${itemId}/stream${containerExt(source)}?${p}`;
  }

  // Ask the server to extract/convert a subtitle stream to SRT so the browser's
  // own <track> can draw it. Used for the two cases the in-browser renderers do
  // NOT cover: an external (sidecar) subtitle, and an EMBEDDED text subtitle
  // (S_TEXT/UTF8 etc.) — Subtitles only renders ass/ssa/pgs, so text subs would
  // otherwise silently show nothing. ASS/PGS are still demuxed and drawn client-
  // side (effects, embedded fonts, HDR-safe), never routed here.
  subtitleUrl(itemId, source, stream, format = 'srt') {
    const p = new URLSearchParams();
    if (this.token) p.set('api_key', this.token);
    return `${this.server}/Videos/${itemId}/${source?.Id || itemId}/Subtitles/${stream.Index}/Stream.${format}?${p}`;
  }

  // ---- item actions: what Emby's own right-click menu offers ---------------
  // Measured against two live servers: PlayedItems / FavoriteItems answer 405 to
  // a GET, i.e. the route exists and POST/DELETE is the way in. A reverse-proxied
  // Emby hides whole branches of the API (verified: the same paths 404 there), so
  // every one of these can legitimately fail — callers surface that as "this
  // server does not allow it", never as a silent no-op.
  markPlayed(id, played = true) {
    const p = `/Users/${this.userId}/PlayedItems/${id}`;
    return played ? this._post(p) : this._del(p);
  }
  setFavorite(id, fav = true) {
    const p = `/Users/${this.userId}/FavoriteItems/${id}`;
    return fav ? this._post(p) : this._del(p);
  }
  // Emby's thumbs up/down. null clears it.
  setLike(id, likes) {
    const p = `/Users/${this.userId}/Items/${id}/Rating`;
    return likes == null ? this._del(p) : this._post(p, null, { Likes: likes });
  }
  similar(id, limit = 12) {
    return this._get(`/Items/${id}/Similar`, { UserId: this.userId, Limit: limit, Fields: EmbyClient.FIELDS }).then(r => r.Items || []);
  }
  playlists() { return this.items({ types: 'Playlist', recursive: true, limit: 100 }); }
  createPlaylist(name, ids) {
    return this._post('/Playlists', null, { Name: name, Ids: [].concat(ids).join(','), UserId: this.userId, MediaType: 'Video' });
  }
  addToPlaylist(playlistId, ids) {
    return this._post(`/Playlists/${playlistId}/Items`, null, { Ids: [].concat(ids).join(','), UserId: this.userId });
  }
  deleteItem(id) { return this._del(`/Items/${id}`); }
  // replaceAll = Emby's "replace all metadata", i.e. re-scrape rather than fill gaps.
  refreshMetadata(id, { replaceAll = false, recursive = false } = {}) {
    return this._post(`/Items/${id}/Refresh`, null, {
      Recursive: recursive,
      MetadataRefreshMode: replaceAll ? 'FullRefresh' : 'Default',
      ImageRefreshMode: replaceAll ? 'FullRefresh' : 'Default',
      ReplaceAllMetadata: replaceAll, ReplaceAllImages: replaceAll,
    });
  }
  // Emby wants the WHOLE item back, not a patch: send the item you read, edited.
  updateItem(item) { return this._post(`/Items/${item.Id}`, item); }
  // "Identify": search the metadata providers, then apply one of the hits.
  remoteSearch(type, { name, year, itemId }) {
    return this._post(`/Items/RemoteSearch/${type}`, {
      SearchInfo: { Name: name, Year: year || null, ProviderIds: {} },
      ItemId: itemId, IncludeDisabledProviders: true,
    }).then(r => r || []);
  }
  applyRemoteSearch(id, result, replaceImages = true) {
    return this._post(`/Items/RemoteSearch/Apply/${id}`, result, { ReplaceAllImages: replaceImages });
  }

  // ---- admin ---------------------------------------------------------------
  // Only reachable with IsAdministrator, and even then a proxied server may have
  // the route stripped (measured: /ScheduledTasks and /Sessions 404 on one line
  // while /System/Info and /Library/VirtualFolders answer fine). The dashboard
  // therefore probes each card independently instead of assuming a whole tier.
  systemInfo() { return this._get('/System/Info'); }
  itemCounts() { return this._get('/Items/Counts'); }
  allUsers() { return this._get('/Users'); }
  virtualFolders() { return this._get('/Library/VirtualFolders'); }
  scanAll() { return this._post('/Library/Refresh'); }
  tasks() { return this._get('/ScheduledTasks'); }
  runTask(id) { return this._post(`/ScheduledTasks/Running/${id}`); }
  stopTask(id) { return this._del(`/ScheduledTasks/Running/${id}`); }
  activity(limit = 20) { return this._get('/System/ActivityLog/Entries', { Limit: limit }).then(r => r.Items || []); }
  sessions() { return this._get('/Sessions'); }
  sessionMessage(id, text) { return this._post(`/Sessions/${id}/Message`, { Text: text, Header: 'LinWeb', TimeoutMs: 8000 }); }
  sessionStop(id) { return this._post(`/Sessions/${id}/Playing/Stop`); }
  devices() { return this._get('/Devices').then(r => r.Items || r || []); }
  deleteDevice(id) { return this._del('/Devices', { Id: id }); }
  setUserPolicy(userId, policy) { return this._post(`/Users/${userId}/Policy`, policy); }
  restartServer() { return this._post('/System/Restart'); }

  // ---- progress (best-effort: never let a report break playback) -----------
  reportStart(itemId, source, playSessionId) {
    return this._post('/Sessions/Playing', { ItemId: itemId, MediaSourceId: source?.Id, PlaySessionId: playSessionId, PlayMethod: 'DirectStream', CanSeek: true }).catch(() => {});
  }
  reportProgress(itemId, source, playSessionId, positionSec, paused) {
    return this._post('/Sessions/Playing/Progress', { ItemId: itemId, MediaSourceId: source?.Id, PlaySessionId: playSessionId, PositionTicks: Math.round(positionSec * 1e7), IsPaused: !!paused, PlayMethod: 'DirectStream', EventName: 'TimeUpdate' }).catch(() => {});
  }
  reportStopped(itemId, source, playSessionId, positionSec) {
    return this._post('/Sessions/Playing/Stopped', { ItemId: itemId, MediaSourceId: source?.Id, PlaySessionId: playSessionId, PositionTicks: Math.round(positionSec * 1e7) }).catch(() => {});
  }
}

// ---- small shared helpers the UI needs -------------------------------------

// Decide how a chosen Emby subtitle stream reaches the screen:
//   'client'      -> demux from the container, draw in-browser (ass/ssa via
//                    JASSUB, pgs via libpgs, srt/subrip via the browser's own
//                    TextTrack). Keeps effects + embedded fonts, no server work.
//                    Only for EMBEDDED streams.
//   'text'        -> have Emby serve it as SRT into the browser's <track>. For
//                    EXTERNAL (sidecar) text subs, which are not in the
//                    container and so cannot be demuxed.
//   'unsupported' -> an external bitmap sub (external pgs/vobsub); no clean path.
//
// Embedded SRT used to be routed to 'text' as well, back when there was no
// in-browser renderer for it. There is one now (src/subs/text.js), and the
// round trip it replaced was the flaky part: Emby extracts an embedded text
// stream with ffmpeg on first request, so that URL intermittently answers 404 /
// 500 / an empty body, which surfaced as "字幕加载失败" on a random subset of
// plays. Demuxing it locally has no such window.
// Matched whole, not by substring: "dvb_teletext" contains "text" but has no
// local renderer, and routing it here would have shown nothing at all.
const CLIENT_TEXT = new Set(['ass', 'ssa', 'srt', 'subrip', 'mov_text', 'tx3g', 'webvtt', 'vtt']);
export function subtitleDelivery(stream) {
  const codec = (stream?.Codec || '').toLowerCase();
  const bitmap = /pgs|vobsub|dvbsub|dvb_subtitle|dvd_subtitle/.test(codec);
  if (!stream?.IsExternal && (bitmap || CLIENT_TEXT.has(codec))) return 'client';
  if (!bitmap) return 'text';
  return 'unsupported';
}

export const ticksToSec = t => (t || 0) / 1e7;
export const fmtRuntime = ticks => {
  const s = ticksToSec(ticks); if (!s) return '';
  const h = Math.floor(s / 3600), m = Math.round(s % 3600 / 60);
  return h ? `${h}h ${m}m` : `${m}m`;
};
export const progressOf = item => {
  const pct = item?.UserData?.PlayedPercentage;
  if (pct) return pct / 100;
  const pos = item?.UserData?.PlaybackPositionTicks, run = item?.RunTimeTicks;
  return pos && run ? pos / run : 0;
};
