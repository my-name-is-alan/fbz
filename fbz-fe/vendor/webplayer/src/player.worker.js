// Demux + remux engine, running in a Web Worker.
//
// Everything that reads the file, parses Matroska, and repackages tracks into
// fMP4 now lives off the main thread. libmedia's lesson: a 4K remux burst on the
// main thread competes with compositing and the audio clock; in a worker it
// cannot. The main thread (src/player.js) keeps only the parts that MUST be
// there -- MediaSource and its SourceBuffers -- and receives finished fragments.
//
// Protocol (main -> worker):
//   open   { input }                     open source, parse header, describe
//   play   { video, audio, transcode }   track numbers to remux / forward raw
//   time   { currentTime }               drives the read-ahead throttle + seek
//   seek   { seconds }
//   sub    { track, on }                 enable/disable a subtitle track
//   dispose
// (worker -> main):
//   info { info }                        describe() result (supported=null; main fills it)
//   init { track, mime, data }           init segment for a SourceBuffer
//   fragment { track, data }             one fMP4 fragment
//   audioBlock { track, time, data }     raw block for a track the main thread transcodes
//   subtitle { track, time, duration, data }
//   eof
//   log { msg, level }

import { FileSource, HttpSource, TRACK_VIDEO, TRACK_AUDIO, TRACK_SUBTITLE } from './demux/matroska.js';
import { openDemuxer } from './demux/open.js';
import { buildRemuxer, SUBTITLE_CODECS, audioNote } from './remux/tracks.js';
import { colourFromTrack, isHdr, parseHvcC, scanAccessUnit, TRANSFER_NAMES, PRIMARY_NAMES } from './demux/hevc.js';
import { parseVp9Keyframe } from './demux/vp9.js';

const BUFFER_AHEAD = 20;           // seconds of media to keep read ahead of the playhead
const READ_CHUNK = 2 << 20;        // cluster payload consumed per demux pass

const log = (msg, level = 'info') => postMessage({ type: 'log', msg, level });

let demuxer = null;
let info = null;
let streams = [];          // { track, remuxer } for MSE-remuxed tracks
let transcodeTrack = null; // track number whose blocks are forwarded raw
const subs = new Set();    // subtitle track numbers currently enabled
let readPos = 0;
let filling = false;
let eof = false;
let alive = false;
let generation = 0;
let playhead = 0;
let fillPasses = 0;

onmessage = async (e) => {
  const m = e.data;
  try {
    if (m.type === 'open') await open(m.input);
    else if (m.type === 'play') await play(m);
    else if (m.type === 'time') { playhead = m.currentTime; if (alive) fill(); }
    else if (m.type === 'seek') await seekTo(m.seconds);
    else if (m.type === 'sub') { if (m.on) { subs.add(m.track); backfill(m.track); } else subs.delete(m.track); }
    else if (m.type === 'dispose') dispose();
  } catch (err) {
    log(`worker ${m.type}: ${err.message}`, 'error');
    // `code` travels separately: structured clone drops an Error's own
    // properties, so the main thread would otherwise see the message and nothing
    // it can branch on. src/player.js needs it to choose the native fallback.
    if (m.type === 'open' || m.type === 'play') postMessage({ type: 'error', op: m.type, message: err.message, code: err.code });
  }
};

async function open(input) {
  // Opening a new file resets the engine: bump the generation so any fill loop
  // still running against the old demuxer stops at its next check.
  generation++;
  alive = false;
  streams = [];
  transcodeTrack = null;
  subs.clear();
  eof = false;
  fillPasses = 0;

  // input: a URL string, a { url, size } object (size from Emby, so open() need
  // not read a cross-origin Content-Range), or a File/Blob for local playback.
  let source;
  if (typeof input === 'string') source = await new HttpSource(input, { log }).open();
  else if (input && typeof input.url === 'string') source = await new HttpSource(input.url, { size: input.size, log }).open();
  else source = new FileSource(input);
  // Which container it is comes from the bytes, not the extension or Emby's
  // Container field -- both lie routinely on a .strm-backed item.
  demuxer = await openDemuxer(source);
  info = await describe();
  info.container = demuxer.container;
  log(`容器：${demuxer.container} · ${info.video.length} 视频 / ${info.audio.length} 音频 / ${info.subtitles.length} 字幕轨`);
  postMessage({ type: 'info', info });
}

