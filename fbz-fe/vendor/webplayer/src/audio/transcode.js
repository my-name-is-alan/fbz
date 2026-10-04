// Decoded PCM -> Opus -> fMP4, so an undecodable audio track becomes an
// ordinary SourceBuffer the browser can sync against the video itself.
//
// Encoding uses the browser's own AudioEncoder (measured as supporting opus),
// not a wasm encoder: it is native, hardware-assisted where available, and
// keeps the wasm side to decoding only.

import { buildRemuxer } from '../remux/tracks.js';
import { OUT_RATE, OUT_CHANNELS, RAW_FORMATS } from './decode.js';

/** Anything can be thrown, and wasm libraries routinely throw non-Errors. */
export function describeError(e) {
  if (e instanceof Error) return `${e.name}: ${e.message}`;
  if (e && typeof e === 'object') return JSON.stringify(e, Object.getOwnPropertyNames(e)).slice(0, 300);
  return `${typeof e} ${String(e)}`;
}

/**
 * A synthetic Matroska-shaped track for the Opus we are about to produce, so
 * the existing fMP4 muxer can be reused unchanged.
 *
 * CodecPrivate has to be a real OpusHead because that is what buildRemuxer
 * converts into dOps -- little-endian here, big-endian there. Handing it
 * anything else produces a box Chrome rejects by silently detaching the
 * MediaSource, which is exactly the bug that was fixed on main.
 */
function opusTrack(trackNumber) {
  const head = new Uint8Array(19);
  head.set([0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64]);   // "OpusHead"
  const dv = new DataView(head.buffer);
  head[8] = 1;                       // OpusHead version
  head[9] = OUT_CHANNELS;
  dv.setUint16(10, 312, true);       // pre-skip, the libopus default at 48 kHz
  dv.setUint32(12, OUT_RATE, true);
  dv.setInt16(16, 0, true);          // output gain
  head[18] = 0;                      // channel mapping family 0 (mono/stereo)
  return {
    number: trackNumber, type: 2, codecId: 'A_OPUS', codecPrivate: head,
    language: 'und', name: 'transcoded', defaultDuration: 0,
    audio: { channels: OUT_CHANNELS, sampleRate: OUT_RATE, outputSampleRate: 0, bitDepth: 0 },
  };
}

/**
 * Turns windows of an undecodable audio track into fMP4 Opus fragments.
 *
 * Timestamps come from the source packets, not from a running sample count:
 * a decode that drops or pads a frame would otherwise slide the audio against
 * the picture by a growing amount, which is exactly the drift a transcode is
 * supposed to avoid.
 */
export class AudioTranscoder {
  /**
   * @param sourceTrack the Matroska track being replaced
   * @param decoder     a SoftwareAudioDecoder
   * @param onFragment  called with each fMP4 fragment, init segment first
   */
  constructor(sourceTrack, decoder, duration, { onFragment, log = () => {} }) {
    this.format = RAW_FORMATS[sourceTrack.codecId];
    if (!this.format) throw new Error(`no software decoder for ${sourceTrack.codecId}`);
    this.decoder = decoder;
    this.log = log;
    this.onFragment = onFragment;
    this.remuxer = buildRemuxer(opusTrack(sourceTrack.number), duration);
    this.mime = this.remuxer.mime;

    this._pending = [];     // source packets awaiting a full window
    this._windowStart = null;
    this._busy = false;
    this._queue = [];
    this._encoder = null;
    this._chunks = 0;
    this.generation = 0;
  }

  /** Seconds of source audio decoded per ffmpeg invocation. */
  static WINDOW = 8;

  initSegment() { return this.remuxer.initSegment(); }

  /** Feed one demuxed packet of the source track. */
  push(block) {
    if (this._windowStart === null) this._windowStart = block.time;
    this._pending.push(block);
    if (block.time - this._windowStart >= AudioTranscoder.WINDOW) this._drain();
  }

  /** Force the partial window through, e.g. at end of stream. */
  flush() { if (this._pending.length) this._drain(); }

  /** Drop everything in flight; used on seek, where the timeline restarts. */
  reset() {
    this.generation++;
    this._pending = [];
    this._windowStart = null;
    this._queue = [];
    try { this._encoder?.close(); } catch {}
    this._encoder = null;
  }

  _drain() {
    const packets = this._pending;
    this._pending = [];
    this._windowStart = null;
    this._queue.push({ packets, generation: this.generation });
    this._pump();
  }

  async _pump() {
    // One window at a time: ffmpeg.wasm is a single instance with one shared
    // filesystem, and concurrent exec() calls interleave inside it.
    if (this._busy || !this._queue.length) return;
    this._busy = true;
    const { packets, generation } = this._queue.shift();
    try {
      await this._transcode(packets, generation);
    } catch (e) {
      // ffmpeg.wasm rejects with a bare string, not an Error, so reading
      // .message here reported "undefined" and hid the real failure for
      // several rounds. Never assume a rejection is an Error.
      this.log(`audio transcode failed: ${describeError(e)}`, 'error');
    } finally {
      this._busy = false;
      if (this._queue.length) this._pump();
    }
  }

  async _transcode(packets, generation) {
    if (generation !== this.generation) return;
    const startTime = packets[0].time;

    let total = 0;
    for (const p of packets) total += p.data.length;
    const raw = new Uint8Array(total);
    let at = 0;
    for (const p of packets) { raw.set(p.data, at); at += p.data.length; }

    const pcm = await this.decoder.decode(raw, this.format);
    if (generation !== this.generation || !pcm.length) return;

    const encoder = await this._ensureEncoder(generation);
    if (!encoder) return;

    // AudioData wants planar or interleaved; f32-interleaved is what ffmpeg
    // produced and what 'f32' describes.
    const frames = Math.floor(pcm.length / OUT_CHANNELS);
    const data = new AudioData({
      format: 'f32',
      sampleRate: OUT_RATE,
      numberOfFrames: frames,
      numberOfChannels: OUT_CHANNELS,
      timestamp: Math.round(startTime * 1e6),
      data: pcm,
    });
    encoder.encode(data);
    data.close();
    await encoder.flush();
  }

  async _ensureEncoder(generation) {
    if (this._encoder && generation === this.generation) return this._encoder;
    const enc = new AudioEncoder({
      output: (chunk) => this._onChunk(chunk, generation),
      error: (e) => this.log(`AudioEncoder: ${e.message}`, 'error'),
    });
    enc.configure({ codec: 'opus', sampleRate: OUT_RATE, numberOfChannels: OUT_CHANNELS, bitrate: 160000 });
    this._encoder = enc;
    return enc;
  }

  _onChunk(chunk, generation) {
    if (generation !== this.generation) return;
    const buf = new Uint8Array(chunk.byteLength);
    chunk.copyTo(buf);
    // The remuxer speaks in Matroska-shaped blocks; every Opus packet is a
    // sync sample, and the timestamps are the source's own.
    this.remuxer.push({
      data: buf,
      time: chunk.timestamp / 1e6,
      duration: (chunk.duration ?? 20000) / 1e6,
      keyframe: true,
    });
    this._chunks++;
    if (this.remuxer.pendingCount >= 50) this._emit();
  }

  _emit() {
    const frag = this.remuxer.flush();
    if (frag) this.onFragment(frag);
  }

  /** Emit whatever the encoder has produced so far. */
  emitPending() { this._emit(); }

  get encodedChunks() { return this._chunks; }

  async destroy() {
    this.reset();
    try { await this._encoder?.close(); } catch {}
  }
}
