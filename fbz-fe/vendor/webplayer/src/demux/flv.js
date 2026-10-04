// FLV demuxer.
//
// Same contract as MatroskaDemuxer and Mp4Demuxer: Matroska-shaped tracks,
// readBlocks(pos, max, state), seekTo(sec, track). No browser demuxes FLV, so
// without this an .flv has no leg at all -- and its payload is already exactly
// what MSE wants: the AVCDecoderConfigurationRecord in a sequence-header tag is
// byte-identical to avcC, and the NALUs in every later tag are already length-
// prefixed. So the "transcoding" here is nil; it is pure re-boxing.
//
// Covers what real files actually contain: AVC (codec 7) and HEVC (codec 12,
// the widely-deployed Chinese extension, plus the enhanced-RTMP FourCC header),
// AAC (format 10) and MP3 (format 2).

const i24 = (b, o) => ((b[o] << 16) | (b[o + 1] << 8) | b[o + 2]) << 8 >> 8;
const u24 = (b, o) => (b[o] << 16) | (b[o + 1] << 8) | b[o + 2];
const u32 = (b, o) => b[o] * 0x1000000 + ((b[o + 1] << 16) | (b[o + 2] << 8) | b[o + 3]);
const fourcc = (b, o) => String.fromCharCode(b[o], b[o + 1], b[o + 2], b[o + 3]);

const coded = (code, msg) => Object.assign(new Error(msg), { code });

const TAG_AUDIO = 8, TAG_VIDEO = 9, TAG_SCRIPT = 18;
export const TRACK_VIDEO = 1, TRACK_AUDIO = 2;
const VIDEO_TRACK = 1, AUDIO_TRACK = 2;         // FLV has no track IDs; these are ours

// Non-AAC sound rates live in two bits of the tag header.
const FLV_RATES = [5512, 11025, 22050, 44100];
const ENHANCED = { hvc1: 'V_MPEGH/ISO/HEVC', hev1: 'V_MPEGH/ISO/HEVC', av01: 'V_AV1', vp09: 'V_VP9', avc1: 'V_MPEG4/ISO/AVC' };

export class FlvDemuxer {
  constructor(source) {
    this.src = source;
    this.duration = 0;
    this.tracks = [];
    this.attachments = [];
    this.dataStart = 13;          // 9-byte header + PreviousTagSize0
    this.keyframes = [];          // { time, pos } -- from onMetaData, then grown as we read
    this._meta = null;
  }

  async parseHeader() {
    const head = await this.src.read(0, 13);
    if (head.length < 13 || head[0] !== 0x46 || head[1] !== 0x4c || head[2] !== 0x56) {
      throw coded('NOT_FLV', 'not an FLV file (no "FLV" signature)');
    }
    const headerSize = u32(head, 5);
    this.dataStart = (headerSize >= 9 ? headerSize : 9) + 4;

    const video = { number: VIDEO_TRACK, type: TRACK_VIDEO, codecId: '', codecPrivate: null,
                    language: 'und', name: '', default: true, forced: false, defaultDuration: 0,
                    video: { width: 0, height: 0, displayWidth: 0, displayHeight: 0, colour: null },
                    audio: null, encodings: null };
    const audio = { number: AUDIO_TRACK, type: TRACK_AUDIO, codecId: '', codecPrivate: null,
                    language: 'und', name: '', default: true, forced: false, defaultDuration: 0,
                    video: null, audio: { sampleRate: 44100, channels: 2, bitDepth: 16, outputSampleRate: 0 },
                    encodings: null };

    // Sequence headers sit near the front, but not always as the very first
    // tags -- a script tag and some filler routinely come first. Bounded scan:
    // whatever configuration is found in the first few MB is what the file has.
    let pos = this.dataStart, scanned = 0;
    while (pos < this.src.size && scanned < (6 << 20)) {
      const buf = await this.src.read(pos, Math.min(1 << 20, this.src.size - pos));
      if (buf.length < 15) break;
      let p = 0;
      while (p + 11 <= buf.length) {
        const type = buf[p] & 0x1f;
        const size = u24(buf, p + 1);
        if (p + 11 + size + 4 > buf.length) break;
        const data = buf.subarray(p + 11, p + 11 + size);
        if (type === TAG_SCRIPT) this._readMeta(data);
        else if (type === TAG_VIDEO && size > 1) this._readVideoConfig(video, data);
        else if (type === TAG_AUDIO && size > 1) this._readAudioConfig(audio, data);
        p += 11 + size + 4;
      }
      if (!p) break;
      pos += p; scanned += p;
      if (video.codecId && audio.codecId) break;
    }

    if (video.codecId) this.tracks.push(video);
    if (audio.codecId) this.tracks.push(audio);
    if (!this.tracks.length) throw coded('NOT_FLV', 'FLV carries no AVC/HEVC/AAC/MP3 track');

    if (this._meta) {
      if (this._meta.duration > 0) this.duration = this._meta.duration;
      if (this._meta.width) { video.video.width = this._meta.width; video.video.height = this._meta.height; }
      const k = this._meta.keyframes;
      if (k?.times?.length > 0 && k.times.length === k.filepositions?.length) {
        this.keyframes = k.times.map((t, i) => ({ time: t, pos: k.filepositions[i] }))
                               .filter(x => x.pos >= this.dataStart).sort((a, b) => a.pos - b.pos);
      }
    }
    // No onMetaData duration -- and plenty of files written by capture tools
    // have none. The trailing PreviousTagSize points straight at the last tag,
    // so the real end timestamp is two reads away rather than a full scan.
    if (!(this.duration > 0)) this.duration = await this._durationFromTail();
    if (!video.video.width && video.codecPrivate) this._sizeFromConfig(video);
    return this;
  }

