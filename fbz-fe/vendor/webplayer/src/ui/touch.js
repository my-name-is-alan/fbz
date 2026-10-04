// The phone layer for both players.
//
// The desktop chrome assumes two things a phone does not have: a cursor that can
// hover (the volume slider only exists on `:hover`, so on touch it does not exist
// at all) and a pointer precise enough for a 5px seek rail. And it is missing the
// third thing a phone player needs, which no desktop ever asked for: gestures.
//
// So this file adds, without touching either page's existing desktop CSS:
//   * touchStyles()  -- a coarse-pointer override sheet, injected like menuStyles()
//   * attachTouch()  -- the gesture vocabulary every mature mobile player has
//   * fullscreen()   -- fullscreen that keeps the overlays, on every engine
//
// WebKit is covered, which is not the same as covering Safari-the-app: every
// browser on an iPhone is WKWebView underneath, so "Chrome on iOS" has Safari's
// limits exactly. The two that shape this file are that `Element.requestFullscreen`
// does not exist there (only `video.webkitEnterFullscreen`, which would drop every
// overlay we draw) and that writes to `video.volume` are ignored. Both are
// feature-detected, never sniffed.
//
// Gestures only bind to `pointerType === 'touch'`, so a laptop with a
// touchscreen keeps its mouse behaviour intact and gains the touch one.

const near = (a, b, d) => Math.abs(a - b) < d;

