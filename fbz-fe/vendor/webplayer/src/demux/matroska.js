// Matroska demuxer. Reads structure only — it never touches codec payloads,
// which is the whole point: HEVC blocks come out byte-identical and go
// straight into fMP4, so HDR and Dolby Vision RPU survive untouched.
import { Reader, readId, readSize, readUint, readInt, readFloat, readString, eachChild } from './ebml.js';

const ID = {
  EBML: 0x1a45dfa3, Segment: 0x18538067,
  SeekHead: 0x114d9b74, Seek: 0x4dbb, SeekID: 0x53ab, SeekPosition: 0x53ac,
  Info: 0x1549a966, TimestampScale: 0x2ad7b1, Duration: 0x4489, MuxingApp: 0x4d80, WritingApp: 0x5741,
  Tracks: 0x1654ae6b, TrackEntry: 0xae,
  TrackNumber: 0xd7, TrackUID: 0x73c5, TrackType: 0x83, FlagDefault: 0x88, FlagForced: 0x55aa,
  DefaultDuration: 0x23e383, CodecID: 0x86, CodecPrivate: 0x63a2, CodecDelay: 0x56aa,
  Language: 0x22b59c, LanguageBCP47: 0x22b59d, Name: 0x536e,
  Video: 0xe0, PixelWidth: 0xb0, PixelHeight: 0xba, DisplayWidth: 0x54b0, DisplayHeight: 0x54ba,
  Colour: 0x55b0, MatrixCoefficients: 0x55b1, Range: 0x55b9,
  TransferCharacteristics: 0x55ba, Primaries: 0x55bb,
  MaxCLL: 0x55bc, MaxFALL: 0x55bd, MasteringMetadata: 0x55d0,
  Audio: 0xe1, SamplingFrequency: 0xb5, OutputSamplingFrequency: 0x78b5, Channels: 0x9f, BitDepth: 0x6264,
  Attachments: 0x1941a469, AttachedFile: 0x61a7,
  FileName: 0x466e, FileMimeType: 0x4660, FileData: 0x465c, FileUID: 0x46ae,
  Cues: 0x1c53bb6b, CuePoint: 0xbb, CueTime: 0xb3, CueTrackPositions: 0xb7,
  CueTrack: 0xf7, CueClusterPosition: 0xf1, CueRelativePosition: 0xf0,
  Cluster: 0x1f43b675, Timestamp: 0xe7, SimpleBlock: 0xa3,
  BlockGroup: 0xa0, Block: 0xa1, BlockDuration: 0x9b, ReferenceBlock: 0xfb,
  CRC32: 0xbf, Void: 0xec,
  ContentEncodings: 0x6d80, ContentEncoding: 0x6240, ContentEncodingScope: 0x5032,
  ContentEncodingType: 0x5033, ContentCompression: 0x5034,
  ContentCompAlgo: 0x4254, ContentCompSettings: 0x4255, ContentEncryption: 0x5035,
};

// ContentCompAlgo values. Only 0 and 3 occur in practice; bzlib and lzo1x were
// never adopted by any muxer still in use and would need a wasm decoder.
const COMP_ZLIB = 0, COMP_BZLIB = 1, COMP_LZO1X = 2, COMP_HEADER_STRIP = 3;
const COMP_NAMES = { 0: 'zlib', 1: 'bzlib', 2: 'lzo1x', 3: 'header-stripping' };

/**
 * Inflate a zlib stream using the platform, so no compression library is
 * bundled. `DecompressionStream` is in every browser this player targets and
 * in Node 18+, which keeps the demuxer identical on both sides.
 */
async function inflate(data) {
  const ds = new DecompressionStream('deflate');
  const out = new Response(new Blob([data]).stream().pipeThrough(ds));
  return new Uint8Array(await out.arrayBuffer());
}

export const TRACK_VIDEO = 1, TRACK_AUDIO = 2, TRACK_SUBTITLE = 17;

/** Random-access byte source over a local File. */
export class FileSource {
  constructor(file) { this.file = file; this.size = file.size; this.name = file.name; }
  async read(offset, length) {
    const end = Math.min(offset + length, this.size);
    if (offset >= this.size) return new Uint8Array(0);
    return new Uint8Array(await this.file.slice(offset, end).arrayBuffer());
  }
}

