/* agents.js: the MCP exchange, full vs compact discovery, and the policy log. */
(function () {
  'use strict';
  const { $, $$, reduce, pauseButton, watchView, show } = window.DGBP;
  const still = reduce();

  /* ---------------------------------------------------------------- the exchange */
  const D = 16000;
  const appear = (el, tl, t, dy) => {
    tl.to(el, 'o', [[0, 0], [t, 0], [t + 320, 1, 'out']]);
    if (dy !== 0) tl.to(el, 'y', [[t, dy || 10], [t + 480, 0, 'out']]);
  };
  function spin(tl, spinEl, okEl, t0, t1) {
    tl.to(spinEl, 'o', [[0, 0], [t0, 1, 'hold'], [t1, 0, 'hold']]);
    tl.to(spinEl, 'r', [[t0, 0], [t1, (t1 - t0) * 0.6, 'lin']]);
    tl.to(okEl, 'o', [[0, 0], [t1, 1, 'hold']]);
    tl.to(okEl, 's', [[t1, 0.4], [t1 + 260, 1, 'back']]);
  }
  function build() {
    const tl = new window.DGB.Timeline(D);
    const e = (id) => document.getElementById(id);
    tl.to(e('agBody'), 'o', [[15200, 1], [15900, 0]]);
    appear(e('agU'), tl, 300);
    // search
    appear(e('agS'), tl, 1300);
    spin(tl, e('agSSpin'), e('agSOk'), 1300, 2500);
    appear(e('agSR'), tl, 2600, 0);
    // upload
    appear(e('agP'), tl, 3600);
    spin(tl, e('agPSpin'), e('agPOk'), 3600, 4600);
    appear(e('agPR'), tl, 4700, 0);
    // run
    appear(e('agR'), tl, 5700);
    spin(tl, e('agRSpin'), e('agROk'), 5700, 9500);
    tl.text(e('agRSt'), [[0, 'running'], [9500, 'done']]);
    tl.to(e('agBar'), 'sx', [[0, 0], [6200, 0], [9400, 1, 'inOut']]);
    appear(e('agFile'), tl, 9700, 6);
    // reply
    appear(e('agB'), tl, 10600);
    return tl;
  }
  show({
    root: $('#agShow'),
    duration: D,
    build,
    stillChapter: 3,
    chapters: [
      { label: 'Search', t: 0, still: 3000, cap: 'The agent searches for “transcribe” and gets one match, instead of reading every tool’s schema.' },
      { label: 'Upload', t: 3500, still: 5100, cap: 'The recording goes to the server once and comes back as a file id.' },
      { label: 'Run', t: 5600, still: 8200, cap: 'speech.transcribe runs on the server, after the token’s policy allows it.' },
      { label: 'File', t: 9600, still: 12000, cap: 'The result is a file link the agent can download: interview.srt.' },
    ],
  });

  /* ---------------------------------------------------------------- discovery: full vs compact */
  const disc = $('#disc');
  if (disc) {
    const wall = $('#discWall'), nEl = $('#discN');
    const tools = window.DGB_TOOLS || [];
    const META = 8;
    if (wall && tools.length) {
      wall.innerHTML = tools.map((t) => `<span>${t[2].replace('.', '_')}</span>`).join(' ');
    }
    const FULL = tools.length ? tools.length + META : 257;
    const btns = $$('.seg-b', disc);
    let mode = 'compact', raf = 0;
    function count(to) {
      cancelAnimationFrame(raf);
      const from = Number(nEl.textContent) || 0;
      if (still || from === to) { nEl.textContent = String(to); return; }
      const t0 = performance.now();
      const step = (now) => {
        const p = Math.min(1, (now - t0) / 520);
        const e = 1 - Math.pow(1 - p, 3);
        nEl.textContent = String(Math.round(from + (to - from) * e));
        if (p < 1) raf = requestAnimationFrame(step);
      };
      raf = requestAnimationFrame(step);
    }
    function set(m) {
      mode = m;
      disc.dataset.m = m;
      btns.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.m === m)));
      count(m === 'full' ? FULL : META);
    }
    let auto = !still, timer = 0, vis = false, flips = 0;
    const stopAuto = () => { auto = false; clearTimeout(timer); timer = 0; };
    btns.forEach((b) => b.addEventListener('click', () => { stopAuto(); set(b.dataset.m); }));
    const tick = () => {
      timer = 0;
      if (!auto || !vis || document.hidden) return;
      set(mode === 'full' ? 'compact' : 'full');
      if (++flips >= 4) { stopAuto(); return; }
      timer = setTimeout(tick, mode === 'full' ? 3200 : 3800);
    };
    const kick = () => { if (auto && vis && !document.hidden && !timer) timer = setTimeout(tick, 1400); };
    watchView(disc, (v) => { vis = v; if (!v) { clearTimeout(timer); timer = 0; } else kick(); });
    document.addEventListener('visibilitychange', kick);
    set('compact');
  }

  /* ---------------------------------------------------------------- policy log */
  const log = $('#polLog');
  if (log && !still) {
    const items = Array.from(log.children);
    log.classList.add('is-live');
    let i = 0, timer = 0, vis = false, paused = false;
    const step = () => {
      timer = 0;
      if (!vis || paused || document.hidden) return;
      if (i >= items.length) { items.forEach((li) => li.classList.remove('on')); i = 0; timer = setTimeout(step, 700); return; }
      items[i++].classList.add('on');
      timer = setTimeout(step, i >= items.length ? 2800 : i < 2 ? 160 : 900); // the first two arrive together
    };
    const kick = () => { if (vis && !paused && !document.hidden && !timer) timer = setTimeout(step, 300); };
    watchView($('#policy'), (v) => { vis = v; kick(); });
    document.addEventListener('visibilitychange', kick);
    pauseButton($('.pol-play'), 'the policy log animation', (p) => {
      paused = p;
      if (p) { clearTimeout(timer); timer = 0; items.forEach((li) => li.classList.add('on')); i = items.length; }
      else kick();
    });
  } else if (log) {
    const b = $('.pol-play');
    if (b) b.hidden = true;
  }
})();
