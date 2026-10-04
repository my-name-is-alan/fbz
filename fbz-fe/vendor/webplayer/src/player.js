// MSE playback controller.
//
// The heavy work -- reading the file, demuxing Matroska, remuxing to fMP4 --
// runs in src/player.worker.js, off the main thread, so a 4K remux burst can no
// longer stall compositing or the audio clock. This file keeps only what has to
// live on the main thread: the MediaSource and its SourceBuffers, the software
// audio transcoder (it drives the browser's AudioEncoder, main-thread only), and
// the glue that forwards playhead/seek to the worker and appends what it sends
// back. The public API is unchanged from the old single-thread Player.
import { buildRemuxer, audioNote } from './remux/tracks.js';
import { SoftwareAudioDecoder, RAW_FORMATS } from './audio/decode.js';
import { AudioTranscoder, describeError } from './audio/transcode.js';

const KEEP_BEHIND = 12;           // seconds retained behind the playhead before eviction

/**
 * Serialises appendBuffer calls (MSE allows only one in flight) and recovers
 * from a full SourceBuffer by evicting already-played media.
 */
class BufferQueue {
  constructor(sb, video, ms, log) {
    this.sb = sb; this.video = video; this.ms = ms; this.log = log;
    this.q = []; this.backlog = 0; this.evictions = 0; this.dead = false;
    sb.addEventListener('updateend', () => this._pump());
  }
  push(data) { this.q.push(data); this.backlog += data.length; this._pump(); }
  _pump() {
    if (this.dead || !this.q.length || this.sb.updating || this.ms.readyState !== 'open') return;
    const data = this.q[0];
    try {
      this.sb.appendBuffer(data);
      this.q.shift(); this.backlog -= data.length;
    } catch (e) {
      if (e.name === 'QuotaExceededError') { this._evict(); return; }
      this.q.shift(); this.backlog -= data.length;
      // A SourceBuffer detached from its MediaSource never comes back, and the
      // detaching is a consequence of the decode error one line above it in the
      // log. Retrying repeats this message until it has buried the only line
      // worth reading, so say it once and stop.
      if (e.name === 'InvalidStateError') { this.dead = true; this.clear(); }
      this.log(`append failed (${e.name}): ${e.message}${this.dead ? ' — 这条 SourceBuffer 已失效，停止投喂（真正的原因在上一行）' : ''}`, 'error');
    }
  }
  _evict() {
    const b = this.sb.buffered;
    if (!b.length) { this.q.shift(); this.log('quota exceeded with empty buffer; dropping fragment', 'error'); return; }
    const start = b.start(0);
    const cutoff = this.video.currentTime - KEEP_BEHIND;
    if (cutoff - start < 1) { setTimeout(() => this._pump(), 300); return; }
    this.evictions++;
    try { this.sb.remove(start, cutoff); } catch { setTimeout(() => this._pump(), 300); }
  }
  clear() { this.q.length = 0; this.backlog = 0; }
  get idle() { return !this.q.length && !this.sb.updating; }
}

export class Player {
  constructor(video) {
    this.video = video;
    this.info = null;
    this.log = () => {};
    this.onSubtitlePacket = null;
    this.onStalled = null;        // read loop died after playback began

    this.mediaSource = null;
    this.streams = [];            // { track, mime, kind, sb, queue }
    this.transcoder = null;
    this.audioDecoder = null;
    this.softwareAudio = true;
    this._subs = new Map();       // track number -> entry
    this._fonts = [];
    this._worker = null;
    this._pending = null;         // { resolve, reject } for the in-flight open/play
    this._objectUrl = null;
  }

  _spawn() {
    if (this._worker) return;
    this._worker = new Worker(new URL('./player.worker.js', import.meta.url), { type: 'module' });
    this._worker.onmessage = (e) => this._onWorker(e.data);
    this._worker.onerror = (e) => this.log(`worker crashed: ${e.message}`, 'error');
  }

  _post(msg, transfer) { this._worker?.postMessage(msg, transfer || []); }

  // Three failures mean "this engine can't read the stream", not "the stream is
  // unreachable": the container isn't Matroska (a .strm pointing at an mp4 is
  // the common case), nobody would state the total length, or the server won't
  // do Range. In all three the fetch itself SUCCEEDED, so CORS is already proven
  // and the browser's own decoder deserves a turn. Anything else stays fatal --
  // falling back on a genuine network/CORS error would only fail again, slower,
  // with a worse message.
  // (The container codes are all "we opened it and this engine cannot index
  // it": an unknown container, a fragmented mp4 whose sample tables live in
  // every moof rather than in moov, or a head that sniffed as one thing and
  // failed to parse as it.)
  static NATIVE_FALLBACK = new Set([
    'NOT_MATROSKA', 'NO_SIZE', 'NO_RANGE',
    'UNKNOWN_CONTAINER', 'NOT_MP4', 'NOT_FLV', 'FRAGMENTED_MP4',
  ]);