async function play({ video, audio, transcode }) {
  generation++;
  alive = false;
  streams = [];
  transcodeTrack = transcode ?? null;
  eof = false;

  const chosen = [];
  const byNumber = new Map(demuxer.tracks.map(t => [t.number, t]));
  if (video != null) chosen.push(byNumber.get(video));
  if (audio != null) chosen.push(byNumber.get(audio));

  for (const track of chosen) {
    if (!track) continue;
    const remuxer = buildRemuxer(track, demuxer.duration);
    remuxer.warn = m => log(m, 'warn');
    streams.push({ track, remuxer });
    send('init', { track: track.number, mime: remuxer.mime, data: remuxer.initSegment() });
  }

  readPos = await demuxer.seekTo(0, chosen.find(Boolean).number);
  alive = true;
  fill();
}

// --- the fill loop, read-ahead throttled -----------------------------------

/**
 * How far ahead of the playhead we have already READ, from the video remuxer's
 * last emitted timestamp. This is the LEADING edge; unlike video.buffered it
 * does not lag behind async appends, so the loop cannot run away and read the
 * whole file (which on the main-thread version quota-evicted the playhead region
 * and froze playback). See src/player.js history.
 */
function readAheadSeconds() {
  const s = streams.find(x => x.remuxer.kind === 'video') ?? streams[0];
  const r = s?.remuxer;
  if (!r || r.lastDts == null) return 0;
  return r.lastDts / r.timescale - playhead;
}

async function fill() {
  if (filling || eof || !demuxer) return;
  const gen = generation;
  filling = true;
  try {
    while (gen === generation) {
      if (readAheadSeconds() > BUFFER_AHEAD) break;

      const wanted = new Map(streams.map(s => [s.track.number, s]));
      const state = {};
      let got = 0;
      for await (const block of demuxer.readBlocks(readPos, READ_CHUNK, state)) {
        if (gen !== generation) return;
        const s = wanted.get(block.track);
        if (s) s.remuxer.push(block);
        // block.data is a subarray of the shared read chunk, so it must be
        // COPIED before transfer -- transferring its buffer would neuter every
        // other block in the same chunk. Fragments (own buffers) transfer fine.
        else if (block.track === transcodeTrack) send('audioBlock', { track: block.track, time: block.time, data: block.data.slice() });
        else if (subs.has(block.track)) emitSubtitle(block);
        got++;
      }
      readPos = state.nextPos;

      for (const s of streams) {
        const frag = s.remuxer.flush(state.atEnd);   // force-drain the tail GOP at EOF
        if (frag) send('fragment', { track: s.track.number, data: frag });
      }
      if (fillPasses++ < 3) {
        const vr = streams.find(s => s.remuxer.kind === 'video')?.remuxer;
        log(`fill p${fillPasses}: ${got} blk, pos->${readPos}, vidDts=${vr ? (vr.lastDts / vr.timescale).toFixed(1) : '?'}s, ahead=${readAheadSeconds().toFixed(1)}s`);
      }

      // Blocks whose content encoding could not be undone are dropped inside
      // the demuxer. Say so once per track: a compressed subtitle track that
      // fails to inflate otherwise just renders nothing, with no way to tell
      // that apart from a track that genuinely has no cues here.
      if (state.decodeError) log(`轨道内容解压失败，相关数据包已丢弃：${state.decodeError}`, 'warn');
      if (state.parseError) { log(`demux stopped: ${state.parseError}`, 'warn'); break; }
      if (state.atEnd) { eof = true; postMessage({ type: 'eof' }); break; }
      if (!got) { log(`no blocks at byte ${readPos}; stopping fill`, 'warn'); break; }
      await new Promise(r => setTimeout(r, 0));
    }
  } catch (err) {
    log(`fill error: ${err.message}`, 'error');
    // Until now this was logged and nothing else, so a read that died MID-
    // PLAYBACK just stopped the loop and the picture froze with no explanation.
    // A direct object-storage link makes that a routine event rather than a
    // freak one: a presigned URL expires on a timer (commonly an hour), so a
    // long film outlives its own source. Report it, and the page can re-open on
    // another candidate at the same position.
    postMessage({ type: 'stalled', message: err.message, position: playhead });
  } finally {
    filling = false;
  }
}

