// 一起看 — the playback half. Everything about WHEN the picture moves.
//
// The room has exactly one shared fact: the last explicit command anybody
// gave, stamped with the relay's clock.
//
//   { paused, pos, rate, at }      at = the relay's Date.now() when it landed
//
// Where the film should be right now is a projection off that fact:
//
//   target = paused ? pos : pos + (relayNow - at) / 1000 * rate
//
// Anyone may play, pause, seek or change speed; the command is broadcast and
// everyone — including whoever sent it — re-anchors to it. There is no "only
// the host may touch it" mode, because a room is two to eight people who are
// already talking to each other, and asking permission to pause is a worse
// experience than the occasional double-pause.
//
// The host still exists, for three jobs that need one voice: it re-anchors the
// room every few seconds from its own real playhead (so slow accumulated drift
// does not become everyone's problem), it is the one that presses play again
// after the room has waited for a straggler, and it is the one that says what
// the room is watching.
//
// WHO IS ALLOWED TO SPEAK
// Only a member whose element actually holds a film. This is not a permission
// system, it is arithmetic: an element that never loaded anything reports
// `paused` and `currentTime === 0`, and those describe the absence of a film
// rather than a decision about one. Before this rule a member who could not
// open the source — wrong account, expired link, autoplay refused — broadcast
// "paused at 0" on every failed play(), and every other member dutifully
// jumped to the start and stopped. The room looked like it was pausing itself
// at random. It was not: it was obeying somebody who had nothing to say.
//
// CORRECTING DRIFT
// Jellyfin's SyncPlay nudges playbackRate between 50ms and 3s of drift and
// seeks past 400ms. Those numbers assume a seek is cheap. Here it is not: a
// seek re-reads the container from the nearest keyframe, re-demuxes, and
// re-fills the MSE buffer, which is seconds of black picture — much more
// disruptive than being 800ms out. So the rate window is stretched and the
// seek threshold is pushed well past it: a seek is the last resort, for
// "somebody jumped to a different scene", not for "we drifted".
//
// The rate correction is deliberately gentle (±8%). Anything stronger is
// audible on dialogue, and the whole point is to be watching the same thing,
// not to be watching a chipmunk.

const DEAD_MS   = 80;      // below this, do nothing at all: it is measurement noise
const SEEK_MS   = 2500;    // above this, a nudge would take half a minute — seek
const ABSORB_MS = 8000;    // aim to eat the drift over this long
const MAX_NUDGE = 0.08;    // ±8% of the room's playback rate
const SETTLE_MS = 3000;    // after a seek, let the pipeline refill before judging
const ANCHOR_MS = 5000;    // how often the host re-anchors the room
const TICK_MS   = 500;
const QUIET_MS  = 1200;    // an event this soon after we drove the element is ours
const HOLD_MAX_MS = 20_000; // stop waiting for a straggler that never arrives

// HAVE_METADATA. Below this the element has no film in it: currentTime is 0 and
// paused is true because nothing was ever loaded, not because anybody decided
// anything. Those are the two values that used to get broadcast as if they were
// a human pressing pause at the start of the film — see _canSpeak().
const HAVE_METADATA = 1;

export class PlaybackSync {
  /**
   * @param video the <video> element
   * @param room  a connected Room (src/together/room.js)
   */
  constructor(video, room, { log = () => {}, onChange = () => {} } = {}) {
    this.video = video;
    this.room = room;
    this.log = log;
    this.onChange = onChange;      // called whenever the UI-visible state moves

    this.enabled = false;
    this.state = null;             // { paused, pos, rate, at, from, seq }
    this.waiters = [];             // ids the room is waiting for
    this.drift = 0;                // seconds; + means we are ahead
    this.baseRate = 1;             // the speed the room agreed on
    this.media = null;             // { href, want, from } — what the room is watching

    // Echo suppression, per event. A boolean cleared on setTimeout(…, 0) does
    // not work: a media element queues its events on its own task source, and
    // nothing in the spec orders that against a timer, so the event routinely
    // arrived after the flag had already been cleared and got broadcast as if a
    // human had done it. A deadline per event type cannot lose that race, and
    // keeping them separate means a playbackRate nudge does not also swallow a
    // real pause 200ms later.
    this._quiet = { play: 0, pause: 0, seeking: 0 };
    this._settleUntil = 0;
    this._lastAnchor = 0;
    this._heldPlaying = false;     // were we playing when the room started waiting
    this._holdSince = 0;           // when the current buffer hold began
    this._giveUp = new Set();      // members we have stopped waiting for
    this._wasWaiting = false;      // last value sent, so a repeat is not re-sent
    this._seq = -1;
    this._tick = null;
    this._onEv = {};
  }

