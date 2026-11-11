/* site.js: shared by every page. Theme toggle, the nav's glass, the phone menu, and reveals.
 * No dependencies; every hook is optional, so a page only needs the markup it uses. */
(function () {
  'use strict';
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const root = document.documentElement;
  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* ---------------- theme: follows the system until the visitor picks one (saved as dgb-theme) */
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  const isDark = () => (root.dataset.theme ? root.dataset.theme === 'dark' : mq.matches);
  const themeBtn = $('#themeBtn');
  if (themeBtn) {
    const label = () => themeBtn.setAttribute('aria-label', isDark() ? 'Switch to light theme' : 'Switch to dark theme');
    const apply = (next) => {
      root.dataset.theme = next;
      try { localStorage.setItem('dgb-theme', next); } catch (e) { /* private mode */ }
      label();
      document.dispatchEvent(new CustomEvent('dgb:theme', { detail: next }));
    };
    themeBtn.addEventListener('click', () => {
      const next = isDark() ? 'light' : 'dark';
      if (still || !document.startViewTransition) { apply(next); return; }
      // the new theme opens in a circle from the button
      const r = themeBtn.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2;
      root.style.setProperty('--wipe-x', x + 'px');
      root.style.setProperty('--wipe-y', y + 'px');
      root.style.setProperty('--wipe-r', Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y)) + 'px');
      root.classList.add('theme-wipe');
      const vt = document.startViewTransition(() => apply(next));
      vt.finished.finally(() => root.classList.remove('theme-wipe'));
    });
    if (mq.addEventListener) mq.addEventListener('change', label);
    label();
  }

  /* ---------------- nav: glass once the page moves */
  const nav = $('#nav');
  if (nav) {
    const onScroll = () => nav.classList.toggle('is-stuck', window.scrollY > 8);
    window.addEventListener('scroll', onScroll, { passive: true });
    onScroll();
  }

  /* ---------------- phone menu: a disclosure; Escape, outside click or a wider window closes it */
  const menuBtn = $('#navMenu');
  const links = menuBtn && document.getElementById(menuBtn.getAttribute('aria-controls'));
  if (nav && menuBtn && links) {
    const set = (open, focusBtn) => {
      menuBtn.setAttribute('aria-expanded', String(open));
      menuBtn.setAttribute('aria-label', open ? 'Close menu' : 'Menu');
      nav.classList.toggle('is-open', open);
      if (!open && focusBtn) menuBtn.focus();
    };
    menuBtn.addEventListener('click', () => {
      const open = menuBtn.getAttribute('aria-expanded') !== 'true';
      set(open);
      if (open) { const first = $('a', links); if (first) first.focus(); }
    });
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && nav.classList.contains('is-open')) set(false, true); });
    document.addEventListener('click', (e) => { if (nav.classList.contains('is-open') && !nav.contains(e.target)) set(false); });
    links.addEventListener('click', (e) => { if (e.target.closest('a')) set(false); });
    const wide = window.matchMedia('(min-width: 861px)');
    const onWide = () => { if (wide.matches) set(false); };
    if (wide.addEventListener) wide.addEventListener('change', onWide);
  }

  /* ---------------- cards: a soft light follows the pointer (the CSS reads --mx / --my on hover) */
  if (window.matchMedia('(hover: hover) and (pointer: fine)').matches) {
    const LIT = '.tile, .way, .pw, .teaser, .step, .end-card, .duo-card, .disc, .choose-card, .plat, .doc-links a';
    document.addEventListener('pointermove', (e) => {
      const el = e.target.closest && e.target.closest(LIT);
      if (!el) return;
      const r = el.getBoundingClientRect();
      el.style.setProperty('--mx', (e.clientX - r.left).toFixed(0) + 'px');
      el.style.setProperty('--my', (e.clientY - r.top).toFixed(0) + 'px');
    }, { passive: true });
  }

  /* ---------------- reveals: anything with class "rv" fades up once, the first time it is in view */
  const rv = $$('.rv');
  if (rv.length) {
    if (still || !('IntersectionObserver' in window)) rv.forEach((el) => el.classList.add('in'));
    else {
      const io = new IntersectionObserver((es) => {
        for (const e of es) if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
      }, { rootMargin: '0px 0px -8% 0px' });
      rv.forEach((el) => io.observe(el));
    }
  }
})();
