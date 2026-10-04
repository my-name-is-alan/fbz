// The danmaku panel: everything about comments that needs typing or choosing,
// on the page instead of inside the settings popover.
//
// It used to live in the gear menu, as one group among subtitles and playback
// rate, and that was the wrong home for three reasons that all have the same
// shape -- a popover is a place to FLIP something, not to compose in:
//
//   * the server URL is a URL. Typing one into a field inside a popover that
//     closes on an outside click, over the video, is hostile.
//   * search results are a LIST. The menu can only offer a <select>, so several
//     series of a dozen episodes each collapsed into 60 identical-looking lines
//     -- and when the search returned exactly one candidate the old code took it
//     silently, so being handed the wrong episode looked like broken danmaku.
//   * you cannot see the picture while the popover is open, which is the one
//     thing that tells you whether the comments you just loaded line up.
//
// So the parts that need input and choice are a card in the sidebar, always
// visible, and the toolbar button goes back to being a plain on/off switch.
// Sizes and speeds stay here too rather than in the menu: two homes for one
// feature is the thing that made it confusing, and a single owner means no
// two-way repaint to keep in sync.
//
// Shared by both players the same way src/ui/menu.js is, so play.html (Emby)
// and index.html (local files) cannot drift. It owns the whole feature --
// settings, the per-item track cache, the lookups, the file input -- and the
// pages just mount it.

import { source, parseAny } from './index.js';
import { loadSettings, saveSettings, loadTrack, saveTrack, forgetTrack } from './store.js';

export function panelStyles() {
  return `
  .dmkp h2 { display:flex; align-items:center; justify-content:space-between; gap:8px; }
  .dmkp-sw { display:flex; align-items:center; gap:6px; text-transform:none; letter-spacing:0; font-size:11.5px; color:var(--dim); cursor:pointer; }
  .dmkp-sw input { accent-color:var(--acc); width:14px; height:14px; margin:0; cursor:pointer; }
  .dmkp-sw.off { color:var(--faint); }

  .dmkp-now { display:flex; align-items:baseline; gap:8px; font-size:12.5px; margin-bottom:10px; }
  .dmkp-now b { font-weight:600; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .dmkp-now .n { color:var(--faint); font-variant-numeric:tabular-nums; white-space:nowrap; }
  .dmkp-now .x { margin-left:auto; background:none; border:0; padding:0 2px; color:var(--faint); cursor:pointer; font-size:12px; }
  .dmkp-now .x:hover { color:var(--err); }
  .dmkp-now.empty { color:var(--faint); }
  .dmkp-now.busy { color:var(--dim); }
  .dmkp-now.err  { color:var(--err); display:block; overflow-wrap:anywhere; }

  .dmkp input[type=text] {
    width:100%; background:var(--bg); border:1px solid var(--line); border-radius:6px;
    padding:6px 9px; font-size:12.5px; min-width:0;
  }
  .dmkp input[type=text]:focus { outline:none; border-color:var(--acc-dim); }
  .dmkp input::placeholder { color:var(--faint); }
  .dmkp-q { display:flex; gap:6px; margin-top:6px; }
  .dmkp-q .btn { padding:6px 11px; font-size:12.5px; flex-shrink:0; }
  .dmkp .btn:disabled { opacity:.38; cursor:default; }
  .dmkp .btn:disabled:hover { border-color:var(--line); background:var(--panel-2); }

  /* Results: series first, episodes under it. Scrolls rather than growing the
     sidebar past the video -- a popular title answers with 50+ episodes. */
  .dmkp-res { margin-top:10px; max-height:236px; overflow-y:auto; overscroll-behavior:contain; }
  .dmkp-res:empty { display:none; }
  .dmkp-grp + .dmkp-grp { margin-top:8px; border-top:1px solid var(--line); padding-top:8px; }
  .dmkp-grp > b { display:block; font-size:12px; font-weight:600; margin-bottom:3px; overflow-wrap:anywhere; }
  .dmkp-grp > b i { font-style:normal; font-size:10.5px; color:var(--faint); font-weight:500; margin-left:5px; }
  .dmkp-ep {
    display:block; width:100%; text-align:left; background:none; border:0; cursor:pointer;
    padding:3px 6px; border-radius:5px; font-size:12.5px; color:var(--dim);
    overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
  }
  .dmkp-ep:hover { background:var(--panel-2); color:var(--fg); }
  .dmkp-ep.on { color:var(--acc); }

  .dmkp-more { margin-top:10px; }
  .dmkp-more summary {
    cursor:pointer; list-style:none; font-size:10.5px; text-transform:uppercase;
    letter-spacing:.09em; color:var(--faint); font-weight:650;
    display:flex; align-items:center; justify-content:space-between;
  }
  .dmkp-more summary::-webkit-details-marker { display:none; }
  .dmkp-more summary::after { content:'▾'; transition:.15s; }
  .dmkp-more[open] summary::after { transform:rotate(180deg); }
  .dmkp-more > div { margin-top:9px; display:grid; gap:7px; }
  .dmkp-f { display:grid; grid-template-columns:64px minmax(0,1fr) auto; align-items:center; gap:8px; font-size:12.5px; color:var(--dim); }
  .dmkp-f input[type=range] { width:100%; accent-color:var(--acc); height:4px; cursor:pointer; margin:0; }
  .dmkp-f .h { color:var(--faint); font-size:11.5px; font-variant-numeric:tabular-nums; white-space:nowrap; min-width:44px; text-align:right; }
`;
}