/** The stylesheet the pages inject once, next to menuStyles(). */
export function touchStyles() {
  return `
  /* Both players mark the control bar [hidden] until loadedmetadata, but an
     author \`display\` beats the UA's [hidden] rule -- so the bar has always been
     painted over the "drop a file here" placeholder, on every screen size. Same
     trap emby.html hit with its admin button. 0,2,0 beats .vctl's 0,1,0, so no
     !important is needed. */
  .vctl[hidden] { display:none; }

  /* ---- coarse pointer: reachable targets, no hover-only affordances ---- */
  @media (pointer:coarse) {
    .vbtn { width:46px; height:46px; }
    .vbtn svg { width:24px; height:24px; }
    .vbtn:hover { background:transparent; }            /* :hover sticks after a tap */
    .vol { display:none; }                             /* hover-revealed = unreachable */
    .seek { height:28px; padding:0 4px; }
    .seek-rail { height:6px; }
    .seek-fill::after { opacity:1; width:15px; height:15px; right:-7.5px; }
    .vctl {
      gap:4px;
      padding:34px 8px calc(8px + env(safe-area-inset-bottom));
      padding-left:calc(8px + env(safe-area-inset-left));
      padding-right:calc(8px + env(safe-area-inset-right));
      /* The bar is mostly gradient and spacer -- a third of its height is the
         fade above the seek rail, and the middle of the button row is empty.
         All of that used to swallow taps, which on a phone-sized inline stage
         is half the picture. Only the controls themselves take a touch now. */
      pointer-events:none;
    }
    .vctl .seek, .vctl button, .vctl .volume, .vctl .menu-wrap, .vctl .menu { pointer-events:auto; }
    #stage.idle { cursor:auto; }                       /* nothing to hide */
    .card:hover, .gcell:hover, .libcard:hover { transform:none; }
  }

  /* ---- phone width: the video is the page ---- */
  @media (max-width:760px) {
    header { padding:10px 12px; }
    main { padding:0 0 20px; gap:12px; }
    #stage { border-radius:0; border-left:0; border-right:0; }
    aside { padding:0 12px; }
    .vtime { padding:0 4px; font-size:12px; }
    /* A 16/9 stage is only 219px tall at 390px wide -- the strap line does not
       fit above the control bar, and the headline is the part worth keeping. */
    .placeholder { padding:12px; gap:6px; }
    .placeholder svg { width:30px; height:30px; }
    .placeholder b { font-size:13.5px; }
    .placeholder span { display:none; }
  }

  /* ---- settings menu: rows first, then where the panel lives ---- */
  @media (pointer:coarse) {
    .menu .mrow { min-height:50px; padding:11px 12px; font-size:15px; }
    .menu .mfield { padding:10px 12px; }
    /* 16px is also the threshold below which iOS Safari zooms the page on focus,
       and a field you type into with a thumb wants it anyway. */
    .menu select, .menu input[type=text] { padding:11px 10px; font-size:16px; }
    .menu input[type=range] { height:28px; }
    .menu .mtoggle { min-height:50px; }
    .menu .mtoggle input { width:44px; height:26px; }
  }

  /* Portrait phone: a bottom sheet, thumb-high. */
  @media (max-width:760px) {
    .menu {
      position:fixed; left:0; right:0; bottom:0; top:auto;
      width:100%; max-width:none; border-radius:16px 16px 0 0; border-width:1px 0 0;
      max-height:min(70dvh,520px);
      padding:8px 8px calc(10px + env(safe-area-inset-bottom));
      animation:sheet-up .2s cubic-bezier(.2,.7,.3,1);
    }
    .menu::before {
      content:''; display:block; width:38px; height:4px; margin:2px auto 8px;
      border-radius:2px; background:rgba(255,255,255,.28);
    }
  }

  /* Landscape phone -- the case you actually watch in. The desktop popover
     floats mid-frame and lands on top of the seek bar, and a bottom sheet on a
     390px-tall screen would bury the controls. So it docks to the right edge,
     full height, under the thumb already holding that side. */
  @media (pointer:coarse) and (min-width:761px) and (max-height:520px) {
    .menu {
      position:fixed; right:0; top:0; bottom:0; left:auto;
      width:min(330px,46vw); max-width:none; max-height:none; overflow-y:auto;
      border-radius:16px 0 0 16px; border-width:0 0 0 1px;
      padding:10px 10px calc(10px + env(safe-area-inset-bottom));
      padding-right:calc(10px + env(safe-area-inset-right));
      animation:sheet-right .2s cubic-bezier(.2,.7,.3,1);
    }
  }
  @keyframes sheet-up { from { transform:translateY(100%); } to { transform:none; } }
  @keyframes sheet-right { from { transform:translateX(100%); } to { transform:none; } }

  /* ---- the WebKit fullscreen stand-in ----
     A real :fullscreen element is in the top layer; this is a fixed box at the
     same size for engines that will not put a <div> there. Both must drop the
     16/9 letterbox and the border. 100dvh, not 100vh: on a phone the URL bar
     collapses and 100vh would overflow behind it. */
  #stage.pseudo-fs {
    position:fixed; inset:0; z-index:9999;
    width:100%; height:100dvh; max-width:none;
    border:0; border-radius:0; aspect-ratio:auto; background:#000;
  }
  #stage.pseudo-fs video { object-fit:contain; }
  body.fs-lock { overflow:hidden; }

  /* Inline, the page still has to be scrollable through the video, so only the
     horizontal axis is ours -- a vertical drag is the browser's scroll and the
     brightness/volume gestures simply never start (the UA sends pointercancel).
     Fullscreen has nothing to scroll, so there both axes are ours. */
  #stage.touch { touch-action:pan-y; -webkit-user-select:none; user-select:none; -webkit-touch-callout:none; }
  #stage.touch.pseudo-fs, #stage.touch:fullscreen { touch-action:none; }

  /* ---- gesture feedback ---- */
  #stage .g-dim {
    position:absolute; inset:0; z-index:3; background:#000; opacity:0;
    pointer-events:none; transition:opacity .1s linear;
  }
  #stage .g-hud {
    position:absolute; left:50%; top:50%; transform:translate(-50%,-50%);
    z-index:6; pointer-events:none; opacity:0; transition:opacity .18s;
    background:rgba(0,0,0,.72); color:#fff; border-radius:10px;
    padding:10px 16px; font:600 15px/1.3 ui-sans-serif,system-ui,sans-serif;
    text-align:center; white-space:pre-line; backdrop-filter:blur(6px);
    font-variant-numeric:tabular-nums;
  }
  #stage .g-hud.on { opacity:1; }
  #stage .g-hud.top { top:14%; }
  #stage .g-seek {
    position:absolute; top:0; bottom:0; width:34%; z-index:5; pointer-events:none;
    display:grid; place-items:center; opacity:0; transition:opacity .3s;
    background:radial-gradient(closest-side,rgba(255,255,255,.16),transparent);
    color:#fff; font:700 14px/1 ui-sans-serif,system-ui,sans-serif;
  }
  #stage .g-seek.l { left:0; border-radius:0 50% 50% 0/0 50% 50% 0; }
  #stage .g-seek.r { right:0; border-radius:50% 0 0 50%/50% 0 0 50%; }
  #stage .g-seek.on { opacity:1; transition:opacity .06s; }
  `;
}

