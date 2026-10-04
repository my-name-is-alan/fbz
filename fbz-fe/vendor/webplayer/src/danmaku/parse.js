// Danmaku comes in two shapes and one of them is a lie about the other.
//
//   bilibili XML   <d p="time,mode,size,color,ctime,pool,uidhash,rowid,?">text</d>
//   dandanplay     { comments: [ { cid, p: "time,mode,color,uid", m: "text" } ] }
//
// Note the two `p` layouts are NOT the same list with a field removed: colour is
// index 3 in bilibili and index 2 in dandanplay, so a parser that guesses gets
// white comments on one source and garbage on the other. Both are parsed into
// one neutral shape here, and only here:
//
//   { time: seconds, mode: 'rtl'|'ltr'|'top'|'bottom', color: '#rrggbb', text }
//
// Everything downstream (styling, filtering, the renderer) works on that shape,
// which is also what makes this file the one part of the feature node can test.

/** bilibili/dandanplay mode number -> the renderer's direction name. */
const MODE = { 1: 'rtl', 2: 'rtl', 3: 'rtl', 4: 'bottom', 5: 'top', 6: 'ltr' };

/** 24-bit int (16777215) -> '#ffffff'. Anything out of range falls back to white. */
export function intToHex(n) {
  const v = Number(n);
  if (!Number.isFinite(v) || v < 0 || v > 0xffffff) return '#ffffff';
  return '#' + (v | 0).toString(16).padStart(6, '0');
}

/**
 * bilibili's XML dump (the `list.so` / `dm/web` payload, and what most exporters
 * write). Parsed with regex rather than DOMParser so it runs in node too --
 * these files are machine-written and flat, there is no nesting to get wrong.
 */
export function parseBiliXml(text) {
  const out = [];
  for (const m of String(text).matchAll(/<d\b[^>]*\bp="([^"]*)"[^>]*>([\s\S]*?)<\/d>/g)) {
    const f = m[1].split(',');
    const mode = MODE[Number(f[1])];
    if (!mode) continue;                      // 7=positioned, 8=code, 9=BAS: no renderer
    const time = Number(f[0]);
    if (!Number.isFinite(time)) continue;
    const body = unescapeXml(m[2]).trim();
    if (body) out.push({ time, mode, color: intToHex(f[3]), text: body });
  }
  return out;
}

/**
 * dandanplay's JSON, and the several self-hosted servers that copy its shape.
 * Accepts the envelope ({comments:[...]}) or a bare array, and a `p` that is
 * either the documented comma string or an already-split array -- proxies
 * disagree about that one and both are unambiguous.
 */
export function parseDandan(input) {
  const raw = Array.isArray(input) ? input : Array.isArray(input?.comments) ? input.comments : [];
  const out = [];
  for (const c of raw) {
    const f = Array.isArray(c?.p) ? c.p : String(c?.p ?? '').split(',');
    const time = Number(f[0]);
    const mode = MODE[Number(f[1])];
    const body = String(c?.m ?? '').trim();
    if (!Number.isFinite(time) || !mode || !body) continue;
    out.push({ time, mode, color: intToHex(f[2]), text: body });
  }
  return out;
}

/** Sniff the format from the bytes, so the caller never has to ask the user. */
export function parseAny(text) {
  const s = String(text).trim();
  if (s.startsWith('<')) return parseBiliXml(s);
  try { return parseDandan(JSON.parse(s)); } catch { return []; }
}

function unescapeXml(s) {
  return s.replace(/&(#x?[0-9a-fA-F]+|amp|lt|gt|quot|apos);/g, (all, e) => {
    if (e === 'amp') return '&'; if (e === 'lt') return '<'; if (e === 'gt') return '>';
    if (e === 'quot') return '"'; if (e === 'apos') return "'";
    const n = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
    return Number.isFinite(n) ? String.fromCodePoint(n) : all;
  });
}

/**
 * Thin a comment list down to `limit` comments per second, keeping the earliest
 * in each second. A popular episode dumps 3000+ comments; past a few dozen on
 * screen at once they are unreadable anyway and the renderer drops frames, so
 * this is a legibility control that happens to also be the performance control.
 * `limit <= 0` means no cap.
 */
export function throttle(list, limit) {
  if (!(limit > 0)) return list;
  const perSec = new Map();
  const out = [];
  for (const c of list) {
    const k = Math.floor(c.time);
    const n = perSec.get(k) || 0;
    if (n >= limit) continue;
    perSec.set(k, n + 1);
    out.push(c);
  }
  return out;
}

/** Drop comments matching any blocked word (plain substring) or /regex/ entry. */
export function block(list, words) {
  const terms = (words || []).map(w => String(w).trim()).filter(Boolean);
  if (!terms.length) return list;
  const res = [], subs = [];
  for (const t of terms) {
    const m = /^\/(.*)\/(\w*)$/.exec(t);
    if (m) { try { res.push(new RegExp(m[1], m[2])); continue; } catch {} }
    subs.push(t.toLowerCase());
  }
  return list.filter(c => {
    const low = c.text.toLowerCase();
    return !subs.some(s => low.includes(s)) && !res.some(r => r.test(c.text));
  });
}
