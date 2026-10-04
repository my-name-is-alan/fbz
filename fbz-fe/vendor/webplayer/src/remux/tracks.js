// Maps Matroska codec IDs onto MP4 sample entries + MSE codec strings.
// Anything not listed here has no repackaging path and must fall back to a
// software decoder (or be dropped).
import { TrackRemuxer, box, fullBox, u32, u16, bytes, concat, visualSampleEntry, audioSampleEntry, esdsBox, FOURCC } from './mp4.js';
import { hevcCodecString, colourFromTrack, parseHvcC } from '../demux/hevc.js';
import { vp9Level, vp9CodecString, vpcCPayload } from '../demux/vp9.js';

/** Decode an AVC configuration record enough to build "avc1.PPCCLL". */
function avcCodecString(cp) {
  if (!cp || cp.length < 4) return 'avc1';
  return `avc1.${[cp[1], cp[2], cp[3]].map(b => b.toString(16).padStart(2, '0')).join('')}`;
}

function av1CodecString(cp) {
  // av1C: marker/version, seq_profile(3)|seq_level_idx(5), high_bitdepth etc.
  if (!cp || cp.length < 4) return 'av01.0.00M.08';
  const profile = cp[1] >> 5;
  const level = cp[1] & 0x1f;
  const tier = (cp[2] >> 7) & 1;
  const highBitdepth = (cp[2] >> 6) & 1;
  const twelveBit = (cp[2] >> 5) & 1;
  const depth = twelveBit ? 12 : highBitdepth ? 10 : 8;
  return `av01.${profile}.${String(level).padStart(2, '0')}${tier ? 'H' : 'M'}.${String(depth).padStart(2, '0')}`;
}

