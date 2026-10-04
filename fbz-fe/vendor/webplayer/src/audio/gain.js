// An OPTIONAL gain stage between the <video> element and the speakers.
//
// Why this exists, measured rather than assumed (ffmpeg ebur128 over 3 minutes):
//
//     houshi.mkv        5.1 E-AC3   -25.9 LUFS
//     ... centre channel only       -26.2 LUFS
//     mozahng...ass.mkv stereo AAC  -18.2 LUFS
//     web streaming norm            -16 .. -14 LUFS
//
// So the quiet-film complaint is not a bug in the E-AC3 path: the downmix
// loses nothing (the centre channel is just as quiet as the mix), the film is
// simply mastered for a cinema, ~10 dB below what a browser page sounds like.
// `video.volume` is clamped to 1.0 and has none, so the only place to make up
// that headroom is WebAudio.
//
// The catch, and why this is now OPT-IN: routing an MSE-backed <video> through
// createMediaElementSource() forks the audio off the element's own playout
// clock. The two clocks then drift, and the browser papers over the drift by
// re-seeking -- audible as stutter and visible as the picture and sound pulling
// apart. The first version built this graph for EVERY file in the constructor,
// so even a file that needed no boost, playing with no subtitles, paid the
// drift tax. That regressed smooth playback.
//
// So: nothing is routed until the viewer actually asks for a boost. Until then
// the element plays natively, exactly as it did before this file existed.
// createMediaElementSource cannot be undone once called, so "off after on"
// means unity gain through the graph, not a true bypass -- but a session that
// never touches the loudness control never touches WebAudio at all.

/** Target programme loudness, as gated RMS dBFS. Calibrated in tools/test-gain. */
export const TARGET_DBFS = -20;

/** Blocks quieter than this are silence/room tone and must not drag the average down. */
const GATE_DBFS = -55;

/** Ceiling on automatic makeup. Beyond this the limiter is doing all the work. */
export const MAX_AUTO_GAIN = 4;

const dbToLin = db => 10 ** (db / 20);

/**
 * A stateless soft-clip curve for a WaveShaper. Transparent below `knee`, then
 * a tanh shoulder that lands anything at or beyond full scale on ~0.98. This
 * replaces the DynamicsCompressor the first version used: a compressor has
 * lookahead (a few ms), which is exactly the audio-ahead-of-video latency we
 * are trying not to add. A WaveShaper is instantaneous and has no state.
 */
function softClipCurve(knee = 0.7, n = 2048) {
  const curve = new Float32Array(n);
  const span = 1 - knee;
  for (let i = 0; i < n; i++) {
    const x = (i / (n - 1)) * 2 - 1;         // -1 .. 1
    const a = Math.abs(x);
    const y = a <= knee ? a : knee + span * Math.tanh((a - knee) / span);
    curve[i] = Math.sign(x) * Math.min(0.98, y);
  }
  return curve;
}

/**
 * Routes a media element through `gain -> soft limiter -> speakers` ONLY once a
 * boost is requested, and works out how much makeup the material needs.
 */
export class VolumeStage {
  constructor(video, { log = () => {} } = {}) {
    this.video = video;
    this.log = log;
    // Whether WebAudio *could* be used. The graph is not built yet.
    this.available = !!(window.AudioContext ?? window.webkitAudioContext);
    this.engaged = false;    // graph built and routing (irreversible once true)
    this.auto = false;       // OFF by default: native playback, no drift
    this._boost = 1;         // manual multiplier on top of the automatic gain
    this._autoGain = 1;
    this._measured = null;   // gated RMS dBFS of the programme so far
    this._sumSq = 0;
    this._blocks = 0;
    this._timer = null;
    this.ctx = null;
  }

  /** The loudness control is offered whenever WebAudio exists to power it. */
  get ok() { return this.available; }

