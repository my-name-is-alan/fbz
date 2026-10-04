// ISOBMFF demuxer -- mp4 / mov / m4v.
//
// This exists so an mp4 gets what an mkv already got: a track list you can
// choose from, embedded subtitles, and a colour verdict read off the real
// stream. Handing the file to <video> plays it, but the browser picks the
// tracks and nothing here ever sees the bytes.
//
// It presents EXACTLY the interface MatroskaDemuxer does -- `.tracks` carrying
// MATROSKA codec IDs, `readBlocks(pos, max, state)`, `seekTo(sec, track)` --
// so the fMP4 remuxer, the HDR/DV scan, the subtitle pipeline and both pages'
// pickers work on an mp4 with no changes at all. The translation lives here, at
// the boundary, and nowhere else. That is also why the codec IDs are Matroska's
// and not the fourccs sitting right there in stsd: one vocabulary downstream.
//
// Progressive files only. A fragmented mp4 (moov/mvex + moof) is a different
// index entirely; it is DETECTED and refused with a code that routes to the
// native <video> leg, rather than parsed halfway and played wrong.

const fourcc = (b, o) => String.fromCharCode(b[o], b[o + 1], b[o + 2], b[o + 3]);
const u16 = (b, o) => (b[o] << 8) | b[o + 1];
const u32 = (b, o) => b[o] * 0x1000000 + ((b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]);
const i32 = (b, o) => (b[o] << 24) | (b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3];
const u64 = (b, o) => u32(b, o) * 4294967296 + u32(b, o + 4);

const coded = (code, msg) => Object.assign(new Error(msg), { code });

/** Iterate the boxes laid out in `buf`. Stops rather than throwing on a bad size. */
function* boxes(buf, start = 0, end = buf.length) {
  let p = start;
  while (p + 8 <= end) {
    let size = u32(buf, p);
    const type = fourcc(buf, p + 4);
    let hdr = 8;
    if (size === 1) { if (p + 16 > end) return; size = u64(buf, p + 8); hdr = 16; }
    else if (size === 0) size = end - p;
    if (size < hdr || p + size > end) return;
    yield { type, hdr, size, body: buf.subarray(p + hdr, p + size) };
    p += size;
  }
}
const child = (buf, type) => { for (const b of boxes(buf)) if (b.type === type) return b.body; return null; };
/** Descend a path of box types, e.g. path(trak, 'mdia', 'minf', 'stbl'). */
const path = (buf, ...types) => types.reduce((b, t) => (b ? child(b, t) : null), buf);

// ISO 639-2/T packed into 15 bits, 5 bits per letter, biased by 0x60.
function unpackLanguage(v) {
  const s = [(v >> 10) & 31, (v >> 5) & 31, v & 31].map(c => String.fromCharCode(c + 0x60)).join('');
  return /^[a-z]{3}$/.test(s) ? s : 'und';
}

/**
 * Pull the AudioSpecificConfig (and the object type) out of an esds box.
 * That config IS what Matroska stores as an AAC track's CodecPrivate, so the
 * remuxer needs no special case for where it came from.
 */
function esdsInfo(body) {
  let p = 4, oti = 0, dsi = null;                 // skip the FullBox version/flags
  const len = () => { let v = 0, b; do { b = body[p++]; v = (v << 7) | (b & 0x7f); } while ((b & 0x80) && p < body.length); return v; };
  const walk = (end) => {
    while (p + 2 <= end) {
      const tag = body[p++];
      // Two statements on purpose: `Math.min(p + len(), end)` reads p BEFORE
      // len() runs, and len() advances it -- which silently truncated every
      // AudioSpecificConfig to one byte. The remuxer still built a plausible
      // "mp4a.40.2" out of it and MSE would have rejected the init segment
      // without an error anywhere.
      const n = len();
      const stop = Math.min(p + n, end);
      if (tag === 0x03) {                          // ES_Descriptor
        p += 2;
        const f = body[p++];
        if (f & 0x80) p += 2;                      // dependsOn_ES_ID
        if (f & 0x40) p += 1 + body[p];            // URL
        if (f & 0x20) p += 2;                      // OCR_ES_Id
        walk(stop);
      } else if (tag === 0x04) {                   // DecoderConfigDescriptor
        oti = body[p];
        p += 13;
        walk(stop);
      } else if (tag === 0x05) {                   // DecoderSpecificInfo
        dsi = body.subarray(p, stop);
      }
      p = stop;
    }
  };
  walk(body.length);
  return { oti, dsi };
}

