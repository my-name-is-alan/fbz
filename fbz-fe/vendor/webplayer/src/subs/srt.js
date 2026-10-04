// SRT -> WebVTT.
//
// There is no renderer here on purpose: <track kind="subtitles"> already draws
// WebVTT, positions it, respects the user's caption styling, and keeps working
// in fullscreen and Picture-in-Picture. Converting is a few lines; drawing text
// ourselves would be a few hundred and worse.
//
// The one thing that needs real work is that SRT in the wild carries ASS
// override tags. houshisrt.srt uses {\an8} to lift lines off the bottom, and a
// converter that passes those through renders the literal text "{\an8}" on
// screen -- so they are translated into VTT cue settings instead of dropped.

/** ASS \anN -> WebVTT cue settings. 1-3 bottom, 4-6 middle, 7-9 top. */
const ALIGN = { 1: 'start', 2: 'center', 3: 'end', 4: 'start', 5: 'center', 6: 'end',
                7: 'start', 8: 'center', 9: 'end' };
const LINE  = { 1: null, 2: null, 3: null, 4: '50%', 5: '50%', 6: '50%',
                7: '5%', 8: '5%', 9: '5%' };

const TIME = /(\d+):(\d{2}):(\d{2})[,.](\d{1,3})\s*-->\s*(\d+):(\d{2}):(\d{2})[,.](\d{1,3})/;

const stamp = (h, m, s, ms) =>
  `${String(h).padStart(2, '0')}:${m}:${s}.${String(ms).padEnd(3, '0')}`;

/**
 * Text is inserted into a VTT body, where `&` and `<` start entities and tags.
 * SRT's own <i>/<b>/<u> are valid VTT and are kept; anything else that looks
 * like a tag is escaped so it shows up as the characters the author wrote.
 */
function escapeText(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/<(?!\/?[ibu]>)/g, '&lt;')
    .replace(/&amp;(lt|gt|amp|nbsp|#\d+);/g, '&$1;');
}

/** Convert an SRT document to a WebVTT document. */
export function srtToVtt(srt) {
  // Strip a UTF-8 BOM and normalise line endings; SRT is CRLF more often than not.
  const text = srt.replace(/^﻿/, '').replace(/\r\n?/g, '\n');
  const out = ['WEBVTT', ''];
  let cues = 0;

  for (const block of text.split(/\n{2,}/)) {
    const lines = block.split('\n').filter(l => l.trim() !== '');
    if (!lines.length) continue;
    // The counter line is optional in practice; find the timing line instead.
    const ti = lines.findIndex(l => TIME.test(l));
    if (ti < 0) continue;
    const m = lines[ti].match(TIME);
    let body = lines.slice(ti + 1).join('\n');
    if (!body.trim()) continue;

    // Positioning override, wherever in the cue it appears.
    let settings = '';
    const an = body.match(/\{\\an?([1-9])\}/);
    if (an) {
      const n = Number(an[1]);
      const parts = [];
      if (ALIGN[n] && ALIGN[n] !== 'center') parts.push(`align:${ALIGN[n]}`);
      if (LINE[n]) parts.push(`line:${LINE[n]}`);
      if (parts.length) settings = ' ' + parts.join(' ');
    }
    // Remaining ASS override blocks have no VTT equivalent; showing them as
    // text is strictly worse than showing the line without the effect.
    body = body.replace(/\{[^}]*\}/g, '').trim();
    if (!body) continue;

    out.push(`${stamp(m[1], m[2], m[3], m[4])} --> ${stamp(m[5], m[6], m[7], m[8])}${settings}`);
    out.push(escapeText(body), '');
    cues++;
  }
  if (!cues) throw new Error('no cues found — is this an SRT file?');
  return out.join('\n');
}

/**
 * Attach an SRT file to a <video> as a native text track.
 * Returns the <track> element so the caller can remove it again.
 */
export function attachSrt(video, srt, { label = 'SRT', language = 'und', asDefault = true } = {}) {
  const url = URL.createObjectURL(new Blob([srtToVtt(srt)], { type: 'text/vtt' }));
  const el = document.createElement('track');
  el.kind = 'subtitles';
  el.label = label;
  el.srclang = language;
  el.src = url;
  el.addEventListener('load', () => URL.revokeObjectURL(url), { once: true });
  video.appendChild(el);
  // The mode has to be set after the element is in the document, and `default`
  // alone does not show the track when it is added to a video already playing.
  if (asDefault) el.track.mode = 'showing';
  return el;
}
