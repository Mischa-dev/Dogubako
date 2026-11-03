/* finder.js: all 249 tools, searchable. A frosted palette over a wall of every tool name;
 * matches light up as you type. Until the visitor types, it searches by itself. */
(function () {
  'use strict';
  const CATS = window.DGB_CATS, TOOLS = window.DGB_TOOLS;
  const $ = (s) => document.querySelector(s);
  const input = $('#fq'), res = $('#fres'), count = $('#fcount'), catsEl = $('#fcats'), wall = $('#fwall'), cli = $('#fcli'), finder = $('#finder');
  if (!input) return;
  const still = window.DGB.reduceMotion();
  const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const icon = (n) => `<svg class="i" aria-hidden="true"><use href="#i-${n}"/></svg>`;

  /* wall of names */
  let k = 0, html = '';
  CATS.forEach((c, ci) => {
    html += `<span class="fw-cat">${esc(c[1])}</span>`;
    TOOLS.forEach((t, i) => { if (t[0] === ci) html += `<span class="fw-t" data-i="${i}" style="--k:${k++}">${esc(t[1])}</span> `; });
  });
  wall.innerHTML = html;
  const wallEls = Array.from(wall.querySelectorAll('.fw-t'));
  const wallByIdx = new Map(wallEls.map((el) => [Number(el.dataset.i), el]));

  /* category chips */
  let cat = -1;
  const chip = (ci, label, n) => `<button type="button" class="fc" data-c="${ci}" aria-pressed="${ci === -1}">${esc(label)} <span class="mono">${n}</span></button>`;
  catsEl.innerHTML = chip(-1, 'All', TOOLS.length) + CATS.map((c, ci) => chip(ci, c[1], TOOLS.filter((t) => t[0] === ci).length)).join('');
  catsEl.addEventListener('click', (e) => {
    const b = e.target.closest('.fc');
    if (!b) return;
    userTook();
    cat = Number(b.dataset.c);
    catsEl.querySelectorAll('.fc').forEach((x) => x.setAttribute('aria-pressed', String(x === b)));
    run();
  });

  /* search: name first, then keywords, description and id; every word has to match somewhere */
  function score(t, words) {
    const name = t[1].toLowerCase(), kw = t[4].toLowerCase(), desc = t[3].toLowerCase(), id = t[2];
    let s = 0;
    for (const w of words) {
      let best = 0;
      if (name.startsWith(w)) best = 6;
      else if (new RegExp('(^|[\\s(/&-])' + w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).test(name)) best = 4;
      else if (name.includes(w)) best = 3;
      else if (kw.includes(w)) best = 2;
      else if (id.includes(w)) best = 1.5;
      else if (desc.includes(w)) best = 1;
      if (!best) return 0;
      s += best;
    }
    return s;
  }
  function mark(name, words) {
    let out = esc(name);
    for (const w of words) {
      if (!w) continue;
      const re = new RegExp('(' + esc(w).replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'i');
      if (!/<mark>/.test(out) || !re.test(out.replace(/<mark>.*?<\/mark>/g, ''))) out = out.replace(re, '<mark>$1</mark>');
    }
    return out;
  }
  const cmd = (id) => { const [c, r] = id.split('.'); return `dogubako ${c} ${r.replace(/_/g, '-')} --help`; };

  let results = [], sel = 0, prevIds = [];
  function run() {
    const q = input.value.trim().toLowerCase();
    const words = q.split(/\s+/).filter(Boolean);
    const pool = TOOLS.map((t, i) => ({ t, i })).filter((x) => cat === -1 || x.t[0] === cat);
    const scored = words.length ? pool.map((x) => ({ ...x, s: score(x.t, words) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s || a.i - b.i) : pool;
    results = scored;
    sel = 0;
    const n = scored.length;
    count.textContent = n === 0 ? 'No tools' : n === 1 ? '1 tool' : `${n} tools`;
    // wall
    const filtering = words.length > 0 || cat !== -1;
    wall.classList.toggle('is-q', filtering);
    const hits = new Set(scored.map((x) => x.i));
    for (const [i, el] of wallByIdx) el.classList.toggle('hit', filtering && hits.has(i));
    // list
    if (!n) {
      prevIds = [];
      res.innerHTML = `<li class="fr-empty" role="option" aria-selected="false" aria-disabled="true">Nothing matches “${esc(q)}”. Try a file type or a verb, like “mp4” or “shrink”.</li>`;
      input.removeAttribute('aria-activedescendant');
      cli.textContent = 'dogubako tools search ' + (q || '…');
      return;
    }
    // rows that keep the same tool in the same place do not animate again, so typing stays calm
    const ids = scored.slice(0, 6).map((x) => x.t[2]);
    let fresh = 0;
    res.innerHTML = scored.slice(0, 6).map((x, j) => `<li class="fr${prevIds[j] === x.t[2] ? '' : ' fr-new'}" role="option" id="fr-${j}" aria-selected="${j === 0}" data-j="${j}"${prevIds[j] === x.t[2] ? '' : ` style="animation-delay:${(fresh++) * 14}ms"`}>
      <span class="fr-ic">${icon(CATS[x.t[0]][2])}</span>
      <span class="fr-t"><span class="fr-n">${mark(x.t[1], words)}</span><span class="fr-d">${esc(x.t[3])}</span></span>
      <span class="fr-id">${x.t[2]}</span></li>`).join('');
    prevIds = ids;
    select(0);
  }
  function select(j) {
    const rows = res.querySelectorAll('.fr');
    if (!rows.length) return;
    sel = Math.max(0, Math.min(rows.length - 1, j));
    rows.forEach((r, i) => r.setAttribute('aria-selected', String(i === sel)));
    input.setAttribute('aria-activedescendant', 'fr-' + sel);
    cli.textContent = cmd(results[sel].t[2]);
  }
  res.addEventListener('click', (e) => { const r = e.target.closest('.fr'); if (r) { userTook(); select(Number(r.dataset.j)); input.focus(); } });
  input.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); select(sel + 1); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); select(sel - 1); }
    else if (e.key === 'Escape') { input.value = ''; run(); }
  });
  input.addEventListener('input', () => { userTook(); run(); });
  input.addEventListener('focus', userTook);

  /* until someone types, it types for them */
  const QUERIES = ['compress', 'subtitles', 'pdf', 'hash', 'qr', 'csv', 'metadata', 'dns', 'shrink'];
  let auto = !still, timer = 0, qi = 0, inView = false;
  function userTook() {
    if (!auto) return;
    auto = false; clearTimeout(timer);
    count.setAttribute('aria-live', 'polite');
  }
  function typeNext() {
    if (!auto) return;
    if (!inView || document.hidden) { timer = setTimeout(typeNext, 400); return; }
    const q = QUERIES[qi % QUERIES.length]; qi++;
    let i = 0;
    const typeCh = () => {
      if (!auto) return;
      input.value = q.slice(0, ++i); run();
      if (i < q.length) timer = setTimeout(typeCh, 70 + Math.random() * 40);
      else timer = setTimeout(erase, 1900);
    };
    const erase = () => {
      if (!auto) return;
      if (!inView || document.hidden) { timer = setTimeout(erase, 400); return; }
      input.value = input.value.slice(0, -1); run();
      if (input.value) timer = setTimeout(erase, 28);
      else timer = setTimeout(typeNext, 380);
    };
    typeCh();
  }
  count.setAttribute('aria-live', 'off');

  /* first sight: the number counts up and every name lights in turn */
  const bigN = $('#bigN');
  let seen = false;
  new IntersectionObserver((es) => {
    for (const e of es) {
      inView = e.isIntersecting;
      if (inView && !seen) {
        seen = true;
        if (!still) {
          wall.classList.remove('pre');
          wall.classList.add('lit-run');
          setTimeout(() => { wall.classList.remove('lit-run'); wall.classList.add('lit'); }, 1500);
          timer = setTimeout(typeNext, 1600);
        }
      }
    }
  }, { threshold: 0.25 }).observe(finder);

  /* the number counts up once its heading is properly on screen */
  if (!still && bigN) {
    const head = bigN.closest('h2');
    bigN.textContent = '0';
    new IntersectionObserver((es, o) => {
      if (!es.some((e) => e.isIntersecting)) return;
      o.disconnect();
      const t0 = performance.now();
      const step = (now) => {
        const p = Math.min(1, (now - t0) / 1100);
        bigN.textContent = String(Math.round(249 * (1 - Math.pow(1 - p, 3))));
        if (p < 1) requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    }, { threshold: 0.5, rootMargin: '0px 0px -8% 0px' }).observe(head);
  }

  if (still) { input.value = 'compress'; wall.classList.add('lit'); }
  else wall.classList.add('pre');
  run();
})();