/**
 * Bind the touch gesture vocabulary to a player stage.
 *
 *   tap              show the controls; tap again to hide
 *   double-tap L/R   seek -/+ 10s, repeated taps accumulate (10, 20, 30...)
 *   long press       2x while held
 *   drag horizontal  scrub, committed on release
 *   drag vertical    left half = brightness, right half = volume
 *
 * @param stage  the positioned wrapper every overlay lives in
 * @param video  the <video>
 * @param opts   { onWake, onTogglePlay, seekStep, log }
 */
export function attachTouch(stage, video, opts = {}) {
  const { onWake = () => {}, seekStep = 10, log = () => {} } = opts;

  const dim = Object.assign(document.createElement('div'), { className: 'g-dim' });
  const hud = Object.assign(document.createElement('div'), { className: 'g-hud' });
  const zoneL = Object.assign(document.createElement('div'), { className: 'g-seek l' });
  const zoneR = Object.assign(document.createElement('div'), { className: 'g-seek r' });
  stage.append(dim, hud, zoneL, zoneR);

  // iOS ignores writes to video.volume entirely -- the hardware buttons own it,
  // and there is no flag to ask. So: write a value, read it back. One probe,
  // restored immediately, and the gesture can then say so instead of sliding a
  // readout up and down while nothing changes.
  const volumeWritable = (() => {
    const was = video.volume;
    try { video.volume = was === 0.5 ? 0.4 : 0.5; const ok = video.volume !== was; video.volume = was; return ok; }
    catch { return false; }
  })();

  let hudTimer = null;
  const showHud = (text, ms = 700, top = false) => {
    hud.textContent = text;
    hud.classList.toggle('top', top);
    hud.classList.add('on');
    clearTimeout(hudTimer);
    if (ms) hudTimer = setTimeout(() => hud.classList.remove('on'), ms);
  };
  const hideHud = () => { clearTimeout(hudTimer); hud.classList.remove('on'); };

  const fmt = s => {
    s = Math.max(0, Math.floor(s));
    const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), x = s % 60;
    return h ? `${h}:${String(m).padStart(2, '0')}:${String(x).padStart(2, '0')}`
             : `${m}:${String(x).padStart(2, '0')}`;
  };
  const seekTo = t => {
    const d = video.duration;
    video.currentTime = Number.isFinite(d) ? Math.max(0, Math.min(d - .1, t)) : Math.max(0, t);
  };

  // ---- brightness ---------------------------------------------------------
  // No web API can touch the panel, so this is a black sheet over the picture.
  // It sits under the controls (z-index 3 vs 4) so the OSD stays legible while
  // the frame dims -- which is what you actually want at 2am.
  let bright = 1;
  const setBright = v => { bright = Math.max(.15, Math.min(1, v)); dim.style.opacity = String(1 - bright); };

  // ---- gesture state ------------------------------------------------------
  let g = null;                 // live drag: {x0,y0,t0,mode,base,left}
  let holdTimer = null, held = false, rateWas = 1;
  let tapTime = 0, tapX = 0, tapCount = 0, tapSide = 0, hideTimer = null;

  const cancelHold = () => { clearTimeout(holdTimer); holdTimer = null; };
  const endHold = () => {
    if (!held) return;
    held = false; video.playbackRate = rateWas; hideHud();
  };

  stage.addEventListener('pointerdown', e => {
    if (e.pointerType !== 'touch') return;
    if (e.target.closest('.vctl, .menu')) return;      // the controls own their taps
    const r = stage.getBoundingClientRect();
    g = { x0: e.clientX, y0: e.clientY, t0: performance.now(), mode: null, w: r.width, h: r.height,
          left: e.clientX - r.left < r.width / 2, base: 0 };
    stage.setPointerCapture(e.pointerId);
    holdTimer = setTimeout(() => {
      if (!g || g.mode) return;
      held = true; rateWas = video.playbackRate; video.playbackRate = 2;
      showHud('2× 快进中', 0, true);
    }, 480);
  });

  stage.addEventListener('pointermove', e => {
    if (!g || e.pointerType !== 'touch') return;
    const dx = e.clientX - g.x0, dy = e.clientY - g.y0;
    if (!g.mode) {
      if (held) return;                                 // a hold is not a drag
      if (Math.hypot(dx, dy) < 14) return;
      cancelHold();
      g.mode = Math.abs(dx) > Math.abs(dy) ? 'seek' : (g.left ? 'bright' : 'vol');
      g.base = g.mode === 'seek' ? video.currentTime : g.mode === 'vol' ? video.volume : bright;
      onWake();
    }
    if (g.mode === 'seek') {
      // A full swipe across the frame is 90s -- fine control near the thumb,
      // and it does not depend on duration (a 3h film would make 1px = 20s).
      const d = video.duration;
      const delta = (dx / g.w) * 90;
      const t = Math.max(0, Math.min(Number.isFinite(d) ? d : Infinity, g.base + delta));
      g.pending = t;
      showHud(`${fmt(t)}  ${delta >= 0 ? '+' : '−'}${fmt(Math.abs(delta))}`, 0);
    } else if (g.mode === 'vol') {
      if (!volumeWritable) return showHud('音量请用手机侧键', 0);
      const v = Math.max(0, Math.min(1, g.base - dy / (g.h * .7)));
      video.volume = v; video.muted = v === 0;
      showHud(`音量 ${Math.round(v * 100)}%`, 0);
    } else {
      setBright(g.base - dy / (g.h * .7));
      showHud(`亮度 ${Math.round(bright * 100)}%`, 0);
    }
  });

  const finish = e => {
    if (!g || e.pointerType !== 'touch') return;
    cancelHold();
    const wasHeld = held;
    endHold();
    const { mode } = g;
    const pending = g.pending;
    g = null;
    if (mode === 'seek' && pending != null) { seekTo(pending); showHud(fmt(pending), 500); onWake(); return; }
    // Re-show rather than schedule a bare hide: a plain timer would still be
    // pending when the next gesture starts and would blank ITS readout 500ms in.
    if (mode) { showHud(hud.textContent, 500); return; }
    if (wasHeld) return;                                // a long press is not a tap

    // ---- taps --------------------------------------------------------------
    const now = performance.now();
    const r = stage.getBoundingClientRect();
    const rel = (e.clientX - r.left) / r.width;
    const side = rel < .34 ? -1 : rel > .66 ? 1 : 0;

    if (now - tapTime < 320 && near(e.clientX, tapX, 60) && side !== 0 && side === tapSide) {
      // Second (and third, and fourth) tap in the same edge zone.
      clearTimeout(hideTimer);
      tapCount++;
      // The first tap of the run only arms the gesture, so the running total is
      // one step behind the tap count -- tap twice and you have moved 10s, not 20.
      const jump = side * seekStep * (tapCount - 1);
      seekTo(video.currentTime + side * seekStep);
      const z = side < 0 ? zoneL : zoneR;
      z.textContent = `${side < 0 ? '«' : '»'} ${Math.abs(jump)}s`;
      z.classList.add('on');
      clearTimeout(z._t); z._t = setTimeout(() => z.classList.remove('on'), 420);
      onWake();
    } else if (now - tapTime < 320 && near(e.clientX, tapX, 60) && side === 0) {
      clearTimeout(hideTimer);
      tapCount = 0;
      opts.onTogglePlay?.();
    } else {
      tapCount = side === 0 ? 0 : 1;
      if (stage.classList.contains('idle')) {
        onWake();                                       // first tap always reveals
      } else {
        // Deferred so a second tap can cancel it -- otherwise every double-tap
        // seek would blink the controls off on its way through.
        hideTimer = setTimeout(() => stage.classList.add('idle'), 320);
      }
    }
    tapTime = now; tapX = e.clientX; tapSide = side;
  };
  stage.addEventListener('pointerup', finish);
  stage.addEventListener('pointercancel', e => { cancelHold(); endHold(); g = null; hideHud(); });

  // Without this the browser's own long-press wins: text selection and the
  // image callout menu, neither of which means anything on a video surface.
  stage.classList.add('touch');
  stage.addEventListener('contextmenu', e => { if (!e.target.closest('.vctl, .menu')) e.preventDefault(); });

  keepAwake(video, log);
  return { setBright, get brightness() { return bright; }, volumeWritable };
}