  async _durationFromTail() {
    const size = this.src.size;
    if (!(size > 15)) return 0;
    const tail = await this.src.read(size - 4, 4);
    if (tail.length < 4) return 0;
    const lastTag = size - 4 - u32(tail, 0);
    if (lastTag < this.dataStart || lastTag > size - 11) return 0;
    const head = await this.src.read(lastTag, 11);
    if (head.length < 11) return 0;
    return ((head[7] << 24) | u24(head, 4)) / 1000;
  }

  _readVideoConfig(t, data) {
    const b0 = data[0];
    if (b0 & 0x80) {                                    // enhanced RTMP: FourCC header
      const cc = fourcc(data, 1);
      if ((b0 & 0x0f) !== 0) return;                    // not the sequence-start packet
      t.codecId = ENHANCED[cc] ?? `V_FLV/${cc}`;
      t.codecPrivate = new Uint8Array(data.subarray(5));
      return;
    }
    const codec = b0 & 0x0f;
    if (codec !== 7 && codec !== 12) return;            // only AVC/HEVC carry a config record
    if (data[1] !== 0) return;                          // AVCPacketType 0 = sequence header
    t.codecId = codec === 7 ? 'V_MPEG4/ISO/AVC' : 'V_MPEGH/ISO/HEVC';
    t.codecPrivate = new Uint8Array(data.subarray(5));
  }

  _readAudioConfig(t, data) {
    const format = data[0] >> 4;
    if (format === 10) {                                // AAC
      if (data[1] !== 0) return;                        // AACPacketType 0 = AudioSpecificConfig
      t.codecId = 'A_AAC';
      t.codecPrivate = new Uint8Array(data.subarray(2));
      const cfg = t.codecPrivate;
      if (cfg.length >= 2) {
        const RATES = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
        const idx = ((cfg[0] & 7) << 1) | (cfg[1] >> 7);
        t.audio.sampleRate = RATES[idx] ?? 44100;
        t.audio.channels = (cfg[1] >> 3) & 15 || 2;
      }
      return;
    }
    if (format === 2) {                                 // MP3
      t.codecId = 'A_MPEG/L3';
      t.audio.sampleRate = FLV_RATES[(data[0] >> 2) & 3];
      t.audio.channels = (data[0] & 1) ? 2 : 1;
    }
  }

  /** Width/height off the SPS, for a file whose onMetaData omitted them. */
  _sizeFromConfig(t) {
    // Deliberately not a full SPS parse: the remuxer writes width/height into
    // tkhd only as a hint, and the decoder takes the real dimensions from the
    // bitstream. A wrong guess would be worse than 0.
    t.video.width ||= 0;
    t.video.height ||= 0;
  }

  trackByNumber(n) { return this.tracks.find(t => t.number === n); }

  /**
   * Nearest known keyframe position at or before `seconds`.
   *
   * onMetaData usually carries an index and that is the whole answer. When it
   * does not, `this.keyframes` is grown from what readBlocks has already seen
   * -- so seeking backwards into watched material is exact, and seeking forward
   * past everything known lands on the furthest keyframe and reads on from
   * there. That degrades; scanning the file for a resync point would stall.
   */
  async seekTo(seconds) {
    if (seconds <= 0 || !this.keyframes.length) return this.dataStart;
    let best = this.keyframes[0];
    for (const k of this.keyframes) { if (k.time <= seconds) best = k; else break; }
    return best.pos;
  }

  _remember(time, pos) {
    if (this.keyframes.some(k => k.pos === pos)) return;
    this.keyframes.push({ time, pos });
    this.keyframes.sort((a, b) => a.time - b.time);
  }