// stsd fourcc -> the Matroska codec ID the rest of the player speaks. Anything
// absent here still appears in the track list (so the UI can say what it is)
// but has no remux path, exactly as an unknown Matroska codec would.
const VIDEO_CODECS = {
  avc1: 'V_MPEG4/ISO/AVC', avc3: 'V_MPEG4/ISO/AVC',
  hvc1: 'V_MPEGH/ISO/HEVC', hev1: 'V_MPEGH/ISO/HEVC',
  dvh1: 'V_MPEGH/ISO/HEVC', dvhe: 'V_MPEGH/ISO/HEVC',   // Dolby Vision, HEVC base layer
  av01: 'V_AV1', vp09: 'V_VP9', vp08: 'V_VP8',
};
const AUDIO_CODECS = {
  'mp4a': 'A_AAC', 'ac-3': 'A_AC3', 'ec-3': 'A_EAC3', 'Opus': 'A_OPUS', 'fLaC': 'A_FLAC',
  'dtsc': 'A_DTS', 'dtse': 'A_DTS', 'dtsh': 'A_DTS_HD', 'dtsl': 'A_DTS_HD',
  'alac': 'A_ALAC', 'mp3 ': 'A_MPEG/L3', '.mp3': 'A_MPEG/L3',
  'sowt': 'A_PCM/INT/LIT', 'twos': 'A_PCM/INT/BIG', 'lpcm': 'A_PCM/INT/LIT',
};
// tx3g/wvtt sample payloads are unwrapped into plain UTF-8 in readBlocks, so
// they arrive at the subtitle pipeline in the same shape a Matroska SRT track
// does. stpp is TTML: named honestly, no renderer claimed.
const SUB_CODECS = { tx3g: 'S_TEXT/UTF8', text: 'S_TEXT/UTF8', wvtt: 'S_TEXT/UTF8', stpp: 'S_TEXT/TTML', c608: 'S_TEXT/CEA608' };

export const TRACK_VIDEO = 1, TRACK_AUDIO = 2, TRACK_SUBTITLE = 17;

export class Mp4Demuxer {
  constructor(source) {
    this.src = source;
    this.duration = 0;
    this.tracks = [];
    this.attachments = [];      // ISOBMFF has no font attachment concept
    this.timescale = 1000;
  }

  async parseHeader() {
    const moov = await this._findMoov();
    if (child(moov, 'mvex')) {
      // Fragmented: the sample tables are in each moof, not here. Refusing with
      // a routable code is the honest move -- the browser demuxes these fine.
      throw coded('FRAGMENTED_MP4', 'fragmented mp4 (moov/mvex): no sample table to index');
    }

    const mvhd = child(moov, 'mvhd');
    if (mvhd) {
      const v1 = mvhd[0] === 1;
      this.timescale = u32(mvhd, v1 ? 20 : 12) || 1000;
      const dur = v1 ? u64(mvhd, 24) : u32(mvhd, 16);
      this.duration = dur / this.timescale;
    }

    for (const b of boxes(moov)) {
      if (b.type !== 'trak') continue;
      const t = this._parseTrak(b.body);
      if (t) this.tracks.push(t);
    }
    if (!this.tracks.length) throw coded('NOT_MP4', 'mp4 has no readable tracks');
    // Duration can be absent from mvhd on a file still being written; the
    // longest track is a better answer than zero (the UI shows it, and the
    // remuxer writes it into every init segment).
    if (!(this.duration > 0)) this.duration = Math.max(0, ...this.tracks.map(t => t._duration || 0));
    return this;
  }

