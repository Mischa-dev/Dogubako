/* download.js: highlight the visitor's system, open the box, and show how to watch releases. */
(function () {
  'use strict';
  const { $, reduce, pauseButton, watchView } = window.DGBP;
  const still = reduce();

  /* ---------------------------------------------------------------- the visitor's system */
  const ua = navigator.userAgent || '';
  const plat = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
  const touchMac = /mac/i.test(plat) && navigator.maxTouchPoints > 1; // iPadOS reports a Mac
  const os = /android|iphone|ipad|ipod/i.test(ua) || touchMac ? ''
    : /mac/i.test(plat) ? 'mac'
    : /win/i.test(plat) ? 'windows'
    : /linux|x11|cros/i.test(plat) ? 'linux' : '';
  if (os) {
    const card = $(`.plat[data-os="${os}"]`);
    if (card) card.classList.add('is-you');
  }

  /* ---------------------------------------------------------------- the lid */
  const lid = $('.dl-hero .lid-mark');
  if (lid) { if (still) lid.classList.add('open'); else setTimeout(() => lid.classList.add('open'), 500); }

  /* ---------------------------------------------------------------- Watch → Custom → Releases */
  const watch = $('#watch');
  if (!watch || !window.DGB || !window.DGB.Timeline) return;
  const D = 7000;
  const e = (id) => document.getElementById(id);
  const tap = (tl, el, t) => {
    tl.to(el, 'o', [[0, 0], [t, 0], [t + 60, 0.9], [t + 520, 0]]);
    tl.to(el, 's', [[t, 0.3], [t + 520, 1.5, 'out']]);
  };
  const tl = new window.DGB.Timeline(D);
  tap(tl, e('wTap0'), 600);
  tl.cls(e('wBtn'), 'is-on', [[0, 0], [700, 1], [4900, 0]]);
  tl.to(e('wMenu'), 'o', [[0, 0], [800, 0], [1000, 1], [2400, 1], [2560, 0]]);
  tl.to(e('wMenu'), 'y', [[800, -6], [1100, 0, 'out']]);
  tl.cls(e('wCustom'), 'is-on', [[0, 0], [1800, 1], [2560, 0]]);
  tap(tl, e('wTap1'), 1850);
  tl.to(e('wSub'), 'o', [[0, 0], [2400, 0], [2600, 1], [4800, 1], [5000, 0]]);
  tl.to(e('wSub'), 'x', [[2400, 12], [2750, 0, 'out']]);
  tl.cls(e('wRel'), 'is-on', [[0, 0], [3400, 1], [4500, 0]]);
  tap(tl, e('wTap2'), 3450);
  tl.cls(e('wCbx'), 'is-on', [[0, 0], [3550, 1], [6800, 0]]);
  tap(tl, e('wTap3'), 4550);

  const player = new window.DGB.Player({ duration: D, render: (t) => tl.render(t) });
  const pb = pauseButton($('.watch-play'), 'the animation', (p) => { if (p) player.pause(); else player.play(); });
  watchView(watch, (v) => player.setVisible(v));
  if (still) { player.seek(4200); pb.set(true); }
  else { player.seek(0); pb.set(false); }
})();
