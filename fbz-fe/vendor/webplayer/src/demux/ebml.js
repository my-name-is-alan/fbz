// EBML primitives. Matroska is EBML all the way down, so everything the
// demuxer does bottoms out in these four reads.

/** Byte source: random access over a File or an HTTP resource. */
export class Reader {
  constructor(buf, base = 0) { this.buf = buf; this.base = base; this.pos = 0; }
  get abs() { return this.base + this.pos; }
  get left() { return this.buf.length - this.pos; }
  u8() { return this.buf[this.pos++]; }
  bytes(n) { const v = this.buf.subarray(this.pos, this.pos + n); this.pos += n; return v; }
  skip(n) { this.pos += n; }
}

/**
 * Read an EBML element ID. Unlike a size VINT the length marker is *kept* —
 * IDs are compared as their full encoded form (0x1A45DFA3 etc).
 */
export function readId(r) {
  const first = r.buf[r.pos];
  if (first === undefined) return null;
  const len = 8 - Math.floor(Math.log2(first));   // 0x80->1, 0x40->2, 0x20->3, 0x10->4
  if (first === 0 || len > 4 || len > r.left) return null;
  let id = 0;
  for (let i = 0; i < len; i++) id = id * 256 + r.u8();
  return id;
}

/**
 * Read a size VINT. The length marker bit IS stripped here.
 * Returns null for the all-ones "unknown size" form (live/streamed clusters).
 */
export function readSize(r) {
  const first = r.buf[r.pos];
  if (first === undefined) return undefined;
  const len = 8 - Math.floor(Math.log2(first));
  if (first === 0 || len > 8 || len > r.left) return undefined;
  let v = r.u8() & (0xff >> len);
  let allOnes = v === (0xff >> len);
  for (let i = 1; i < len; i++) {
    const b = r.u8();
    if (b !== 0xff) allOnes = false;
    v = v * 256 + b;                              // *256 not <<8: sizes exceed 32 bits
  }
  return allOnes ? null : v;
}

export function readUint(bytes) {
  let v = 0;
  for (const b of bytes) v = v * 256 + b;
  return v;
}

export function readInt(bytes) {
  if (!bytes.length) return 0;
  let v = bytes[0] & 0x80 ? -1 : 0;               // sign-extend from the top bit
  for (const b of bytes) v = v * 256 + b;
  return v;
}

export function readFloat(bytes) {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.length);
  if (bytes.length === 4) return dv.getFloat32(0);
  if (bytes.length === 8) return dv.getFloat64(0);
  return 0;
}

const UTF8 = new TextDecoder('utf-8');
export function readString(bytes) {
  let end = bytes.length;
  while (end > 0 && bytes[end - 1] === 0) end--;   // Matroska pads strings with NULs
  return UTF8.decode(bytes.subarray(0, end));
}

/**
 * Walk the direct children of a master element, calling fn(id, reader, size).
 * fn returns true to consume the payload itself, false to have it skipped.
 */
export function eachChild(r, end, fn) {
  while (r.pos < end) {
    const start = r.pos;
    const id = readId(r);
    if (id === null) { r.pos = start; break; }
    const size = readSize(r);
    if (size === undefined) { r.pos = start; break; }
    // Unknown-size master elements run to the next sibling; only Segment and
    // Cluster legally use this, and both are handled by the caller.
    if (size === null) { fn(id, r, null); break; }
    const next = r.pos + size;
    if (next > end) { r.pos = start; break; }
    if (!fn(id, r, size)) r.pos = next;
    else r.pos = next;
  }
}