/**
 * Random-access byte source over HTTP, using Range requests.
 *
 * A signed/tokenised direct link expires; a long film or a seek after the TTL
 * would otherwise fail with 401/403/410 mid-playback. So the URL that is
 * actually fetched can be REFRESHED: pass `resolve`, an async function that
 * turns the original (page) URL into a fresh `{ url, expiresAt? }`. The direct
 * link is re-resolved before a request that would race the expiry, and again if
 * a request is rejected as expired -- then the SAME byte range is retried. The
 * content is identical across re-signs, so offsets and size stay stable. With
 * no `resolve`, this behaves as a plain Range fetcher (a direct URL).
 *
 * `resolve` is deliberately transport-agnostic: it can call a Cloudflare Worker,
 * a self-hosted Node endpoint, or anything else. The client does not care where
 * the extractor runs.
 */
const EXPIRED = new Set([401, 403, 410]);

// A failure the CALLER can route around rather than just report. Every one of
// these means "this engine cannot read the stream", never "the network is
// broken" -- the fetch itself succeeded, so CORS is already proven working and
// the browser's own decoder is still worth a try. src/player.js turns exactly
// these three into the native <video> fallback; anything else stays fatal.
const coded = (code, msg) => Object.assign(new Error(msg), { code });

export class HttpSource {
  constructor(url, { resolve = null, fetch: fetchImpl = null, size = 0, log = () => {} } = {}) {
    this.log = log;
    this.origin = url;        // the URL the user gave; handed to resolve()
    this.url = url;           // the URL actually fetched (a direct link)
    this.resolve = resolve;
    this.expiresAt = 0;       // ms epoch; 0 = unknown/never
    // A caller that already knows the byte length (e.g. Emby's PlaybackInfo
    // MediaSource.Size) may seed it. Reads never touch a response header, so a
    // known size lets open() skip its dependence on a CORS-exposed Content-Range
    // -- which a 302-to-CDN stream cannot be relied on to expose.
    this.size = size;
    this.name = url.split('/').pop();
    // Wrap, don't capture: `const f = globalThis.fetch; f(...)` throws "Illegal
    // invocation" because fetch must run with globalThis as its receiver.
    this._fetch = fetchImpl || ((input, init) => globalThis.fetch(input, init));
  }

  async _refresh() {
    if (!this.resolve) return;
    const r = await this.resolve(this.origin);
    if (!r?.url) throw new Error('resolve() returned no url');
    this.url = r.url;
    this.expiresAt = r.expiresAt || 0;
  }

  /** True if the link is known to expire within `ms` from now. */
  _stale(ms = 5000) { return this.expiresAt > 0 && Date.now() > this.expiresAt - ms; }

  // A tokenised Emby /Videos/stream URL 302-redirects to a CDN on another origin.
  // The Range probe below is preflighted, and the Fetch spec forbids a preflighted
  // request from following a cross-origin redirect ("Request requires preflight,
  // which is disallowed to follow cross-origin redirect") -- so it would throw a
  // CORS error on the raw Emby URL. Resolve the redirect HERE with a simple
  // (no-Range => un-preflighted) GET, which IS allowed to follow it, and adopt the
  // final CDN URL; the Range probe then hits the CDN directly. Doing this at
  // fetch-time (not once at hand-off, before a page navigation) means the link is
  // always fresh, so a short-TTL/one-shot CDN link can't go stale in transit.
  async _followRedirect() {
    let r;
    try { r = await this._fetch(this.url); }
    catch { return; }                          // network/CORS: let the Range probe surface it
    if (r.url && r.url !== this.url) { this.url = r.url; this.name = r.url.split('/').pop(); }
    // Second thing worth taking off this response: its Content-Length. A .strm-
    // backed item has no Size in Emby's PlaybackInfo (the server never probed the
    // remote file), so nothing seeds the size from above, and a proxied stream
    // need not CORS-expose Content-Range to the Range probe below. This plain GET
    // is the one response whose Content-Length IS the whole file -- keep it as the
    // fallback total before the body is dropped, and open() has no reason to fail.
    if (!(this.size > 0)) this.size = Number(r.headers.get('content-length')) || 0;
    r.body?.cancel().catch(() => {});          // the final URL was all we wanted; drop the body
  }

