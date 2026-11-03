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
    themeBtn.addEventListener('click', () => {
      const next = isDark() ? 'light' : 'dark';
      root.dataset.theme = next;
      try { localStorage.setItem('dgb-theme', next); } catch (e) { /* private mode */ }
      label();
      document.dispatchEvent(new CustomEvent('dgb:theme', { detail: next }));
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