  /**
   * Is this element entitled to an opinion about where the film is?
   *
   * An element that never loaded anything still reads `paused === true` and
   * `currentTime === 0`, and those are not facts about the film — they are the
   * absence of one. Broadcasting them is what let a member who could not open
   * the source at all drag the whole room back to the start and pause it, over
   * and over, which is the failure this guard exists to stop. Below
   * HAVE_METADATA a member listens and says nothing.
   */
  get _canSpeak() { return this.video.readyState >= HAVE_METADATA; }

  /** Drive the element ourselves, and do not mistake the fallout for a person. */
  _drive(events, fn) {
    const until = Date.now() + QUIET_MS;
    for (const e of events) this._quiet[e] = until;
    try { fn(); } catch (e) { this.log(`一起看：${e.message}`, 'warn'); }
  }
  _ours(ev) { return Date.now() < this._quiet[ev]; }

  start() {
    if (this.enabled) return;
    this.enabled = true;
    const v = this.video;
    // Explicit user actions, and only those. `seeking` rather than `seeked`: the
    // other members should start moving while this browser is still refilling,
    // not after.
    //
    // There is deliberately no `ratechange` listener. The correction loop writes
    // playbackRate every tick, so that event fires constantly for reasons no
    // human caused — and each one used to put a full state frame on the wire,
    // which is how a member who was merely drifting ended up re-anchoring the
    // room several times a second. A speed the viewer actually chose arrives
    // through setRate(), which broadcasts on purpose.
    this._bind(v, 'play',        () => { if (!this._ours('play')) this._broadcast(); });
    this._bind(v, 'pause',       () => { if (!this._ours('pause')) this._broadcast(); });
    this._bind(v, 'seeking',     () => {
      if (this._ours('seeking')) return;
      this._settleUntil = Date.now() + SETTLE_MS;
      this._broadcast();
    });
    // Buffering. `waiting` fires when the element runs out of data; `playing`
    // when it has some again. Together they are the lockstep signal.
    this._bind(v, 'waiting',     () => this._setWaiting(true));
    this._bind(v, 'playing',     () => this._setWaiting(false));
    this._bind(v, 'canplay',     () => this._setWaiting(false));
    this._tick = setInterval(() => this._correct(), TICK_MS);
    // Seed the room with where this member already is, so a second joiner is
    // not told the film is at 0 just because nobody has touched it yet.
    if (this.room.isHost) this._broadcast();
  }

  stop() {
    if (!this.enabled) return;
    this.enabled = false;
    clearInterval(this._tick);
    for (const [ev, fn] of Object.entries(this._onEv)) this.video.removeEventListener(ev, fn);
    this._onEv = {};
    this.video.playbackRate = this.baseRate;
    this.state = null; this.waiters = []; this.drift = 0;
    this._giveUp.clear();
    this.onChange();
  }

  /** Relay frames the panel hands over. */
  onMessage(m) {
    if (!this.enabled) return;
    if (m.t === 'state') this._adopt(m);
    else if (m.t === 'wait') {
      // A member that came back deserves to be waited for again.
      if (m.waiting === false && m.from) this._giveUp.delete(m.from);
      this._setWaiters(m.waiters);
    }
    else if (m.t === 'media') this._onMedia(m);
    else if (m.t === 'welcome') {
      if (m.media) this._onMedia({ ...m.media, replay: true });
      if (m.state) this._adopt({ ...m.state, seq: -1 });
    }
    else if (m.t === 'leave') {
      // A member that vanished mid-buffer would otherwise hold the room
      // paused forever.
      this._giveUp.delete(m.member?.id);
      this._setWaiters(this.waiters.filter(id => id !== m.member?.id));
    }
  }

  _setWaiters(list) {
    const next = (list || []).filter(id => !this._giveUp.has(id));
    const was = this.waiting;
    this.waiters = next;
    if (this.waiting && !was) this._holdSince = Date.now();
    this._applyHold();
  }

