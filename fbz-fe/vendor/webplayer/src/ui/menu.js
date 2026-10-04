// The in-frame settings menu, as a two-level panel instead of one long column.
//
// Both players grew the same way: every feature added another <select> to the
// gear popover until it was taller than a phone in portrait. Every mature
// player answers this the same way -- a root list of groups, each showing its
// current value, opening into a panel with a back arrow (YouTube, bilibili,
// ArtPlayer's `setting`) -- so that is what this is. Two levels, no more: a
// third would need breadcrumbs and nobody's playback settings are that deep.
//
// It is declarative because there are two pages: they describe the groups they
// have and this file owns the DOM, the navigation and the styling. The pages
// never query the menu's elements -- controls that live in a closed panel do
// not exist -- so state lives in the caller and comes back through `value`.
//
// Row kinds: select | range | toggle | text | button | note.

const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

export class SettingsMenu {
  /** @param root the popover element the menu renders into */
  constructor(root) {
    this.root = root;
    this.groups = [];
    this.open = null;          // key of the group whose panel is showing
    root.addEventListener('click', e => {
      // Stopped before anything else. Navigating repaints the popover with
      // innerHTML, so by the time this click reaches document the element it
      // came from is detached -- and the page's "a click outside closes the
      // menu" test is `popover.contains(e.target)`, which is false for a node
      // that is no longer in the tree. Result: opening a panel shut the whole
      // menu. A click inside the menu is never a click outside it, so it has
      // no business reaching document at all.
      e.stopPropagation();
      const g = e.target.closest('[data-group]');
      if (g) { this.open = g.dataset.group; return this.render(); }
      if (e.target.closest('[data-back]')) { this.open = null; return this.render(); }
      const b = e.target.closest('[data-row]');
      if (b?.tagName === 'BUTTON') this._row(b.dataset.row)?.onClick?.();
    });
    root.addEventListener('change', e => this._fire(e, 'onChange'));
    root.addEventListener('input', e => this._fire(e, 'onInput'));
  }

  _fire(e, hook) {
    const el = e.target.closest('[data-row]');
    if (!el) return;
    const row = this._row(el.dataset.row);
    if (!row?.[hook]) return;
    row[hook](el.type === 'checkbox' ? el.checked : el.value);
  }

  _row(id) {
    for (const g of this.groups) for (const r of g.rows || []) if (r.id === id) return r;
    return null;
  }

  /** Replace the whole model and repaint. Called whenever anything changes. */
  set(groups) {
    this.groups = groups.filter(Boolean);
    if (this.open && !this.groups.some(g => g.key === this.open)) this.open = null;
    this.render();
  }

  /** Repaint from the current model (values are read fresh on every paint). */
  render() {
    const g = this.groups.find(x => x.key === this.open);
    this.root.innerHTML = g ? this.panel(g) : this.rootList();
  }

  rootList() {
    return this.groups.map(g => `<button class="mrow" data-group="${esc(g.key)}">
      <span class="mrow-l">${esc(g.label)}</span>
      <span class="mrow-v">${esc(g.value ?? '')}</span><span class="mrow-x">›</span>
    </button>`).join('');
  }

  panel(g) {
    return `<button class="mrow mhead" data-back="1"><span class="mrow-x">‹</span><span class="mrow-l">${esc(g.label)}</span></button>`
      + (g.rows || []).map(r => this.control(r)).join('');
  }

  control(r) {
    const id = `data-row="${esc(r.id ?? '')}"`;
    switch (r.type) {
      case 'select':
        return `<label class="mfield"><span>${esc(r.label)}</span><select ${id}>${
          r.options.map(o => `<option value="${esc(o.value)}"${o.disabled ? ' disabled' : ''}${
            String(o.value) === String(r.value) ? ' selected' : ''}>${esc(o.label)}</option>`).join('')
        }</select></label>`;
      case 'range':
        return `<label class="mfield mrange"><span>${esc(r.label)}<i>${esc(r.hint ?? r.value)}</i></span>
          <input type="range" ${id} min="${r.min}" max="${r.max}" step="${r.step ?? 1}" value="${r.value}"></label>`;
      case 'toggle':
        return `<label class="mfield mtoggle"><span>${esc(r.label)}</span>
          <input type="checkbox" ${id}${r.value ? ' checked' : ''}></label>`;
      case 'text':
        return `<label class="mfield"><span>${esc(r.label)}</span>
          <input type="text" ${id} value="${esc(r.value ?? '')}" placeholder="${esc(r.placeholder ?? '')}" spellcheck="false"></label>`;
      case 'button':
        return `<button class="mrow mact" ${id}${r.disabled ? ' disabled' : ''}><span class="mrow-l">${esc(r.label)}</span>
          <span class="mrow-v">${esc(r.value ?? '')}</span></button>`;
      case 'note':
        return `<div class="mnote">${esc(r.text)}</div>`;
      default: return '';
    }
  }
}

/** The stylesheet both pages need for the markup above. Injected once. */
export function menuStyles() {
  return `
  .menu { padding:6px; min-width:250px; max-width:min(320px,86vw); max-height:min(60vh,420px); overflow-y:auto; }
  .menu .mrow {
    display:flex; align-items:center; gap:10px; width:100%; padding:9px 10px;
    background:transparent; border:0; border-radius:6px; color:inherit; cursor:pointer;
    text-align:left; font:inherit; min-height:40px;
  }
  .menu .mrow:hover:not(:disabled) { background:rgba(255,255,255,.09); }
  .menu .mrow:disabled { opacity:.45; cursor:default; }
  .menu .mrow-l { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .menu .mrow-v { color:var(--dim); font-size:12.5px; max-width:46%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .menu .mrow-x { color:var(--faint); font-size:15px; line-height:1; }
  .menu .mhead { border-bottom:1px solid rgba(255,255,255,.1); border-radius:6px 6px 0 0; margin-bottom:4px; font-weight:600; }
  .menu .mhead .mrow-l { flex:none; }
  .menu .mfield { display:flex; flex-direction:column; gap:5px; padding:7px 10px; }
  .menu .mfield > span { font-size:10.5px; text-transform:uppercase; letter-spacing:.08em; color:var(--faint); font-weight:650; display:flex; justify-content:space-between; gap:8px; }
  .menu .mfield > span i { font-style:normal; text-transform:none; letter-spacing:0; color:var(--dim); font-variant-numeric:tabular-nums; }
  .menu select, .menu input[type=text] { width:100%; background:#0c1014; color:var(--fg); border:1px solid var(--line); border-radius:5px; padding:6px 8px; font-size:13px; }
  .menu select:hover, .menu input[type=text]:hover { border-color:var(--acc-dim); }
  .menu select:focus-visible, .menu input:focus-visible { outline:2px solid var(--acc); outline-offset:1px; }
  .menu input[type=range] { width:100%; accent-color:var(--acc); }
  .menu .mtoggle { flex-direction:row; align-items:center; justify-content:space-between; min-height:38px; cursor:pointer; }
  .menu .mtoggle > span { font-size:13px; text-transform:none; letter-spacing:0; color:inherit; font-weight:400; }
  .menu .mtoggle input { width:36px; height:20px; accent-color:var(--acc); cursor:pointer; }
  .menu .mnote { padding:2px 10px 8px; font-size:11px; line-height:1.5; color:var(--faint); }
  `;
}
