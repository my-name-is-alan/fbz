// Fragmented MP4 writer for MSE.
//
// This is a *repackager*, not a transcoder. Codec payloads are copied byte for
// byte out of Matroska blocks, which is why HDR signalling and the Dolby Vision
// RPU (an in-band NAL the decoder ignores) survive intact all the way to the
// compositor. Nothing here understands video.

const FOURCC = s => [s.charCodeAt(0), s.charCodeAt(1), s.charCodeAt(2), s.charCodeAt(3)];

function box(type, ...payload) {
  let len = 8;
  for (const p of payload) len += p.length;
  const out = new Uint8Array(len);
  const dv = new DataView(out.buffer);
  dv.setUint32(0, len);
  out.set(FOURCC(type), 4);
  let o = 8;
  for (const p of payload) { out.set(p, o); o += p.length; }
  return out;
}

const fullBox = (type, version, flags, ...payload) =>
  box(type, new Uint8Array([version, flags >> 16 & 0xff, flags >> 8 & 0xff, flags & 0xff]), ...payload);

function u32(...vals) {
  const a = new Uint8Array(vals.length * 4);
  const dv = new DataView(a.buffer);
  vals.forEach((v, i) => dv.setUint32(i * 4, v >>> 0));
  return a;
}
function u16(...vals) {
  const a = new Uint8Array(vals.length * 2);
  const dv = new DataView(a.buffer);
  vals.forEach((v, i) => dv.setUint16(i * 2, v & 0xffff));
  return a;
}
function u64(v) {
  const a = new Uint8Array(8);
  new DataView(a.buffer).setBigUint64(0, BigInt(Math.round(v)));
  return a;
}
const bytes = (...v) => new Uint8Array(v);
const concat = arrs => {
  let n = 0; for (const a of arrs) n += a.length;
  const out = new Uint8Array(n);
  let o = 0; for (const a of arrs) { out.set(a, o); o += a.length; }
  return out;
};

const UNITY_MATRIX = u32(0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000);

// --- sample entries -------------------------------------------------------

/**
 * `colr` carries colour primaries / transfer / matrix into the MP4.
 * Required here because Matroska may omit its Colour element entirely
 * (houshi.mkv does), leaving the SPS VUI as the only source of truth.
 */
function colrBox(colour) {
  if (!colour || colour.primaries === 2) return new Uint8Array(0);
  return box('colr', bytes(...FOURCC('nclx')),
             u16(colour.primaries, colour.transfer, colour.matrix),
             bytes(colour.fullRange ? 0x80 : 0x00));
}

/**
 * Static HDR10 metadata: `mdcv` (mastering display) and `clli` (content light
 * level), recovered from SEI 137/144 by src/demux/hevc.js.
 *
 * Without these a display has no idea what the content was graded on and has
 * to guess when tone-mapping to its own peak brightness. The field order is
 * the SEI's own -- primaries are GREEN, BLUE, RED, which is not the order
 * anyone expects and is silent when wrong: the gamut is merely the wrong one.
 */
function hdrBoxes(mastering, cll) {
  const out = [];
  if (mastering) {
    const m = mastering;
    out.push(box('mdcv',
      u16(m.green[0], m.green[1], m.blue[0], m.blue[1], m.red[0], m.red[1],
          m.whitePoint[0], m.whitePoint[1]),
      u32(m.maxLuminance, m.minLuminance)));
  }
  if (cll) out.push(box('clli', u16(cll.maxCLL, cll.maxFALL)));
  return out;
}

function visualSampleEntry(fourcc, track, colour, codecBoxes) {
  const w = track.video.width, h = track.video.height;
  return box(fourcc,
    bytes(0, 0, 0, 0, 0, 0), u16(1),                 // reserved, data_reference_index
    u16(0, 0), u32(0, 0, 0),                          // pre_defined / reserved
    u16(w, h),
    u32(0x00480000, 0x00480000),                      // 72 dpi
    u32(0), u16(1),                                   // reserved, frame_count
    new Uint8Array(32),                               // compressorname
    u16(0x0018, 0xffff),                              // depth, pre_defined
    ...codecBoxes,
    colrBox(colour),
    ...hdrBoxes(track.mastering, track.cll));
}

