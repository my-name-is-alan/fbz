// Two kinds of danmaku state outlive the page, and they want different homes.
//
//   settings  -- one small object, read synchronously while the controls render
//                -> localStorage, same as every other LinWeb preference.
//   the track -- which source an item was matched to, plus the comments
//                themselves (an episode is 3k+ of them; a file the viewer
//                dragged in cannot be re-read without another drag)
//                -> IndexedDB, which is async but is not a 5 MB shared bucket.
//
// Caching the comments is what makes "持久化" mean something to the viewer: the
// second time an episode is opened the danmaku is simply there, with no match
// round-trip and no network at all.

const SETTINGS_KEY = 'linweb:danmaku';
const DB_NAME = 'linweb-danmaku';
const STORE = 'tracks';

export const DEFAULTS = {
  on: false,          // opt-in, like every other overlay in this player
  opacity: 0.85,
  fontSize: 24,       // px at a 1080p-tall stage; scaled to the real height
  speed: 144,         // px/s, the renderer's own default
  area: 1,            // fraction of the stage height comments may occupy
  limit: 8,           // comments per second kept; 0 = keep all
  bold: true,
  block: [],          // words, or /regex/ entries
  api: '',            // dandanplay-compatible base URL; empty = offline only
};

export function loadSettings() {
  try { return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) || '{}') }; }
  catch { return { ...DEFAULTS }; }
}

export function saveSettings(s) {
  try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(s)); } catch {}
}

// --- the per-item track cache ----------------------------------------------

let dbp = null;
function db() {
  if (dbp) return dbp;
  dbp = new Promise((res, rej) => {
    if (!globalThis.indexedDB) return rej(new Error('no IndexedDB'));
    const r = indexedDB.open(DB_NAME, 1);
    r.onupgradeneeded = () => { if (!r.result.objectStoreNames.contains(STORE)) r.result.createObjectStore(STORE, { keyPath: 'key' }); };
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  });
  return dbp;
}

const tx = async (mode, fn) => {
  const d = await db();
  return new Promise((res, rej) => {
    const t = d.transaction(STORE, mode);
    const req = fn(t.objectStore(STORE));
    req.onsuccess = () => res(req.result);
    req.onerror = () => rej(req.error);
  });
};

/** @returns {Promise<{key,name,at,comments}|null>} */
export async function loadTrack(key) {
  if (!key) return null;
  try { return (await tx('readonly', s => s.get(key))) || null; } catch { return null; }
}

export async function saveTrack(key, name, comments) {
  if (!key) return;
  try { await tx('readwrite', s => s.put({ key, name, at: Date.now(), comments })); } catch {}
}

export async function forgetTrack(key) {
  if (!key) return;
  try { await tx('readwrite', s => s.delete(key)); } catch {}
}