  /**
   * moov sits at the END of anything not written with -movflags faststart, so
   * walking top-level boxes by their own sizes is the only way to reach it --
   * and it costs one 16-byte read per box rather than scanning the file.
   */
  async _findMoov() {
    let pos = 0;
    const size = this.src.size;
    let first = true;
    while (pos + 8 <= size) {
      const head = await this.src.read(pos, 16);
      if (head.length < 8) break;
      let bsize = u32(head, 0);
      const type = fourcc(head, 4);
      let hdr = 8;
      if (bsize === 1) { bsize = u64(head, 8); hdr = 16; }
      else if (bsize === 0) bsize = size - pos;
      if (first && !/^(ftyp|moov|free|skip|wide|mdat|pnot|junk)$/.test(type)) {
        throw coded('NOT_MP4', 'not an ISO base media file (no ftyp/moov)');
      }
      first = false;
      if (type === 'moov') return await this.src.read(pos + hdr, bsize - hdr);
      if (bsize < hdr) throw coded('NOT_MP4', `bad box size ${bsize} at ${pos}`);
      pos += bsize;
    }
    throw coded('NOT_MP4', 'no moov box');
  }

  _parseTrak(trak) {
    const tkhd = child(trak, 'tkhd');
    const mdia = child(trak, 'mdia');
    const mdhd = mdia && child(mdia, 'mdhd');
    const hdlr = mdia && child(mdia, 'hdlr');
    const stbl = path(mdia, 'minf', 'stbl');
    if (!tkhd || !mdhd || !hdlr || !stbl) return null;

    const v1 = mdhd[0] === 1;
    const timescale = u32(mdhd, v1 ? 20 : 12) || 1000;
    const mediaDur = v1 ? u64(mdhd, 24) : u32(mdhd, 16);
    const language = unpackLanguage(u16(mdhd, v1 ? 32 : 20));
    const handler = fourcc(hdlr, 8);
    const trackId = tkhd[0] === 1 ? u32(tkhd, 20) : u32(tkhd, 12);

    const stsd = child(stbl, 'stsd');
    const entry = stsd ? [...boxes(stsd, 8, stsd.length)][0] : null;   // 4 version/flags + 4 count
    if (!entry) return null;

    const t = {
      number: trackId, type: 0, codecId: '', codecPrivate: null,
      language, name: this._trackName(trak), default: (tkhd[3] & 1) !== 0, forced: false,
      defaultDuration: 0, video: null, audio: null, encodings: null,
      _timescale: timescale, _duration: mediaDur / timescale, _fourcc: entry.type,
      _shift: this._editShift(trak, timescale),
    };

    if (handler === 'vide') { t.type = TRACK_VIDEO; this._visual(t, entry); }
    else if (handler === 'soun') { t.type = TRACK_AUDIO; this._audio(t, entry); }
    else if (handler === 'text' || handler === 'sbtl' || handler === 'subt' || handler === 'clcp') {
      t.type = TRACK_SUBTITLE;
      t.codecId = SUB_CODECS[entry.type] ?? `S_MP4/${entry.type}`;
    } else return null;   // hint/meta/tmcd tracks are not playable material

    t.samples = buildSamples(stbl, timescale, t._shift);
    if (!t.samples.count) return null;
    // One frame's worth of time, in ns -- what Matroska calls DefaultDuration.
    // The remuxer uses it as the duration fallback for the very last sample.
    const firstDelta = t.samples.dts.length > 1 ? t.samples.dts[1] - t.samples.dts[0] : 0;
    if (firstDelta > 0) t.defaultDuration = Math.round(firstDelta * 1e9);
    return t;
  }

  /** udta/name or the ISO meta/ilst title, whichever the muxer wrote. */
  _trackName(trak) {
    const udta = child(trak, 'udta');
    const name = udta && child(udta, 'name');
    if (name) return new TextDecoder().decode(name).replace(/\0+$/, '');
    return '';
  }