export function buildRemuxer(track, duration) {
  const id = track.codecId;
  const colour = track.type === 1 ? colourFromTrack(track) : null;

  // ---- video ----
  if (id === 'V_MPEGH/ISO/HEVC') {
    const cfg = parseHvcC(track.codecPrivate);
    if (!cfg) return null;
    // hvc1 (not hev1): parameter sets live in the sample entry, out of band.
    const codecString = hevcCodecString(track.codecPrivate, 'hvc1');
    const entry = visualSampleEntry('hvc1', track, colour, [box('hvcC', track.codecPrivate)]);
    return new TrackRemuxer(track, { kind: 'video', codecString, sampleEntry: entry, duration, colour,
                                     nalLengthSize: (cfg.lengthSizeMinusOne & 3) + 1 });
  }
  if (id === 'V_MPEG4/ISO/AVC') {
    const entry = visualSampleEntry('avc1', track, colour, [box('avcC', track.codecPrivate)]);
    // avcC byte 4 is lengthSizeMinusOne; 4-byte prefixes are usual but not required.
    const nalLengthSize = track.codecPrivate?.length > 4 ? (track.codecPrivate[4] & 3) + 1 : 4;
    return new TrackRemuxer(track, { kind: 'video', codecString: avcCodecString(track.codecPrivate),
                                     sampleEntry: entry, duration, colour, nalLengthSize });
  }
  if (id === 'V_AV1') {
    const entry = visualSampleEntry('av01', track, colour, [box('av1C', track.codecPrivate)]);
    return new TrackRemuxer(track, { kind: 'video', codecString: av1CodecString(track.codecPrivate), sampleEntry: entry, duration, colour });
  }
  if (id === 'V_VP9') {
    // Matroska carries no CodecPrivate for VP9, so the profile and bit depth
    // come from a keyframe header that the caller parsed and hung on the track
    // (see Player._describe). Without it there is nothing to derive them from
    // and a guess would mislabel the stream, so refuse instead.
    const cfg = track.vp9;
    if (!cfg) return null;
    const fps = track.defaultDuration ? 1e9 / track.defaultDuration : 30;
    const level = vp9Level(cfg.width, cfg.height, fps);
    // vpcC is a FullBox: the four version/flags bytes are part of it, and
    // writing the record as a plain box shifts every field by four.
    const entry = visualSampleEntry('vp09', track, colour,
      [fullBox('vpcC', 1, 0, vpcCPayload(cfg, level, colour))]);
    return new TrackRemuxer(track, { kind: 'video', codecString: vp9CodecString(cfg, level),
                                     sampleEntry: entry, duration, colour });
  }

  // ---- audio ----
  const a = track.audio ?? { sampleRate: 48000, channels: 2 };
  const rate = a.outputSampleRate || a.sampleRate;

  if (id === 'A_AAC') {
    const entry = audioSampleEntry('mp4a', a.channels, rate, esdsBox(track.codecPrivate ?? new Uint8Array([0x12, 0x10])));
    // profile byte 0 >> 3 gives the AudioObjectType (2 = AAC-LC)
    const aot = track.codecPrivate?.length ? (track.codecPrivate[0] >> 3) : 2;
    return new TrackRemuxer(track, { kind: 'audio', codecString: `mp4a.40.${aot || 2}`, sampleEntry: entry, duration, sampleRate: rate });
  }
  if (id === 'A_MPEG/L3') {
    // MP3 is the audio track on a large slice of older MKV rips and on nearly
    // every legacy FLV, and it was reported "unhandled" purely because nothing
    // had written the four lines. objectTypeIndication 0x6b = MPEG-1 Layer 3;
    // there is no DecoderSpecificInfo, the frame headers carry everything.
    const entry = audioSampleEntry('mp4a', a.channels, rate, esdsBox(null, 0x6b));
    return new TrackRemuxer(track, { kind: 'audio', codecString: 'mp4a.6B', sampleEntry: entry, duration, sampleRate: rate });
  }
  if (id === 'A_FLAC') {
    // dfLa wraps the raw STREAMINFO metadata block Matroska stores in CodecPrivate.
    const streamInfo = stripFlacHeader(track.codecPrivate);
    const entry = audioSampleEntry('fLaC', a.channels, rate, fullBox('dfLa', 0, 0, streamInfo));
    return new TrackRemuxer(track, { kind: 'audio', codecString: 'flac', sampleEntry: entry, duration, sampleRate: rate });
  }
  if (id === 'A_OPUS') {
    const dOps = dOpsFromOpusHead(track.codecPrivate);
    if (!dOps) return null;
    const entry = audioSampleEntry('Opus', a.channels, rate, box('dOps', dOps));
    return new TrackRemuxer(track, { kind: 'audio', codecString: 'opus', sampleEntry: entry, duration, sampleRate: rate });
  }
  if (id === 'A_AC3' || id === 'A_EAC3') {
    // Emitted for completeness; probe says Edge/Chrome reject ec-3 in MSE, so
    // the player is expected to route these to the software decoder instead.
    const fourcc = id === 'A_AC3' ? 'ac-3' : 'ec-3';
    const entry = audioSampleEntry(fourcc, a.channels, rate);
    return new TrackRemuxer(track, { kind: 'audio', codecString: fourcc, sampleEntry: entry, duration, sampleRate: rate });
  }

  return null;
}

/**
 * Convert Matroska's OpusHead into an MP4 OpusSpecificBox payload.
 *
 * These carry the same fields and are NOT interchangeable: OpusHead is
 * little-endian and starts at version 1, while dOps is big-endian and must be
 * version 0. Copying the bytes across -- which is what stripping the 8-byte
 * "OpusHead" magic amounts to -- turns a pre-skip of 312 into 14337 and a
 * sample rate of 48000 into 2159738880.
 *
 * ffmpeg reads the result without complaining. Chrome rejects it by silently
 * detaching the MediaSource, with no error on the element and no exception
 * anywhere, which is why this survived a README claiming Opus support.
 */
function dOpsFromOpusHead(cp) {
  if (!cp || cp.length < 19) return null;
  const head = cp[0] === 0x4f && cp[1] === 0x70 && cp[2] === 0x75 && cp[3] === 0x73;   // "Opus"
  const body = head ? cp.subarray(8) : cp;
  if (body.length < 11) return null;
  const le = new DataView(body.buffer, body.byteOffset, body.byteLength);

  const channels = body[1];
  const preSkip = le.getUint16(2, true);
  const inputRate = le.getUint32(4, true);
  const gain = le.getInt16(8, true);
  const family = body[10];

  const mappingLen = family === 0 ? 0 : 2 + channels;
  const out = new Uint8Array(11 + mappingLen);
  const be = new DataView(out.buffer);
  out[0] = 0;                       // dOps Version is 0, not OpusHead's 1
  out[1] = channels;
  be.setUint16(2, preSkip, false);
  be.setUint32(4, inputRate, false);
  be.setInt16(8, gain, false);
  out[10] = family;
  // Channel mapping is a byte table, so it carries across unchanged.
  if (mappingLen) out.set(body.subarray(11, 11 + mappingLen), 11);
  return out;
}

