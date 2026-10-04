// HEVC bitstream helpers.
//
// Two things the container cannot be trusted for:
//   1. the MSE codec string  -- must be exact or SourceBuffer construction throws
//   2. colour / HDR metadata -- houshi.mkv has NO container Colour element at all;
//      its PQ/BT.2020 signalling lives only in the SPS VUI.
// Both are recovered here from hvcC + SPS.

/** Strip emulation-prevention bytes (00 00 03 -> 00 00) to get RBSP. */
function unescapeRbsp(nal) {
  const out = new Uint8Array(nal.length);
  let o = 0, zeros = 0;
  for (let i = 0; i < nal.length; i++) {
    const b = nal[i];
    if (zeros >= 2 && b === 0x03) { zeros = 0; continue; }
    out[o++] = b;
    zeros = b === 0 ? zeros + 1 : 0;
  }
  return out.subarray(0, o);
}

class BitReader {
  constructor(buf) { this.buf = buf; this.bit = 0; }
  // Only safe up to 32 bits; use skip() for wider runs of bits we discard,
  // because `v << 1` overflows and silently corrupts the read position logic.
  u(n) { let v = 0; for (let i = 0; i < n; i++) v = ((v * 2) + this.u1()); return v; }
  skip(n) { this.bit += n; if ((this.bit >> 3) > this.buf.length) throw new RangeError('bitstream overrun'); }
  u1() {
    const byte = this.buf[this.bit >> 3];
    if (byte === undefined) throw new RangeError('bitstream overrun');
    const v = (byte >> (7 - (this.bit & 7))) & 1;
    this.bit++;
    return v;
  }
  ue() {                                    // Exp-Golomb unsigned
    let lz = 0;
    while (this.u1() === 0) { if (++lz > 32) throw new RangeError('bad exp-golomb'); }
    return lz === 0 ? 0 : ((1 << lz) >>> 0) - 1 + this.u(lz);
  }
  se() { const k = this.ue(); return k & 1 ? (k + 1) >> 1 : -(k >> 1); }
}

// H.265 7.3.3: one profile_tier_level "profile block" is exactly 88 bits --
// 2 space + 1 tier + 5 idc + 32 compatibility + 48 constraint. Getting the
// constraint run wrong by even 4 bits desyncs every field after it.
const PTL_PROFILE_BITS = 2 + 1 + 5 + 32 + 48;

function profileTierLevel(br, maxSubLayersMinus1) {
  br.skip(PTL_PROFILE_BITS);
  br.skip(8);                               // general_level_idc
  const prof = [], lvl = [];
  for (let i = 0; i < maxSubLayersMinus1; i++) { prof.push(br.u1()); lvl.push(br.u1()); }
  if (maxSubLayersMinus1 > 0) for (let i = maxSubLayersMinus1; i < 8; i++) br.skip(2);
  for (let i = 0; i < maxSubLayersMinus1; i++) {
    if (prof[i]) br.skip(PTL_PROFILE_BITS);
    if (lvl[i]) br.skip(8);
  }
}

function stRefPicSet(br, idx, numSets, numDeltaPocs) {
  let inter = 0;
  if (idx !== 0) inter = br.u1();
  if (inter) {
    if (idx === numSets) br.ue();          // delta_idx_minus1
    br.u1();                                // delta_rps_sign
    br.ue();                                // abs_delta_rps_minus1
    const refIdx = idx - 1;                 // only valid when delta_idx_minus1 == 0
    const n = numDeltaPocs[refIdx] ?? 0;
    let count = 0;
    for (let j = 0; j <= n; j++) {
      const used = br.u1();
      let useDelta = 1;
      if (!used) useDelta = br.u1();
      if (used || useDelta) count++;
    }
    numDeltaPocs[idx] = count;
  } else {
    const neg = br.ue(), pos = br.ue();
    numDeltaPocs[idx] = neg + pos;
    for (let i = 0; i < neg; i++) { br.ue(); br.u1(); }
    for (let i = 0; i < pos; i++) { br.ue(); br.u1(); }
  }
}

