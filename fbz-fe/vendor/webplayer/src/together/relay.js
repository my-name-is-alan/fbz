// 一起看 — the relay's brain, with no I/O in it.
//
// Both bundled relays are the same twenty decisions wrapped in a different
// socket API, so the decisions live here and get imported by each:
//
//   worker/watch-together.js   Cloudflare Worker + Durable Object
//   tools/together-server.mjs  bare node:http, no dependencies
//
// which also means the protocol can be tested without a socket at all
// (tools/test-together.mjs).
//
// The relay is deliberately close to dumb. It keeps who is here, who is
// buffering, who the host is, and the last state frame it saw — and forwards
// everything else. It never decides where the film should be; that is sync.js
// on the client, so a room does not break when someone runs an older relay.
//
// A `conn` is anything with `.send(object)` and, optionally, `.close()`.

const CODE = 'abcdefghijkmnpqrstuvwxyz23456789';
const newId = () => {
  const r = (globalThis.crypto ?? {}).getRandomValues?.(new Uint8Array(8));
  if (!r) throw new Error('no crypto.getRandomValues in this runtime');
  return [...r].map(b => CODE[b % CODE.length]).join('');
};

const MAX_NAME = 24;
const MAX_CHAT = 500;
const MAX_MEMBERS = 32;
const MAX_PASS = 64;
const MAX_HREF = 4096;    // a signed stream URL with a token is long; a novel is not

/**
 * How long a member may be silent before the room stops counting it.
 *
 * Clients ping every 25s (PING_SLOW_MS in room.js), so this is two missed
 * pings plus slack. It has to be an application-level rule rather than a socket
 * timeout: a socket timeout is reset by WRITES as well as reads (measured), and
 * the host broadcasts an anchor to every member every 5 seconds — so a member
 * whose network died is written to forever and never goes idle. It stayed in
 * the member list, and could be handed the host seat, after which nobody
 * re-anchored the room at all.
 */
export const IDLE_MS = 70_000;

export class Relay {
  constructor({ now = () => Date.now(), makeId = newId } = {}) {
    this.now = now;
    this.makeId = makeId;
    this.conns = new Map();   // conn -> { id, name, waiting, seen }
    this.host = null;
    this.state = null;        // last { paused, pos, rate, from, seq, s }
    this.media = null;        // last { href, want, from } — what the room is on
    this.pass = null;         // set by whoever opened the room; '' means none
  }

  get members() { return [...this.conns.values()].map(m => ({ id: m.id, name: m.name })); }
  get waiters() { return [...this.conns.values()].filter(m => m.waiting).map(m => m.id); }
  get size() { return this.conns.size; }

  /** Re-attach a socket that outlived this object (a hibernated Durable Object). */
  restore(conn, meta) {
    if (!meta?.id) return;
    this.conns.set(conn, { id: meta.id, name: meta.name || '匿名', waiting: !!meta.waiting, seen: this.now() });
    if (meta.host) this.host = meta.id;
    if (meta.pass != null) this.pass = meta.pass;
  }

  /**
   * Turn away a socket with a reason it can show a person.
   *
   * A bare close code cannot: the browser only hands the page a number, so
   * "密码不对" and "房间满了" and "the wifi blinked" all arrive as the same
   * event and the panel guesses. One frame first, then the close.
   */
  _deny(conn, why) {
    try { conn.send({ t: 'denied', why }); } catch {}
    try { conn.close?.(); } catch {}
    return null;
  }

  /**
   * @returns the member record, or null if the socket was turned away — the
   *          relay has already told it why and closed it.
   */
  _join(conn, name, pass) {
    const given = String(pass ?? '').slice(0, MAX_PASS);
    // The first one in sets the password, exactly as it sets the host seat.
    // An empty room forgets it, which is what makes a code reusable tomorrow.
    if (!this.conns.size) this.pass = given;
    else if ((this.pass || '') !== given) return this._deny(conn, 'pass');
    if (this.conns.size >= MAX_MEMBERS) return this._deny(conn, 'full');

    const me = { id: this.makeId(), name: String(name || '匿名').slice(0, MAX_NAME),
                 waiting: false, seen: this.now() };
    this.conns.set(conn, me);
    // First one in is the host. Not a privilege — see sync.js; it is only the
    // member that re-anchors the room and restarts it after a buffer wait.
    if (!this.host || !this.members.some(m => m.id === this.host)) this.host = me.id;
    const s = this.now();
    // `media` before `state`: knowing WHICH film the numbers refer to is what
    // stops a joiner from applying somebody else's playhead to nothing at all.
    conn.send({ t: 'welcome', you: me.id, host: this.host, members: this.members,
                media: this.media, state: this.state, s });
    this._others(conn, { t: 'join', member: { id: me.id, name: me.name }, host: this.host, members: this.members });
    // A member that joins mid-wait must be told, or it plays on alone.
    if (this.waiters.length) conn.send({ t: 'wait', from: me.id, waiting: false, waiters: this.waiters });
    return me;
  }