/** The sliders, as data -- six near-identical rows are not worth six blocks. */
const KNOBS = [
  { k: 'opacity',  label: '不透明度', min: 0.1, max: 1,   step: 0.05, fmt: v => `${Math.round(v * 100)}%` },
  { k: 'fontSize', label: '字号',     min: 12,  max: 48,  step: 1,    fmt: v => `${v}` },
  { k: 'speed',    label: '速度',     min: 60,  max: 320, step: 10,   fmt: v => `${v}` },
  { k: 'area',     label: '显示区域', min: 0.2, max: 1,   step: 0.1,  fmt: v => `上 ${Math.round(v * 100)}%` },
  { k: 'limit',    label: '每秒上限', min: 0,   max: 40,  step: 1,    fmt: v => (v ? `${v} 条` : '不限') },
];

function el(tag, props = {}, ...kids) {
  const n = Object.assign(document.createElement(tag), props);
  n.append(...kids.filter(Boolean));
  return n;
}

export class DanmakuPanel {
  /**
   * @param layer  the DanmakuLayer to drive
   * @param button the toolbar danmaku button, kept in sync with the switch
   */
  constructor(layer, { key = '', hint = '', log = () => {}, button = null } = {}) {
    this.layer = layer;
    this.log = log;
    this.button = button;
    this.key = key;
    this.s = loadSettings();
    this.name = '';       // where the loaded track came from
    this.busy = '';       // in-flight status
    this.err = '';        // last failure, shown in place of the status
    this.results = [];    // [{title, kind, episodes:[{id,title}]}]
    this.chosen = null;   // episode id currently loaded, so the list can mark it

    this.build();
    if (hint) this.q.value = hint;
    this.paint();
  }

  // --- DOM ------------------------------------------------------------------