  async open() {
    if (this.resolve) await this._refresh();   // turn the page URL into a direct link first
    else await this._followRedirect();         // ...or just follow a plain 302 to its CDN target
    // Probe with a tiny Range GET, NOT a HEAD: many CDNs advertise
    // Accept-Ranges only on real GET responses (and some reject HEAD outright),
    // so a HEAD/Accept-Ranges check falsely rejects a server that DOES support
    // Range. A 206 both confirms partial support and carries the total size in
    // Content-Range ("bytes 0-1/TOTAL"), in one request. The total must be
    // CORS-exposed; that is the server's to do (Access-Control-Expose-Headers).
    const r = await this._fetch(this.url, { headers: { Range: 'bytes=0-1' } });
    if (r.status !== 206) throw coded('NO_RANGE', `server does not support Range requests (probe -> ${r.status})`);
    const total = Number(r.headers.get('content-range')?.split('/')[1]);
    await r.arrayBuffer().catch(() => {});    // drain the 2-byte probe body
    // Prefer the header when the server exposes it (authoritative for the URL
    // actually being fetched); otherwise fall back to a caller-seeded size. Only
    // fail when we have neither -- the 206 already proved Range works, so the
    // sole missing piece is the total, and reads never need the header.
    if (total > 0) this.size = total;
    else if (!(this.size > 0)) throw coded('NO_SIZE', 'server did not expose a Content-Range total size (check Access-Control-Expose-Headers)');
    return this;
  }

  async read(offset, length) {
    const end = Math.min(offset + length, this.size) - 1;
    if (offset > end) return new Uint8Array(0);
    const range = `bytes=${offset}-${end}`;
    // Refresh proactively if the link is about to expire, to avoid a wasted
    // request and a visible stall at the exact wrong moment (a seek).
    if (this._stale()) await this._refresh();
    let r = await this._fetch(this.url, { headers: { Range: range } });
    if (EXPIRED.has(r.status) && this.resolve) {
      await this._refresh();                    // link died early -- get a fresh one and retry the same bytes
      r = await this._fetch(this.url, { headers: { Range: range } });
    }
    if (!r.ok) throw new Error(`GET range -> ${r.status}`);
    const body = new Uint8Array(await r.arrayBuffer());

    // r.ok covers 200 as well as 206, and a 200 to a Range request means the
    // header was ignored: this is the WHOLE file, not the slice asked for.
    // Returning it as though it began at `offset` hands the demuxer the head of
    // the file while it believes it holds the tail -- timestamps jump back to
    // zero and every sample decodes as garbage. Firefox then rejects the frames
    // outright (ConvertSampleToAVCC); Chromium is quieter about it, which is why
    // the same stream can look fine in one browser and broken in the other.
    if (r.status === 200) {
      if (!this._warnedNoRange) {
        this._warnedNoRange = true;
        this.log(`${this.name}: 服务器忽略了 Range，每次读取都会拉回整个文件（已就地切片，但会很慢）`, 'warn');
      }
      return body.subarray(Math.min(offset, body.length), Math.min(end + 1, body.length));
    }

    // A 206 can still carry a different range than the one requested -- caches
    // and CDNs clamp. Content-Range is only readable when CORS exposes it, so
    // this is a check when it can be made, not a requirement.
    const served = /bytes\s+(\d+)-/i.exec(r.headers.get('Content-Range') || '');
    if (served && Number(served[1]) !== offset) {
      throw new Error(`Range mismatch: asked for byte ${offset}, server served ${served[1]}`);
    }
    return body;
  }
}

export class MatroskaDemuxer {
  constructor(source) {
    this.src = source;
    this.timestampScale = 1000000;   // ns per tick; 1e6 => tick == 1ms
    this.duration = 0;               // seconds
    this.tracks = [];
    this.attachments = [];
    this.cues = [];                  // { time (s), pos (abs byte offset of cluster) }
    this.segmentStart = 0;
    this.segmentEnd = 0;
  }