function audioSampleEntry(fourcc, channels, sampleRate, ...codecBoxes) {
  return box(fourcc,
    bytes(0, 0, 0, 0, 0, 0), u16(1),
    u16(0, 0), u32(0),
    u16(channels, 16), u16(0, 0),
    u32(Math.round(sampleRate) << 16 >>> 0),          // 16.16 fixed point
    ...codecBoxes);
}

/** ESDS descriptor wrapping an AAC AudioSpecificConfig. */
function esdsBox(asc, oti = 0x40) {
  const descr = (tag, payload) => concat([bytes(tag, 0x80, 0x80, 0x80, payload.length), payload]);
  // MP3 has no DecoderSpecificInfo -- everything a decoder needs is in the
  // frame headers. Emitting an empty descriptor instead of omitting it is what
  // makes Chromium reject the track.
  const decoderSpecific = asc?.length ? descr(0x05, asc) : new Uint8Array(0);
  const decoderConfig = descr(0x04, concat([
    bytes(oti, 0x15),                                 // objectTypeIndication, streamType=audio
    bytes(0, 0, 0),                                   // bufferSizeDB
    u32(0, 0),                                        // maxBitrate, avgBitrate
    decoderSpecific,
  ]));
  const slConfig = descr(0x06, bytes(0x02));
  const es = descr(0x03, concat([u16(1), bytes(0x00), decoderConfig, slConfig]));
  return fullBox('esds', 0, 0, es);
}

// --- init segment ---------------------------------------------------------

const TRACK_ID = 1;   // one SourceBuffer per track, so each stream is track 1

/**
 * Build the fMP4 initialization segment (ftyp + moov) for a single track.
 * @param {object} cfg { kind:'video'|'audio', timescale, duration, sampleEntry, ... }
 */
function initSegment(cfg) {
  const ftyp = box('ftyp', bytes(...FOURCC('iso6')), u32(1),
                   bytes(...FOURCC('iso6')), bytes(...FOURCC('isom')),
                   bytes(...FOURCC('mp41')), bytes(...FOURCC('dash')));

  const mvhd = fullBox('mvhd', 0, 0,
    u32(0, 0, 1000, Math.round(cfg.duration * 1000)),  // created, modified, timescale, duration
    u32(0x00010000), u16(0x0100), u16(0), u32(0, 0),
    UNITY_MATRIX, u32(0, 0, 0, 0, 0, 0), u32(TRACK_ID + 1));

  const isVideo = cfg.kind === 'video';
  // tkhd v0 after duration: reserved[2] u32, then FOUR 16-bit fields --
  // layer, alternate_group, volume, reserved. Emitting only three shifts the
  // matrix and dimensions by two bytes; ffmpeg tolerates it, Chrome rejects
  // the whole init segment and silently detaches the MediaSource.
  const tkhd = fullBox('tkhd', 0, 3,                   // flags 3 = enabled | in movie
    u32(0, 0, TRACK_ID, 0, Math.round(cfg.duration * 1000)),
    u32(0, 0),                                          // reserved[2]
    u16(0, 0, isVideo ? 0 : 0x0100, 0),                 // layer, alternate_group, volume, reserved
    UNITY_MATRIX,
    u32(isVideo ? (cfg.width << 16) >>> 0 : 0, isVideo ? (cfg.height << 16) >>> 0 : 0));

  const mdhd = fullBox('mdhd', 0, 0,
    u32(0, 0, cfg.timescale, Math.round(cfg.duration * cfg.timescale)),
    u16(0x55c4), u16(0));                              // language 'und', pre_defined

  const hdlr = fullBox('hdlr', 0, 0, u32(0),
    bytes(...FOURCC(isVideo ? 'vide' : 'soun')),
    u32(0, 0, 0), bytes(...new TextEncoder().encode(isVideo ? 'VideoHandler\0' : 'SoundHandler\0')));

  const dinf = box('dinf', box('dref', bytes(0, 0, 0, 0), u32(1), fullBox('url ', 0, 1)));
  const stbl = box('stbl',
    fullBox('stsd', 0, 0, u32(1), cfg.sampleEntry),
    fullBox('stts', 0, 0, u32(0)),
    fullBox('stsc', 0, 0, u32(0)),
    fullBox('stsz', 0, 0, u32(0, 0)),
    fullBox('stco', 0, 0, u32(0)));

  const minf = box('minf',
    isVideo ? box('vmhd', bytes(0, 0, 0, 1), u16(0, 0, 0, 0)) : box('smhd', bytes(0, 0, 0, 0), u16(0, 0)),
    dinf, stbl);

  const trak = box('trak', tkhd, box('mdia', mdhd, hdlr, minf));
  const mvex = box('mvex', fullBox('trex', 0, 0, u32(TRACK_ID, 1, 0, 0, 0)));
  return concat([ftyp, box('moov', mvhd, trak, mvex)]);
}