async function seekTo(seconds) {
  if (!alive || !streams.length) return;
  generation++;
  const primary = streams[0].track;
  readPos = await demuxer.seekTo(seconds, primary.number);
  eof = false;
  // Move the playhead the throttle reads AND reset each remuxer's lastDts. The
  // read-ahead throttle is `lastDts - playhead`; on a BACKWARD seek the stale
  // (higher) lastDts minus the new (lower) playhead is a big positive number,
  // so fill() thinks it is already 20s ahead and reads nothing -- while the main
  // thread has just cleared the buffer, so playback freezes. Forward seeks make
  // it negative and read fine, which is why only rewind hung.
  playhead = seconds;
  for (const s of streams) { s.remuxer.pending.length = 0; s.remuxer.lastDts = null; }
  postMessage({ type: 'flush', seconds });   // main drops its queues + buffered island
  while (filling) await new Promise(r => setTimeout(r, 10));
  fill();
}

// --- subtitles -------------------------------------------------------------

function emitSubtitle(block) {
  // Copy: block.data is a view into the shared read chunk (see fill()).
  send('subtitle', { track: block.track, time: block.time, duration: block.duration || 0, data: block.data.slice() });
}

/** Re-read the buffered region for a newly enabled subtitle track. */
async function backfill(track) {
  if (!alive) return;
  const from = Math.max(0, playhead - 2);
  const ahead = Math.max(0, readAheadSeconds());
  const gen = generation;
  let pos = await demuxer.seekTo(from, track);
  const until = from + ahead + 2;
  let budget = 24;
  while (budget-- > 0 && gen === generation) {
    const state = {};
    let past = false;
    for await (const block of demuxer.readBlocks(pos, 2 << 20, state)) {
      if (gen !== generation) return;
      if (block.track !== track) continue;
      if (block.time > until) { past = true; break; }
      if (block.time + (block.duration || 0) < from) continue;
      emitSubtitle(block);
    }
    if (past || state.atEnd || state.parseError || state.nextPos <= pos) break;
    pos = state.nextPos;
  }
}

function dispose() {
  alive = false;
  generation++;
  streams = [];
  subs.clear();
}

// --- helpers ---------------------------------------------------------------

/** postMessage with the payload's ArrayBuffer transferred (zero-copy). */
function send(type, payload) {
  const buf = payload.data?.buffer;
  postMessage({ type, ...payload }, buf ? [buf] : []);
}

/**
 * Human-readable summary of the file. Identical to the old Player._describe,
 * moved here because it needs the demuxer. `supported` is left null: only the
 * main thread has MediaSource.isTypeSupported, so it fills that in per mime.
 */