function scalingListData(br) {
  for (let sizeId = 0; sizeId < 4; sizeId++) {
    for (let matrixId = 0; matrixId < 6; matrixId += sizeId === 3 ? 3 : 1) {
      if (!br.u1()) br.ue();                // pred_mode_flag / pred_matrix_id_delta
      else {
        const n = Math.min(64, 1 << (4 + (sizeId << 1)));
        if (sizeId > 1) br.se();            // scaling_list_dc_coef_minus8
        for (let i = 0; i < n; i++) br.se();
      }
    }
  }
}

/**
 * Parse an HEVC SPS NAL (payload without the 2-byte NAL header removed by caller?
 * pass the full NAL including header). Returns colour info from the VUI.
 */
export function parseSps(nal) {
  const rbsp = unescapeRbsp(nal.subarray(2));   // drop 2-byte NAL header
  const br = new BitReader(rbsp);
  br.u(4);                                       // sps_video_parameter_set_id
  const maxSubLayersMinus1 = br.u(3);
  br.u1();                                       // temporal_id_nesting
  profileTierLevel(br, maxSubLayersMinus1);
  br.ue();                                       // sps_seq_parameter_set_id
  const chromaFormat = br.ue();
  if (chromaFormat === 3) br.u1();
  const width = br.ue(), height = br.ue();
  if (br.u1()) { br.ue(); br.ue(); br.ue(); br.ue(); }   // conformance window
  const bitDepthLuma = br.ue() + 8;
  const bitDepthChroma = br.ue() + 8;
  const log2MaxPocLsb = br.ue() + 4;             // needed later for long-term ref POC width
  const subLayerOrdering = br.u1();
  for (let i = subLayerOrdering ? 0 : maxSubLayersMinus1; i <= maxSubLayersMinus1; i++) {
    br.ue(); br.ue(); br.ue();
  }
  br.ue(); br.ue(); br.ue(); br.ue(); br.ue(); br.ue();  // CTB / TB sizes + hierarchy depths
  if (br.u1() && br.u1()) scalingListData(br);   // scaling_list_enabled && data_present
  br.u1();                                       // amp_enabled
  br.u1();                                       // sao_enabled
  if (br.u1()) {                                 // pcm_enabled
    br.u(4); br.u(4); br.ue(); br.ue(); br.u1();
  }
  const numSets = br.ue();
  const numDeltaPocs = [];
  for (let i = 0; i < numSets; i++) stRefPicSet(br, i, numSets, numDeltaPocs);
  if (br.u1()) {                                 // long_term_ref_pics_present
    const n = br.ue();
    for (let i = 0; i < n; i++) { br.skip(log2MaxPocLsb); br.u1(); }
  }
  br.u1();                                       // sps_temporal_mvp_enabled
  br.u1();                                       // strong_intra_smoothing

  const out = { width, height, bitDepthLuma, bitDepthChroma, chromaFormat,
                fullRange: false, primaries: 2, transfer: 2, matrix: 2, hasVui: false };
  if (!br.u1()) return out;                      // vui_parameters_present_flag
  out.hasVui = true;

  if (br.u1()) {                                 // aspect_ratio_info_present
    const idc = br.u(8);
    if (idc === 255) { br.u(16); br.u(16); }
  }
  if (br.u1()) br.u1();                          // overscan
  if (br.u1()) {                                 // video_signal_type_present
    br.u(3);                                     // video_format
    out.fullRange = br.u1() === 1;
    if (br.u1()) {                               // colour_description_present
      out.primaries = br.u(8);
      out.transfer = br.u(8);
      out.matrix = br.u(8);
    }
  }
  return out;
}

/** Split an hvcC configuration record into its parameter-set NAL arrays. */
export function parseHvcC(cp) {
  if (!cp || cp.length < 23 || cp[0] !== 1) return null;
  const nalArrays = [];
  const num = cp[22];
  let p = 23;
  for (let i = 0; i < num && p + 3 <= cp.length; i++) {
    const nalType = cp[p] & 0x3f;
    const count = (cp[p + 1] << 8) | cp[p + 2];
    p += 3;
    const nals = [];
    for (let j = 0; j < count && p + 2 <= cp.length; j++) {
      const len = (cp[p] << 8) | cp[p + 1];
      p += 2;
      nals.push(cp.subarray(p, p + len));
      p += len;
    }
    nalArrays.push({ nalType, nals });
  }
  return {
    generalProfileSpace: cp[1] >> 6,
    generalTierFlag: (cp[1] >> 5) & 1,
    generalProfileIdc: cp[1] & 0x1f,
    generalProfileCompatibility: (cp[2] << 24 | cp[3] << 16 | cp[4] << 8 | cp[5]) >>> 0,
    constraintBytes: cp.subarray(6, 12),
    generalLevelIdc: cp[12],
    lengthSizeMinusOne: cp[21] & 3,
    nalArrays,
  };
}

