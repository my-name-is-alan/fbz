// Picks a renderer for a subtitle track and keeps it fed from the player.
//
// Three formats, three completely different mechanisms, one switch:
//
//   SRT   -> converted to WebVTT and handed to the browser's own <track>.
//   ASS   -> JASSUB (libass in wasm), events pushed one at a time.
//   PGS   -> libpgs, fed a .sup stream rebuilt from the Matroska packets.
//
// The player streams subtitle packets from the same sequential read as video,
// so everything here is incremental: renderers are created once and then
// receive packets as the buffer advances.

import { AssOverlay } from './ass.js';
import { PgsFeed } from './pgs.js';
import { TextCues } from './text.js';

export { srtToVtt, attachSrt } from './srt.js';

export class Subtitles {
  /**
   * @param player a loaded Player
   * @param vendor URL prefix for the bundled renderers (public/vendor)
   */
  constructor(player, { vendor, log = () => {} } = {}) {
    this.player = player;
    this.video = player.video;
    // Resolved against the page, not this module: a dynamic import() inside
    // src/subs/ would otherwise look for src/subs/vendor/, and the worker and
    // wasm URLs are handed to other realms where a relative path means
    // something different again.
    this.vendor = new URL(vendor ?? 'vendor/', document.baseURI).href.replace(/\/$/, '');
    this.log = log;
    this.active = null;      // { index, format, renderer }
    this._index = -1;
    player.onSubtitlePacket = (block, entry) => this._onPacket(block, entry);
  }

  /** Formats with a renderer wired up. */
  static SUPPORTED = new Set(['ass', 'ssa', 'pgs', 'srt', 'vtt']);

  get current() { return this._index; }

  /**
   * Turn on the subtitle track at `index` (an index into player.info.subtitles).
   * Pass -1 to turn subtitles off.
   */
  async select(index) {
    await this._teardown();
    if (index < 0) { this._index = -1; return null; }

    const entry = this.player.info.subtitles[index];
    if (!entry) throw new Error(`no subtitle track at index ${index}`);
    const format = entry.format;
    if (!Subtitles.SUPPORTED.has(format)) {
      this.log(`subtitle format "${format}" has no renderer yet`, 'warn');
      return null;
    }

    this._index = index;

    // The sink is installed BEFORE the track is enabled. enableSubtitle()
    // backfills the already-buffered region immediately, and both renderers
    // queue what arrives before they finish booting -- but only if there is
    // something for _onPacket to hand the packets to.
    this.active = format === 'pgs' ? this._startPgs()
                : format === 'srt' || format === 'vtt' ? this._startText(entry)
                : this._startAss(entry);
    this.active.index = index;
    this.active.format = format;

    this.player.enableSubtitle(index);
    await this.active.ready;
    return this.active;
  }

  _startAss(entry) {
    const fonts = this.player.fontAttachments().map(f => f.data);
    const header = entry.track.codecPrivate;
    if (!header?.length) throw new Error('ASS track has no CodecPrivate — cannot build a script');
    // AssOverlay accepts packets straight away and replays them once libass
    // has parsed the header, so nothing needs queueing at this level.
    const renderer = new AssOverlay(this.video, header, fonts,
      { vendor: this.vendor, log: this.log });
    return { renderer, fonts: fonts.length, ready: renderer.ready };
  }

  /** SRT in an MKV, tx3g/wvtt in an mp4: one cue per packet, no wasm involved. */
  _startText(entry) {
    const renderer = new TextCues(this.video, {
      label: entry.label || entry.format?.toUpperCase() || '字幕',
      language: entry.track?.language || 'und',
      log: this.log,
    });
    return { renderer, ready: renderer.ready };
  }

  _startPgs() {
    let renderer = null;
    // libpgs re-parses the whole .sup and re-decodes every bitmap in it on
    // each load, so the feed keeps only a window around the playhead and
    // batches changes. Until the renderer exists it just accumulates.
    const feed = new PgsFeed(sup => {
      if (!renderer) return;
      renderer.loadFromBuffer(sup.buffer.slice(sup.byteOffset, sup.byteOffset + sup.length));
      this.log(`PGS: ${feed.packets.length} display sets loaded (${(sup.length / 1024) | 0} KB)`);
    });
    // The window can only follow the playhead if something tells it where the
    // playhead is. timeupdate fires about four times a second, which is far
    // more often than the window boundary moves.
    const onTime = () => feed.setTime(this.video.currentTime);
    this.video.addEventListener('timeupdate', onTime);
    this.video.addEventListener('seeked', onTime);
    const entry = { feed, renderer: null, detach: () => {
      this.video.removeEventListener('timeupdate', onTime);
      this.video.removeEventListener('seeked', onTime);
    } };
    entry.ready = (async () => {
      const { PgsRenderer } = await import(`${this.vendor}/libpgs.js`);
      renderer = new PgsRenderer({
        video: this.video,
        workerUrl: `${this.vendor}/libpgs.worker.js`,
        // The PGS canvas is authored at the disc's resolution -- 1080p even on
        // a 4K remux -- and libpgs scales it to the element. 'contain' matches
        // a <video> with the default object-fit.
        aspectRatio: 'contain',
      });
      entry.renderer = renderer;
      feed.flush();   // draw whatever arrived while the worker was starting
    })();
    return entry;
  }

  _onPacket(block, entry) {
    const a = this.active;
    if (!a || entry.track.number !== this.player.info.subtitles[a.index]?.track.number) return;
    if (a.feed) a.feed.push(block);
    else a.renderer.push(block);
  }

  async _teardown() {
    if (this._index >= 0) this.player.disableSubtitle(this._index);
    const a = this.active;
    this.active = null;
    if (!a) return;
    try {
      await a.ready.catch(() => {});   // never dispose a half-built renderer
      if (a.feed) { a.detach?.(); a.feed.reset(); a.renderer?.dispose(); }
      else await a.renderer.destroy();
    } catch (e) { this.log(`subtitle teardown: ${e.message}`, 'warn'); }
  }

  /** Number of subtitles handed to the renderer so far — used by the tests. */
  get delivered() {
    const a = this.active;
    if (!a) return 0;
    return a.feed ? a.feed.packets.length : a.renderer.eventCount;
  }

  destroy() { return this._teardown(); }
}
