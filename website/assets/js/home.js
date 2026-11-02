/* home.js: the smaller pieces of the home page. One tool four ways in, the trailer frame,
 * the agent policy log, and the box that opens on the download card. */
(function () {
  'use strict';
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

  /* ------------------------------------------------ one tool, four ways in
   * One description per tool; the four views are generated from it, the way the app, CLI, HTTP API
   * and MCP server are generated from the tool spec. */
  const WAYS = {
    'image.compress': { name: 'Compress Images', params: [
      { k: 'input', label: 'Images', type: 'files', v: ['dune.jpg'] },
      { k: 'quality', label: 'Quality', type: 'int', v: 75, max: 100 },
      { k: 'format', label: 'Output format', type: 'choice', v: 'webp', show: 'WebP' },
    ] },
    'video.compress': { name: 'Compress Video', params: [
      { k: 'input', label: 'Video', type: 'files', v: ['talk.mov'] },
      { k: 'mode', label: 'Mode', type: 'choice', v: 'size', show: 'To a target file size' },
      { k: 'target_mb', label: 'Target size (MB)', type: 'int', v: 25, max: 100 },
    ] },
    'speech.transcribe': { name: 'Transcribe Audio / Video', params: [
      { k: 'input', label: 'Audio / video', type: 'files', v: ['interview.m4a'] },
      { k: 'formats', label: 'Formats', type: 'multi', v: ['srt', 'txt'], all: ['txt', 'srt', 'vtt', 'json', 'md'] },
    ] },
    'pdf.merge': { name: 'Merge PDFs', out: 'merged.pdf', params: [
      { k: 'input', label: 'PDF files (in order)', type: 'files', v: ['cover.pdf', 'lease-signed.pdf'] },
    ] },
    'dev.json': { name: 'JSON Formatter & Validator', params: [
      { k: 'text', label: 'JSON', type: 'text', v: '{"b":1,"a":[1,2]}' },
      { k: 'sort_keys', label: 'Sort object keys', type: 'bool', v: true },
    ] },
    'files.hash': { name: 'File Hash', params: [
      { k: 'input', label: 'Files', type: 'files', v: ['ubuntu.iso'] },
      { k: 'algorithms', label: 'Algorithms', type: 'multi', v: ['sha256', 'blake3'], all: ['md5', 'sha1', 'sha256', 'sha512', 'blake3'] },
    ] },
  };
  const shellQuote = (s) => (/^[\w@%+=:,./-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`);
  const cliName = (id) => { const [cat, rest] = id.split('.'); return `dogubako ${cat} ${rest.replace(/_/g, '-')}`; };
  const kebab = (k) => k.replace(/_/g, '-');
  const fileIds = (p) => p.v.map((_, i) => `f_${i + 1}`);
  const args = (t) => Object.fromEntries(t.params.map((p) => [p.k, p.type === 'files' ? fileIds(p) : p.v]));
  function jsonHtml(v, ind = 2, lvl = 0) {
    const pad = (n) => '\n' + ' '.repeat(ind * n);
    if (Array.isArray(v)) return '[' + v.map((x) => jsonHtml(x, ind, lvl)).join(', ') + ']';
    if (v && typeof v === 'object') return '{' + Object.keys(v).map((k) => `${pad(lvl + 1)}<span class="k">${esc(JSON.stringify(k))}</span>: ${jsonHtml(v[k], ind, lvl + 1)}`).join(',') + pad(lvl) + '}';
    if (typeof v === 'string') return `<span class="s">${esc(JSON.stringify(v))}</span>`;
    return `<span class="n">${esc(String(v))}</span>`;
  }
  const VIEWS = {
    app(id, t) {
      const rows = t.params.map((p) => {
        let v;
        if (p.type === 'files') v = p.v.map((f) => `<span class="m-chip">${esc(f)}</span>`).join('');
        else if (p.type === 'int') v = `<span class="m-range" style="--v:${p.v / p.max}"><span></span></span><span class="m-num mono">${p.v}</span>`;
        else if (p.type === 'choice') v = `<span class="m-select">${esc(p.show)}</span>`;
        else if (p.type === 'multi') v = p.all.map((a) => `<span class="m-chip${p.v.includes(a) ? ' is-on' : ''}">${esc(a.toUpperCase())}</span>`).join('');
        else if (p.type === 'bool') v = `<span class="m-toggle${p.v ? ' is-on' : ''}"></span>`;
        else v = `<span class="m-text mono">${esc(p.v)}</span>`;
        return `<div class="m-row"><span class="m-label">${esc(p.label)}</span><span class="m-val">${v}</span></div>`;
      }).join('');
      return `<div class="mock"><div class="m-title">${esc(t.name)}</div>${rows}<div class="m-run"><span class="m-btn">Run</span></div></div>`;
    },
    cli(id, t) {
      const text = t.params.find((p) => p.type === 'text');
      const parts = [];
      t.params.forEach((p) => {
        if (p.type === 'files') parts.push(...p.v.map(shellQuote));
        else if (p.type === 'bool') { if (p.v) parts.push(`--${kebab(p.k)}`); }
        else if (p.type === 'multi') parts.push(`--${kebab(p.k)} ${p.v.join(',')}`);
        else if (p.type !== 'text') parts.push(`--${kebab(p.k)} ${p.v}`);
      });
      if (t.out) parts.push(`-o ${t.out}`);
      const pipe = text ? `echo ${esc(shellQuote(text.v))} | ` : '';
      return `<pre class="wcode"><code><span class="p">$</span> ${pipe}${esc(cliName(id))} ${esc(parts.join(' '))}</code></pre>`;
    },
    api(id, t) {
      const f = t.params.find((p) => p.type === 'files');
      const up = f ? f.v.map((n, i) => `<span class="p">PUT</span>  /api/v1/files/raw?name=${esc(n)}\n     <span class="c"># → f_${i + 1}</span>\n`).join('') : '';
      return `<pre class="wcode"><code>${up}<span class="p">POST</span> /api/v1/tools/${esc(id)}/run\n${jsonHtml({ params: args(t), wait_seconds: 60 })}</code></pre>`;
    },
    mcp(id, t) {
      return `<pre class="wcode"><code><span class="c"># tools/call</span> <span class="p">dogubako_run_tool</span>\n${jsonHtml({ tool: id, arguments: args(t) })}</code></pre>`;
    },
  };
  const waysBtns = $$('#waysPick button');
  function showWay(id, animate) {
    const t = WAYS[id];
    if (!t) return;
    waysBtns.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.tool === id)));
    $('#wayId').textContent = id;
    for (const [k, render] of Object.entries(VIEWS)) {
      const el = $(`#way-${k} .way-body`);
      if (!el) continue;
      el.innerHTML = render(id, t);
      if (animate && !still) { el.classList.remove('is-new'); void el.offsetWidth; el.classList.add('is-new'); }
    }
  }
  waysBtns.forEach((b) => b.addEventListener('click', () => showWay(b.dataset.tool, true)));
  if (waysBtns.length) showWay('image.compress', false);

  /* ------------------------------------------------ the trailer frame
   * The video is copied in at deploy time. Until a HEAD request finds it, the frame shows its
   * placeholder; once it does, the player takes the frame and the hero gets its "Watch" link. */
  const frame = $('#trailerFrame'), ph = $('#trailerPh'), link = $('#trailerLink');
  const SRC = 'trailer/dogubako-alpha-trailer.mp4';
  let video = null;
  if (frame && window.fetch) {
    fetch(SRC, { method: 'HEAD' }).then((r) => {
      const type = r.headers.get('content-type') || 'video/mp4';
      if (!r.ok || !/video|octet-stream/.test(type)) return;
      video = document.createElement('video');
      video.className = 'trailer-video';
      video.controls = true;
      video.playsInline = true;
      video.preload = 'none';
      video.poster = 'trailer/poster.jpg';
      video.setAttribute('aria-label', 'Dogubako alpha trailer');
      const source = document.createElement('source');
      source.src = SRC; source.type = 'video/mp4';
      video.appendChild(source);
      frame.appendChild(video);
      frame.classList.add('has-video');
      if (ph) ph.hidden = true;
      if (link) link.hidden = false;
    }).catch(() => { /* keep the placeholder */ });
  }
  if (link) {
    link.addEventListener('click', (e) => {
      if (!video) return;
      e.preventDefault();
      const sec = $('#trailer');
      const to = sec.getBoundingClientRect().top + window.scrollY - Math.max(72, (window.innerHeight - frame.offsetHeight) / 2);
      const glide = window.DGB.demo ? window.DGB.demo.glide : (y) => { window.scrollTo(0, y); return Promise.resolve(); };
      const playing = video.play(); // inside the click, so the browser lets it start with sound
      if (playing && playing.catch) playing.catch(() => {});
      glide(Math.max(0, to)).then(() => video.focus({ preventScroll: true }));
    });
  }

  /* ------------------------------------------------ the agent's policy checks tick through, once */
  const pol = $$('#polLog li');
  if (pol.length) {
    if (still) pol.forEach((li) => li.classList.add('on'));
    else {
      let i = 0, timer = 0, vis = false;
      const step = () => {
        timer = 0;
        if (i >= pol.length) return;
        if (!vis || document.hidden) return;
        pol[i++].classList.add('on');
        if (i < pol.length) timer = setTimeout(step, 750);
      };
      const kick = () => { if (vis && !document.hidden && !timer && i < pol.length) timer = setTimeout(step, 400); };
      new IntersectionObserver((es) => { for (const e of es) vis = e.isIntersecting; kick(); }, { threshold: 0.5 }).observe($('#polLog'));
      document.addEventListener('visibilitychange', kick);
    }
  }

  /* ------------------------------------------------ the box opens when the download card arrives */
  const dl = $('#dlCard');
  if (dl && !still) new IntersectionObserver((es, o) => { for (const e of es) if (e.isIntersecting) { setTimeout(() => dl.classList.add('open'), 350); o.disconnect(); } }, { threshold: 0.5 }).observe(dl);
})();