/**
 * Build the RFC 6381 codec string for MSE, e.g. "hvc1.2.4.L150.B0".
 * Getting this wrong makes addSourceBuffer throw, so it is derived from
 * hvcC rather than guessed from the Matroska CodecID.
 */
export function hevcCodecString(cp, fourcc = 'hvc1') {
  const c = parseHvcC(cp);
  if (!c) return fourcc;
  const space = ['', 'A', 'B', 'C'][c.generalProfileSpace];
  // compatibility flags are written in reverse bit order, as a hex value
  let rev = 0;
  for (let i = 0; i < 32; i++) rev = (rev << 1) | ((c.generalProfileCompatibility >>> i) & 1);
  const parts = [fourcc, `${space}${c.generalProfileIdc}`, (rev >>> 0).toString(16),
                 `${c.generalTierFlag ? 'H' : 'L'}${c.generalLevelIdc}`];
  // trailing constraint bytes, most-significant first, zero bytes trimmed
  const cons = [...c.constraintBytes];
  while (cons.length && cons[cons.length - 1] === 0) cons.pop();
  for (const b of cons) parts.push(b.toString(16).toUpperCase());
  return parts.join('.');
}

/** Colour info, preferring the SPS VUI because the container may omit it. */
export function colourFromTrack(track) {
  const c = parseHvcC(track.codecPrivate);
  const sps = c?.nalArrays.find(a => a.nalType === 33)?.nals[0];
  if (sps) {
    try {
      const v = parseSps(sps);
      if (v.hasVui && v.primaries !== 2) {
        return { primaries: v.primaries, transfer: v.transfer, matrix: v.matrix,
                 fullRange: v.fullRange, bitDepth: v.bitDepthLuma, source: 'sps-vui' };
      }
      // VUI present but no colour description: still trust the bit depth
      if (track.video?.colour) {
        const cc = track.video.colour;
        return { primaries: cc.primaries ?? 2, transfer: cc.transfer ?? 2, matrix: cc.matrix ?? 2,
                 fullRange: cc.range === 2, bitDepth: v.bitDepthLuma, source: 'container' };
      }
      return { primaries: 2, transfer: 2, matrix: 2, fullRange: v.fullRange,
               bitDepth: v.bitDepthLuma, source: 'sps-nocolour' };
    } catch { /* fall through to container */ }
  }
  const cc = track.video?.colour;
  if (cc) return { primaries: cc.primaries ?? 2, transfer: cc.transfer ?? 2, matrix: cc.matrix ?? 2,
                   fullRange: cc.range === 2, bitDepth: 8, source: 'container' };
  return null;
}

export const TRANSFER_NAMES = { 1: 'BT.709', 6: 'BT.601', 8: 'Linear', 14: 'BT.2020-10', 15: 'BT.2020-12', 16: 'PQ (SMPTE 2084)', 18: 'HLG' };
export const PRIMARY_NAMES = { 1: 'BT.709', 5: 'BT.601 PAL', 6: 'BT.601 NTSC', 9: 'BT.2020', 12: 'Display P3' };

export function isHdr(colour) {
  return !!colour && (colour.transfer === 16 || colour.transfer === 18);
}

/**
 * Scan an access unit for Dolby Vision RPU / enhancement-layer NALs.
 * DV carries its RPU in unspecified NAL type 62 and EL data in type 63.
 */
export function scanDolbyVisionNals(data, lengthSize = 4) {
  const { rpu, el } = scanAccessUnit(data, lengthSize);
  return { rpu, el };
}

const SEI_MASTERING_DISPLAY = 137, SEI_CONTENT_LIGHT_LEVEL = 144, SEI_USER_DATA_T35 = 4;

