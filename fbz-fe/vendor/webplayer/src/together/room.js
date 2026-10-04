// 一起看 — the transport half: one WebSocket to a relay, and the clock that
// makes everyone's idea of "now" the same.
//
// WHY A RELAY AT ALL
// Two browsers cannot find each other. Even a fully peer-to-peer design needs
// somewhere to exchange the first SDP, so "no server" is never actually zero;
// it is only ever "someone else's server". So this speaks a small, boring
// protocol — JSON objects over a plain WebSocket — and the address is typed in
// by the viewer. Two relays that speak it ship with the player:
//
//   worker/watch-together.js   Cloudflare Worker + Durable Object (free tier)
//   tools/together-server.mjs  bare node:http, no dependencies, self-hosted
//
// Anything else that speaks the protocol below works just as well; nothing in
// this file knows which one it is talking to. The URL is never committed, it
// lives in localStorage, exactly like the Emby servers.
//
// WHAT GOES OVER IT
// Playback state, a DESCRIPTION of what the room is watching, chat, and WebRTC
// signalling.
//
// Not the film. The `media` frame carries an item reference and a URL, a few
// hundred bytes; every member then fetches the source itself, straight from
// wherever it lives, exactly as it would if it had opened the episode by hand.
// Proxying the video through the relay would turn a free tier into a bandwidth
// bill and put a second hop between every viewer and their own server, to solve
// a problem nobody has — the members can all already reach the file, that is
// why they are in the room.
//
// And not voice: once two members have exchanged an offer through here, their
// audio goes peer to peer and the relay never sees a byte of it (see voice.js).
// A relay therefore stays cheap no matter how long the film is.
//
// PROTOCOL
// Client -> relay
//   { t:'hello',  name, pass }               first frame after the socket opens
//   { t:'ping',   c }                        c = the client's Date.now()
//   { t:'state',  paused, pos, rate, seq }   my playback position
//   { t:'media',  href, want }               what I am watching
//   { t:'wait',   waiting }                  I am buffering / I am ready
//   { t:'chat',   text, pos }                pos = playhead when it was typed
//   { t:'signal', to, data }                 WebRTC SDP/ICE for one member
//   { t:'claim' }                            take the host seat
// Relay -> client
//   { t:'welcome', you, host, members, media, state, s }
//   { t:'pong',    c, s }                    s = the relay's Date.now()
//   { t:'join'|'leave', member, host, members }
//   { t:'state',   from, paused, pos, rate, seq, s }
//   { t:'media',   from, href, want, s }
//   { t:'wait',    from, waiting, waiters }
//   { t:'chat',    from, name, text, pos, s }
//   { t:'signal',  from, data }
//   { t:'host',    host }
//   { t:'denied',  why }                     'pass' | 'full', then the socket closes
//
// The relay does no interpretation beyond keeping the member list, remembering
// the last state frame for whoever joins next, and picking a host. Every rule
// about WHAT to do with a state frame is in sync.js, on the client, so a relay
// can be replaced without touching playback.

const PING_FAST_MS = 900;      // while the offset estimate is still coarse
const PING_SLOW_MS = 25_000;   // afterwards; also keeps an idle socket alive
const FAST_PINGS = 4;
const SAMPLES = 8;             // offset samples kept; the lowest-RTT one wins

/** A room code a person can read out loud. No 0/O/1/I. */
export function newRoomCode() {
  const abc = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
  const r = crypto.getRandomValues(new Uint8Array(6));
  return [...r].map(b => abc[b % abc.length]).join('');
}

export class Room {
  /**
   * @param {(ev: object) => void} onEvent every relay frame, plus synthetic
   *        { t:'status', state, detail } frames for the connection itself.
   */
  constructor({ onEvent = () => {}, log = () => {} } = {}) {
    this.onEvent = onEvent;
    this.log = log;
    this.ws = null;
    this.url = '';
    this.code = '';
    this.name = '';
    this.pass = '';
    this.id = null;          // assigned by the relay in `welcome`
    this.host = null;
    this.members = [];
    this.status = 'idle';    // idle | connecting | open | retrying | closed
    this._samples = [];      // { rtt, offset }
    this._offset = 0;
    this._pings = 0;
    this._timer = null;
    this._retry = 0;
    this._closing = false;
    this._seq = 0;
  }

  get connected() { return this.ws?.readyState === WebSocket.OPEN && this.id != null; }
  get isHost() { return this.id != null && this.host === this.id; }
  /** How far this browser's clock is behind the relay's, in ms. */
  get offset() { return this._offset; }
  /** The relay's clock, as read from here. Everyone in the room agrees on it. */
  now() { return Date.now() + this._offset; }
  /** Round-trip time of the sample the offset came from — shown in the UI. */
  get rtt() { return this._samples.length ? Math.min(...this._samples.map(s => s.rtt)) : null; }

