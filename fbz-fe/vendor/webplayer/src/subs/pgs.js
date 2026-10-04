// PGS (Blu-ray graphic subtitles) out of Matroska.
//
// Matroska stores an S_HDMV/PGS block as a bare run of segments -- type (u8),
// length (u16 BE), body -- with the presentation time carried by the block
// timestamp. A .sup file stores the same segments, but prefixes each one with
// a 10-byte header: "PG", then PTS and DTS as 32-bit 90 kHz values.
//
// Every PGS decoder on npm reads .sup, because that is the format you get out
// of a Blu-ray. Rather than reimplement RLE decoding, palette handling and
// window composition, this rebuilds the 10 bytes per segment that the muxer
// dropped and hands the decoder exactly the format it already parses.
//
// Verified against the test files by tools/test-pgs.mjs, which round-trips
// real demuxed packets through libpgs.

const PG_MAGIC = 0x50, PG_MAGIC2 = 0x47;   // "PG"
export const PGS_TIMEBASE = 90000;

export const SEGMENT_PCS = 0x16, SEGMENT_WDS = 0x17, SEGMENT_PDS = 0x14,
             SEGMENT_ODS = 0x15, SEGMENT_END = 0x80;

const KNOWN = new Set([SEGMENT_PCS, SEGMENT_WDS, SEGMENT_PDS, SEGMENT_ODS, SEGMENT_END]);

/**
 * Split a Matroska PGS block into its segments.
 * Returns null if the payload is not a clean run of segments, which is the
 * signal that something upstream is wrong -- most likely track compression
 * that was not undone.
 */
export function splitSegments(data) {
  const out = [];
  let p = 0;
  while (p + 3 <= data.length) {
    const type = data[p];
    const len = (data[p + 1] << 8) | data[p + 2];
    if (!KNOWN.has(type) || p + 3 + len > data.length) return null;
    out.push(data.subarray(p, p + 3 + len));
    p += 3 + len;
  }
  return p === data.length ? out : null;
}

/**
 * Build a .sup byte stream from demuxed Matroska packets.
 * `packets` is [{ time (seconds), data (Uint8Array) }] in presentation order.
 *
 * DTS is written as 0. Real .sup files from Blu-ray discs do the same for
 * everything except ODS, and no decoder in this space uses it for timing --
 * they key off PTS, which is what the block timestamp gives us exactly.
 */
export function packetsToSup(packets) {
  const parts = [];
  let total = 0;
  for (const pkt of packets) {
    const segments = splitSegments(pkt.data);
    if (!segments) continue;
    const pts = Math.round(pkt.time * PGS_TIMEBASE) >>> 0;
    for (const seg of segments) {
      const out = new Uint8Array(10 + seg.length);
      out[0] = PG_MAGIC; out[1] = PG_MAGIC2;
      out[2] = (pts >>> 24) & 0xff; out[3] = (pts >>> 16) & 0xff;
      out[4] = (pts >>> 8) & 0xff;  out[5] = pts & 0xff;
      // DTS stays zero.
      out.set(seg, 10);
      parts.push(out);
      total += out.length;
    }
  }
  const buf = new Uint8Array(total);
  let at = 0;
  for (const p of parts) { buf.set(p, at); at += p.length; }
  return buf;
}

/**
 * Accumulates PGS packets as they arrive from the demuxer and keeps a decoder
 * fed with a .sup stream covering the playhead.
 *
 * libpgs has no incremental API: loadFromBuffer() re-parses the whole stream
 * and re-decodes every RLE bitmap in it. The first version of this class kept
 * every packet of the film and reloaded all of them every 100 ms, so the cost
 * of displaying one subtitle grew with how long the film had been playing --
 * quadratic, and on a 4K HDR file it ate enough main-thread time to visibly
 * drop frames after a few minutes. It also never de-duplicated, so every seek
 * and every backfill appended a second copy of a region it already held.
 *
 * A PGS display set is entirely self-contained -- unlike ASS, nothing depends
 * on earlier state -- so only the sets around the playhead need to be loaded.
 * Keeping a bounded window makes each reload a fixed, small cost no matter how
 * long playback has been running.
 */
export class PgsFeed {
  /**
   * @param onUpdate called with the full .sup bytes whenever the window changes
   * @param behind   seconds to keep behind the playhead (a set already on
   *                 screen must stay loaded or it would blink out)
   * @param ahead    seconds to keep ahead, which is the player's read-ahead
   */
  constructor(onUpdate, { behind = 30, ahead = 120 } = {}) {
    this.onUpdate = onUpdate;
    this.packets = [];
    this.bad = 0;
    this.reloads = 0;
    this._keys = new Set();
    this._behind = behind;
    this._ahead = ahead;
    this._time = 0;
    this._dirty = false;
    this._timer = null;
  }

  /**
   * Identity of a display set. The block timestamp alone is not enough: a
   * PGS track can carry a clear-screen set at the same timestamp as the set
   * that replaces it.
   */
  static key(p) { return `${Math.round(p.time * 1000)}:${p.data.length}`; }

  push(packet) {
    if (!splitSegments(packet.data)) { this.bad++; return; }
    const key = PgsFeed.key(packet);
    // A seek re-reads the region it lands in and enableSubtitle() backfills
    // whatever is already buffered, so the same display set genuinely arrives
    // more than once. Without this the array -- and every reload built from
    // it -- grew a duplicate per visit.
    if (this._keys.has(key)) return;
    this._keys.add(key);
    this.packets.push({ time: packet.time, data: packet.data });
    this._schedule();
  }

  /** Tell the feed where the playhead is, so it can drop what is out of range. */
  setTime(seconds) {
    if (!Number.isFinite(seconds)) return;
    this._time = seconds;
    if (this._trim()) this._schedule();
  }

  _trim() {
    const lo = this._time - this._behind, hi = this._time + this._ahead;
    const keep = this.packets.filter(p => p.time >= lo && p.time <= hi);
    if (keep.length === this.packets.length) return false;
    this.packets = keep;
    this._keys = new Set(keep.map(PgsFeed.key));
    return true;
  }

  _schedule() {
    this._dirty = true;
    // Packets arrive in bursts of one read chunk; rebuilding per packet would
    // re-parse the whole window dozens of times per chunk.
    if (this._timer) return;
    this._timer = setTimeout(() => { this._timer = null; this.flush(); }, 250);
  }

  flush() {
    if (!this._dirty || !this.packets.length) return;
    this._dirty = false;
    this.packets.sort((a, b) => a.time - b.time);
    this.reloads++;
    this.onUpdate(packetsToSup(this.packets));
  }

  reset() {
    this.packets = [];
    this._keys = new Set();
    this._dirty = false;
    clearTimeout(this._timer);
    this._timer = null;
  }
}