/**
 * Everything an access unit can say about its own dynamic range, in one pass.
 *
 * The container carries none of this. Four separate mechanisms end up here:
 *
 *   * static HDR10 mastering metadata and content light levels, as SEI 137/144
 *   * HDR10+ (SMPTE ST 2094-40) and HDR Vivid (CUVA), both hiding inside
 *     ITU-T T.35 user data (SEI 4) and told apart only by their registration
 *     codes
 *   * Dolby Vision, in NAL types 62 and 63
 *
 * The two T.35 formats are mutually exclusive in practice but nothing stops a
 * file carrying both, so both are reported rather than the first one found.
 */
export function scanAccessUnit(data, lengthSize = 4) {
  const out = { rpu: false, el: false, hdr10plus: false, hdrVivid: false,
                mastering: null, cll: null };
  let p = 0;
  while (p + lengthSize <= data.length) {
    let len = 0;
    for (let i = 0; i < lengthSize; i++) len = (len << 8) | data[p + i];
    p += lengthSize;
    if (len <= 0 || p + len > data.length) break;
    const nal = data.subarray(p, p + len);
    const nalType = (nal[0] >> 1) & 0x3f;
    if (nalType === 62) out.rpu = true;
    else if (nalType === 63) out.el = true;
    else if (nalType === 39 || nalType === 40) parseSeiNal(nal, out);
    p += len;
  }
  return out;
}

/** Walk the sei_message() list in one SEI NAL. */
function parseSeiNal(nal, out) {
  // Emulation prevention bytes have to go before any of the fixed-width reads
  // below, or a 0x000003 inside a payload shifts everything after it.
  const rbsp = unescapeRbsp(nal);
  let p = 2;   // past the two-byte NAL header
  while (p < rbsp.length) {
    let type = 0;
    while (p < rbsp.length && rbsp[p] === 0xff) { type += 255; p++; }
    if (p >= rbsp.length) return;
    type += rbsp[p++];
    let size = 0;
    while (p < rbsp.length && rbsp[p] === 0xff) { size += 255; p++; }
    if (p >= rbsp.length) return;
    size += rbsp[p++];
    if (size <= 0 || p + size > rbsp.length) return;
    readSeiPayload(type, rbsp.subarray(p, p + size), out);
    p += size;
    if (rbsp[p] === 0x80) return;   // rbsp_trailing_bits
  }
}

function readSeiPayload(type, body, out) {
  const u16 = i => (body[i] << 8) | body[i + 1];
  const u32 = i => ((body[i] << 24) | (body[i + 1] << 16) | (body[i + 2] << 8) | body[i + 3]) >>> 0;

  if (type === SEI_MASTERING_DISPLAY && body.length >= 24) {
    // Primaries are stored G, B, R -- not R, G, B. Getting that order wrong
    // produces a plausible-looking gamut that is simply the wrong one.
    out.mastering = {
      green: [u16(0), u16(2)], blue: [u16(4), u16(6)], red: [u16(8), u16(10)],
      whitePoint: [u16(12), u16(14)],
      maxLuminance: u32(16), minLuminance: u32(20),   // units of 0.0001 cd/m^2
    };
    return;
  }
  if (type === SEI_CONTENT_LIGHT_LEVEL && body.length >= 4) {
    out.cll = { maxCLL: u16(0), maxFALL: u16(2) };
    return;
  }
  if (type !== SEI_USER_DATA_T35 || body.length < 6) return;

  // ITU-T T.35: a country code, then whatever that country's registrant says.
  let i = 0;
  const country = body[i++];
  if (country === 0xff) i++;                       // extension byte
  const provider = (body[i] << 8) | body[i + 1]; i += 2;
  const oriented = (body[i] << 8) | body[i + 1]; i += 2;

  // HDR10+: US (0xB5), provider 0x003C, oriented 0x0001, application id 4.
  if (country === 0xb5 && provider === 0x003c && oriented === 0x0001 && body[i] === 4) {
    out.hdr10plus = true;
    return;
  }
  // HDR Vivid / CUVA: China (0x26), provider 0x0004, oriented 0x0005.
  if (country === 0x26 && provider === 0x0004 && oriented === 0x0005) {
    out.hdrVivid = true;
  }
}