  /** @returns true if anything changed that the caller should persist. */
  message(conn, msg) {
    const me = this.conns.get(conn);
    if (!msg || typeof msg.t !== 'string') return false;

    if (!me) return msg.t === 'hello' ? !!this._join(conn, msg.name, msg.pass) : false;
    // Any frame at all counts as being alive — the ping is only the fallback
    // for a member who is watching quietly and touching nothing.
    me.seen = this.now();

    switch (msg.t) {
      case 'ping':
        conn.send({ t: 'pong', c: msg.c, s: this.now() });
        return false;

      case 'state': {
        if (!Number.isFinite(msg.pos)) return false;
        // Stamped HERE, with the relay's clock, which is the one clock every
        // member has measured its offset against. A sender's own timestamp
        // would be in a frame of reference nobody else shares.
        this.state = { paused: !!msg.paused, pos: msg.pos, rate: Number(msg.rate) || 1,
                       from: me.id, seq: msg.seq ?? 0, s: this.now() };
        this._all({ t: 'state', ...this.state });
        return true;
      }

      // What the room is watching. Kept, like `state`, so a joiner five minutes
      // late is told the film rather than left staring at an empty player —
      // which was the whole reason a member could never get a picture.
      //
      // The relay forwards the descriptor and does not read it. The bytes of
      // the film never come near here: every member fetches the source itself,
      // with its own credentials where it has them (see panel.js). A relay
      // stays cheap whether the room is watching a trailer or a 4K remux.
      case 'media': {
        const href = typeof msg.href === 'string' ? msg.href.slice(0, MAX_HREF) : '';
        const want = msg.want && typeof msg.want === 'object' ? msg.want : null;
        if (!href && !want) return false;
        this.media = { href, want, from: me.id, s: this.now() };
        this._all({ t: 'media', ...this.media });
        return true;
      }

      case 'wait': {
        const waiting = !!msg.waiting;
        if (me.waiting === waiting) return false;
        me.waiting = waiting;
        this._all({ t: 'wait', from: me.id, waiting, waiters: this.waiters });
        return true;
      }

      case 'chat': {
        const text = String(msg.text ?? '').slice(0, MAX_CHAT);
        if (!text.trim()) return false;
        this._all({ t: 'chat', from: me.id, name: me.name, text,
                    pos: Number.isFinite(msg.pos) ? msg.pos : null, s: this.now() });
        return false;
      }

      case 'signal': {
        // Point to point, and only inside the room: this carries SDP, which
        // describes the sender's network. Broadcasting it would hand every
        // member's candidate addresses to everyone whether they asked to talk
        // or not.
        for (const [c, m] of this.conns) if (m.id === msg.to) c.send({ t: 'signal', from: me.id, data: msg.data });
        return false;
      }

      case 'claim':
        this.host = me.id;
        this._all({ t: 'host', host: this.host });
        return true;

      default:
        return false;
    }
  }

  /**
   * Drop members nobody has heard from. Each runtime drives this off its own
   * timer (an interval on Node, a Durable Object alarm on Cloudflare) because
   * neither socket layer will report the difference between a member who is
   * quiet and a member who is gone.
   *
   * @returns true if the room changed.
   */
  sweep(maxIdle = IDLE_MS) {
    const cut = this.now() - maxIdle;
    let changed = false;
    for (const [conn, m] of [...this.conns]) {
      if ((m.seen ?? 0) > cut) continue;
      // leave() does the broadcast, the host handover and the wait release.
      if (this.leave(conn)) changed = true;
      try { conn.close?.(); } catch {}
    }
    return changed;
  }

  leave(conn) {
    const me = this.conns.get(conn);
    if (!me) return false;
    this.conns.delete(conn);
    const wasWaiting = me.waiting;
    // The host leaving must not leave the room without one: nobody would
    // re-anchor it, and a buffer wait would never be released.
    if (this.host === me.id) this.host = this.conns.size ? this.members[0].id : null;
    this._all({ t: 'leave', member: { id: me.id, name: me.name }, host: this.host, members: this.members });
    if (wasWaiting) this._all({ t: 'wait', from: me.id, waiting: false, waiters: this.waiters });
    // An empty room is not a room. Forgetting the password here is what lets a
    // code you like be reused next week with a different one -- and forgetting
    // the film keeps a joiner from being sent to last night's episode.
    if (!this.conns.size) { this.pass = null; this.state = null; this.media = null; }
    return true;
  }

  _all(msg)          { for (const c of this.conns.keys()) c.send(msg); }
  _others(not, msg)  { for (const c of this.conns.keys()) if (c !== not) c.send(msg); }
}