// --- media fragments ------------------------------------------------------

const TRUN_FLAGS = 0x000f01;   // data-offset + sample duration/size/flags present

/**
 * Build one moof+mdat fragment.
 * @param {Array} samples [{ data, duration, keyframe }] in decode order
 * @param {number} baseTime in track timescale units
 */
function fragment(samples, baseTime, seq) {
  const mfhd = fullBox('mfhd', 0, 0, u32(seq));
  const tfhd = fullBox('tfhd', 0, 0x020000, u32(TRACK_ID));   // default-base-is-moof
  const tfdt = fullBox('tfdt', 1, 0, u64(baseTime));

  const trunPayload = new Uint8Array(4 + 4 + samples.length * 16);
  const dv = new DataView(trunPayload.buffer);
  dv.setUint32(0, samples.length);
  // data_offset patched below once the moof size is known
  let o = 8;
  for (const s of samples) {
    dv.setUint32(o, s.duration);
    dv.setUint32(o + 4, s.data.length);
    // 0x02000000 = sample_is_non_sync (not a random access point)
    dv.setUint32(o + 8, s.keyframe ? 0x02000000 : 0x01010000);
    dv.setInt32(o + 12, s.cts ?? 0);
    o += 16;
  }
  const trun = fullBox('trun', 0, TRUN_FLAGS, trunPayload);
  const traf = box('traf', tfhd, tfdt, trun);
  const moof = box('moof', mfhd, traf);

  // data_offset is relative to the start of moof: moof size + mdat header
  const trunOffsetInMoof = moof.length - trun.length + 12;
  new DataView(moof.buffer).setInt32(trunOffsetInMoof + 4, moof.length + 8);

  const payload = concat(samples.map(s => s.data));
  return concat([moof, box('mdat', payload)]);
}

// --- public API -----------------------------------------------------------

/**
 * Repackages one Matroska track into fMP4. Feed it demuxer blocks; it emits
 * an init segment once, then a fragment per flush().
 */
export class TrackRemuxer {
  /**
   * @param {object} track  Matroska track entry
   * @param {object} opts   { colour, codecString, sampleEntryBoxes }
   */
  constructor(track, opts) {
    this.track = track;
    this.kind = opts.kind;
    this.codecString = opts.codecString;
    this.mime = `${this.kind === 'video' ? 'video' : 'audio'}/mp4; codecs="${this.codecString}"`;
    // Video uses ms ticks (matching Matroska's usual 1ms TimestampScale);
    // audio uses its own sample rate so frame durations stay exact.
    this.timescale = this.kind === 'video' ? 1000 : Math.round(opts.sampleRate || 48000);
    this.duration = opts.duration || 0;
    this.sampleEntry = opts.sampleEntry;
    this.width = track.video?.width ?? 0;
    this.height = track.video?.height ?? 0;
    this.seq = 1;
    this.pending = [];
    this.baseTime = null;
    this.defaultDuration = track.defaultDuration ? track.defaultDuration / 1e9 : 0;
    // Set only for the length-prefixed codecs (AVC/HEVC). AV1 and VP9 carry no
    // such chain, so there is nothing to walk and nothing to check.
    this.nalLengthSize = opts.nalLengthSize || 0;
    this.warn = null;            // the worker hangs its log() here
    this._nalWarned = false;
  }

  initSegment() {
    return initSegment({
      kind: this.kind, timescale: this.timescale, duration: this.duration,
      sampleEntry: this.sampleEntry, width: this.width, height: this.height,
    });
  }

