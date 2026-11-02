// Page behaviour outside the 3D scene: theme, the four-ways-in comparison,
// scroll reveals, the terminal, and loading (or skipping) the WebGL bench.
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const html = document.documentElement;
const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

// ------------------------------------------------------------------ theme
const toggle = $('#theme-toggle');
const dark = () => (html.getAttribute('data-theme') || (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')) === 'dark';
function syncToggle() { toggle.setAttribute('aria-label', dark() ? 'Switch to light theme' : 'Switch to dark theme'); }
toggle.addEventListener('click', () => {
  const next = dark() ? 'light' : 'dark';
  html.setAttribute('data-theme', next);
  try { localStorage.setItem('dgb-theme', next); } catch { /* private mode */ }
  syncToggle();
  document.dispatchEvent(new Event('dgb:theme'));
});
syncToggle();

// ------------------------------------------------------------------ the 3D bench, or its still
function toStatic() {
  html.classList.remove('has-3d', 'scene-ready');
  const inside = $('#inside');
  if (inside) inside.style.height = '';
}
if (html.classList.contains('has-3d')) {
  import('./scene.js').then((m) => m.start()).catch((err) => { console.warn('3D bench unavailable, showing the still.', err); toStatic(); });
}

// ------------------------------------------------------------------ one tool, four ways in
// One description per tool; the four views are generated from it, the way the app,
// CLI, HTTP API and MCP server are generated from the tool spec.
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
  return esc(String(v));
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
    return `<pre class="code"><code><span class="p">$</span> ${pipe}${esc(cliName(id))} ${esc(parts.join(' '))}</code></pre>`;
  },
  api(id, t) {
    const f = t.params.find((p) => p.type === 'files');
    const up = f ? f.v.map((n, i) => `<span class="p">PUT</span>  /api/v1/files/raw?name=${esc(n)}\n     <span class="c"># → f_${i + 1}</span>\n`).join('') : '';
    return `<pre class="code"><code>${up}<span class="p">POST</span> /api/v1/tools/${esc(id)}/run\n${jsonHtml({ params: args(t), wait_seconds: 60 })}</code></pre>`;
  },
  mcp(id, t) {
    return `<pre class="code"><code><span class="c"># tools/call</span> <span class="p">dogubako_run_tool</span>\n${jsonHtml({ tool: id, arguments: args(t) })}</code></pre>`;
  },
};
const waysBtns = $$('#ways-pick button');
function showWay(id, animate = true) {
  const t = WAYS[id];
  waysBtns.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.tool === id)));
  $('#way-id').textContent = id;
  for (const [k, render] of Object.entries(VIEWS)) {
    const el = $(`#way-${k} .way-body`);
    el.innerHTML = render(id, t);
    if (animate && !calm) { el.classList.remove('is-new'); void el.offsetWidth; el.classList.add('is-new'); }
  }
}
waysBtns.forEach((b) => b.addEventListener('click', () => showWay(b.dataset.tool)));
showWay('image.compress', false);

// ------------------------------------------------------------------ reveals (restrained, once)
if (!calm && 'IntersectionObserver' in window) {
  html.classList.add('motion');
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      e.target.classList.add('is-in');
      io.unobserve(e.target);
    }
  }, { rootMargin: '0px 0px -8% 0px', threshold: 0.12 });
  $$('.reveal').forEach((el, i) => { el.style.transitionDelay = `${(i % 4) * 60}ms`; io.observe(el); });
}

// ------------------------------------------------------------------ the terminal types itself once, when seen
const term = $('.term-body code');
if (term && !calm && 'IntersectionObserver' in window) {
  const full = term.innerHTML;
  const lines = full.split('\n');
  term.innerHTML = lines.map(() => '').join('\n');
  let started = false;
  const io = new IntersectionObserver(async (entries) => {
    if (started || !entries.some((e) => e.isIntersecting)) return;
    started = true;
    io.disconnect();
    const shown = lines.map(() => '');
    const caret = '<span class="caret" aria-hidden="true"></span>';
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const isCmd = line.startsWith('<span class="p">$</span>');
      if (isCmd) {
        const prefix = '<span class="p">$</span> ';
        const text = line.slice(prefix.length).replace(/&amp;/g, '&');
        for (let c = 0; c <= text.length; c += 2) {
          shown[i] = prefix + esc(text.slice(0, c)) + caret;
          term.innerHTML = shown.join('\n');
          await new Promise((r) => setTimeout(r, 14));
        }
      }
      shown[i] = line;
      term.innerHTML = shown.join('\n') + (i === lines.length - 1 ? ' ' + caret : '');
      await new Promise((r) => setTimeout(r, isCmd ? 260 : 90));
    }
  }, { threshold: 0.4 });
  io.observe(term);
}

// ------------------------------------------------------------------ the receipt prints line by line
const receipt = $('.receipt-paper');
if (receipt && !calm && 'IntersectionObserver' in window) {
  const rows = $$('.r-lines li, .r-total div', receipt);
  rows.forEach((r) => { r.style.opacity = '0'; r.style.transform = 'translateY(4px)'; });
  const io = new IntersectionObserver((entries) => {
    if (!entries.some((e) => e.isIntersecting)) return;
    io.disconnect();
    rows.forEach((r, i) => r.animate([{ opacity: 0, transform: 'translateY(4px)' }, { opacity: 1, transform: 'none' }], { duration: 380, delay: 200 + i * 120, easing: 'cubic-bezier(.16,1,.3,1)', fill: 'forwards' }));
  }, { threshold: 0.35 });
  io.observe(receipt);
}
