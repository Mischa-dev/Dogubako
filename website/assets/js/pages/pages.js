/* pages.js: shared helpers for the sub-pages (copy buttons, animated "shows" with chapters,
 * pause buttons). Loaded after timeline.js on the pages that animate. */
(function () {
  'use strict';
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const reduce = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* ---------------------------------------------------------------- copy to the clipboard */
  let status = null;
  function announce(msg) {
    if (!status) {
      status = document.createElement('p');
      status.className = 'copy-status';
      status.setAttribute('role', 'status');
      document.body.appendChild(status);
    }
    status.textContent = '';
    setTimeout(() => { status.textContent = msg; }, 40);
  }
  async function copyText(text) {
    try {
      if (navigator.clipboard && window.isSecureContext) { await navigator.clipboard.writeText(text); return true; }
    } catch (e) { /* fall through */ }
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.cssText = 'position:fixed;left:-9999px;top:0;opacity:0';
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand && document.execCommand('copy');
      ta.remove();
      return !!ok;
    } catch (e) { return false; }
  }
  function selectContents(el) {
    try {
      const r = document.createRange();
      r.selectNodeContents(el);
      const s = window.getSelection();
      s.removeAllRanges();
      s.addRange(r);
    } catch (e) { /* nothing to do */ }
  }
  $$('[data-copy], [data-copy-from]').forEach((btn) => {
    const label = $('span', btn);
    const idle = label ? label.textContent : '';
    let timer = 0;
    btn.addEventListener('click', async () => {
      const src = btn.dataset.copyFrom ? $(btn.dataset.copyFrom) : null;
      const text = btn.dataset.copy != null ? btn.dataset.copy : (src ? src.textContent.trim() : '');
      const ok = await copyText(text);
      clearTimeout(timer);
      if (ok) {
        btn.classList.add('is-done');
        if (label) label.textContent = 'Copied';
        announce('Copied to the clipboard.');
        timer = setTimeout(() => { btn.classList.remove('is-done'); if (label) label.textContent = idle; }, 2200);
        return;
      }
      // No clipboard here: show the text where it can be selected and copied by hand.
      const fb = btn.dataset.fallback ? $(btn.dataset.fallback) : null;
      if (fb) {
        fb.value = text;
        fb.hidden = false;
        fb.focus();
        fb.select();
        if (label) label.textContent = 'Select the text and copy it';
        announce('Copying isn’t available here. The text is selected: press Ctrl+C, or Command+C on a Mac.');
      } else {
        const code = btn.parentElement && $('code', btn.parentElement);
        if (code) selectContents(code);
        announce('Copying isn’t available here. The command is selected: press Ctrl+C, or Command+C on a Mac.');
      }
    });
  });

  /* ---------------------------------------------------------------- pause buttons */
  function pauseButton(btn, what, onChange) {
    if (!btn) return { set() {} };
    const set = (paused) => {
      btn.setAttribute('aria-pressed', String(paused));
      btn.setAttribute('aria-label', (paused ? 'Play ' : 'Pause ') + what);
      onChange(paused);
    };
    btn.addEventListener('click', () => set(btn.getAttribute('aria-pressed') !== 'true'));
    return { set };
  }

  /* run a callback when an element enters or leaves the screen */
  function watchView(el, cb, margin) {
    if (!el) return;
    new IntersectionObserver((es) => { for (const e of es) cb(e.isIntersecting); }, { rootMargin: margin || '0px' }).observe(el);
  }

  /* typing: [[t, 'd'], [t+40, 'do'], …] */
  function typeKeys(t0, str, step, from) {
    const keys = [[0, from || '']];
    for (let i = 1; i <= str.length; i++) keys.push([t0 + i * step, str.slice(0, i)]);
    return keys;
  }

  /* ---------------------------------------------------------------- a timeline with chapters */
  // opts: { root, duration, build() -> Timeline, chapters: [{label, t, still, cap}], stillChapter }
  function show(opts) {
    const DGB = window.DGB;
    const root = opts.root;
    if (!root || !DGB || !DGB.Timeline) return null;
    const still = reduce();
    const D = opts.duration;
    const CH = opts.chapters;
    const pipsEl = $('.show-pips', root), capEl = $('.show-cap', root), playBtn = $('.show-play', root);
    let tl = opts.build();
    let userPaused = still;
    let current = -1;

    const pipEls = CH.map((c, i) => {
      const li = document.createElement('li');
      li.innerHTML = `<button type="button" class="pip" aria-label="Step ${i + 1}: ${c.label}"><span class="pip-n">${i + 1}</span><span class="pip-l">${c.label}</span><span class="pip-bar" aria-hidden="true"><i></i></span></button>`;
      pipsEl.appendChild(li);
      const b = li.firstChild;
      b.addEventListener('click', () => jump(i));
      return b;
    });
    const chapterAt = (t) => { let k = 0; for (let i = 0; i < CH.length; i++) if (t >= CH[i].t) k = i; return k; };
    function onTime(t) {
      const k = chapterAt(t);
      if (k !== current) {
        current = k;
        pipEls.forEach((b, i) => { if (i === k) b.setAttribute('aria-current', 'step'); else b.removeAttribute('aria-current'); });
        capEl.textContent = CH[k].cap;
      }
      const end = k + 1 < CH.length ? CH[k + 1].t : D;
      const p = Math.max(0, Math.min(1, (t - CH[k].t) / (end - CH[k].t)));
      $('.pip-bar i', pipEls[k]).style.transform = `scaleX(${userPaused ? 1 : p.toFixed(4)})`;
      pipEls.forEach((b, i) => { if (i !== k) $('.pip-bar i', b).style.transform = 'scaleX(0)'; });
    }
    const player = new DGB.Player({ duration: D, render: (t) => tl.render(t), onTime });
    const pb = pauseButton(playBtn, 'the animation', (paused) => {
      userPaused = paused;
      if (paused) player.pause(); else player.play();
      onTime(player.t);
    });
    function jump(i) {
      current = -1;
      if (userPaused) player.seek(CH[i].still);
      else player.seek(CH[i].t + 1);
    }
    watchView(root, (v) => player.setVisible(v));
    // layout-dependent tracks are rebuilt when fonts arrive or the size changes
    const rebuild = () => { const t = player.t; tl.clear(); tl = opts.build(); tl.invalidate(); player.seek(t); };
    if (opts.rebuildOnResize) {
      let w = root.clientWidth;
      window.addEventListener('resize', () => { if (root.clientWidth !== w) { w = root.clientWidth; cancelAnimationFrame(rebuild.r); rebuild.r = requestAnimationFrame(rebuild); } });
    }
    if (document.fonts && document.fonts.ready && opts.rebuildOnResize) document.fonts.ready.then(rebuild);

    if (still) {
      player.seek(CH[opts.stillChapter == null ? CH.length - 1 : opts.stillChapter].still);
      pb.set(true);
    } else {
      player.seek(0);
      pb.set(false);
    }
    return { player, jump, setPaused: pb.set };
  }

  /* ---------------------------------------------------------------- code blocks that scroll sideways */
  // A code block wider than its box can only be scrolled with the keyboard if it can take focus.
  function scrollableCode() {
    $$('pre.code').forEach((pre) => {
      if (pre.scrollWidth > pre.clientWidth + 1) {
        if (!pre.hasAttribute('tabindex')) { pre.setAttribute('tabindex', '0'); pre.dataset.scrolls = ''; }
      } else if ('scrolls' in pre.dataset) {
        pre.removeAttribute('tabindex'); delete pre.dataset.scrolls;
      }
    });
  }
  scrollableCode();
  let rsT = 0;
  addEventListener('resize', () => { clearTimeout(rsT); rsT = setTimeout(scrollableCode, 150); });
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(scrollableCode);

  window.DGBP = { $, $$, reduce, copyText, announce, pauseButton, watchView, typeKeys, show };
})();