  /**
   * The room code is a path segment, not a query parameter: a relay that
   * routes by URL (a Durable Object is addressed by name) needs it before it
   * reads a single frame.
   *
   * Whatever the viewer pasted is accepted. A relay hands out an https:// URL
   * (Cloudflare prints one, a tunnel prints one) and people paste what they
   * were given, so http/https are folded to ws/wss -- note that dropping the
   * "http" from "https" leaves the s, which is why one replace covers both.
   * A bare host with no scheme at all gets the page's own: an https page
   * cannot open a ws://, so guessing ws:// there only produces a mixed-content
   * error whose text says nothing about what to fix.
   */
  _wsUrl() {
    let base = this.url.trim().replace(/\/+$/, '');
    if (/^https?:\/\//i.test(base)) base = base.replace(/^http/i, 'ws');
    else if (!/^wss?:\/\//i.test(base)) base = (location.protocol === 'https:' ? 'wss://' : 'ws://') + base;
    return `${base}/room/${encodeURIComponent(this.code)}`;
  }

  connect(url, code, name, pass = '') {
    this.disconnect();
    this._closing = false;
    this.url = url; this.code = code; this.name = name; this.pass = pass;
    this._open();
  }

  _open() {
    this._setStatus('connecting');
    let ws;
    try { ws = new WebSocket(this._wsUrl()); }
    catch (e) { this._setStatus('closed', `地址不对：${e.message}`); return; }
    this.ws = ws;

    ws.onopen = () => {
      this._retry = 0;
      this._samples = []; this._pings = 0;
      ws.send(JSON.stringify({ t: 'hello', name: this.name, pass: this.pass }));
      this._schedulePing(0);
    };
    ws.onmessage = e => {
      let m; try { m = JSON.parse(e.data); } catch { return; }
      this._handle(m);
    };
    ws.onerror = () => { /* onclose always follows; report there so it is said once */ };
    ws.onclose = ev => {
      clearTimeout(this._timer);
      this.id = null;
      if (this._closing) { this._setStatus('closed'); return; }
      // 1000/1001 are a deliberate shutdown at the other end; anything else is
      // a network event worth retrying. Either way the room is gone until the
      // socket is back, so the UI is told before the first retry, not after
      // the last -- a silent five seconds reads as "it just stopped working".
      const wait = Math.min(15_000, 700 * 2 ** this._retry++);
      this._setStatus('retrying', `连接断开（${ev.code}）${(wait / 1000).toFixed(1)}s 后重试`);
      this._timer = setTimeout(() => this._open(), wait);
    };
  }

  disconnect() {
    this._closing = true;
    clearTimeout(this._timer);
    try { this.ws?.close(1000, 'bye'); } catch {}
    this.ws = null;
    this.id = null; this.host = null; this.members = [];
    this._setStatus('idle');
  }

  send(msg) {
    if (this.ws?.readyState !== WebSocket.OPEN) return false;
    this.ws.send(JSON.stringify(msg));
    return true;
  }

  /** Broadcast my playback state. seq lets a receiver drop a reordered frame. */
  sendState({ paused, pos, rate }) {
    return this.send({ t: 'state', paused, pos, rate, seq: ++this._seq });
  }
  /** Say what this member is watching, so the rest of the room can open it. */
  sendMedia({ href, want }) { return this.send({ t: 'media', href, want }); }
  sendChat(text, pos) { return this.send({ t: 'chat', text, pos }); }
  sendWait(waiting)   { return this.send({ t: 'wait', waiting }); }
  signal(to, data)    { return this.send({ t: 'signal', to, data }); }
  claimHost()         { return this.send({ t: 'claim' }); }

  nameOf(id) { return this.members.find(m => m.id === id)?.name || '???'; }

  // --- internals -----------------------------------------------------------

  _setStatus(state, detail = '') {
    this.status = state;
    this.onEvent({ t: 'status', state, detail });
  }

  _schedulePing(delay) {
    clearTimeout(this._timer);
    this._timer = setTimeout(() => {
      if (this.ws?.readyState !== WebSocket.OPEN) return;
      this.send({ t: 'ping', c: Date.now() });
      this._schedulePing(this._pings++ < FAST_PINGS ? PING_FAST_MS : PING_SLOW_MS);
    }, delay);
  }

  /**
   * NTP's estimator, minus the parts that need a symmetric path. One sample:
   *   rtt    = t4 - t1
   *   offset = s - (t1 + rtt/2)
   * Averaging samples is worse than picking the one with the lowest RTT: a
   * delayed packet is delayed in ONE direction, which biases its offset by
   * exactly the asymmetry, and the mean carries that bias in. The quickest
   * exchange is the one least likely to have been queued anywhere.
   */
  _onPong(m) {
    const t4 = Date.now();
    const rtt = t4 - m.c;
    if (rtt < 0 || rtt > 10_000) return;
    this._samples.push({ rtt, offset: m.s - (m.c + rtt / 2) });
    if (this._samples.length > SAMPLES) this._samples.shift();
    let best = this._samples[0];
    for (const s of this._samples) if (s.rtt < best.rtt) best = s;
    this._offset = best.offset;
  }

  _handle(m) {
    switch (m.t) {
      case 'pong': this._onPong(m); break;
      case 'welcome':
        this.id = m.you; this.host = m.host; this.members = m.members || [];
        // The relay's clock is in the welcome too, so the very first state
        // frame can be interpreted before any ping has come back.
        if (m.s) this._offset = m.s - Date.now();
        this._setStatus('open');
        break;
      case 'join': case 'leave':
        this.members = m.members || this.members;
        this.host = m.host ?? this.host;
        break;
      case 'host': this.host = m.host; break;
      // Turned away on purpose. Retrying a wrong password just retries a wrong
      // password, once a second, forever -- so this is the one close that is
      // not a network event and must not be reconnected through.
      case 'denied':
        this._closing = true;
        this._setStatus('closed', m.why === 'pass' ? '密码不对，进不去这个房间' : '房间满了');
        break;
    }
    this.onEvent(m);
  }
}
