// Dogubako website. No framework, no tracking, no uploads: dropped files are
// only looked at (name, type, size) and drawn on the bench, like the app does.
(() => {
  'use strict';
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => Array.from(el.querySelectorAll(s));
  const root = document.documentElement;
  const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

  // ------------------------------------------------------------- theme
  const isDark = () => root.dataset.theme ? root.dataset.theme === 'dark' : darkQuery.matches;
  const themeBtn = $('#theme-toggle');
  const syncThemeLabel = () => themeBtn.setAttribute('aria-label', isDark() ? 'Switch to light theme' : 'Switch to dark theme');
  themeBtn.addEventListener('click', () => {
    const next = isDark() ? 'light' : 'dark';
    root.dataset.theme = next;
    try { localStorage.setItem('dgb-theme', next); } catch (e) { /* private mode */ }
    syncThemeLabel();
    showShot(currentShot);
  });
  darkQuery.addEventListener('change', () => { syncThemeLabel(); showShot(currentShot); });
  syncThemeLabel();

  // ------------------------------------------------------------- bench drawings
  // Same deterministic waveform as the app, so a file always draws the same way.
  function bars(seed, n) {
    let h = 2166136261;
    for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
    const out = [];
    for (let i = 0; i < n; i++) {
      h = Math.imul(h ^ (h >>> 13), 1274126177);
      const r = ((h >>> 0) % 1000) / 1000;
      out.push(Math.round(8 + r * 36 * (0.45 + 0.55 * Math.sin(i / 4) ** 2)));
    }
    return out;
  }
  function fillWave(el, seed) {
    el.replaceChildren(...bars(seed, 42).map((h) => { const s = document.createElement('span'); s.style.height = h + 'px'; return s; }));
  }
  $$('.bo-wave[data-seed]').forEach((el) => fillWave(el, el.dataset.seed));

  const ext = (name) => (name.includes('.') ? name.split('.').pop().toLowerCase() : '');
  const KIND_BY_EXT = {
    image: 'jpg jpeg png gif webp avif heic heif tif tiff bmp svg ico raw cr2 nef dng',
    pdf: 'pdf',
    video: 'mp4 mov mkv webm avi m4v wmv flv mts',
    audio: 'mp3 wav flac m4a aac ogg opus aiff wma',
    sheet: 'csv tsv xlsx xls ods',
    text: 'txt md json yaml yml toml xml html htm css js ts tsx jsx py rs go java c h cpp sql srt vtt log ini env sh',
    doc: 'doc docx odt rtf epub ppt pptx odp pages key',
    archive: 'zip tar gz tgz 7z rar bz2 xz zst',
  };
  function kindOf(name, mime) {
    const e = ext(name);
    for (const [k, list] of Object.entries(KIND_BY_EXT)) if (list.split(' ').includes(e)) return k;
    if (mime) {
      if (mime.startsWith('image/')) return 'image';
      if (mime.startsWith('video/')) return 'video';
      if (mime.startsWith('audio/')) return 'audio';
      if (mime.startsWith('text/')) return 'text';
    }
    return 'other';
  }

  // Likeliest tools per kind of file, in the order the app's drop screen ranks them.
  const SUGGEST = {
    image: [['Convert Images', 'PNG, JPEG, WebP, AVIF, HEIC and more'], ['Compress Images', 'Smaller files, same look'], ['Redact Regions', 'Blur, pixelate or black out parts of the image'], ['Remove Image Metadata', 'Strip EXIF, GPS location and comments']],
    pdf: [['Compress PDF', 'Shrink scans and exports'], ['Merge PDFs', 'Combine files into one'], ['OCR PDF', 'Make scanned pages searchable'], ['Extract Text', 'Get the words out of the pages']],
    video: [['Compress Video', 'Hit a target size, keep it watchable'], ['Trim Video', 'Cut to the part that matters'], ['Video to GIF', 'Loop a moment'], ['Extract Audio from Video', 'Keep only the sound']],
    audio: [['Transcribe Audio / Video', 'Offline speech to text, SRT or VTT'], ['Convert Audio', 'MP3, FLAC, WAV, Opus and more'], ['Normalize Audio', 'Even out the loudness (EBU R128)'], ['Remove Silence', 'Cut the dead air']],
    sheet: [['CSV Viewer', 'Open it as a table'], ['Column Statistics', 'Counts, types and ranges per column'], ['Filter & Sort CSV', 'Find the rows you need'], ['Clean CSV Data', 'Trim, dedupe and tidy']],
    text: [['Word & Character Count', 'Characters, words and lines'], ['Compare Text (Diff)', 'See what changed'], ['Find & Replace', 'Change words or patterns'], ['Convert Text Encoding', 'Fix garbled characters']],
    doc: [['Convert Office Documents', 'To PDF and other formats'], ['Extract Text from Document', 'Plain text out of the file'], ['Word & Character Count', 'Characters, words and lines'], ['Detect File Type', 'What is this, really?']],
    archive: [['List Archive Contents', 'See what’s inside before extracting'], ['Extract Archive', 'Safely, with bomb protection'], ['File Hash', 'SHA-256 and friends'], ['Create Archive', 'zip, tar or 7z']],
    other: [['Detect File Type', 'What is this, really?'], ['File Hash', 'SHA-256 and friends'], ['Hex Viewer', 'Look at the bytes'], ['Create Archive', 'zip, tar or 7z']],
  };

  function formatBytes(n) {
    if (!Number.isFinite(n)) return '';
    const u = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
    return (i === 0 ? n : n.toFixed(n < 10 ? 1 : 0)) + ' ' + u[i];
  }

  const list = $('#intent-list');
  const fileLabel = $('#intent-file');
  function showIntent(name, kind) {
    fileLabel.textContent = name;
    list.replaceChildren(...SUGGEST[kind].map(([t, d], i) => {
      const li = document.createElement('li');
      li.className = 'is-new';
      li.innerHTML = '<span class="n"></span><span><b></b><span class="d"></span></span>';
      li.querySelector('.n').textContent = String(i + 1);
      li.querySelector('b').textContent = t;
      li.querySelector('.d').textContent = d;
      return li;
    }));
    $$('.bench-object').forEach((o) => o.classList.toggle('is-on', o.dataset.name === name));
  }

  const shelf = $('#bench-shelf');
  shelf.addEventListener('click', (e) => {
    const obj = e.target.closest('.bench-object');
    if (obj) showIntent(obj.dataset.name, obj.dataset.kind);
  });
  $$('.bench-object').forEach((o) => { o.tabIndex = 0; o.setAttribute('role', 'button'); });
  shelf.addEventListener('keydown', (e) => {
    if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('.bench-object')) { e.preventDefault(); e.target.click(); }
  });

  function drawThing(kind, file, url) {
    const thing = document.createElement('span');
    thing.className = 'bo-thing';
    const tpl = {
      pdf: $('.bench-object[data-kind="pdf"] .bo-thing'),
      sheet: $('.bench-object[data-kind="sheet"] .bo-thing'),
    };
    if (kind === 'image' && url) {
      const img = document.createElement('img'); img.className = 'bo-photo'; img.alt = ''; img.src = url; thing.append(img);
    } else if (kind === 'video') {
      thing.innerHTML = '<span class="bo-film" aria-hidden="true"><span class="bo-sprockets"></span><span class="bo-frame"></span><span class="bo-sprockets"></span></span>';
      if (url) {
        const v = document.createElement('video'); v.className = 'bo-frame'; v.muted = true; v.playsInline = true; v.preload = 'metadata'; v.src = url + '#t=0.1';
        thing.querySelector('span.bo-frame').replaceWith(v);
      }
    } else if (kind === 'audio') {
      const w = document.createElement('span'); w.className = 'bo-wave'; w.setAttribute('aria-hidden', 'true'); fillWave(w, file.name); thing.append(w);
    } else if (tpl[kind]) {
      thing.innerHTML = tpl[kind].innerHTML;
    } else if (kind === 'text' || kind === 'doc') {
      thing.innerHTML = '<span class="bo-sheet bo-text" aria-hidden="true"><span class="bo-code bo-w60"></span><span class="bo-code bo-w80 bo-in"></span><span class="bo-code bo-w70 bo-in"></span><span class="bo-code bo-w50 bo-in"></span><span class="bo-code bo-w30"></span></span>';
      thing.firstChild.style.position = 'relative';
    } else {
      thing.innerHTML = '<span class="bo-card" aria-hidden="true"><span class="bo-ext"></span></span>';
      thing.querySelector('.bo-ext').textContent = (ext(file.name) || 'file').toUpperCase().slice(0, 5);
    }
    return thing;
  }

  const objectUrls = [];
  function putOnBench(files) {
    const added = [];
    for (const file of Array.from(files).slice(0, 6)) {
      const kind = kindOf(file.name, file.type);
      // Only previewable media gets an object URL; it never leaves this tab.
      const url = (kind === 'image' || kind === 'video') && file.size < 200e6 ? URL.createObjectURL(file) : '';
      if (url) objectUrls.push(url);
      const fig = document.createElement('figure');
      fig.className = 'bench-object is-new';
      fig.dataset.kind = kind; fig.dataset.name = file.name;
      fig.tabIndex = 0; fig.setAttribute('role', 'button');
      const cap = document.createElement('figcaption');
      cap.innerHTML = '<span class="bo-name"></span><span class="bo-meta mono"></span>';
      cap.firstChild.textContent = file.name;
      cap.lastChild.textContent = 'Just now · ' + formatBytes(file.size);
      fig.append(drawThing(kind, file, url), cap);
      shelf.prepend(fig);
      added.push(fig);
    }
    // Keep the shelf to a tidy handful.
    $$('.bench-object', shelf).slice(8).forEach((o) => o.remove());
    if (added.length) {
      shelf.scrollTo({ left: 0, behavior: 'smooth' });
      const last = added[added.length - 1];
      showIntent(last.dataset.name, last.dataset.kind);
    }
  }
  window.addEventListener('pagehide', () => objectUrls.forEach((u) => URL.revokeObjectURL(u)));

  $('#demo-file').addEventListener('change', (e) => { putOnBench(e.target.files); e.target.value = ''; });

  // Drop anywhere on the page, like the app.
  const demo = $('#drop-demo');
  let depth = 0;
  const hasFiles = (e) => e.dataTransfer && Array.from(e.dataTransfer.types || []).includes('Files');
  window.addEventListener('dragenter', (e) => { if (!hasFiles(e)) return; depth++; demo.classList.add('is-over'); });
  window.addEventListener('dragleave', () => { depth = Math.max(0, depth - 1); if (!depth) demo.classList.remove('is-over'); });
  window.addEventListener('dragover', (e) => { if (hasFiles(e)) e.preventDefault(); });
  window.addEventListener('drop', (e) => {
    if (!hasFiles(e)) return;
    e.preventDefault(); depth = 0; demo.classList.remove('is-over');
    putOnBench(e.dataTransfer.files);
    if (window.scrollY > demo.offsetTop) demo.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });

  showIntent('dune.jpg', 'image');

  // ------------------------------------------------------------- tour
  // Screens that have a dark capture follow the theme; the rest stay light.
  const HAS_DARK = new Set(['home', 'tool-image-result', 'tool-live-json', 'redact-editor']);
  const HAS_LIGHT = new Set(['home', 'drop-screen', 'tool-image-result', 'tool-live-json', 'redact-editor', 'command-palette']);
  const tourImg = $('#tour-img');
  let currentShot = 'home';
  function showShot(shot) {
    currentShot = shot;
    const variant = isDark() && HAS_DARK.has(shot) ? 'dark' : HAS_LIGHT.has(shot) ? 'light' : 'dark';
    const src = `screenshots/${shot}-${variant}.webp`;
    if (tourImg.getAttribute('src') === src) return;
    tourImg.classList.add('is-swapping');
    const next = new Image();
    next.onload = next.onerror = () => { tourImg.src = src; tourImg.classList.remove('is-swapping'); };
    next.src = src;
  }
  const tourTabs = $$('.tour-tabs [role="tab"]');
  tourTabs.forEach((tab) => tab.addEventListener('click', () => {
    tourTabs.forEach((t) => t.setAttribute('aria-selected', String(t === tab)));
    tourImg.alt = tab.textContent + ' screen';
    $('#tour-caption').textContent = tab.dataset.caption;
    showShot(tab.dataset.shot);
  }));
  // Preload the rest once the page is idle so switching tabs is instant.
  (window.requestIdleCallback || ((f) => setTimeout(f, 1500)))(() => {
    tourTabs.forEach((t) => { const i = new Image(); i.src = `screenshots/${t.dataset.shot}-light.webp`; });
  });
  showShot('home');

  // ------------------------------------------------------------- tabs with keyboard support
  function tabs(container) {
    const ts = $$('[role="tab"]', container);
    const select = (tab) => {
      ts.forEach((t) => {
        const on = t === tab;
        t.setAttribute('aria-selected', String(on));
        t.tabIndex = on ? 0 : -1;
        const panel = document.getElementById(t.getAttribute('aria-controls'));
        if (panel && panel.id !== 'tour-panel') panel.hidden = !on;
      });
    };
    ts.forEach((t, i) => {
      t.addEventListener('click', () => select(t));
      t.addEventListener('keydown', (e) => {
        const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
        if (!d) return;
        e.preventDefault();
        const n = ts[(i + d + ts.length) % ts.length];
        n.focus(); n.click();
      });
    });
  }
  tabs($('.ways-tabs'));
  tabs($('.tour-tabs'));

  // ------------------------------------------------------------- trailer
  // Show the player only when the video is actually deployed next to the page.
  const video = $('#trailer-video');
  const src = video && $('source', video).getAttribute('src');
  if (src) {
    fetch(src, { method: 'HEAD' }).then((r) => {
      if (!r.ok || !/video/.test(r.headers.get('content-type') || 'video/mp4')) return;
      video.hidden = false;
      $('#trailer-placeholder').hidden = true;
      $('#trailer').classList.add('has-video');
    }).catch(() => { /* keep the placeholder */ });
  }

  // ------------------------------------------------------------- downloads
  const ua = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || navigator.userAgent || '';
  const os = /mac/i.test(ua) ? 'mac' : /win/i.test(ua) ? 'windows' : /linux|x11/i.test(ua) && !/android/i.test(navigator.userAgent) ? 'linux' : '';
  if (os) {
    const card = $(`.dl[data-os="${os}"]`);
    if (card) {
      card.classList.add('is-yours');
      card.parentElement.prepend(card);
      $('#hero-download').textContent = 'Get it for ' + $('.dl-os', card).textContent;
    }
  }
})();