  push(block) {
    if (this.baseTime === null) this.baseTime = block.time;
    if (this.nalLengthSize && !this._nalWarned) this._checkNalChain(block);
    this.pending.push(block);
  }

  /**
   * Walk the AVCC/HVCC length prefixes the way a decoder does, and say so if
   * they do not land exactly on the end of the sample.
   *
   * This is the same walk Firefox performs in H264ChangeMonitor, where a break
   * surfaces as "MediaResult ... ConvertSampleToAVCC" -- a Gecko-internal name
   * that says nothing about which frame or which byte. Chromium is quieter
   * about it, which is worse: the file plays there and the fault only shows up
   * on someone else's machine. The walk is per-NAL, not per-byte, so it costs a
   * handful of additions per frame; it reports once and changes nothing, because
   * dropping the sample would be a guess until a real file proves it necessary.
   */
  _checkNalChain(block) {
    const d = block.data, n = this.nalLengthSize;
    let p = 0;
    while (p < d.length) {
      if (p + n > d.length) break;
      let len = 0;
      for (let i = 0; i < n; i++) len = len * 256 + d[p + i];
      p += n + len;
    }
    if (p === d.length) return;
    this._nalWarned = true;
    this.warn?.(`${this.kind} sample at ${block.time.toFixed(2)}s: ${n}-byte NAL length chain `
      + `overruns to ${p} in a ${d.length}-byte sample. Firefox rejects this as ConvertSampleToAVCC; `
      + `Chromium may play it anyway. Only the first such sample is reported.`);
  }

  get pendingCount() { return this.pending.length; }

  /**
   * Emit a fragment for everything buffered. Returns null when empty.
   *
   * Matroska blocks carry only a presentation timestamp, in decode order. With
   * B-frames those PTS values are not monotonic, but MP4 requires a monotonic
   * decode timeline plus per-sample composition offsets. Since the set of DTS
   * equals the set of PTS for a closed GOP, sorting the PTS values and handing
   * them out in decode order recovers DTS exactly; CTS is then PTS - DTS, which
   * is always >= 0 (so trun version 0 with unsigned offsets stays valid).
   */
  flush(force = false) {
    if (!this.pending.length) return null;
    const ts = this.timescale;

    // A fragment's DTS is recovered by sorting its PTS, which is only correct
    // when the fragment is a whole number of CLOSED GOPs -- the trick relies on
    // {DTS} == {PTS} as sets, true only when every frame's references are inside
    // the fragment. The fill loop flushes per fixed-size read chunk, which cuts
    // GOPs anywhere; a fragment that starts or ends mid-GOP gets wrong DTS, and
    // the browser silently refuses to extend the buffered range past the first
    // fragment -- playback freezes a few seconds in. So for video, emit only up
    // to the last keyframe and keep the trailing partial GOP for next time.
    // Audio frames are all independent, so they always flush whole. `force`
    // drains the tail at end-of-stream.
    let blocks;
    if (this.kind === 'video' && !force) {
      let lastKf = -1;
      for (let i = this.pending.length - 1; i > 0; i--) if (this.pending[i].keyframe) { lastKf = i; break; }
      if (lastKf <= 0) return null;            // no complete GOP buffered yet
      blocks = this.pending.slice(0, lastKf);
      this.pending = this.pending.slice(lastKf);
    } else {
      blocks = this.pending;
      this.pending = [];
    }

    const pts = blocks.map(b => Math.round(b.time * ts));
    const dts = [...pts].sort((a, b) => a - b);

    const fallback = Math.max(1, Math.round((this.defaultDuration || 1 / 30) * ts));
    const samples = blocks.map((b, i) => {
      // Duration must be measured on the decode timeline, not on PTS.
      let dur = i + 1 < dts.length ? dts[i + 1] - dts[i]
              : b.duration ? Math.round(b.duration * ts) : fallback;
      if (!dur || dur <= 0) dur = fallback;
      return { data: b.data, duration: dur, keyframe: b.keyframe, cts: pts[i] - dts[i] };
    });

    this.lastDts = dts[dts.length - 1];
    return fragment(samples, dts[0], this.seq++);
  }
}

export { box, fullBox, u32, u16, bytes, concat, visualSampleEntry, audioSampleEntry, esdsBox, FOURCC };
