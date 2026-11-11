/* palette.js: the site's own command palette, the way the app has one. ⌘K / Ctrl+K (or the
 * search button in the nav) opens it on every page; it searches the pages, a few actions and all
 * the tools. A tool opens its entry in the docs. The tool list (tools-data.js) is loaded the first
 * time the palette opens, unless the page already has it. */
(function () {
  'use strict';
  const self = document.currentScript;
  const ROOT = self ? new URL('../../', self.src).href : '/';
  const DOCS = 'https://docs.dogubako.app';
  const REPO = 'https://github.com/Mischa-dev/Dogubako';
  const MAX = 40;
  const mac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
  const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  const icon = (n) => (document.getElementById('i-' + n) ? `<svg class="i" aria-hidden="true"><use href="#i-${n}"/></svg>` : '');

  /* the nav buttons show the right shortcut for the visitor's keyboard */
  const openers = Array.from(document.querySelectorAll('[data-palette]'));
  openers.forEach((b) => {
    const k = b.querySelector('kbd');
    if (k) k.textContent = mac ? '⌘K' : 'Ctrl K';
    b.setAttribute('aria-keyshortcuts', mac ? 'Meta+K' : 'Control+K');
  });

  /* ---------------------------------------------------------------- what can be found */
  const FIXED = [
    { n: 'Home', d: 'What Dogubako is, and the app in motion', href: ROOT, ic: 'box', k: 'start index landing' },
    { n: 'All tools', d: 'The whole catalog, by category', href: ROOT + 'tools/', ic: 'search', k: 'catalog list browse categories' },
    { n: 'Download', d: 'The Mac app, or build it from source', href: ROOT + 'download/', ic: 'download', k: 'install dmg mac macos windows linux get alpha release' },
    { n: 'Self-hosting', d: 'One server for a household or a team', href: ROOT + 'self-hosting/', ic: 'server', k: 'docker compose server nas vps tailscale host homelab' },
    { n: 'Agents', d: 'MCP, the HTTP API and the command line', href: ROOT + 'agents/', ic: 'bot', k: 'mcp api cli ai claude automation scripts' },
    { n: 'Documentation', d: 'docs.dogubako.app', href: DOCS + '/', ic: 'file-text', k: 'docs guide manual reference help', ext: true },
    { n: 'Source on GitHub', d: 'Mischa-dev/Dogubako', href: REPO, ic: 'code-xml', k: 'github repository code issues star open source', ext: true },
    { n: 'Report a problem', d: 'Open an issue on GitHub', href: REPO + '/issues', ic: 'shield-check', k: 'bug issue feedback support', ext: true },
    { n: 'Switch theme', d: 'Light or dark', ic: 'sun', k: 'dark light mode appearance colour color', act: () => { const b = document.getElementById('themeBtn'); if (b) b.click(); } },
  ];
  let tools = null; // [{n, d, id, href, ic, k}]
  function buildTools() {
    const C = window.DGB_CATS, T = window.DGB_TOOLS;
    if (!C || !T) return;
    // the docs' heading anchors (github-slugger rules, the same as scripts/build-tools-page.mjs)
    const seen = C.map(() => new Map());
    const slug = (ci, text) => {
      const m = seen[ci], base = text.toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '').replace(/ /g, '-');
      let s = base, n = m.get(base) || 0;
      while (m.has(s)) s = `${base}-${++n}`;
      m.set(base, n); m.set(s, 0);
      return s;
    };
    tools = T.map((t) => ({ n: t[1], d: t[3], id: t[2], k: t[4], ic: C[t[0]][2], href: `${DOCS}/tools/${C[t[0]][0]}/#${slug(t[0], t[1])}`, ext: true }));
  }
  let loading = null;
  function loadTools() {
    if (tools) return Promise.resolve();
    if (window.DGB_TOOLS) { buildTools(); return Promise.resolve(); }
    if (!loading) loading = new Promise((done) => {
      const s = document.createElement('script');
      s.src = ROOT + 'assets/js/tools-data.js';
      s.onload = () => { buildTools(); done(); };
      s.onerror = () => done(); // offline: the pages and actions still work
      document.head.appendChild(s);
    });
    return loading;
  }

  function score(x, words) {
    const name = x.n.toLowerCase(), kw = (x.k || '').toLowerCase(), desc = (x.d || '').toLowerCase(), id = x.id || '';
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
      const re = new RegExp('(' + esc(w).replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + ')', 'i');
      if (!/<mark>/.test(out)) out = out.replace(re, '<mark>$1</mark>');
    }
    return out;
  }

  /* ---------------------------------------------------------------- the dialog, built on first use */
  let dlg, input, list, foot, rows = [], sel = 0, lastFocus = null;
  function build() {
    dlg = document.createElement('dialog');
    dlg.className = 'cmdk';
    dlg.setAttribute('aria-label', 'Search Dogubako');
    dlg.innerHTML = `<div class="cmdk-box">
      <label class="cmdk-in">${icon('search')}<input type="text" placeholder="Search tools and pages" autocomplete="off" spellcheck="false" role="combobox" aria-expanded="true" aria-controls="cmdkList" aria-autocomplete="list" aria-label="Search tools and pages"><kbd>esc</kbd></label>
      <ul class="cmdk-list" id="cmdkList" role="listbox" aria-label="Results"></ul>
      <div class="cmdk-foot mono"><span class="cmdk-count" aria-live="polite"></span><span class="cmdk-keys"><kbd>↑</kbd><kbd>↓</kbd> move <kbd>↵</kbd> open</span></div>
    </div>`;
    document.body.appendChild(dlg);
    input = dlg.querySelector('input'); list = dlg.querySelector('.cmdk-list'); foot = dlg.querySelector('.cmdk-count');
    input.addEventListener('input', render);
    input.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown') { e.preventDefault(); select(sel + 1); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); select(sel - 1); }
      else if (e.key === 'Home' && rows.length) { e.preventDefault(); select(0); }
      else if (e.key === 'End' && rows.length) { e.preventDefault(); select(rows.length - 1); }
      else if (e.key === 'Enter') { e.preventDefault(); go(sel, e.metaKey || e.ctrlKey); }
    });
    list.addEventListener('click', (e) => { const li = e.target.closest('.cmdk-row'); if (li) go(Number(li.dataset.j), e.metaKey || e.ctrlKey); });
    list.addEventListener('pointermove', (e) => { const li = e.target.closest('.cmdk-row'); if (li && Number(li.dataset.j) !== sel) select(Number(li.dataset.j), true); });
    dlg.addEventListener('click', (e) => { if (e.target === dlg) dlg.close(); }); // the backdrop
    dlg.addEventListener('close', () => { if (lastFocus && lastFocus.focus) lastFocus.focus(); });
  }
  function render() {
    const words = input.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const pool = FIXED.concat(tools || []);
    let found;
    if (!words.length) found = FIXED.slice();
    else found = pool.map((x, i) => ({ x, i, s: score(x, words) + (x.id ? 0 : 0.5) })).filter((r) => r.s > 0.5).sort((a, b) => b.s - a.s || a.i - b.i).map((r) => r.x);
    const total = found.length;
    rows = found.slice(0, MAX);
    if (!rows.length) {
      list.innerHTML = `<li class="cmdk-empty" role="option" aria-selected="false" aria-disabled="true">Nothing matches “${esc(input.value.trim())}”. Try a file type or a verb, like “mp4” or “shrink”.</li>`;
      foot.textContent = 'No results';
      input.removeAttribute('aria-activedescendant');
      return;
    }
    list.innerHTML = rows.map((x, j) => `<li class="cmdk-row" role="option" id="cmdk-${j}" data-j="${j}" aria-selected="false">
      <span class="cmdk-ic">${icon(x.ic)}</span>
      <span class="cmdk-t"><span class="cmdk-n">${mark(x.n, words)}</span><span class="cmdk-d">${esc(x.d || '')}</span></span>
      <span class="cmdk-id">${x.id ? esc(x.id) : x.act ? 'action' : x.ext ? 'opens ' + esc(new URL(x.href).hostname) : 'page'}</span></li>`).join('');
    foot.textContent = !words.length ? (tools ? `Type to search ${tools.length} tools` : 'Type to search') : total === 1 ? '1 result' : total > MAX ? `${MAX} of ${total} results` : `${total} results`;
    list.scrollTop = 0;
    select(0, true);
  }
  function select(j, quiet) {
    if (!rows.length) return;
    sel = Math.max(0, Math.min(rows.length - 1, j));
    const els = list.querySelectorAll('.cmdk-row');
    els.forEach((el, i) => el.setAttribute('aria-selected', String(i === sel)));
    input.setAttribute('aria-activedescendant', 'cmdk-' + sel);
    if (!quiet && els[sel]) els[sel].scrollIntoView({ block: 'nearest' });
  }
  function go(j, newTab) {
    const x = rows[j];
    if (!x) return;
    if (x.act) { lastFocus = null; dlg.close(); x.act(); return; }
    if (newTab) { window.open(x.href, '_blank', 'noopener'); return; }
    dlg.close();
    window.location.href = x.href;
  }
  function open() {
    if (!dlg) build();
    if (dlg.open) return;
    if (typeof dlg.showModal !== 'function') { window.location.href = ROOT + 'tools/'; return; }
    lastFocus = document.activeElement;
    input.value = '';
    render();
    dlg.showModal();
    input.focus();
    loadTools().then(() => { if (dlg.open) { const j = sel; render(); if (!input.value) select(j, true); } });
  }

  openers.forEach((b) => b.addEventListener('click', open));
  document.addEventListener('keydown', (e) => {
    if ((e.key === 'k' || e.key === 'K') && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) {
      e.preventDefault();
      if (dlg && dlg.open) dlg.close(); else open();
    }
  });
  window.DGB = Object.assign(window.DGB || {}, { palette: { open } });
})();
