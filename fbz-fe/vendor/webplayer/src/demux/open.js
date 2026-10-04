// Pick a demuxer by what the bytes ARE, not by what the file is called.
//
// A .strm points at object storage where the extension is frequently a lie (or
// absent), and an Emby "Container" field is whatever ffprobe said months ago.
// Sixteen bytes settle it, and the read is free: every demuxer's first act is
// to read the head of the file anyway.

import { MatroskaDemuxer } from './matroska.js';
import { Mp4Demuxer } from './mp4.js';
import { FlvDemuxer } from './flv.js';

const fourcc = (b, o) => String.fromCharCode(b[o], b[o + 1], b[o + 2], b[o + 3]);
const MP4_BRANDS = /^(ftyp|moov|free|skip|wide|mdat|pnot)$/;

/** Container name from a 16-byte head, or null. Exported for tests. */
export function sniff(head) {
  if (!head || head.length < 12) return null;
  if (head[0] === 0x1a && head[1] === 0x45 && head[2] === 0xdf && head[3] === 0xa3) return 'matroska';
  if (head[0] === 0x46 && head[1] === 0x4c && head[2] === 0x56 && head[3] === 0x01) return 'flv';
  if (MP4_BRANDS.test(fourcc(head, 4))) return 'mp4';
  return null;
}

export const DEMUXERS = { matroska: MatroskaDemuxer, mp4: Mp4Demuxer, flv: FlvDemuxer };

/**
 * Open `source` with whichever demuxer speaks its container.
 *
 * An unrecognised head throws with `code: 'UNKNOWN_CONTAINER'`, which is one of
 * the codes src/player.js routes to the native <video> leg -- so ".avi that
 * nothing here demuxes" and "mp4 that this does" both end up somewhere sensible
 * instead of both saying "no EBML header".
 */
export async function openDemuxer(source) {
  const head = await source.read(0, 16);
  const kind = sniff(head);
  if (!kind) {
    const hex = [...head.subarray(0, 8)].map(b => b.toString(16).padStart(2, '0')).join(' ');
    throw Object.assign(new Error(`unrecognised container (starts with ${hex})`), { code: 'UNKNOWN_CONTAINER' });
  }
  const demuxer = await new DEMUXERS[kind](source).parseHeader();
  demuxer.container = kind;
  return demuxer;
}