  async parseHeader() {
    // 256KB covers the EBML header + SeekHead + usually Info/Tracks too.
    let head = await this.src.read(0, 262144);
    const r = new Reader(head, 0);

    // The commonest way to land here is a .strm pointing at an mp4 on object
    // storage: Emby hands over a perfectly good stream that this remuxer simply
    // does not speak. Coded, so the caller can hand it to the browser instead.
    if (readId(r) !== ID.EBML) throw coded('NOT_MATROSKA', 'not a Matroska file (no EBML header)');
    r.skip(readSize(r));

    if (readId(r) !== ID.Segment) throw new Error('no Segment element');
    const segSize = readSize(r);
    this.segmentStart = r.abs;
    this.segmentEnd = segSize === null ? this.src.size : this.segmentStart + segSize;

    // Follow SeekHead instead of scanning — mandatory for the 21GB file, where
    // Cues and Attachments sit at the very end.
    const seek = {};
    const scanTop = async (buf, base) => {
      const rr = new Reader(buf, base);
      while (rr.left > 12) {
        const start = rr.pos;
        const id = readId(rr);
        if (id === null) break;
        const size = readSize(rr);
        if (size === undefined) break;
        if (size === null || rr.pos + size > buf.length) {
          // payload not in this window; record where it is and stop
          if (id === ID.Cluster) { this.firstCluster ??= base + start; }
          break;
        }
        const payload = buf.subarray(rr.pos, rr.pos + size);
        if (id === ID.SeekHead) this._parseSeekHead(payload, seek);
        else if (id === ID.Info) this._parseInfo(payload);
        else if (id === ID.Tracks) this._parseTracks(payload);
        else if (id === ID.Attachments) this._parseAttachments(payload);
        else if (id === ID.Cues) this._parseCues(payload);
        else if (id === ID.Cluster) { this.firstCluster ??= base + start; break; }
        rr.pos += size;
      }
    };
    await scanTop(head.subarray(this.segmentStart), this.segmentStart);

    // Fetch anything SeekHead pointed at that we haven't parsed yet.
    for (const [id, relPos] of Object.entries(seek)) {
      const want = Number(id);
      if (want === ID.Tracks && this.tracks.length) continue;
      if (want === ID.Attachments && this.attachments.length) continue;
      if (want === ID.Cues && this.cues.length) continue;
      if (![ID.Tracks, ID.Attachments, ID.Cues, ID.Info].includes(want)) continue;

      const abs = this.segmentStart + relPos;
      if (abs >= this.src.size) continue;
      const probe = await this.src.read(abs, 16);
      const pr = new Reader(probe, abs);
      if (readId(pr) !== want) continue;
      const size = readSize(pr);
      if (size === undefined || size === null) continue;
      const payload = await this.src.read(abs + pr.pos, size);
      if (want === ID.Info) this._parseInfo(payload);
      else if (want === ID.Tracks) this._parseTracks(payload);
      else if (want === ID.Attachments) this._parseAttachments(payload);
      else if (want === ID.Cues) this._parseCues(payload);
    }

    if (!this.tracks.length) throw new Error('no tracks found');

    // CodecPrivate can itself be compressed (scope 2). Do it here rather than in
    // the parser because inflating is async and _parseTracks is not.
    for (const t of this.tracks) {
      if (!t.codecPrivate?.length) continue;
      if (!(t.encodings ?? []).some(e => e.scope & 2)) continue;
      try { t.codecPrivate = await this._decodeContent(t, t.codecPrivate, 2); }
      catch (e) { t.decodeError = e.message; }
    }
    return this;
  }

  trackByNumber(n) {
    this._byNumber ??= new Map(this.tracks.map(t => [t.number, t]));
    return this._byNumber.get(n);
  }

