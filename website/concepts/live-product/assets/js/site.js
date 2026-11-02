/* site.js: theme, nav, the "ways in" switcher, reveals and the small looping details. */
(function () {
  'use strict';
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const still = window.DGB.reduceMotion();
  const root = document.documentElement;

  /* ---------------- theme: follows the system until the visitor picks one */
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  const isDark = () => root.dataset.theme ? root.dataset.theme === 'dark' : mq.matches;
  const themeBtn = $('#themeBtn');
  const label = () => themeBtn.setAttribute('aria-label', isDark() ? 'Switch to light theme' : 'Switch to dark theme');
  themeBtn.addEventListener('click', () => {
    const next = isDark() ? 'light' : 'dark';
    root.dataset.theme = next;
    try { localStorage.setItem('dgb-theme', next); } catch (e) { /* private mode */ }
    label();
  });
  mq.addEventListener && mq.addEventListener('change', label);
  label();

  /* ---------------- nav gets its glass once the page moves */
  const nav = $('#nav');
  const onScroll = () => nav.classList.toggle('is-stuck', window.scrollY > 8);
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  /* ---------------- reveal on scroll */
  const rv = $$('.sec-head, .tile, .ways-copy, .ways-stage, .local-copy, .server-copy, .server-vis, .dl, .finder-pal');
  rv.forEach((el) => el.classList.add('rv'));
  const io = new IntersectionObserver((es) => {
    for (const e of es) if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
  }, { rootMargin: '0px 0px -8% 0px' });
  rv.forEach((el) => io.observe(el));
  // stagger bento tiles a touch
  $$('.tile').forEach((t, i) => { t.style.transitionDelay = (i % 3) * 70 + 'ms'; });

  /* ---------------- cabinet drawers slide in */
  const cab = $('#cabinet');
  $$('.drawer', cab).forEach((d, i) => d.style.setProperty('--i', i));
  new IntersectionObserver((es, o) => { for (const e of es) if (e.isIntersecting) { cab.classList.add('in'); o.disconnect(); } }, { threshold: 0.2 }).observe(cab);

  /* ---------------- ways in: tabs (arrow keys move between them), auto-advance until touched */
  const tabs = $$('#waysTabs [role="tab"]');
  let cur = 0, auto = !still, t0 = performance.now(), raf = 0, waysVisible = false;
  const DWELL = 5200;
  function selectTab(i, focus) {
    cur = (i + tabs.length) % tabs.length;
    tabs.forEach((t, k) => {
      const on = k === cur;
      t.setAttribute('aria-selected', String(on));
      t.tabIndex = on ? 0 : -1;
      $('#' + t.getAttribute('aria-controls')).hidden = !on;
      $('.wt-bar i', t).style.transform = 'scaleX(0)';
    });
    if (focus) tabs[cur].focus();
    t0 = performance.now();
  }
  const stopAuto = () => { auto = false; tabs.forEach((t) => { $('.wt-bar i', t).style.transform = 'scaleX(0)'; }); };
  tabs.forEach((t, i) => {
    t.addEventListener('click', () => { stopAuto(); selectTab(i); });
    t.addEventListener('keydown', (e) => {
      const k = e.key;
      if (k === 'ArrowDown' || k === 'ArrowRight') { e.preventDefault(); stopAuto(); selectTab(cur + 1, true); }
      else if (k === 'ArrowUp' || k === 'ArrowLeft') { e.preventDefault(); stopAuto(); selectTab(cur - 1, true); }
      else if (k === 'Home') { e.preventDefault(); stopAuto(); selectTab(0, true); }
      else if (k === 'End') { e.preventDefault(); stopAuto(); selectTab(tabs.length - 1, true); }
    });
  });
  $('.ways-stage').addEventListener('pointerenter', () => { if (auto) t0 = performance.now(); });
  function waysTick(now) {
    raf = 0;
    if (!auto || !waysVisible || document.hidden) return;
    const p = (now - t0) / DWELL;
    if (p >= 1) selectTab(cur + 1);
    else $('.wt-bar i', tabs[cur]).style.transform = `scaleX(${p.toFixed(4)})`;
    raf = requestAnimationFrame(waysTick);
  }
  new IntersectionObserver((es) => {
    for (const e of es) {
      waysVisible = e.isIntersecting;
      if (waysVisible && auto && !raf) { t0 = performance.now(); raf = requestAnimationFrame(waysTick); }
    }
  }, { threshold: 0.35 }).observe($('#ways'));
  document.addEventListener('visibilitychange', () => { if (!document.hidden && waysVisible && auto && !raf) { t0 = performance.now(); raf = requestAnimationFrame(waysTick); } });

  /* ---------------- policy checks tick through, one call at a time */
  const pol = $$('#polLog li');
  if (still) pol.forEach((li) => li.classList.add('on'));
  else {
    let i = 0, timer = 0, vis = false;
    const step = () => {
      timer = 0;
      if (!vis || document.hidden) return;
      if (i >= pol.length) { pol.forEach((li) => li.classList.remove('on')); i = 0; timer = setTimeout(step, 700); return; }
      pol[i++].classList.add('on');
      timer = setTimeout(step, i >= pol.length ? 2600 : 850);
    };
    new IntersectionObserver((es) => { for (const e of es) { vis = e.isIntersecting; if (vis && !timer) timer = setTimeout(step, 300); } }, { threshold: 0.3 }).observe($('#policy'));
    document.addEventListener('visibilitychange', () => { if (!document.hidden && vis && !timer) timer = setTimeout(step, 300); });
  }

  /* ---------------- the box opens when the download section arrives */
  const dl = $('.dl');
  if (!still) new IntersectionObserver((es, o) => { for (const e of es) if (e.isIntersecting) { setTimeout(() => dl.classList.add('open'), 350); o.disconnect(); } }, { threshold: 0.5 }).observe(dl);
})();