  /**
   * The room said what it is watching. Nothing is opened from here — the panel
   * owns that decision, because auto-navigating a page out from under somebody
   * who is already watching the right film is worse than the problem.
   */
  _onMedia(m) {
    if (!m.want && !m.href) return;
    this.media = { href: m.href || '', want: m.want || null, from: m.from };
    this.onChange();
  }

  /** Where the room says the film is, right now. null when nothing is set. */
  target() {
    const s = this.state;
    if (!s) return null;
    if (s.paused) return s.pos;
    return s.pos + (this.room.now() - s.at) / 1000 * (s.rate || 1);
  }

  get waiting() { return this.waiters.length > 0; }

  // --- outgoing ------------------------------------------------------------

  _bind(el, ev, fn) { this._onEv[ev] = fn; el.addEventListener(ev, fn); }

  _broadcast() {
    if (!this.enabled || !this.room.connected || !this._canSpeak) return;
    const v = this.video;
    if (!Number.isFinite(v.currentTime)) return;
    // Report the speed the room should run at, not the corrected one this
    // element happens to be running at — otherwise a nudge propagates and the
    // whole room slowly accelerates.
    this.room.sendState({ paused: v.paused, pos: v.currentTime, rate: this.baseRate });
    this._lastAnchor = Date.now();
  }

  /** Tell the room what this member is watching. Host only — see panel.js. */
  shareMedia({ href = '', want = null } = {}) {
    if (!this.room.connected) return false;
    this.media = { href, want, from: this.room.id };
    return this.room.sendMedia({ href, want });
  }

  _setWaiting(waiting) {
    // A member with nothing loaded is not buffering, it is empty — and holding
    // seven people because an eighth never opened the film is the bug, not the
    // feature. It stays silent until it has metadata.
    if (waiting && !this._canSpeak) return;
    if (this._wasWaiting === waiting) return;
    this._wasWaiting = waiting;
    this.room.sendWait(waiting);
  }

  // --- incoming ------------------------------------------------------------

  _adopt(m) {
    // Frames can overtake each other on a reconnect. seq is per sender, so it
    // only orders frames from the same one; a different sender always wins,
    // because it is a fresh human action.
    if (m.from === this.state?.from && m.seq != null && m.seq <= this._seq) return;
    this._seq = m.seq ?? this._seq;
    this.state = { paused: !!m.paused, pos: m.pos, rate: m.rate || 1, at: m.s ?? this.room.now(), from: m.from };
    if (m.rate && m.rate !== this.baseRate) this.baseRate = m.rate;

    if (m.from === this.room.id) { this.onChange(); return; }   // our own frame, echoed

    const v = this.video;
    // Nothing is loaded here: there is no playhead to move and calling play()
    // on an empty element only produces a rejection, whose `pause` event used
    // to go straight back out as "this member paused the film". Adopt the
    // numbers so the UI can show them, touch nothing.
    if (!this._canSpeak) { this.onChange(); return; }

    const want = this.target();
    this._drive(['play', 'pause', 'seeking'], () => {
      // Jump only when it is a real jump. A remote pause/play at roughly our
      // position must not turn into a seek, or every pause in the room costs
      // everyone a buffer refill.
      if (Number.isFinite(want) && Math.abs(v.currentTime - want) > SEEK_MS / 1000) {
        v.currentTime = Math.max(0, want);
        this._settleUntil = Date.now() + SETTLE_MS;
      }
      if (this.state.paused && !v.paused) v.pause();
      else if (!this.state.paused && v.paused && !this.waiting) this._play();
      v.playbackRate = this.baseRate;
    });
    this.onChange();
  }

  /**
   * play(), with the two ways it fails told apart. Autoplay policy is the
   * common one and it is not an error in the room — it is this browser asking
   * for a click. Either way the rejection must not become a state frame, which
   * is why the quiet window is re-armed around the async half as well.
   */
  _play() {
    const p = this.video.play();
    if (!p?.catch) return;
    p.catch(e => {
      this._quiet.pause = Date.now() + QUIET_MS;
      this.log(e.name === 'NotAllowedError'
        ? '一起看：浏览器拦了自动播放，点一下画面就跟上了'
        : `一起看：无法播放（${e.message}）`, 'warn');
      this.onChange();
    });
  }

