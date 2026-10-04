// Embedded TEXT subtitle tracks -> the browser's own caption renderer.
//
// SRT inside an MKV, and tx3g/WebVTT inside an mp4, are all the same thing once
// demuxed: one packet per cue, carrying UTF-8 and a duration. Until now the
// player demuxed them and then dropped them ("no renderer yet") -- which is why
// an MKV whose only subtitle track was SRT appeared to have no subtitles at all.
//
// There is deliberately no drawing code here. <track> already lays out, styles
// and positions captions, honours the viewer's OS caption settings, and keeps
// working in fullscreen and Picture-in-Picture. Cues are just handed to it as
// they stream in.

/** ASS \anN, which turns up inside SRT far more often than it should. */
const ALIGN = { 1: 'start', 2: 'center', 3: 'end', 4: 'start', 5: 'center', 6: 'end',
                7: 'start', 8: 'center', 9: 'end' };
const LINE = { 4: '50%', 5: '50%', 6: '50%', 7: '5%', 8: '5%', 9: '5%' };

const DEFAULT_CUE = 3;      // seconds, when the container states no duration

export class TextCues {
  constructor(video, { label = '字幕', language = 'und', log = () => {} } = {}) {
    this.video = video;
    this.log = log;
    this.eventCount = 0;
    this.ready = Promise.resolve();
    this._seen = new Set();
    this._decoder = new TextDecoder('utf-8');

    // addTextTrack rather than a <track> element: a <track> only accepts cues
    // after its (empty) source has loaded, so early packets race the load and
    // vanish. The track cannot be removed afterwards, so exactly one is created
    // per element and reused -- switching subtitle tracks reuses this one.
    this.track = video._linwebTextTrack ??= video.addTextTrack('subtitles', label, language);
    this.track.mode = 'showing';
    this._clear();
  }

  push(block) {
    let text = this._decoder.decode(block.data).replace(/\0+$/, '').trim();
    if (!text) return;

    // Positioning override, wherever in the cue it appears; then drop the
    // remaining override blocks, because showing "{\fad(200,200)}" as text is
    // strictly worse than showing the line without the effect.
    const an = text.match(/\{\\an?([1-9])\}/);
    text = text.replace(/\{[^}]*\}/g, '').trim();
    if (!text) return;

    const start = Math.max(0, block.time);
    const end = start + (block.duration > 0 ? block.duration : DEFAULT_CUE);
    // Enabling a track backfills the already-buffered region, and a seek
    // re-reads it, so the same cue arrives more than once by design.
    const key = `${start.toFixed(3)}|${text}`;
    if (this._seen.has(key)) return;
    this._seen.add(key);

    let cue;
    try { cue = new VTTCue(start, end, text); }
    catch (e) { this.log(`subtitle cue rejected at ${start.toFixed(2)}s: ${e.message}`, 'warn'); return; }
    if (an) {
      const n = Number(an[1]);
      if (ALIGN[n] && ALIGN[n] !== 'center') cue.align = ALIGN[n];
      if (LINE[n]) { cue.line = parseInt(LINE[n], 10); cue.snapToLines = false; }
    }
    this.track.addCue(cue);
    this.eventCount++;
  }

  _clear() {
    // cues is live; removing while iterating skips half of them.
    for (const c of [...(this.track.cues ?? [])]) { try { this.track.removeCue(c); } catch {} }
    this._seen.clear();
    this.eventCount = 0;
  }

  async destroy() {
    this._clear();
    this.track.mode = 'disabled';
  }
}