  /**
   * The edit list, reduced to a single time shift.
   *
   * Two shapes cover essentially every real file: an empty edit (media_time
   * -1) that DELAYS the track, and a normal edit whose media_time trims the
   * front. Ignoring both is how an mp4 ends up with audio a frame or two out --
   * silent, and blamed on the remuxer.
   */
  _editShift(trak, timescale) {
    const elst = path(trak, 'edts', 'elst');
    if (!elst) return 0;
    const v1 = elst[0] === 1;
    const n = u32(elst, 4);
    let shift = 0, p = 8;
    for (let i = 0; i < n && i < 4; i++) {
      const segDur = v1 ? u64(elst, p) : u32(elst, p);
      const mediaTime = v1 ? (u32(elst, p + 8) & 0x80000000 ? -1 : u64(elst, p + 8)) : i32(elst, p + 4);
      p += v1 ? 20 : 12;
      if (mediaTime < 0) shift += segDur / this.timescale;    // empty edit: movie timescale
      else if (i === 0) shift -= mediaTime / timescale;       // trim the front
    }
    return shift;
  }

  _visual(t, entry) {
    const b = entry.body;
    t.codecId = VIDEO_CODECS[entry.type] ?? `V_MP4/${entry.type}`;
    t.video = { width: u16(b, 24), height: u16(b, 26), displayWidth: 0, displayHeight: 0, colour: null };
    const inner = b.subarray(78);
    // The configuration record is byte-identical to Matroska's CodecPrivate for
    // every one of these, which is the whole reason the remuxer needs no branch.
    t.codecPrivate = child(inner, 'avcC') ?? child(inner, 'hvcC') ?? child(inner, 'av1C') ?? child(inner, 'vpcC') ?? null;
    if (t.codecPrivate) t.codecPrivate = new Uint8Array(t.codecPrivate);
    // vpcC is a FullBox; the demuxer side of VP9 wants the raw config fields,
    // which src/demux/vp9.js reads off a keyframe anyway -- so leave it unset
    // for VP9 and let the existing keyframe probe do its job.
    if (t.codecId === 'V_VP9') t.codecPrivate = null;

    const colr = child(inner, 'colr');
    if (colr && (fourcc(colr, 0) === 'nclx' || fourcc(colr, 0) === 'nclc')) {
      t.video.colour = { primaries: u16(colr, 4), transfer: u16(colr, 6), matrix: u16(colr, 8),
                         range: (colr[10] & 0x80) ? 2 : 1 };
    }
    const mdcv = child(inner, 'mdcv'), clli = child(inner, 'clli');
    if (clli && t.video.colour) { t.video.colour.maxCLL = u16(clli, 0); t.video.colour.maxFALL = u16(clli, 2); }
    if (mdcv) t.masteringBox = new Uint8Array(mdcv);
  }

  _audio(t, entry) {
    const b = entry.body;
    t.codecId = AUDIO_CODECS[entry.type] ?? `A_MP4/${entry.type}`;
    const version = u16(b, 8);
    const channels = u16(b, 16) || 2;
    const bitDepth = u16(b, 18) || 16;
    const rate = u32(b, 24) >>> 16;
    t.audio = { sampleRate: rate || 48000, channels, bitDepth, outputSampleRate: 0 };
    // QuickTime v1/v2 sound descriptions push the child boxes further out.
    const inner = b.subarray(version === 2 ? 72 : version === 1 ? 44 : 28);
    // ...and a .mov often buries esds one level deeper, inside 'wave'.
    const wave = child(inner, 'wave');
    const esds = child(inner, 'esds') ?? (wave ? child(wave, 'esds') : null);
    if (esds) {
      const { dsi } = esdsInfo(esds);
      if (dsi?.length) {
        t.codecPrivate = new Uint8Array(dsi);
        // AudioSpecificConfig is authoritative for the real rate: HE-AAC states
        // half the output rate in the sample entry, and trusting that gives a
        // track that plays an octave low.
        const cfg = parseAudioSpecificConfig(dsi);
        if (cfg?.sampleRate) t.audio.sampleRate = cfg.sampleRate;
        if (cfg?.channels) t.audio.channels = cfg.channels;
      }
    }
    const dops = child(inner, 'dOps');
    if (dops) {
      // The remuxer converts OpusHead -> dOps; here the file already holds a
      // dOps, so wrap it back into the OpusHead shape that path expects.
      t.codecPrivate = opusHeadFromDops(dops);
      t.audio.sampleRate = 48000;
    }
    const dfla = child(inner, 'dfLa');
    if (dfla) t.codecPrivate = new Uint8Array(dfla.subarray(4));   // strip the FullBox header
  }