/** Matroska sometimes stores the full "fLaC" stream header; MP4 wants just the blocks. */
function stripFlacHeader(cp) {
  if (!cp) return new Uint8Array(0);
  const isMagic = cp[0] === 0x66 && cp[1] === 0x4c && cp[2] === 0x61 && cp[3] === 0x43;
  return isMagic ? cp.subarray(4) : cp;
}

/**
 * Why an audio track will not play, in terms of what would fix it.
 *
 * "unsupported" on its own is useless to anyone reading the UI: it does not
 * distinguish a codec no browser will ever take from one that merely needs a
 * decoder wired up, and it hides the fact that a 5.1 track will be downmixed
 * to stereo regardless of which route it takes.
 */
export const AUDIO_NOTES = {
  A_DTS: {
    name: 'DTS',
    reason: 'No browser ships a DTS decoder, and DTS is patent-encumbered.',
    route: 'Needs a wasm decoder, then re-encoding to Opus for MSE.',
  },
  A_TRUEHD: {
    name: 'Dolby TrueHD',
    reason: 'Lossless MLP. No browser decodes it; it is a Blu-ray format.',
    route: 'Needs a wasm decoder. Most discs also carry an AC-3 core track — prefer that if present.',
  },
  A_MLP: {
    name: 'MLP',
    reason: 'The lossless packing TrueHD is built on. Not decoded by any browser.',
    route: 'Needs a wasm decoder.',
  },
  A_EAC3: {
    name: 'Dolby Digital Plus (E-AC3)',
    reason: 'MSE and WebCodecs both reject ec-3 here; support depends on an OS decoder that this platform does not expose.',
    route: 'Needs a wasm decoder, then re-encoding to Opus for MSE.',
  },
  A_AC3: {
    name: 'Dolby Digital (AC-3)',
    reason: 'MSE and WebCodecs both reject ac-3 here.',
    route: 'Needs a wasm decoder, then re-encoding to Opus for MSE.',
  },
  A_DTS_HD: { name: 'DTS-HD', reason: 'Extension of DTS; no browser decodes it.', route: 'Needs a wasm decoder.' },
  // Not "unhandled": mp4a.6B is built and Chromium plays it. Firefox refuses
  // the codec, so the note has to blame the engine rather than the repackager.
  'A_MPEG/L3': {
    name: 'MP3',
    reason: 'This browser rejects mp4a.6B in MSE (Firefox does; Chromium does not).',
    route: 'Decoded in software and re-encoded to Opus.',
  },
};
AUDIO_NOTES['A_MPEG/L2'] = AUDIO_NOTES['A_MPEG/L1'] = AUDIO_NOTES['A_MPEG/L3'];

/** What is known about an audio track the player cannot hand to MSE. */
export function audioNote(track, channels) {
  const n = AUDIO_NOTES[track.codecId]
    ?? { name: track.codecId.replace(/^A_/, ''), reason: 'No repackaging path for this codec.', route: 'Unhandled.' };
  return channels > 2
    ? { ...n, downmix: `${channels} channels will be downmixed to stereo — WebAudio reports maxChannelCount 2.` }
    : n;
}

export const SUBTITLE_CODECS = {
  'S_TEXT/ASS': 'ass', 'S_TEXT/SSA': 'ssa',
  'S_TEXT/UTF8': 'srt', 'S_TEXT/WEBVTT': 'vtt',
  'S_HDMV/PGS': 'pgs', 'S_VOBSUB': 'vobsub', 'S_DVBSUB': 'dvbsub',
};

export { u32, u16, concat, FOURCC };