/**
 * Keep the screen on while playing -- without it the phone dims mid-film.
 * Chrome/Edge 84, Firefox 126, Safari 16.4. Where it is missing there is no
 * other lever, so the failure is silent by design.
 */
export function keepAwake(video, log = () => {}) {
  if (!navigator.wakeLock) return;
  let lock = null;
  const acquire = async () => {
    if (lock || video.paused || document.visibilityState !== 'visible') return;
    try { lock = await navigator.wakeLock.request('screen'); lock.addEventListener('release', () => { lock = null; }); }
    catch (e) { log(`屏幕常亮失败: ${e.message}`, 'warn'); }
  };
  const release = () => { lock?.release().catch(() => {}); lock = null; };
  video.addEventListener('play', acquire);
  video.addEventListener('pause', release);
  video.addEventListener('ended', release);
  // A lock dies whenever the tab is hidden and is not restored on its own.
  document.addEventListener('visibilitychange', () => document.visibilityState === 'visible' ? acquire() : release());
}

/**
 * Fullscreen the STAGE, never the <video>.
 *
 * The distinction is the whole point: every overlay this player draws -- ASS and
 * PGS subtitles, danmaku, the Anime4K output -- is a sibling canvas of the video,
 * not a child of it. Fullscreening the video element would show the decoded frame
 * and drop all of them. The stage is their common parent, so it is what goes into
 * the top layer.
 *
 * Which is exactly what an iPhone will not do: `Element.requestFullscreen` does
 * not exist there, and the only fullscreen WebKit offers on that device is
 * `video.webkitEnterFullscreen` -- the native player, which knows nothing about
 * our canvases. So the fallback is NOT that lesser fullscreen; it is a fixed,
 * viewport-sized box that keeps every layer and looks the same.
 *
 * On a phone it also asks for landscape. A desktop has nothing to rotate and
 * rejects the lock, which is not an error worth surfacing.
 */