  trackByNumber(n) {
    this._byNumber ??= new Map(this.tracks.map(t => [t.number, t]));
    return this._byNumber.get(n);
  }

  /**
   * Byte offset to resume reading from for a given time.
   *
   * Matroska answers this with a cluster, which contains every track. An mp4
   * interleaves per sample, so the equivalent is the LOWEST offset among the
   * points each track has to restart from -- start at the video keyframe's own
   * offset and the audio that belongs with it is already behind the playhead.
   */
  async seekTo(seconds, trackNumber) {
    const primary = this.trackByNumber(trackNumber) ?? this.tracks.find(t => t.type === TRACK_VIDEO) ?? this.tracks[0];
    const target = primary ? keyframeAtOrBefore(primary.samples, seconds) : 0;
    let lowest = Infinity;
    for (const t of this.tracks) {
      const s = t.samples;
      const i = firstAtOrAfter(s, t.type === TRACK_VIDEO ? target : target - 0.001);
      if (i < s.count) lowest = Math.min(lowest, s.offset[i]);
    }
    return Number.isFinite(lowest) ? lowest : 0;
  }

  /**
   * Yield every track's samples that start inside [offset, offset+maxBytes),
   * in file order, as { track, time, duration, keyframe, data } -- the exact
   * shape MatroskaDemuxer._block produces.
   *
   * `state.nextPos` is where to resume. It is NOT offset+maxBytes: it is the
   * end of the last sample actually consumed, and skipping a gap (moov at the
   * tail, free space between chunks) moves it forward to the next real sample.
   * A caller that assumes otherwise re-reads or silently drops media.
   */
  async *readBlocks(offset, maxBytes = 4 << 20, state = {}) {
    state.nextPos = offset;
    state.atEnd = false;

    const hi = offset + maxBytes;
    const picked = [];
    let nextGap = Infinity;
    for (const t of this.tracks) {
      const s = t.samples;
      let i = lowerBoundOffset(s, offset);
      if (i < s.count && s.offset[i] >= hi) nextGap = Math.min(nextGap, s.offset[i]);
      for (; i < s.count && s.offset[i] < hi; i++) picked.push({ t, i });
    }

    if (!picked.length) {
      // Nothing here. Either a gap before the next sample, or genuinely done --
      // and those two must not be confused: reporting a gap as EOF truncates
      // playback, reporting EOF as a gap spins the fill loop forever.
      if (Number.isFinite(nextGap)) { state.nextPos = nextGap; return; }
      state.nextPos = offset;
      state.atEnd = true;
      return;
    }

    picked.sort((a, b) => a.t.samples.offset[a.i] - b.t.samples.offset[b.i]);
    const first = picked[0].t.samples.offset[picked[0].i];
    let last = 0;
    for (const p of picked) last = Math.max(last, p.t.samples.offset[p.i] + p.t.samples.size[p.i]);

    // One read for the whole span. Samples are interleaved, so a per-sample
    // read would turn a 2MB pass into thousands of Range requests.
    const buf = await this.src.read(first, last - first);
    if (!buf.length) { state.parseError = `short read at ${first}`; return; }

    for (const { t, i } of picked) {
      const s = t.samples;
      const start = s.offset[i] - first;
      const data = buf.subarray(start, start + s.size[i]);
      if (data.length < s.size[i]) { state.parseError = `sample ${i} truncated at ${s.offset[i]}`; break; }
      const time = s.dts[i] + s.cts[i];
      const block = { track: t.number, time, duration: s.dur[i], keyframe: !!s.key[i], data };
      if (t.type === TRACK_SUBTITLE) {
        const text = unwrapTextSample(t._fourcc, data);
        if (!text) continue;                       // an empty tx3g sample is a subtitle CLEAR, not a cue
        block.data = text;
      }
      yield block;
    }

    state.nextPos = last;
    if (last >= this.src.size) state.atEnd = true;
  }
}