  build() {
    this.sw = el('input', { type: 'checkbox', checked: this.s.on });
    this.sw.addEventListener('change', () => this.set({ on: this.sw.checked }));
    this.swLabel = el('label', { className: 'dmkp-sw' }, this.sw, el('span', { textContent: '显示' }));

    this.now = el('div', { className: 'dmkp-now' });

    this.api = el('input', { type: 'text', placeholder: '弹幕服务地址 https://…', value: this.s.api || '' });
    const saveApi = () => { const v = this.api.value.trim(); if (v !== this.s.api) this.set({ api: v }); else this.paint(); };
    this.api.addEventListener('change', saveApi);
    this.api.addEventListener('keydown', e => { if (e.key === 'Enter') { saveApi(); this.q.focus(); } });

    this.q = el('input', { type: 'text', placeholder: '按片名搜索…' });
    this.q.addEventListener('keydown', e => { if (e.key === 'Enter') this.search(); });
    this.q.addEventListener('input', () => this.paint());
    this.go = el('button', { className: 'btn', textContent: '搜索', type: 'button' });
    this.go.addEventListener('click', () => this.search());

    this.res = el('div', { className: 'dmkp-res' });
    // One listener for the whole list instead of one per episode: a search can
    // answer with hundreds of rows and the list is rebuilt on every search.
    this.res.addEventListener('click', e => {
      const b = e.target.closest('.dmkp-ep');
      if (b) this.fetchEpisode(b.dataset.id, b.dataset.label);
    });

    this.file = Object.assign(document.createElement('input'),
      { type: 'file', accept: '.xml,.json,text/xml,application/json', hidden: true });
    this.file.addEventListener('change', () => { this.importFile(this.file.files[0]); this.file.value = ''; });
    const fileBtn = el('button', { className: 'btn', textContent: '载入本地文件 (.xml / .json)', type: 'button' });
    fileBtn.addEventListener('click', () => this.file.click());

    this.knobs = KNOBS.map(kn => {
      const hint = el('span', { className: 'h' });
      const input = el('input', { type: 'range', min: kn.min, max: kn.max, step: kn.step, value: this.s[kn.k] });
      input.addEventListener('input', () => {
        // Repaint the hint straight from the event so dragging stays live even
        // though set() rebuilds the renderer.
        hint.textContent = kn.fmt(Number(input.value));
        this.set({ [kn.k]: Number(input.value) });
      });
      return { kn, input, hint, row: el('div', { className: 'dmkp-f' }, el('span', { textContent: kn.label }), input, hint) };
    });

    this.blockInput = el('input', { type: 'text', placeholder: '屏蔽词：前方高能,/^\\d+$/', value: (this.s.block || []).join(',') });
    this.blockInput.addEventListener('change', () =>
      this.set({ block: this.blockInput.value.split(',').map(x => x.trim()).filter(Boolean) }));

    this.el = el('div', { className: 'card dmkp' },
      el('h2', {}, el('span', { textContent: '弹幕' }), this.swLabel),
      this.now,
      this.api,
      el('div', { className: 'dmkp-q' }, this.q, this.go),
      this.res,
      el('details', { className: 'dmkp-more' },
        el('summary', { textContent: '外观与过滤' }),
        el('div', {}, ...this.knobs.map(k => k.row), this.blockInput, fileBtn,
          el('p', { className: 'note', textContent: '第三方弹幕站不放行浏览器跨域、且要密钥，网页无法直连——填一个你自己部署的、兼容弹弹play接口的地址。留空则只用本地文件。' }))),
      this.file);
  }

  // --- state ----------------------------------------------------------------

  /** The single write path for settings: persist, re-render the layer, repaint. */
  set(patch) {
    this.s = { ...this.s, ...patch };
    saveSettings(this.s);
    this.layer?.apply(this.s);
    this.paint();
  }

  /** Point the panel at a different item (index.html opens files one by one). */
  async setItem({ key = '', hint = '' } = {}) {
    this.key = key;
    this.results = [];
    this.chosen = null;
    this.name = '';
    this.err = '';
    if (hint) this.q.value = hint;
    this.layer?.setComments([], '');
    await this.restore();
  }

  /** Bring back the track this item was matched to last time, if any. */
  async restore() {
    const saved = await loadTrack(this.key);
    if (saved?.comments?.length) { this.name = saved.name; this.layer?.setComments(saved.comments, saved.name); }
    this.set({});      // applies stored settings to the layer and repaints
  }

