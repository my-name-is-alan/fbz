// VP9 uncompressed frame header.
//
// Matroska stores no CodecPrivate for V_VP9 -- the profile, bit depth and
// chroma subsampling exist only in the bitstream. Everything an MP4 sample
// entry needs about a VP9 track therefore has to be read out of a keyframe.
//
// Guessing instead is not a small error: the codec string carries the profile
// and bit depth, so a hardcoded one declares 8-bit Profile 0 content as 10-bit
// Profile 2. That either fails outright in MSE or, worse, is accepted and
// decoded against the wrong configuration.

const FRAME_MARKER = 2;
const SYNC_CODE = 0x498342;
const CS_RGB = 7;

class Bits {
  constructor(buf) { this.buf = buf; this.pos = 0; }
  f(n) {
    let v = 0;
    for (let i = 0; i < n; i++) {
      const byte = this.buf[this.pos >> 3];
      if (byte === undefined) throw new Error('VP9 header truncated');
      v = (v << 1) | ((byte >> (7 - (this.pos & 7))) & 1);
      this.pos++;
    }
    return v >>> 0;
  }
}

/**
 * VP9 level from the luma sample rate and picture size, per the VP9 spec's
 * level table. The sample entry has to carry one, and picking the smallest
 * level the stream actually fits is what a conforming encoder would write.
 */
const LEVELS = [
  [10, 829440, 36864], [11, 2764800, 73728], [20, 4608000, 122880],
  [21, 9216000, 245760], [30, 20736000, 552960], [31, 36864000, 983040],
  [40, 83558400, 2228224], [41, 160432128, 2228224], [50, 311951360, 8912896],
  [51, 588251136, 8912896], [52, 1176502272, 8912896], [60, 1176502272, 35651584],
  [61, 2353004544, 35651584], [62, 4706009088, 35651584],
];

export function vp9Level(width, height, fps = 30) {
  const size = width * height;
  const rate = size * fps;
  for (const [level, maxRate, maxSize] of LEVELS) {
    if (rate <= maxRate && size <= maxSize) return level;
  }
  return 62;
}

/**
 * Parse the uncompressed header of a VP9 keyframe.
 * Returns null for anything that is not a keyframe, so callers can just feed
 * frames until one comes back.
 */
export function parseVp9Keyframe(data) {
  const br = new Bits(data);
  if (br.f(2) !== FRAME_MARKER) return null;
  const profileLow = br.f(1), profileHigh = br.f(1);
  const profile = (profileHigh << 1) | profileLow;
  if (profile === 3) br.f(1);                     // reserved_zero
  if (br.f(1)) return null;                       // show_existing_frame
  if (br.f(1) !== 0) return null;                 // frame_type: 0 == KEY_FRAME
  br.f(1);                                        // show_frame
  br.f(1);                                        // error_resilient_mode
  if (br.f(24) !== SYNC_CODE) return null;

  // colour_config()
  let bitDepth = 8;
  if (profile >= 2) bitDepth = br.f(1) ? 12 : 10;
  const colorSpace = br.f(3);
  let subsamplingX = 1, subsamplingY = 1, fullRange = false;
  if (colorSpace !== CS_RGB) {
    fullRange = !!br.f(1);
    if (profile === 1 || profile === 3) {
      subsamplingX = br.f(1); subsamplingY = br.f(1); br.f(1);
    }
  } else {
    fullRange = true;
    if (profile === 1 || profile === 3) { subsamplingX = 0; subsamplingY = 0; br.f(1); }
  }

  const width = br.f(16) + 1;
  const height = br.f(16) + 1;
  return { profile, bitDepth, colorSpace, fullRange, subsamplingX, subsamplingY, width, height };
}

/** RFC 6381 codec string: vp09.<profile>.<level>.<bitDepth>. */
export function vp9CodecString(cfg, level) {
  const p = String(cfg.profile).padStart(2, '0');
  const l = String(level).padStart(2, '0');
  const d = String(cfg.bitDepth).padStart(2, '0');
  return `vp09.${p}.${l}.${d}`;
}

/**
 * VPCodecConfigurationRecord payload (the part after the FullBox header).
 * Chroma subsampling is the enumerated value, not the two flags: 0 = 4:2:0
 * vertical, 1 = 4:2:0 colocated, 2 = 4:2:2, 3 = 4:4:4.
 */
export function vpcCPayload(cfg, level, colour) {
  const chroma = cfg.subsamplingX && cfg.subsamplingY ? 1
               : cfg.subsamplingX && !cfg.subsamplingY ? 2 : 3;
  return new Uint8Array([
    cfg.profile, level,
    (cfg.bitDepth << 4) | (chroma << 1) | (cfg.fullRange ? 1 : 0),
    colour?.primaries ?? 2,
    colour?.transfer ?? 2,
    colour?.matrix ?? 2,
    0, 0,                                       // codecIntializationDataSize
  ]);
}