  /**
   * Somebody is buffering: everyone stops until they are not. Without this the
   * member on the slow line falls a minute behind over an evening and the only
   * fix is a manual seek, which then yanks everybody else.
   */
  _applyHold() {
    const v = this.video;
    this._drive(['play', 'pause'], () => {
      if (this.waiting) {
        if (!v.paused) { this._heldPlaying = true; v.pause(); }
      } else if (this._heldPlaying) {
        this._heldPlaying = false;
        // One voice presses play, or eight members each broadcast a state
        // frame at once and the last one wins by luck.
        if (this.room.isHost) { setTimeout(() => this._resume(), 120); }
      }
    });
    this.onChange();
  }

  /**
   * Waiting for a straggler is right; waiting for a member who is never going
   * to arrive is how the room ends up frozen with no explanation. After
   * HOLD_MAX_MS the room stops counting whoever is still buffering and carries
   * on without them; they rejoin the lockstep the moment they report ready.
   *
   * Decided locally, by every member, off the same clock — so it does not
   * depend on the relay, and it stays true even if the member holding the room
   * is the one whose network died.
   */
  _giveUpOnStragglers() {
    for (const id of this.waiters) this._giveUp.add(id);
    const names = this.waiters.map(id => this.room.nameOf(id)).join('、');
    this._holdSince = 0;
    this._setWaiters([]);
    this.log(`一起看：等了 ${HOLD_MAX_MS / 1000} 秒还没缓冲好，先不等 ${names} 了`, 'warn');
  }

  _resume() {
    if (!this.enabled || this.waiting || !this._canSpeak) return;
    this._drive(['play'], () => this._play());
    this._broadcast();
  }

  // --- the correction loop -------------------------------------------------

  _correct() {
    if (!this.enabled || !this.room.connected) return;
    const v = this.video, s = this.state;

    // Nobody waits forever. Checked here rather than on a timer of its own
    // because this tick is already running at 500ms and a hold does not need
    // finer resolution than that.
    if (this.waiting && this._holdSince && Date.now() - this._holdSince > HOLD_MAX_MS) {
      this._giveUpOnStragglers();
    }

    // The host re-anchors the room from its own real playhead. Everything else
    // in the room is a projection off that, so leaving it stale for an hour
    // means the projection carries an hour's worth of accumulated rate error.
    // A host with nothing loaded has no playhead to anchor from — it would be
    // broadcasting "paused at 0", which is exactly the frame that used to drag
    // everybody back to the start of the film.
    if (this.room.isHost && this._canSpeak && Date.now() - this._lastAnchor > ANCHOR_MS) this._broadcast();

    if (!s || s.paused || v.paused || this.waiting || this.room.isHost || !this._canSpeak) {
      this._setRate(this.baseRate);
      this.drift = 0;
      return;
    }
    if (Date.now() < this._settleUntil) return;

    const want = this.target();
    if (!Number.isFinite(want) || !Number.isFinite(v.currentTime)) return;
    const drift = v.currentTime - want;          // + we are ahead, - we are behind
    this.drift = drift;
    const ms = Math.abs(drift) * 1000;

    if (ms > SEEK_MS) {
      this._drive(['seeking'], () => { v.currentTime = Math.max(0, want); });
      this._settleUntil = Date.now() + SETTLE_MS;
      this.log(`一起看：偏差 ${drift.toFixed(1)}s，已跳转对齐`);
      this._setRate(this.baseRate);
    } else if (ms > DEAD_MS) {
      // Negative drift (behind) has to speed up, so the sign flips.
      const nudge = Math.max(-MAX_NUDGE, Math.min(MAX_NUDGE, -drift * 1000 / ABSORB_MS));
      this._setRate(this.baseRate * (1 + nudge));
    } else {
      this._setRate(this.baseRate);
    }
    this.onChange();
  }

  /** A correction, not a choice. Never reaches the room — see start(). */
  _setRate(rate) {
    if (this.video.playbackRate !== rate) this.video.playbackRate = rate;
  }

  /** The speed the viewer picked, as opposed to a correction. */
  setRate(rate) {
    this.baseRate = rate;
    this._setRate(rate);
    this._broadcast();
  }
}
