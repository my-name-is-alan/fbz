// Effect-heavy ASS subtitles, rendered by JASSUB (libass compiled to wasm).
//
// Nothing here reimplements ASS. libass is the only complete implementation of
// the format and the effects in these files -- \clip, \fad, \t, per-character
// karaoke -- are exactly the parts a hand-written renderer gets wrong. The work
// in this file is feeding it correctly:
//
//   * Matroska stores the [Script Info] and [V4+ Styles] sections in
//     CodecPrivate and each dialogue line in its own block, WITHOUT the Start
//     and End fields -- those come from the block's timestamp and duration.
//     The line format is "ReadOrder,Layer,Style,Name,MarginL,MarginR,MarginV,
//     Effect,Text", which is not the field order of a Dialogue: line.
//
//   * Events are pushed one at a time rather than by rebuilding the script and
//     calling setTrack(). The player streams packets continuously as it
//     buffers, and setTrack() reloads the whole track -- which resets libass
//     state mid-playback.
//
//   * ASSEvent.Style is an INDEX into libass's style table, not a name, so the
//     style names in the packets have to be resolved against getStyles().
//
// Licence note: JASSUB is LGPL-2.1-or-later (it embeds libass), unlike the
// MIT-licensed code in this repository. It is loaded as a separate bundle and
// used through its public API only, so it stays a replaceable component.

/** Fields of a Matroska ASS block, in order. Start/End are deliberately absent. */
const FIELDS = 8;   // ReadOrder, Layer, Style, Name, MarginL, MarginR, MarginV, then Text

/** Tallest subtitle raster libass is asked for, whatever the display. */
export const ASS_MAX_RENDER_HEIGHT = 1080;

/**
 * Split one Matroska ASS packet.
 * Text may contain commas, so only the first 8 fields are split off.
 */
export function parseAssPacket(text) {
  const parts = [];
  let at = 0;
  for (let i = 0; i < FIELDS; i++) {
    const comma = text.indexOf(',', at);
    if (comma < 0) return null;
    parts.push(text.slice(at, comma));
    at = comma + 1;
  }
  return {
    readOrder: Number(parts[0]) || 0,
    layer: Number(parts[1]) || 0,
    style: parts[2],
    name: parts[3],
    marginL: Number(parts[4]) || 0,
    marginR: Number(parts[5]) || 0,
    marginV: Number(parts[6]) || 0,
    effect: parts[7],
    text: text.slice(at),
  };
}

export class AssOverlay {
  /**
   * @param video    the <video> to render over
   * @param header   CodecPrivate of the ASS track (a whole ASS script minus events)
   * @param fonts    embedded font files as Uint8Array, from player.fontAttachments()
   * @param vendor   URL prefix where the bundled JASSUB files live
   */
  constructor(video, header, fonts = [],
              { vendor, log = () => {}, maxRenderHeight = ASS_MAX_RENDER_HEIGHT } = {}) {
    vendor = new URL(vendor ?? 'vendor/', document.baseURI).href.replace(/\/$/, '');
    this.log = log;
    this.video = video;
    this._styleIndex = null;
    this._seen = new Set();     // ReadOrder values already pushed
    this._queue = [];
    this._destroyed = false;
    this.fontsLoaded = fonts.length;

    this._ready = (async () => {
      const { default: JASSUB } = await import(`${vendor}/jassub.js`);
      if (this._destroyed) return;
      this.jassub = new JASSUB({
        video,
        subContent: new TextDecoder().decode(header),
        workerUrl: `${vendor}/jassub-worker.js`,
        wasmUrl: `${vendor}/jassub-worker.wasm`,
        modernWasmUrl: `${vendor}/jassub-worker-modern.wasm`,
        // Embedded subset fonts are the whole point of shipping fonts inside
        // the MKV: these scripts reference names like "7YSH79LA" that exist
        // nowhere else on the system.
        fonts,
        availableFonts: { 'liberation sans': `${vendor}/default.woff2` },
        defaultFont: 'liberation sans',
        // Asking the OS for fonts needs a permission prompt and would make a
        // missing embedded font look like it worked on this machine only.
        queryFonts: false,
        // Cap the subtitle raster. JASSUB defaults maxRenderHeight to 0 --
        // no limit -- so it rasterises at the displayed height times the
        // device pixel ratio: 2160 lines on a 4K screen, 4320 on a HiDPI
        // panel in fullscreen. That is a full-frame RGBA composite per video
        // frame on top of a 4K HEVC decode, and it is the subtitle layer that
        // wins, because it is the one with a live requestVideoFrameCallback.
        // Text rasterised at 1080 and scaled up is very slightly softer; a
        // picture that has stopped updating is not a tradeoff at all.
        maxRenderHeight,
      });
      await this.jassub.ready;
      if (this._destroyed) return;

      // libass identifies styles by position, so build the name -> index map
      // once the track is loaded.
      const styles = await this.jassub.renderer.getStyles();
      this._styleIndex = new Map(styles.map((s, i) => [s.Name, i]));
      this.log(`ASS ready: ${styles.length} styles, ${fonts.length} embedded fonts`);
      const pending = this._queue;
      this._queue = [];
      for (const p of pending) this._push(p);
    })();
  }

  get ready() { return this._ready; }

  /** Feed one demuxed subtitle block. */
  push(block) {
    if (this._destroyed) return;
    if (!this._styleIndex) { this._queue.push(block); return; }
    this._push(block);
  }

  _push(block) {
    const line = parseAssPacket(new TextDecoder().decode(block.data));
    if (!line) return;
    // The player re-reads the same region after a seek, so the same line
    // arrives more than once. ReadOrder is the muxer's unique index for it.
    if (this._seen.has(line.readOrder)) return;
    this._seen.add(line.readOrder);

    // An unknown style name means the script referenced a style it never
    // defined; libass falls back to style 0, so do the same rather than drop
    // the line.
    const style = this._styleIndex.get(line.style) ?? 0;
    this.jassub.renderer.createEvent({
      Start: Math.round(block.time * 1000),
      Duration: Math.round((block.duration || 0) * 1000),
      ReadOrder: line.readOrder,
      Layer: line.layer,
      Style: style,
      Name: line.name,
      MarginL: line.marginL,
      MarginR: line.marginR,
      MarginV: line.marginV,
      Effect: line.effect,
      Text: line.text,
    });
  }

  get eventCount() { return this._seen.size; }

  async destroy() {
    this._destroyed = true;
    try { await this._ready; } catch {}
    try { await this.jassub?.destroy(); } catch {}
    this.jassub = null;
  }
}