export function fullscreen(stage, opts = {}) {
  const { log = () => {}, onChange = () => {} } = opts;
  const isPseudo = () => stage.classList.contains('pseudo-fs');
  const active = () => document.fullscreenElement === stage || isPseudo();

  // Real fullscreen fires its own event; the stand-in has none and so reports
  // itself. Both land in the same callback, so a caller has only one path to
  // repaint from however the state was left -- Esc, back gesture, or the button.
  document.addEventListener('fullscreenchange', () => onChange(active()));

  const enterPseudo = () => {
    stage.classList.add('pseudo-fs');
    document.body.classList.add('fs-lock');
    // Without a history entry the Android back gesture leaves the PAGE rather
    // than the fullscreen, losing the playback position.
    history.pushState({ fs: 1 }, '');
    addEventListener('popstate', onPop);
    onChange(true);
  };
  const exitPseudo = fromPop => {
    stage.classList.remove('pseudo-fs');
    document.body.classList.remove('fs-lock');
    removeEventListener('popstate', onPop);
    if (!fromPop && history.state?.fs) history.back();
    onChange(false);
  };
  function onPop() { if (isPseudo()) exitPseudo(true); }

  async function enter() {
    // Feature-detected, never sniffed: iPad Safari has had requestFullscreen
    // since 16.4 and only iPhone still lacks it, so the test is for the method.
    if (stage.requestFullscreen) {
      try { await stage.requestFullscreen({ navigationUI: 'hide' }); }
      catch (e) { log(`全屏降级为页内全屏: ${e.message}`, 'warn'); enterPseudo(); }
    } else enterPseudo();
    // Chrome wants an active fullscreen for this; WebKit has no
    // ScreenOrientation.lock at all. Both reject, and a rejected lock is not
    // worth surfacing -- the viewer can still turn the phone.
    try { await screen.orientation.lock('landscape'); } catch {}
  }

  async function exit() {
    try { screen.orientation.unlock(); } catch {}
    if (document.fullscreenElement) { try { await document.exitFullscreen(); } catch {} }
    if (isPseudo()) exitPseudo();
  }

  // Esc leaves real fullscreen by itself; the stand-in has to be told.
  addEventListener('keydown', e => { if (e.key === 'Escape' && isPseudo()) exit(); });

  return { active, toggle: () => active() ? exit() : enter(), enter, exit };
}