  _parseSeekHead(buf, out) {
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      if (id !== ID.Seek) return false;
      let sid = null, spos = null;
      eachChild(new Reader(buf.subarray(r.pos, r.pos + size), 0), size, (i2, r2, s2) => {
        if (i2 === ID.SeekID) sid = readUint(r2.buf.subarray(r2.pos, r2.pos + s2));
        else if (i2 === ID.SeekPosition) spos = readUint(r2.buf.subarray(r2.pos, r2.pos + s2));
        return false;
      });
      if (sid !== null && spos !== null) out[sid] = spos;
      return false;
    });
  }

  _parseInfo(buf) {
    let durTicks = 0;
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      const b = buf.subarray(r.pos, r.pos + size);
      if (id === ID.TimestampScale) this.timestampScale = readUint(b);
      else if (id === ID.Duration) durTicks = readFloat(b);
      return false;
    });
    if (durTicks) this.duration = durTicks * this.timestampScale / 1e9;
  }

  _parseTracks(buf) {
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      if (id !== ID.TrackEntry) return false;
      this.tracks.push(this._parseTrackEntry(buf.subarray(r.pos, r.pos + size)));
      return false;
    });
    this.tracks.sort((a, b) => a.number - b.number);
  }

  _parseTrackEntry(buf) {
    const t = { number: 0, type: 0, codecId: '', codecPrivate: null, language: 'und', name: '',
                default: false, forced: false, defaultDuration: 0, video: null, audio: null,
                encodings: null };
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      const b = buf.subarray(r.pos, r.pos + size);
      switch (id) {
        case ID.TrackNumber: t.number = readUint(b); break;
        case ID.TrackType: t.type = readUint(b); break;
        case ID.CodecID: t.codecId = readString(b); break;
        case ID.CodecPrivate: t.codecPrivate = new Uint8Array(b); break;
        case ID.Language: t.language = readString(b); break;
        case ID.LanguageBCP47: t.language = readString(b); break;
        case ID.Name: t.name = readString(b); break;
        case ID.FlagDefault: t.default = readUint(b) !== 0; break;
        case ID.FlagForced: t.forced = readUint(b) !== 0; break;
        case ID.DefaultDuration: t.defaultDuration = readUint(b); break;
        case ID.Video: t.video = this._parseVideo(b); break;
        case ID.Audio: t.audio = this._parseAudio(b); break;
        case ID.ContentEncodings: t.encodings = this._parseEncodings(b); break;
      }
      return false;
    });
    return t;
  }

  /**
   * Compression applied to a track's frames before muxing. Ignoring this does
   * not fail loudly -- the blocks come out looking like valid data of the wrong
   * shape. The PGS tracks in the test files are zlib-compressed, and header
   * stripping is common on audio, so both track types depend on this.
   */
  _parseEncodings(buf) {
    const list = [];
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      if (id !== ID.ContentEncoding) return false;
      const b = buf.subarray(r.pos, r.pos + size);
      // Defaults per the Matroska spec: scope 1 (frames), type 0 (compression).
      const e = { scope: 1, type: 0, algo: COMP_ZLIB, settings: null, encrypted: false };
      eachChild(new Reader(b, 0), b.length, (i2, r2, s2) => {
        const b2 = b.subarray(r2.pos, r2.pos + s2);
        if (i2 === ID.ContentEncodingScope) e.scope = readUint(b2);
        else if (i2 === ID.ContentEncodingType) e.type = readUint(b2);
        else if (i2 === ID.ContentEncryption) e.encrypted = true;
        else if (i2 === ID.ContentCompression) {
          eachChild(new Reader(b2, 0), b2.length, (i3, r3, s3) => {
            const b3 = b2.subarray(r3.pos, r3.pos + s3);
            if (i3 === ID.ContentCompAlgo) e.algo = readUint(b3);
            else if (i3 === ID.ContentCompSettings) e.settings = new Uint8Array(b3);
            return false;
          });
        }
        return false;
      });
      list.push(e);
      return false;
    });
    // Order 0 is applied first when writing, so undo them last-to-first.
    return list.reverse();
  }

  /** Undo one track's content encodings over a frame (scope 1) or CodecPrivate (scope 2). */
  async _decodeContent(track, data, scope) {
    for (const e of track.encodings ?? []) {
      if (!(e.scope & scope)) continue;
      if (e.encrypted || e.type !== 0) {
        throw new Error(`track ${track.number} is encrypted or uses an unsupported encoding type`);
      }
      if (e.algo === COMP_HEADER_STRIP) {
        // The muxer removed a constant prefix from every frame; put it back.
        if (!e.settings?.length) continue;
        const out = new Uint8Array(e.settings.length + data.length);
        out.set(e.settings, 0);
        out.set(data, e.settings.length);
        data = out;
      } else if (e.algo === COMP_ZLIB) {
        data = await inflate(data);
      } else {
        throw new Error(`track ${track.number}: unsupported compression ${COMP_NAMES[e.algo] ?? e.algo}`);
      }
    }
    return data;
  }

  /** True if this track needs per-frame work in the read path. */
  _needsDecode(track) {
    return (track?.encodings ?? []).some(e => e.scope & 1);
  }

  _parseVideo(buf) {
    const v = { width: 0, height: 0, displayWidth: 0, displayHeight: 0, colour: null };
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      const b = buf.subarray(r.pos, r.pos + size);
      switch (id) {
        case ID.PixelWidth: v.width = readUint(b); break;
        case ID.PixelHeight: v.height = readUint(b); break;
        case ID.DisplayWidth: v.displayWidth = readUint(b); break;
        case ID.DisplayHeight: v.displayHeight = readUint(b); break;
        case ID.Colour: {
          const c = {};
          eachChild(new Reader(b, 0), b.length, (i2, r2, s2) => {
            const b2 = b.subarray(r2.pos, r2.pos + s2);
            if (i2 === ID.MatrixCoefficients) c.matrix = readUint(b2);
            else if (i2 === ID.TransferCharacteristics) c.transfer = readUint(b2);
            else if (i2 === ID.Primaries) c.primaries = readUint(b2);
            else if (i2 === ID.Range) c.range = readUint(b2);
            else if (i2 === ID.MaxCLL) c.maxCLL = readUint(b2);
            else if (i2 === ID.MaxFALL) c.maxFALL = readUint(b2);
            return false;
          });
          v.colour = c;
          break;
        }
      }
      return false;
    });
    return v;
  }

  _parseAudio(buf) {
    const a = { sampleRate: 8000, channels: 1, bitDepth: 0, outputSampleRate: 0 };
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      const b = buf.subarray(r.pos, r.pos + size);
      switch (id) {
        case ID.SamplingFrequency: a.sampleRate = readFloat(b); break;
        case ID.OutputSamplingFrequency: a.outputSampleRate = readFloat(b); break;
        case ID.Channels: a.channels = readUint(b); break;
        case ID.BitDepth: a.bitDepth = readUint(b); break;
      }
      return false;
    });
    return a;
  }

  _parseAttachments(buf) {
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      if (id !== ID.AttachedFile) return false;
      const b = buf.subarray(r.pos, r.pos + size);
      const f = { name: '', mime: '', data: null, uid: 0 };
      eachChild(new Reader(b, 0), b.length, (i2, r2, s2) => {
        const b2 = b.subarray(r2.pos, r2.pos + s2);
        if (i2 === ID.FileName) f.name = readString(b2);
        else if (i2 === ID.FileMimeType) f.mime = readString(b2);
        else if (i2 === ID.FileData) f.data = new Uint8Array(b2);
        else if (i2 === ID.FileUID) f.uid = readUint(b2);
        return false;
      });
      if (f.data) this.attachments.push(f);
      return false;
    });
  }

  _parseCues(buf) {
    eachChild(new Reader(buf, 0), buf.length, (id, r, size) => {
      if (id !== ID.CuePoint) return false;
      const b = buf.subarray(r.pos, r.pos + size);
      let time = 0; const positions = [];
      eachChild(new Reader(b, 0), b.length, (i2, r2, s2) => {
        const b2 = b.subarray(r2.pos, r2.pos + s2);
        if (i2 === ID.CueTime) time = readUint(b2);
        else if (i2 === ID.CueTrackPositions) {
          let track = 0, pos = 0;
          eachChild(new Reader(b2, 0), b2.length, (i3, r3, s3) => {
            const b3 = b2.subarray(r3.pos, r3.pos + s3);
            if (i3 === ID.CueTrack) track = readUint(b3);
            else if (i3 === ID.CueClusterPosition) pos = readUint(b3);
            return false;
          });
          positions.push({ track, pos });
        }
        return false;
      });
      for (const p of positions) {
        this.cues.push({ time: time * this.timestampScale / 1e9, track: p.track, pos: this.segmentStart + p.pos });
      }
      return false;
    });
    this.cues.sort((a, b) => a.time - b.time);
  }

  /** Byte offset of the cluster to start at for a given time (seconds). */
  /**
   * Byte offset to start reading from for a given time, index or no index.
   *
   * With Cues this is a lookup. Without them -- which is every file still being
   * written, and anything muxed for streaming -- the only way to find a
   * position is to look at the clusters themselves. Falling back to the start
   * of the file, which is what the Cues-only path does, means seeking silently
   * does nothing on those files.
   *
   * Discovered clusters are added to `this.cues`, so a file with no index
   * builds one as it is used and later seeks get cheaper.
   */
  async seekTo(seconds, trackNumber) {
    if (this.cues.length) return this.seekPosition(seconds, trackNumber);
    if (seconds <= 0) return this.firstCluster ?? this.segmentStart;

    let lo = this.firstCluster ?? this.segmentStart;
    let hi = Math.min(this.segmentEnd, this.src.size);
    let best = lo;

    // Bounded: each probe halves the range, so ~40 covers any file size, and
    // the cap stops a pathological file from turning a seek into a full scan.
    for (let probe = 0; probe < 40 && hi - lo > (1 << 16); probe++) {
      const mid = lo + Math.floor((hi - lo) / 2);
      const found = await this._clusterAtOrAfter(mid, Math.min(hi, mid + (8 << 20)));
      if (!found) { hi = mid; continue; }
      this._rememberCue(found);
      if (found.time <= seconds) { best = found.pos; lo = found.pos + 1; }
      else { hi = found.pos; }
    }

    // The binary search lands near the target but the last cluster whose time
    // is <= the target may be just behind `lo`; walk forward from `best` only
    // if nothing was ever found, so the caller never gets a position past the
    // requested time.
    if (best === (this.firstCluster ?? this.segmentStart)) {
      const first = await this._clusterAtOrAfter(best, best + (8 << 20));
      if (first) this._rememberCue(first);
    }
    return best;
  }

  _rememberCue(c) {
    if (this.cues.some(x => x.pos === c.pos)) return;
    this.cues.push({ time: c.time, pos: c.pos, track: 0, discovered: true });
    this.cues.sort((a, b) => a.pos - b.pos);
  }

  /**
   * First real Cluster at or after `from`, with its timestamp.
   *
   * The 4-byte Cluster ID occurs by chance inside compressed video often
   * enough that finding the bytes is not enough: a match is only accepted if
   * it is followed by a plausible size and a Timestamp element, which is what
   * a real cluster always begins with.
   */
  async _clusterAtOrAfter(from, until) {
    const CHUNK = 1 << 20;
    for (let pos = from; pos < until; pos += CHUNK - 16) {
      const buf = await this.src.read(pos, Math.min(CHUNK, until - pos + 16));
      if (buf.length < 16) return null;
      for (let i = 0; i + 16 <= buf.length; i++) {
        if (buf[i] !== 0x1f || buf[i + 1] !== 0x43 || buf[i + 2] !== 0xb6 || buf[i + 3] !== 0x75) continue;
        const r = new Reader(buf.subarray(i), pos + i);
        readId(r);
        const size = readSize(r);
        if (size === undefined) continue;
        // A cluster begins with its Timestamp -- but not necessarily as the
        // very first child. ffmpeg and mkvmerge both put an optional CRC-32
        // ahead of it, and Void padding can appear too. Requiring Timestamp
        // first rejects every real cluster those muxers write.
        const cr = new Reader(buf.subarray(i + r.pos), 0);
        let ticks = null;
        for (let child = 0; child < 4 && cr.left > 2; child++) {
          const cid = readId(cr);
          if (cid === null) break;
          const csize = readSize(cr);
          if (csize === undefined || csize === null) break;
          if (cid === ID.Timestamp) {
            if (csize < 1 || csize > 8 || csize > cr.left) break;
            ticks = readUint(cr.bytes(csize));
            break;
          }
          if (cid !== ID.CRC32 && cid !== ID.Void) break;   // anything else: not a cluster head
          if (csize > cr.left) break;
          cr.skip(csize);
        }
        if (ticks === null) continue;
        return { pos: pos + i, time: ticks * this.timestampScale / 1e9 };
      }
    }
    return null;
  }

  seekPosition(seconds, trackNumber) {
    const relevant = this.cues.filter(c => !trackNumber || c.track === trackNumber);
    const list = relevant.length ? relevant : this.cues;
    if (!list.length) return this.firstCluster ?? this.segmentStart;
    let best = list[0];
    for (const c of list) { if (c.time <= seconds) best = c; else break; }
    return best.pos;
  }

  /**
   * Read clusters starting at `offset`, yielding blocks until `maxBytes` of
   * cluster payload have been consumed. Yields { track, time, duration, keyframe, data }.
   *
   * `state.nextPos` is written with the byte offset where reading stopped.
   * Callers MUST resume from it rather than assuming offset+maxBytes: element
   * headers and the final cluster's overshoot mean the two are never equal, and
   * resuming at the wrong offset lands mid-cluster and silently reads garbage.
   */
  async *readBlocks(offset, maxBytes = 4 << 20, state = {}) {
    let pos = offset;
    const end = Math.min(this.segmentEnd, this.src.size);
    let consumed = 0;
    state.nextPos = pos;
    state.atEnd = false;

    while (pos < end && consumed < maxBytes) {
      const headBuf = await this.src.read(pos, 16);
      // A failure to parse here is NOT end-of-stream -- it means we resumed at
      // a bad offset. Conflating the two makes the player call endOfStream()
      // and truncate playback, so they are reported as separate flags.
      if (headBuf.length < 4) { state.parseError = 'short read'; break; }
      const hr = new Reader(headBuf, pos);
      const id = readId(hr);
      if (id === null) { state.parseError = `bad element id at ${pos}`; break; }
      const size = readSize(hr);
      if (size === undefined) { state.parseError = `bad element size at ${pos}`; break; }
      const headerLen = hr.pos;

      if (id !== ID.Cluster) {
        // Cues/Chapters/Tags can be interleaved between clusters; skip them.
        if (size === null) { state.parseError = `unknown-size non-cluster at ${pos}`; break; }
        pos += headerLen + size;
        state.nextPos = pos;
        continue;
      }

      // Unknown-size cluster: read a bounded window and stop at the next Cluster ID.
      const clusterSize = size === null ? Math.min(8 << 20, end - pos - headerLen) : size;
      const body = await this.src.read(pos + headerLen, clusterSize);
      consumed += body.length;
      // Decoding happens here, not in _clusterBlocks: inflating is async and
      // that is a plain generator. Tracks with no encodings pay nothing.
      for (const blk of this._clusterBlocks(body)) {
        const track = this.trackByNumber(blk.track);
        if (!this._needsDecode(track)) { yield blk; continue; }
        try { blk.data = await this._decodeContent(track, blk.data, 1); }
        catch (e) {
          if (!track.decodeError) { track.decodeError = e.message; state.decodeError = e.message; }
          continue;   // drop the block rather than hand a decoder garbage
        }
        yield blk;
      }
      pos += headerLen + clusterSize;
      state.nextPos = pos;
    }
    state.nextPos = pos;
    if (pos >= end) state.atEnd = true;
  }

  *_clusterBlocks(buf) {
    let clusterTime = 0;
    const r = new Reader(buf, 0);
    while (r.pos < buf.length) {
      const start = r.pos;
      const id = readId(r);
      if (id === null) break;
      const size = readSize(r);
      if (size === undefined || size === null) break;
      if (r.pos + size > buf.length) { r.pos = start; break; }
      const body = buf.subarray(r.pos, r.pos + size);
      r.pos += size;

      if (id === ID.Timestamp) { clusterTime = readUint(body); continue; }
      if (id === ID.SimpleBlock) { yield* this._block(body, clusterTime, null, true); continue; }
      if (id === ID.BlockGroup) {
        let blockBody = null, duration = null, referenced = false;
        eachChild(new Reader(body, 0), body.length, (i2, r2, s2) => {
          const b2 = body.subarray(r2.pos, r2.pos + s2);
          if (i2 === ID.Block) blockBody = b2;
          else if (i2 === ID.BlockDuration) duration = readUint(b2);
          else if (i2 === ID.ReferenceBlock) referenced = true;
          return false;
        });
        // A BlockGroup with no ReferenceBlock is not predicted from anything -> keyframe.
        if (blockBody) yield* this._block(blockBody, clusterTime, duration, !referenced);
      }
    }
  }

  *_block(buf, clusterTime, groupDuration, keyframeHint) {
    const r = new Reader(buf, 0);
    const track = readSize(r);                     // track number is a plain VINT
    if (track === undefined || track === null) return;
    const rel = (buf[r.pos] << 8 | buf[r.pos + 1]) << 16 >> 16;   // signed int16
    r.pos += 2;
    const flags = r.u8();
    const keyframe = keyframeHint === true ? (flags & 0x80) !== 0 || groupDuration !== null : keyframeHint;

    const tickNs = this.timestampScale;
    const time = (clusterTime + rel) * tickNs / 1e9;
    const duration = groupDuration != null ? groupDuration * tickNs / 1e9 : 0;

    const lacing = (flags >> 1) & 0x03;
    const frames = [];
    if (lacing === 0) {
      frames.push(buf.subarray(r.pos));
    } else {
      const count = r.u8() + 1;
      const sizes = [];
      if (lacing === 2) {                          // fixed-size lacing
        const total = buf.length - r.pos;
        const each = Math.floor(total / count);
        for (let i = 0; i < count; i++) sizes.push(each);
      } else if (lacing === 1) {                   // Xiph lacing
        let sum = 0;
        for (let i = 0; i < count - 1; i++) {
          let v = 0, b;
          do { b = r.u8(); v += b; } while (b === 255);
          sizes.push(v); sum += v;
        }
        sizes.push(buf.length - r.pos - sum);
      } else {                                     // EBML lacing
        let prev = readSize(r);
        sizes.push(prev);
        let sum = prev;
        for (let i = 1; i < count - 1; i++) {
          const sr = readSize(r);
          // EBML lacing stores signed deltas, biased by half the VINT range
          const bits = 7 * Math.ceil(Math.log2(sr + 2) / 7) || 7;
          const delta = sr - (2 ** (bits - 1) - 1);
          prev += delta; sizes.push(prev); sum += prev;
        }
        sizes.push(buf.length - r.pos - sum);
      }
      for (const s of sizes) { frames.push(buf.subarray(r.pos, r.pos + s)); r.pos += s; }
    }

    // Laced frames share ONE timestamp in the container; they must be spread
    // out or they pile up at a single PTS with zero duration. When that happens
    // the MP4 remuxer can only recover a duration from the gap to the NEXT
    // block, so each ~1024-sample AAC frame is handed the whole block's worth of
    // time -- the audio timeline then races ~(sampleRate/1000)x ahead of video,
    // the video buffer starves, and playback freezes after tens of seconds.
    // A BlockGroup carries a Duration to divide; a SimpleBlock (the common case)
    // does not, so fall back to the track's DefaultDuration, which is exactly
    // one frame's worth of time.
    let step = 0;
    if (frames.length > 1) {
      if (duration) step = duration / frames.length;
      else {
        const td = this.tracks.find(t => t.number === track);
        if (td?.defaultDuration) step = td.defaultDuration / 1e9;
      }
    }
    for (let i = 0; i < frames.length; i++) {
      if (!frames[i].length) continue;
      yield { track, time: time + step * i, duration: step || duration, keyframe, data: frames[i] };
    }
  }
}