// --- sample table ----------------------------------------------------------

/**
 * Flatten stts/stsc/stsz/stco/ctts/stss into parallel typed arrays.
 *
 * Typed arrays rather than an array of objects on purpose: a two-hour film is
 * ~500k samples across its tracks, and one object each is tens of megabytes of
 * garbage inside a worker that is also holding read buffers.
 */
function buildSamples(stbl, timescale, shift = 0) {
  const stsz = child(stbl, 'stsz'), stz2 = child(stbl, 'stz2');
  const stco = child(stbl, 'stco'), co64 = child(stbl, 'co64');
  const stsc = child(stbl, 'stsc'), stts = child(stbl, 'stts');
  const ctts = child(stbl, 'ctts'), stss = child(stbl, 'stss');

  // sizes
  let count = 0, sizes;
  if (stsz) {
    const uniform = u32(stsz, 4);
    count = u32(stsz, 8);
    sizes = new Uint32Array(count);
    if (uniform) sizes.fill(uniform);
    else for (let i = 0; i < count; i++) sizes[i] = u32(stsz, 12 + i * 4);
  } else if (stz2) {
    const field = stz2[7];
    count = u32(stz2, 8);
    sizes = new Uint32Array(count);
    for (let i = 0; i < count; i++) {
      if (field === 16) sizes[i] = u16(stz2, 12 + i * 2);
      else if (field === 8) sizes[i] = stz2[12 + i];
      else sizes[i] = (i & 1) ? (stz2[12 + (i >> 1)] & 15) : (stz2[12 + (i >> 1)] >> 4);
    }
  } else return { count: 0 };

  const empty = { count: 0 };
  if (!count || !stsc || !(stco || co64)) return empty;

  // chunk offsets
  const chunkTable = co64 ?? stco;
  const chunkCount = u32(chunkTable, 4);
  const chunkOffset = (c) => co64 ? u64(co64, 8 + c * 8) : u32(stco, 8 + c * 4);

  // stsc runs: first_chunk (1-based), samples_per_chunk
  const runs = u32(stsc, 4);
  const offset = new Float64Array(count);
  let si = 0;
  for (let r = 0; r < runs && si < count; r++) {
    const firstChunk = u32(stsc, 8 + r * 12) - 1;
    const perChunk = u32(stsc, 8 + r * 12 + 4);
    const nextFirst = r + 1 < runs ? u32(stsc, 8 + (r + 1) * 12) - 1 : chunkCount;
    for (let c = firstChunk; c < nextFirst && si < count; c++) {
      let at = chunkOffset(c);
      for (let k = 0; k < perChunk && si < count; k++, si++) { offset[si] = at; at += sizes[si]; }
    }
  }
  if (si < count) count = si;      // a truncated table describes fewer samples than stsz claims

  // decode timestamps, in seconds
  const dts = new Float64Array(count);
  const dur = new Float64Array(count);
  {
    const n = u32(stts, 4);
    let i = 0, t = 0;
    for (let e = 0; e < n && i < count; e++) {
      const runCount = u32(stts, 8 + e * 8);
      const delta = u32(stts, 8 + e * 8 + 4);
      for (let k = 0; k < runCount && i < count; k++, i++) { dts[i] = t / timescale + shift; dur[i] = delta / timescale; t += delta; }
    }
    for (; i < count; i++) { dts[i] = t / timescale + shift; dur[i] = 0; }
  }

  // composition offsets (B-frames). Version 1 stores them signed.
  const cts = new Float64Array(count);
  if (ctts) {
    const signed = ctts[0] === 1;
    const n = u32(ctts, 4);
    let i = 0;
    for (let e = 0; e < n && i < count; e++) {
      const runCount = u32(ctts, 8 + e * 8);
      const off = signed ? i32(ctts, 8 + e * 8 + 4) : u32(ctts, 8 + e * 8 + 4);
      for (let k = 0; k < runCount && i < count; k++, i++) cts[i] = off / timescale;
    }
  }

  // sync samples. No stss at all means every sample is a sync sample -- true of
  // all audio, and of intra-only video.
  const key = new Uint8Array(count);
  if (stss) {
    const n = u32(stss, 4);
    for (let e = 0; e < n; e++) { const s = u32(stss, 8 + e * 4) - 1; if (s >= 0 && s < count) key[s] = 1; }
  } else key.fill(1);

  return { count, offset, size: sizes, dts, cts, dur, key };
}