  /**
   * Build the graph. Called the first time a boost or auto-gain is asked for.
   * Everything the element outputs is permanently routed through the context
   * afterwards -- WebAudio has no way back -- so this happens as late as
   * possible and never for a viewer who leaves the control alone.
   */
  _engage() {
    if (this.engaged || !this.available) return false;
    try {
      const Ctx = window.AudioContext ?? window.webkitAudioContext;
      this.ctx = new Ctx();
      this.source = this.ctx.createMediaElementSource(this.video);
      this.gain = this.ctx.createGain();
      this.shaper = this.ctx.createWaveShaper();
      this.shaper.curve = softClipCurve();
      this.shaper.oversample = '2x';

      // Tapped BEFORE the gain, so the measurement describes the material and
      // does not chase its own output.
      this.analyser = this.ctx.createAnalyser();
      this.analyser.fftSize = 2048;
      this._buf = new Float32Array(this.analyser.fftSize);

      this.source.connect(this.analyser);
      this.source.connect(this.gain);
      this.gain.connect(this.shaper);
      this.shaper.connect(this.ctx.destination);

      this._timer = setInterval(() => this._measure(), 200);
      this.engaged = true;
      this._ramp();
      this.log('响度增益已接入（WebAudio）');
      return true;
    } catch (e) {
      // Once createMediaElementSource has run, the element no longer reaches
      // the speakers on its own. If wiring failed partway, connect the source
      // straight to the output so audio is never lost -- a missing boost is a
      // nuisance, silence is not.
      try { this.source?.connect(this.ctx.destination); } catch {}
      this.available = false;
      this.log(`响度增益不可用，按原始音量播放: ${e.message}`, 'warn');
      return false;
    }
  }

  /** Must be called from a user gesture; harmless (and a no-op) if not engaged. */
  resume() { return this.ctx?.state === 'suspended' ? this.ctx.resume() : Promise.resolve(); }

  _measure() {
    if (!this.engaged || this.video.paused) return;
    this.analyser.getFloatTimeDomainData(this._buf);
    let sq = 0;
    for (let i = 0; i < this._buf.length; i++) sq += this._buf[i] * this._buf[i];
    const ms = sq / this._buf.length;
    // Gate on the block, R128-style: a film is mostly quiet, and averaging the
    // silence in would ask for makeup gain the loud parts cannot survive.
    if (ms <= 0 || 10 * Math.log10(ms) < GATE_DBFS) return;
    this._sumSq += ms;
    this._blocks++;
    // ~5 seconds of gated audio before committing to a number.
    if (this._blocks < 25) return;
    this._measured = 10 * Math.log10(this._sumSq / this._blocks);
    if (this.auto) this._applyAuto();
  }

  _applyAuto() {
    if (this._measured == null) return;
    const want = Math.min(MAX_AUTO_GAIN, Math.max(1, dbToLin(TARGET_DBFS - this._measured)));
    if (Math.abs(want - this._autoGain) < 0.02) return;
    this._autoGain = want;
    this._ramp();
    this.log(`loudness ${this._measured.toFixed(1)} dBFS -> +${(20 * Math.log10(want)).toFixed(1)} dB makeup`);
  }

  _ramp() {
    if (!this.engaged) return;
    // A step change in gain is an audible click. 0.4 s is under the threshold
    // where a viewer notices the level moving.
    this.gain.gain.setTargetAtTime(this._autoGain * this._boost, this.ctx.currentTime, 0.4);
  }

  /** Manual multiplier, 1 = no extra boost. Setting >1 engages the graph. */
  get boost() { return this._boost; }
  set boost(x) {
    this._boost = Math.min(4, Math.max(0.25, Number(x) || 1));
    if (this._boost !== 1 && !this.engaged) this._engage();
    this._ramp();
  }

  setAuto(on) {
    this.auto = !!on;
    if (on) {
      if (!this.engaged) this._engage();
      this._applyAuto();
    } else if (this.engaged) {
      this._autoGain = 1;
      this._ramp();
    }
  }

  /** Total applied gain in dB, for display. 0 when nothing is engaged. */
  get appliedDb() { return this.engaged ? 20 * Math.log10(this._autoGain * this._boost) : 0; }
  get measuredDbfs() { return this._measured; }

  /** Forget the measurement; call when a different file starts. */
  restart() {
    this._sumSq = 0; this._blocks = 0; this._measured = null;
    this._autoGain = 1; this._ramp();
  }

  _teardown() {
    clearInterval(this._timer);
    this._timer = null;
  }

  destroy() {
    this._teardown();
    // The element stays wired to this context for its lifetime once engaged --
    // WebAudio has no way to undo createMediaElementSource -- so the context is
    // kept alive and only the measurement loop stops. Closing it would mute the
    // element. If never engaged, there is nothing to keep.
  }
}