  // The other way the remux path dies: the fetch never completed at all. The
  // browser refuses to say why (CORS block, DNS, offline and mixed content all
  // surface as one bare TypeError -- there is no API that separates them), and
  // for a .strm pointing at object storage the likeliest cause by far is simply
  // no Access-Control-Allow-Origin. That is still playable: a <video> WITHOUT
  // the crossorigin attribute needs no CORS whatsoever. It costs the pixels
  // (tainted canvas) and the audio graph, which is why this leg is entered with
  // cors=false and the UI greys those two out.
  static CORS_BLIND = /Failed to fetch|NetworkError|Load failed|CORS|ERR_FAILED/i;

  // Containers NEITHER leg can open: this engine speaks Matroska only, and no
  // browser demuxes these either. Worth naming, because the failure the viewer
  // otherwise sees is "not a Matroska file" -- perfectly true of an .avi, and
  // perfectly useless.
  static NO_DEMUXER = /\.(avi|flv|rm|rmvb|wmv|asf|ts|m2ts|mts|mpg|mpeg|vob|divx|3gp|ogm)$/i;
  static hint(name) {
    const ext = (String(name || '').split(/[?#]/)[0].match(/\.[a-z0-9]{2,5}$/i) || [''])[0];
    return Player.NO_DEMUXER.test(ext)
      ? `${ext} 容器浏览器不解封装，本播放器也只解 Matroska — 先转封装即可（ffmpeg -i in${ext} -c copy out.mkv）` : '';
  }

  /**
   * Try several sources for the SAME item, best first, and return the first that
   * opens. Used for a .strm item, which can be fetched two ways: straight from
   * the remote URL the .strm points at (no bytes through Emby at all) or from
   * Emby's own proxy of it.
   *
   * Every candidate but the last is tried WITHOUT the native fallback, and that
   * is the whole subtlety: playing a direct link natively costs the track
   * picker, client-side ASS/PGS and the HDR verdict, whereas remuxing the SAME
   * content through the server keeps all three. So a candidate that cannot be
   * remuxed must yield to the next one rather than settle for native. Only when
   * nothing is left does the last candidate get to use every leg it has.
   */
  async loadAny(candidates) {
    const list = [].concat(candidates).filter(Boolean);
    if (!list.length) throw new Error('no source to open');
    let last;
    for (let i = 0; i < list.length; i++) {
      const isLast = i === list.length - 1;
      try {
        return await this.load(list[i], { allowNative: isLast });
      } catch (e) {
        last = e;
        if (!isLast) this.log(`候选源 ${i + 1}/${list.length} 打不开（${e.message}）— 换下一个`, 'warn');
      }
    }
    throw last;
  }

  /**
   * Open a source. Tries the remux pipeline first (it is the only path that
   * gives per-track choice, embedded ass/pgs and the HDR-correct fMP4), and
   * drops to plain <video src> when the container -- or the origin -- is
   * outside its reach. `allowNative: false` suppresses that second leg, for a
   * caller that still has a better candidate to try (see loadAny).
   */
  async load(input, { allowNative = true } = {}) {
    const blob = input instanceof Blob ? input : null;    // a picked or dropped file
    const url = typeof input === 'string' ? input : (input?.url ?? null);
    // Which candidate the open actually came from. loadAny() may have walked
    // past several, and the page reports the byte source to the viewer -- so it
    // must be read from what happened, not from what was preferred.
    this.openedFrom = url ?? null;
    try {
      return await this._loadMse(input);
    } catch (e) {
      if (!allowNative) throw e;
      // A local File has no URL to give a <video> -- which was the ONLY reason
      // the fallback stayed out of reach for a picked .mp4, even though the
      // picker offers .mp4/.m4v. Mint one here, in the failure path, so an
      // ordinary remux load never creates an object URL it must remember to
      // revoke. (_teardown revokes whatever is on the element.)
      const src = url ?? (blob ? URL.createObjectURL(blob) : null);
      if (!src) throw e;
      const spent = () => { if (blob) URL.revokeObjectURL(src); };

      // Readable, wrong container/shape: CORS is proven, so keep the extras.
      if (Player.NATIVE_FALLBACK.has(e.code)) {
        this.log(`${e.message} — 交给浏览器原生解码`, 'warn');
        try {
          return await this.loadNative(src, e.code, true, blob);
        } catch (e2) {
          // Both legs are gone. If the container is one nothing here demuxes,
          // say THAT -- "not a Matroska file" is a true and worthless thing to
          // tell someone who opened an .avi.
          spent();
          const hint = Player.hint(blob?.name || src);
          throw hint ? Object.assign(new Error(hint), { code: 'NO_DEMUXER' })
                     : Object.assign(new Error(e.message), { code: e.code, nativeAlsoFailed: e2.message });
        }
      }
      // Unreadable: go straight to a bare element, no crossorigin attempt. Only
      // ever a remote condition -- a Blob read does not fail on CORS.
      if (!blob && Player.CORS_BLIND.test(e.message)) {
        this.log(`字节流读取失败（${e.message}）— 尝试无跨域凭证的原生播放`, 'warn');
        try {
          return await this.loadNative(src, 'NO_CORS', false);
        } catch (e2) {
          // Report the ORIGINAL failure, which is the one that names a fixable
          // cause; the native retry only ever says "MEDIA_ERR_*" and would send
          // the viewer looking in the wrong place.
          throw Object.assign(new Error(e.message), { code: e.code, nativeAlsoFailed: e2.message });
        }
      }
      spent();
      throw e;
    }
  }

  /**
   * The fallback leg: let the browser demux and decode. Everything this player
   * adds on top of a <video> is built on READING the bytes, so all of it is off
   * here -- no track picking, no client-side ass/pgs, no HDR verdict. Whether
   * Anime4K and the WebAudio gain stage survive comes down to one bit: a media
   * element loaded WITHOUT crossorigin taints the canvas and silences
   * createMediaElementSource. So try crossorigin="anonymous" first (which the
   * probe above already showed the origin allows) and keep them; only if that
   * load fails -- a redirect to a CDN that does not send the header -- retry
   * bare and let the UI grey them out. `nativeCors` is that bit.
   */
  async loadNative(url, reason, tryCors = true, blob = null) {
    await this._teardown();
    const v = this.video;

    const attempt = (cors) => new Promise((resolve, reject) => {
      const done = (fn, arg) => { clearTimeout(timer); v.removeEventListener('loadedmetadata', ok); v.removeEventListener('error', bad); fn(arg); };
      const ok = () => done(resolve);
      const bad = () => done(reject, new Error(v.error ? `原生播放失败 (code ${v.error.code}): ${v.error.message || '浏览器未给出原因'}` : '原生播放失败'));
      const timer = setTimeout(() => done(reject, new Error('原生播放：15 秒内没有拿到元数据')), 15000);
      v.addEventListener('loadedmetadata', ok);
      v.addEventListener('error', bad);
      if (cors) v.setAttribute('crossorigin', 'anonymous'); else v.removeAttribute('crossorigin');
      v.src = url;
      v.load();
    });

    this.nativeCors = tryCors;
    if (tryCors) {
      try {
        await attempt(true);
      } catch (e) {
        // Bare retry. Worth it: it is the difference between playing without the
        // extras and not playing at all. Reached when the stream redirects to a
        // host that sends no CORS header even though the first hop did.
        this.log(`crossorigin 载入失败 (${e.message})，改用无跨域凭证重试 — 超分与增益将不可用`, 'warn');
        this.nativeCors = false;
        await attempt(false);
      }
    } else {
      await attempt(false);
    }

    // A shape render() can consume unchanged. `native` is what the UI branches
    // on; the single pseudo-track keeps the "video/decode" read-out honest
    // instead of inventing codec details nobody measured.
    this.info = {
      native: true, nativeReason: reason, nativeCors: this.nativeCors,
      duration: Number.isFinite(v.duration) ? v.duration : 0,
      // Nothing read the bytes, so there is no container name to report; the
      // basename is the only honest label available. A local file at least
      // still knows its own name and length -- an object URL would show a uuid.
      name: blob?.name || decodeURIComponent(url.split(/[?#]/)[0].split('/').pop() || '远程流'),
      size: blob?.size || 0, fonts: 0,
      hdr: null, dynamicHdr: null, hdr10plus: false, dolbyVision: null,
      video: [{ label: '原生解码', supported: true, track: { codecId: '浏览器内置', language: '' } }],
      audio: [], subtitles: [],
    };
    this.log(`原生播放已就绪（${reason}）· 时长 ${this.info.duration ? this.info.duration.toFixed(1) + 's' : '未知'}`
      + (this.nativeCors ? '' : ' · canvas 受污染，超分/增益停用'));
    return this.info;
  }

  async _loadMse(input) {
    this._spawn();
    // Switching files: tear down the previous MediaSource/streams first, so a
    // rapid load(B) after load(A)/play(A) does not leave the old pipeline
    // appending into a half-gone MediaSource.
    if (this.mediaSource) await this._teardown();
    const info = await new Promise((resolve, reject) => {
      this._pending = { resolve, reject, op: 'open' };
      this._post({ type: 'open', input });
    });
    // Only the main thread has MediaSource.isTypeSupported, so the playability
    // verdict is filled in here rather than in the worker.
    for (const v of info.video) v.supported = v.mime ? MediaSource.isTypeSupported(v.mime) : false;
    for (const a of info.audio) {
      a.supported = a.mime ? MediaSource.isTypeSupported(a.mime) : false;
      if (a.supported) a.note = null;
    }
    this._fonts = (info._fontData ?? []).map(f => ({ name: f.name, mime: f.mime, data: f.data }));
    this.info = info;
    return info;
  }

  /** Start playback with the chosen video/audio track indices. */
  async play({ videoIndex = 0, audioIndex = 0 } = {}) {
    // Native leg: the element already holds the stream and there are no tracks
    // to switch between. Tearing down here would drop the src that IS the
    // playback -- so this is a no-op, not an error, and callers need no branch.
    if (this.info?.native) return;
    await this._teardown();

    const v = this.info.video[videoIndex];
    const a = this.info.audio[audioIndex];
    let videoTrack = null, audioTrack = null, transcodeTrack = null;
    if (v?.supported) videoTrack = v.track.number;
    else if (v) this.log(`video track unplayable via MSE (${v.mime}) — no repackaging path`, 'error');

    if (a?.supported) audioTrack = a.track.number;
    else if (a && RAW_FORMATS[a.track.codecId] && this.softwareAudio !== false) transcodeTrack = a.track;
    else if (a) this.log(`audio "${a.label}" needs a software decoder that is unavailable — playing without audio`, 'warn');

    if (videoTrack == null) throw new Error('nothing playable in this file');

    // A tiny file can reach EOF in the worker before _startTranscode has added
    // the opus SourceBuffer; endOfStream() then closes the MediaSource and the
    // addSourceBuffer fails. Hold off ending until the audio pipeline is built.
    this._audioSetupDone = !transcodeTrack;
    this._eofSeen = false;

    const ms = this.mediaSource = new MediaSource();
    this._objectUrl = URL.createObjectURL(ms);
    this.video.src = this._objectUrl;
    this.video.load();
    await this._awaitSourceOpen(ms);
    this.log(`MediaSource open (readyState=${ms.readyState})`);
    ms.addEventListener('sourceclose', () => this.log('MediaSource CLOSED', 'warn'));
    if (Number.isFinite(this.info.duration) && this.info.duration > 0) ms.duration = this.info.duration;

    // The worker streams init segments + fragments once told to play; the
    // handlers below wire each init into a SourceBuffer. The video (and any
    // native audio) SourceBuffers MUST be added before the transcode one:
    // Chromium caps the SourceBuffer count and rejects a second video/audio SB
    // added after appends to the first have begun, so order matters.
    this._playReady = new Promise((resolve, reject) => { this._pending = { resolve, reject, op: 'play' }; });
    this._expectInit = [videoTrack, audioTrack].filter(x => x != null).length;
    this._post({ type: 'play', video: videoTrack, audio: audioTrack, transcode: transcodeTrack?.number ?? null });

    this.video.addEventListener('timeupdate', this._onTick);
    this.video.addEventListener('waiting', this._onTick);
    this.video.addEventListener('seeking', this._onSeek);
    await this._playReady;   // video (+ native audio) SourceBuffers now exist

    // The transcode SourceBuffer is added last. Blocks the worker forwards
    // before this is ready are dropped (a fraction of a second at the very
    // start); the transcoder keys off source timestamps so it stays in sync.
    if (transcodeTrack) {
      try { await this._startTranscode(transcodeTrack, ms); }
      catch (e) { this.log(`software audio unavailable (${describeError(e)}) — playing without audio`, 'warn'); this.transcoder = null; }
    }
    this._audioSetupDone = true;
    if (this._eofSeen) { this.transcoder?.flush(); this._maybeEnd('audio setup complete after eof'); }
  }

  _onTick = () => { if (this.mediaSource) this._post({ type: 'time', currentTime: this.video.currentTime }); };
  _onSeek = () => {
    if (!this.mediaSource) return;
    // Already buffered? Let the browser handle it, no worker round trip.
    const b = this.video.buffered, t = this.video.currentTime;
    for (let i = 0; i < b.length; i++) if (t >= b.start(i) && t < b.end(i) - 0.5) return;
    this._post({ type: 'seek', seconds: t });
  };

  _onWorker(m) {
    switch (m.type) {
      case 'info': this._pending?.op === 'open' && this._pending.resolve(m.info); this._pending = null; break;
      case 'init': this._addSource(m.track, m.mime, m.data); break;
      case 'fragment': { const s = this.streams.find(x => x.track === m.track); s?.queue.push(new Uint8Array(m.data)); break; }
      case 'audioBlock': this.transcoder?.push({ time: m.time, data: new Uint8Array(m.data) }); this.transcoder?.emitPending?.(); break;
      case 'subtitle': this._emitSubtitle(m); break;
      case 'flush': this._onFlush(); break;
      case 'eof': this._eofSeen = true; this.transcoder?.flush(); this._maybeEnd('worker reached end'); break;
      case 'log': this.log(m.msg, m.level); break;
      // The read loop died after playback had already started. Nothing here can
      // fix it -- the page owns the candidate list and the playhead -- so it is
      // forwarded, once, to whoever is listening.
      case 'stalled': this.onStalled?.(m); break;
      case 'error': this._pending?.reject(Object.assign(new Error(m.message), { code: m.code })); this._pending = null; break;
    }
  }

  _addSource(track, mime, data) {
    if (this.mediaSource?.readyState !== 'open') return;
    let sb;
    try {
      sb = this.mediaSource.addSourceBuffer(mime);
    } catch (e) {
      this.log(`addSourceBuffer("${mime}") failed: ${e.name} ${e.message}`, 'error');
      // ...and tell play() about it. It is awaiting one init per chosen track;
      // returning quietly here leaves that promise unresolved FOREVER, so a
      // rejected SourceBuffer showed up as a player that never started and
      // never said why -- no error, no timeout, nothing to see in the UI.
      if (this._pending?.op === 'play') {
        this._pending.reject(Object.assign(new Error(`SourceBuffer 建立失败 (${mime}): ${e.message}`), { code: 'NO_SOURCEBUFFER' }));
        this._pending = null;
      }
      return;
    }
    sb.mode = 'segments';
    const kind = mime.startsWith('video') ? 'video' : 'audio';
    const queue = new BufferQueue(sb, this.video, this.mediaSource, (msg, lvl) => this.log(`${mime}: ${msg}`, lvl));
    queue.push(new Uint8Array(data));
    this.streams.push({ track, mime, kind, sb, queue });
    this.log(`SourceBuffer ${mime}`);
    if (this.streams.length >= this._expectInit && this._pending?.op === 'play') { this._pending.resolve(); this._pending = null; }
  }

  _onFlush() {
    // A worker seek restarts the timeline; drop queued fragments and the buffered
    // island around the old position so the new fragments are not rejected.
    for (const s of this.streams) {
      s.queue.clear();
      const b = Player._ranges(s.sb);
      if (b?.length) { try { s.sb.remove(b.start(0), b.end(b.length - 1)); } catch {} }
    }
    if (this.transcoder) { this.transcoder.reset(); this.transcoder.queue?.clear(); }
  }

  async _startTranscode(track, ms) {
    if (!window.AudioEncoder) throw new Error('no AudioEncoder in this browser');
    this.audioDecoder ??= new SoftwareAudioDecoder({ log: (msg, l) => this.log(msg, l) });
    let queue = null;
    const tc = new AudioTranscoder(track, this.audioDecoder, this.info.duration, {
      log: (msg, l) => this.log(msg, l),
      onFragment: (frag) => queue?.push(frag),
    });
    if (!MediaSource.isTypeSupported(tc.mime)) throw new Error(`${tc.mime} rejected by MSE`);
    const sb = ms.addSourceBuffer(tc.mime);
    sb.mode = 'segments';
    queue = new BufferQueue(sb, this.video, ms, (msg, lvl) => this.log(`transcoded audio: ${msg}`, lvl));
    queue.push(tc.initSegment());
    tc.queue = queue; tc.sb = sb;
    this.transcoder = tc;
    this.log(`software audio: ${track.codecId} -> ${tc.mime}`);
  }

  static _ranges(sb) { try { return sb.buffered; } catch { return null; } }

  _awaitSourceOpen(ms, visibleTimeout = 10000) {
    return new Promise((resolve, reject) => {
      let timer = null;
      const NET = ['EMPTY', 'IDLE', 'LOADING', 'NO_SOURCE'];
      const finish = fn => { clearTimeout(timer); document.removeEventListener('visibilitychange', arm); fn(); };
      const arm = () => {
        clearTimeout(timer);
        if (document.hidden) { this.log('waiting for the tab to become visible before attaching media'); return; }
        timer = setTimeout(() => finish(() => reject(new Error(
          `sourceopen never fired (${visibleTimeout / 1000}s): ms=${ms.readyState} `
          + `networkState=${NET[this.video.networkState]} readyState=${this.video.readyState}`))), visibleTimeout);
      };
      ms.addEventListener('sourceopen', () => finish(resolve), { once: true });
      document.addEventListener('visibilitychange', arm);
      arm();
    });
  }

  _maybeEnd(why) {
    if (!this._audioSetupDone) return;   // audio SourceBuffer still being added
    const audioBusy = this.transcoder && (!this.transcoder.queue?.idle || this.transcoder._busy || this.transcoder._queue?.length);
    if (this.mediaSource?.readyState === 'open' && !audioBusy && this.streams.every(s => s.queue.idle)) {
      this.log(`endOfStream() <- ${why}`, 'warn');
      try { this.mediaSource.endOfStream(); } catch {}
    }
  }

  // --- subtitles (renderers live on the main thread) -----------------------

  enableSubtitle(index) {
    const entry = this.info?.subtitles?.[index];
    if (!entry) throw new Error(`no subtitle track at index ${index}`);
    this._subs.set(entry.track.number, entry);
    this.log(`subtitles on: ${entry.label}`);
    this._post({ type: 'sub', track: entry.track.number, on: true });   // worker backfills the buffered region
    return entry;
  }
  disableSubtitle(index) {
    const entry = this.info?.subtitles?.[index];
    if (entry) { this._subs.delete(entry.track.number); this._post({ type: 'sub', track: entry.track.number, on: false }); }
  }
  _emitSubtitle(m) {
    const entry = this._subs.get(m.track);
    if (entry && this.onSubtitlePacket) this.onSubtitlePacket({ time: m.time, duration: m.duration, data: new Uint8Array(m.data), track: m.track }, entry);
  }

  fontAttachments() { return this._fonts; }

  dispose() { return this._teardown(); }

  async _teardown() {
    this.video.removeEventListener('timeupdate', this._onTick);
    this.video.removeEventListener('waiting', this._onTick);
    this.video.removeEventListener('seeking', this._onSeek);
    this._post({ type: 'dispose' });
    // Explicitly remove every SourceBuffer before dropping the MediaSource.
    // Nulling the reference alone leaves the SourceBuffers (and their decoders)
    // alive until GC, and Chromium caps how many can exist -- playing a series
    // of files then hits "reached the limit of SourceBuffer objects".
    const ms = this.mediaSource;
    if (ms) {
      const drop = (sb) => { if (!sb) return; try { if (sb.updating) sb.abort(); } catch {} try { ms.removeSourceBuffer(sb); } catch {} };
      for (const s of this.streams) drop(s.sb);
      drop(this.transcoder?.sb);
    }
    this.streams = [];
    if (this.transcoder) { await this.transcoder.destroy?.().catch?.(() => {}); this.transcoder = null; }
    if (ms?.readyState === 'open') { try { ms.endOfStream(); } catch {} }
    this.mediaSource = null;
    if (this.video.src) { URL.revokeObjectURL(this.video.src); this.video.removeAttribute('src'); this.video.load(); }
    // Detaching from the element is what actually releases the old
    // MediaSource's decoder streams, and it is NOT synchronous. Opening the
    // next file straight away can then fail with "reached the limit of
    // SourceBuffer objects" on a brand-new MediaSource holding exactly one --
    // the limit is on what the renderer still has open, not on this object.
    // Bounded: a quarter second, then proceed and let the error surface.
    if (ms) for (let i = 0; i < 25 && ms.readyState !== 'closed'; i++) await new Promise(r => setTimeout(r, 10));
  }
}