/** First sample index whose byte offset is >= `at`. Samples are offset-ordered within a track. */
function lowerBoundOffset(s, at) {
  let lo = 0, hi = s.count;
  while (lo < hi) { const m = (lo + hi) >> 1; if (s.offset[m] < at) lo = m + 1; else hi = m; }
  return lo;
}
/** First sample index whose presentation time is >= `t`. */
function firstAtOrAfter(s, t) {
  for (let i = 0; i < s.count; i++) if (s.dts[i] + s.cts[i] >= t) return i;
  return s.count;
}
/** Presentation time of the last sync sample at or before `t` (0 if none). */
function keyframeAtOrBefore(s, t) {
  let best = 0;
  for (let i = 0; i < s.count; i++) {
    if (!s.key[i]) continue;
    const pt = s.dts[i] + s.cts[i];
    if (pt <= t) best = pt; else break;
  }
  return best;
}

// --- payload shapes --------------------------------------------------------

/**
 * Turn a timed-text sample into the plain UTF-8 a Matroska SRT track carries.
 * tx3g: a 16-bit length then the text (style boxes follow and are dropped).
 * wvtt: WebVTT cue boxes; the payload lives in 'payl'.
 */
function unwrapTextSample(type, data) {
  if (type === 'wvtt') {
    for (const b of boxes(data)) {
      if (b.type !== 'vttc') continue;
      const payl = child(b.body, 'payl');
      if (payl?.length) return new Uint8Array(payl);
    }
    return null;
  }
  if (data.length < 2) return null;
  const n = u16(data, 0);
  return n > 0 ? new Uint8Array(data.subarray(2, 2 + n)) : null;
}

/** Rebuild Matroska's OpusHead from an mp4 dOps, so one Opus path serves both. */
function opusHeadFromDops(dops) {
  const channels = dops[1];
  const mappingLen = dops[10] === 0 ? 0 : 2 + channels;
  const out = new Uint8Array(19 + mappingLen);
  out.set([0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64], 0);   // "OpusHead"
  const le = new DataView(out.buffer);
  out[8] = 1;                                    // OpusHead version
  out[9] = channels;
  le.setUint16(10, (dops[2] << 8) | dops[3], true);              // pre-skip: BE -> LE
  le.setUint32(12, u32(dops, 4), true);                          // input sample rate
  le.setInt16(16, (dops[8] << 8 | dops[9]) << 16 >> 16, true);   // output gain
  out[18] = dops[10];
  if (mappingLen) out.set(dops.subarray(11, 11 + mappingLen), 19);
  return out;
}

const AAC_RATES = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
const AAC_CHANNELS = [0, 1, 2, 3, 4, 5, 6, 8];
/** Enough of an AudioSpecificConfig to correct the sample entry's HE-AAC lie. */
function parseAudioSpecificConfig(dsi) {
  if (!dsi || dsi.length < 2) return null;
  let aot = dsi[0] >> 3;
  let bit = 5;
  const read = (n) => { let v = 0; for (let i = 0; i < n; i++, bit++) v = (v << 1) | ((dsi[bit >> 3] >> (7 - (bit & 7))) & 1); return v; };
  if (aot === 31) aot = 32 + read(6);
  const freqIndex = read(4);
  const sampleRate = freqIndex === 15 ? read(24) : AAC_RATES[freqIndex] ?? 0;
  const channels = AAC_CHANNELS[read(4)] ?? 0;
  // SBR/PS (HE-AAC): an explicit extension states the real, doubled rate.
  if (aot === 5 || aot === 29) {
    const extIndex = read(4);
    return { sampleRate: (extIndex === 15 ? read(24) : AAC_RATES[extIndex]) || sampleRate * 2, channels };
  }
  return { sampleRate, channels };
}