  paint() {
    const n = this.layer?.count || 0;
    this.sw.checked = this.s.on;
    this.swLabel.classList.toggle('off', !this.s.on);
    this.button?.classList.toggle('on', this.s.on && n > 0);
    this.button?.setAttribute('aria-pressed', String(this.s.on && n > 0));

    this.now.className = 'dmkp-now' + (this.err ? ' err' : this.busy ? ' busy' : n ? '' : ' empty');
    this.now.replaceChildren(...(
      this.err ? [document.createTextNode(this.err)]
      : this.busy ? [document.createTextNode(this.busy)]
      : n ? [el('b', { textContent: this.name || '弹幕' }), el('span', { className: 'n', textContent: `${n} 条` }), this.clearBtn()]
      : [document.createTextNode('未加载 · 搜索、载入文件，或把文件拖进画面')]));

    this.go.disabled = !!this.busy || !this.q.value.trim();
    for (const { kn, input, hint } of this.knobs) { input.value = this.s[kn.k]; hint.textContent = kn.fmt(Number(this.s[kn.k])); }
    this.paintResults();
  }

  clearBtn() {
    const b = el('button', { className: 'x', title: '清除本片弹幕', textContent: '✕', type: 'button' });
    b.addEventListener('click', () => this.clear());
    return b;
  }

  paintResults() {
    this.res.replaceChildren(...this.results.map(a => {
      const group = el('div', { className: 'dmkp-grp' },
        el('b', {}, document.createTextNode(a.title), a.kind ? el('i', { textContent: a.kind }) : null));
      for (const e of a.episodes) {
        // The full "Series · Episode" only as the tooltip and the loaded name:
        // inside a group the series is already on the heading above, and
        // repeating it truncates every row at the same useless prefix.
        const label = `${a.title} · ${e.title}`;
        const b = el('button', {
          className: 'dmkp-ep' + (String(e.id) === String(this.chosen) ? ' on' : ''),
          type: 'button', textContent: e.title, title: label,
        });
        b.dataset.id = e.id;
        b.dataset.label = label;
        group.append(b);
      }
      return group;
    }));
  }

  // --- the lookups ----------------------------------------------------------

  async search() {
    const keyword = this.q.value.trim();
    if (!keyword || this.busy) return;
    this.results = [];
    this.err = '';
    this.busy = '搜索中…';
    this.paint();
    try {
      this.results = await source.search(this.s.api, keyword);
      this.busy = '';
      if (!this.results.length) this.err = `没有搜到「${keyword}」，换个片名试试`;
    } catch (e) {
      this.busy = '';
      this.err = e.message;
      this.log(`弹幕搜索失败: ${e.message}`, 'warn');
    }
    this.paint();
  }

  async fetchEpisode(id, label) {
    if (this.busy) return;
    this.err = '';
    this.busy = '下载弹幕…';
    this.paint();
    try {
      const list = await source.comments(this.s.api, id);
      if (!list.length) throw new Error('这一集还没有弹幕');
      this.chosen = id;
      this.setTrack(list, label || `弹幕 ${id}`);
    } catch (e) {
      this.busy = '';
      this.err = e.message;
      this.log(`弹幕下载失败: ${e.message}`, 'warn');
      this.paint();
    }
  }

  async importFile(file) {
    if (!file) return;
    try {
      const list = parseAny(await file.text());
      if (!list.length) throw new Error('文件里没有可识别的弹幕');
      this.chosen = null;
      this.setTrack(list, file.name);
    } catch (e) {
      this.err = e.message;
      this.log(`弹幕文件读取失败: ${e.message}`, 'warn');
      this.paint();
    }
  }

  setTrack(list, name) {
    this.name = name;
    this.busy = '';
    this.err = '';
    this.layer?.setComments(list, name);
    if (this.key) saveTrack(this.key, name, list);
    this.log(`弹幕已载入: ${name}（${list.length} 条）`);
    this.set({ on: true });
  }

  async clear() {
    this.chosen = null;
    this.name = '';
    this.err = '';
    this.layer?.setComments([], '');
    await forgetTrack(this.key);
    this.paint();
  }

  /** The toolbar button: a plain on/off once a track is loaded. */
  toggle() {
    if (this.layer?.count) return this.set({ on: !this.s.on });
    // Nothing to toggle, so send the press to whatever is actually in the way:
    // the server address while it is still blank (searching without one only
    // ever produces an error), the search box once it is filled in.
    this.el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    (this.s.api ? this.q : this.api).focus();
  }
}