async function describe() {
  const dx = demuxer;
  const out = { name: dx.src.name, duration: dx.duration, size: dx.src.size,
                video: [], audio: [], subtitles: [], attachments: dx.attachments.length,
                fonts: 0, hdr: null, dolbyVision: null, dynamicHdr: null,
                hdr10plus: false, hdrVivid: false, mastering: null, cll: null };

  for (const t of dx.tracks) {
    if (t.codecId === 'V_VP9' && !t.vp9) t.vp9 = await probeVp9(t);
  }

  for (const t of dx.tracks) {
    if (t.type === TRACK_VIDEO) {
      const colour = colourFromTrack(t);
      const rx = buildRemuxer(t, dx.duration);
      out.video.push({ track: t, colour, mime: rx?.mime, supported: null,
                       label: `${t.video.width}x${t.video.height} ${t.codecId.split('/').pop()}` });
      if (isHdr(colour)) {
        out.hdr = { transfer: TRANSFER_NAMES[colour.transfer] ?? colour.transfer,
                    primaries: PRIMARY_NAMES[colour.primaries] ?? colour.primaries,
                    bitDepth: colour.bitDepth };
      }
    } else if (t.type === TRACK_AUDIO) {
      const rx = buildRemuxer(t, dx.duration);
      out.audio.push({ track: t, mime: rx?.mime, supported: null,
                       note: audioNote(t, t.audio.channels),
                       label: `${t.codecId.replace('A_', '')} ${t.audio.channels}ch ${Math.round(t.audio.sampleRate / 1000)}kHz`
                              + (t.name ? ` · ${t.name}` : '') + ` [${t.language}]` });
    } else if (t.type === TRACK_SUBTITLE) {
      out.subtitles.push({ track: t, format: SUBTITLE_CODECS[t.codecId] ?? t.codecId,
                           label: `${SUBTITLE_CODECS[t.codecId] ?? t.codecId}${t.name ? ` · ${t.name}` : ''} [${t.language}]` });
    }
  }

  // Fonts are needed by the ASS renderer on the main thread; carry the bytes
  // across with the info so fontAttachments() has them without a round trip.
  const fontFiles = dx.attachments.filter(f => /font|sfnt/i.test(f.mime) || /\.(ttf|otf|ttc)$/i.test(f.name));
  out.fonts = fontFiles.length;
  out._fontData = fontFiles.map(f => ({ name: f.name, mime: f.mime, data: f.data }));

  const v = out.video[0]?.track;
  if (v?.codecId === 'V_MPEGH/ISO/HEVC') {
    const lenSize = (parseHvcC(v.codecPrivate)?.lengthSizeMinusOne ?? 3) + 1;
    let rpu = false, el = false, n = 0;
    for await (const b of dx.readBlocks(await dx.seekTo(0, v.number), 2 << 20)) {
      if (b.track !== v.number) continue;
      const r = scanAccessUnit(b.data, lenSize);
      rpu ||= r.rpu; el ||= r.el;
      out.hdr10plus ||= r.hdr10plus;
      out.hdrVivid ||= r.hdrVivid;
      v.mastering ??= r.mastering;
      v.cll ??= r.cll;
      if (++n > 30) break;
    }
    out.mastering = v.mastering ?? null;
    out.cll = v.cll ?? null;
    if (out.hdr10plus) out.dynamicHdr = { format: 'HDR10+ (SMPTE ST 2094-40)', note: 'Detected and preserved in the stream, but Chromium tone-maps from the static HDR10 metadata only.' };
    else if (out.hdrVivid) out.dynamicHdr = { format: 'HDR Vivid / CUVA (T/UWA 005)', note: 'Detected. No browser implements CUVA tone-mapping; the base layer plays as HDR10.' };
    if (rpu) out.dolbyVision = { el, profile: el ? '7 (dual layer)' : '8.x (single layer)', playable: !el,
      note: el ? 'Dual-layer: the enhancement layer cannot be decoded in a browser; base layer plays as HDR10.'
               : 'Single layer with an HDR10-compatible base layer — plays as HDR10 with the RPU ignored.' };
  }
  return out;
}

async function probeVp9(track) {
  let n = 0;
  for await (const b of demuxer.readBlocks(await demuxer.seekTo(0, track.number), 2 << 20)) {
    if (b.track !== track.number) continue;
    const cfg = parseVp9Keyframe(b.data);
    if (cfg) return cfg;
    if (++n > 60) break;
  }
  return null;
}
