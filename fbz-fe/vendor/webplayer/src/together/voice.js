// 一起看 — voice. A full mesh of WebRTC audio connections, signalled over the
// room's WebSocket and then entirely out of its way.
//
// WHY MESH AND NOT AN SFU
// The usual advice — mesh falls over past four people — is about VIDEO. One
// Opus voice stream is around 30-40 kbit/s, so the worst case here, eight
// people all unmuted, is seven inbound and seven outbound streams: roughly a
// quarter of a megabit each way. That is less than the film. An SFU would mean
// a second server to run, a second thing to keep alive, and every word routed
// through somebody else's machine, to save bandwidth nobody was short of.
//
// WHY THE RELAY NEVER CARRIES THE AUDIO
// It carries the offer and the ICE candidates, which is a few kilobytes per
// pair, once. After that the audio is peer to peer and encrypted end to end by
// DTLS-SRTP, which WebRTC makes mandatory. A relay on a free tier is therefore
// not a bandwidth question, and the operator of the relay cannot listen in.
//
// WHEN THE DIRECT PATH DOES NOT EXIST
// Roughly one connection in ten cannot be hole-punched — symmetric NAT on both
// ends, or a corporate network that drops UDP. That case needs a TURN server,
// which by definition relays the audio and so cannot be free forever. Rather
// than bake in somebody's public credentials (they expire, and they would be
// in the repository), iceServers is a setting: paste whatever you have, or
// leave it empty and accept that one pair in ten will not connect.

/** Google's STUN. Address discovery only — no media ever touches it. */
export const DEFAULT_ICE = [{ urls: 'stun:stun.l.google.com:19302' }];

const MIC = {
  audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  video: false,
};

export class Voice {
  /**
   * @param room a connected Room; used only to carry `signal` frames.
   */
  constructor(room, { log = () => {}, onChange = () => {}, iceServers = DEFAULT_ICE } = {}) {
    this.room = room;
    this.log = log;
    this.onChange = onChange;
    this.iceServers = iceServers?.length ? iceServers : DEFAULT_ICE;

    this.on = false;
    this.muted = false;
    this.stream = null;
    this.peers = new Map();   // id -> { pc, audio, state, on }
  }

  get peerStates() {
    return [...this.peers].map(([id, p]) => ({ id, state: p.state, on: p.on }));
  }

  async enable() {
    if (this.on) return;
    // getUserMedia must be reached from the click, not after it: a permission
    // prompt that appears without one is denied by policy on some setups.
    this.stream = await navigator.mediaDevices.getUserMedia(MIC);
    this.on = true;
    this.setMuted(this.muted);
    this._announce(true);
    this.onChange();
  }

  disable() {
    if (!this.on) return;
    this.on = false;
    for (const id of [...this.peers.keys()]) this._drop(id);
    this.stream?.getTracks().forEach(t => t.stop());
    this.stream = null;
    this._announce(true);      // tell the room to tear its half down too
    this.onChange();
  }

  setMuted(muted) {
    this.muted = muted;
    // The track keeps flowing (silence), so nothing renegotiates and unmuting
    // is instant rather than another round of offer/answer.
    this.stream?.getAudioTracks().forEach(t => { t.enabled = !muted; });
    this.onChange();
  }

  /** Room frames. Only `signal`, `join` and `leave` matter here. */
  onMessage(m) {
    if (m.t === 'leave') return this._drop(m.member?.id);
    if (m.t === 'join') { if (this.on && m.member?.id) this._announceTo(m.member.id, true); return; }
    if (m.t !== 'signal' || !m.from) return;
    const d = m.data || {};
    if (d.hello != null) {
      if (d.hello) this._announceTo(m.from, false);   // answer, do not loop
      const peer = this._slot(m.from);
      peer.on = !!d.on;
      if (!d.on) this._drop(m.from); else this._maybeConnect(m.from);
      this.onChange();
      return;
    }
    this._negotiate(m.from, d).catch(e => this.log(`语音信令失败（${this.room.nameOf(m.from)}）：${e.message}`, 'warn'));
  }

  // --- signalling ----------------------------------------------------------

  _announce(hello) { for (const m of this.room.members) if (m.id !== this.room.id) this._announceTo(m.id, hello); }
  _announceTo(id, hello) { this.room.signal(id, { hello, on: this.on }); }

  _slot(id) {
    let p = this.peers.get(id);
    if (!p) { p = { pc: null, audio: null, state: 'idle', on: false }; this.peers.set(id, p); }
    return p;
  }

  /**
   * Exactly one side of each pair offers, chosen by comparing ids. Both sides
   * offering at once is "glare": the two offers cross, each rejects the other's
   * because it is not in a stable state, and the pair never connects.
   */
  _maybeConnect(id) {
    if (!this.on || !this._slot(id).on || this.peers.get(id).pc) return;
    if (this.room.id < id) this._connect(id, true).catch(e => this.log(`语音连接失败：${e.message}`, 'warn'));
  }

  async _connect(id, offering) {
    const peer = this._slot(id);
    if (peer.pc) return peer.pc;
    const pc = new RTCPeerConnection({ iceServers: this.iceServers });
    peer.pc = pc; peer.state = 'connecting';

    for (const t of this.stream?.getAudioTracks() ?? []) pc.addTrack(t, this.stream);

    pc.onicecandidate = e => { if (e.candidate) this.room.signal(id, { ice: e.candidate }); };
    pc.ontrack = e => {
      // A plain <audio> rather than a WebAudio graph: the player's own gain
      // stage already showed that routing a media element through WebAudio
      // costs stutter, and there is nothing to gain here — the film and the
      // voices want separate volumes, which two elements already give.
      const a = peer.audio ?? Object.assign(new Audio(), { autoplay: true });
      a.srcObject = e.streams[0];
      a.play?.().catch(() => {});
      peer.audio = a;
      this.onChange();
    };
    pc.onconnectionstatechange = () => {
      peer.state = pc.connectionState;
      if (pc.connectionState === 'failed') {
        this.log(`和 ${this.room.nameOf(id)} 打不通直连${this.iceServers.length > 1 ? '' : '（未配置 TURN）'}`, 'warn');
      }
      this.onChange();
    };

    if (offering) {
      await pc.setLocalDescription(await pc.createOffer());
      this.room.signal(id, { sdp: pc.localDescription });
    }
    this.onChange();
    return pc;
  }

  async _negotiate(id, d) {
    if (!this.on) return;
    if (d.sdp) {
      const pc = await this._connect(id, false);
      await pc.setRemoteDescription(d.sdp);
      if (d.sdp.type === 'offer') {
        await pc.setLocalDescription(await pc.createAnswer());
        this.room.signal(id, { sdp: pc.localDescription });
      }
    } else if (d.ice) {
      const pc = this.peers.get(id)?.pc;
      // Candidates routinely arrive before the answer is applied; dropping one
      // costs a path, so the failure is logged rather than thrown.
      if (pc) await pc.addIceCandidate(d.ice).catch(() => {});
    }
  }

  _drop(id) {
    const p = this.peers.get(id);
    if (!p) return;
    try { p.pc?.close(); } catch {}
    if (p.audio) { p.audio.srcObject = null; p.audio.remove?.(); }
    this.peers.delete(id);
    this.onChange();
  }

  destroy() { this.disable(); this.peers.clear(); }
}