  async *readBlocks(offset, maxBytes = 4 << 20, state = {}) {
    let pos = Math.max(offset, this.dataStart);
    const end = this.src.size;
    state.nextPos = pos;
    state.atEnd = false;
    let consumed = 0;

    while (pos < end && consumed < maxBytes) {
      const buf = await this.src.read(pos, Math.min(Math.max(maxBytes - consumed, 1 << 16) + (1 << 16), end - pos));
      if (buf.length < 15) { state.atEnd = true; break; }

      let p = 0;
      while (p + 11 <= buf.length) {
        const type = buf[p] & 0x1f;
        const size = u24(buf, p + 1);
        // A tag straddling the window edge: stop and re-read from its start.
        if (p + 11 + size + 4 > buf.length) break;
        const time = ((buf[p + 7] << 24) | u24(buf, p + 4)) / 1000;
        const data = buf.subarray(p + 11, p + 11 + size);
        const at = pos + p;
        if (type === TAG_VIDEO && size > 1) { const blk = this._videoBlock(data, time, at); if (blk) yield blk; }
        else if (type === TAG_AUDIO && size > 1) { const blk = this._audioBlock(data, time); if (blk) yield blk; }
        p += 11 + size + 4;
      }

      if (!p) {
        // One tag longer than the whole window. Read exactly it, rather than
        // looping forever on a buffer that can never contain it.
        if (buf.length < 11) { state.atEnd = true; break; }
        const size = u24(buf, 1);
        const need = 11 + size + 4;
        if (pos + need > end) { state.atEnd = true; break; }
        const one = await this.src.read(pos, need);
        const type = one[0] & 0x1f;
        const time = ((one[7] << 24) | u24(one, 4)) / 1000;
        const data = one.subarray(11, 11 + size);
        if (type === TAG_VIDEO && size > 1) { const blk = this._videoBlock(data, time, pos); if (blk) yield blk; }
        else if (type === TAG_AUDIO && size > 1) { const blk = this._audioBlock(data, time); if (blk) yield blk; }
        p = need;
      }

      consumed += p;
      pos += p;
      state.nextPos = pos;
    }
    if (pos >= end) state.atEnd = true;
  }

  _videoBlock(data, time, at) {
    const b0 = data[0];
    let keyframe, off, cts = 0;
    if (b0 & 0x80) {                                    // enhanced RTMP
      const packetType = b0 & 0x0f;
      keyframe = ((b0 >> 4) & 0x07) === 1;
      if (packetType === 0 || packetType === 2) return null;   // config / end of sequence
      // 1 = coded frames (24-bit composition time follows), 3 = coded frames X (none)
      off = packetType === 1 ? 8 : 5;
      if (packetType === 1) cts = i24(data, 5) / 1000;
    } else {
      const codec = b0 & 0x0f;
      if (codec !== 7 && codec !== 12) return null;
      if (data[1] !== 1) return null;                   // 0 = config, 2 = end of sequence
      keyframe = (b0 >> 4) === 1;
      cts = i24(data, 2) / 1000;
      off = 5;
    }
    if (off >= data.length) return null;
    if (keyframe) this._remember(time + cts, at);
    return { track: VIDEO_TRACK, time: time + cts, duration: 0, keyframe, data: data.subarray(off) };
  }

  _audioBlock(data, time) {
    const format = data[0] >> 4;
    if (format === 10) {
      if (data[1] !== 1) return null;                   // 0 = AudioSpecificConfig, already taken
      return { track: AUDIO_TRACK, time, duration: 0, keyframe: true, data: data.subarray(2) };
    }
    if (format === 2) return { track: AUDIO_TRACK, time, duration: 0, keyframe: true, data: data.subarray(1) };
    return null;
  }

  // --- AMF0, just enough of it -------------------------------------------
  // onMetaData is where duration, dimensions and the keyframe index live. It is
  // the difference between a seekable FLV and one that can only play forward.

  _readMeta(data) {
    try {
      const r = { b: data, p: 0 };
      const name = amf(r);
      if (name !== 'onMetaData') return;
      const meta = amf(r);
      if (meta && typeof meta === 'object') this._meta = meta;
    } catch { /* a malformed script tag must not sink the file */ }
  }
}

function amf(r) {
  const type = r.b[r.p++];
  const dv = new DataView(r.b.buffer, r.b.byteOffset, r.b.byteLength);
  switch (type) {
    case 0: { const v = dv.getFloat64(r.p, false); r.p += 8; return v; }
    case 1: return r.b[r.p++] !== 0;
    case 2: { const n = (r.b[r.p] << 8) | r.b[r.p + 1]; r.p += 2; const s = new TextDecoder().decode(r.b.subarray(r.p, r.p + n)); r.p += n; return s; }
    case 3: return amfProps(r, {});
    case 8: { r.p += 4; return amfProps(r, {}); }     // ECMA array: the count is advisory
    case 10: { const n = ((r.b[r.p] * 0x1000000) + ((r.b[r.p + 1] << 16) | (r.b[r.p + 2] << 8) | r.b[r.p + 3])); r.p += 4;
               const out = []; for (let i = 0; i < n; i++) out.push(amf(r)); return out; }
    case 11: { const v = dv.getFloat64(r.p, false); r.p += 10; return v; }
    case 5: case 6: return null;
    default: throw new Error(`amf type ${type}`);
  }
}
function amfProps(r, out) {
  while (r.p + 3 <= r.b.length) {
    const n = (r.b[r.p] << 8) | r.b[r.p + 1];
    r.p += 2;
    if (n === 0) { r.p++; return out; }               // "" followed by the 0x09 end marker
    const key = new TextDecoder().decode(r.b.subarray(r.p, r.p + n));
    r.p += n;
    out[key] = amf(r);
  }
  return out;
}
