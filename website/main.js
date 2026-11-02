// Dogubako website. No framework, no tracking, no uploads. The bench runs a
// handful of tools for real, inside this tab, on files you give it.
(() => {
  'use strict';
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => Array.from(el.querySelectorAll(s));
  const h = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const root = document.documentElement;
  root.classList.add('js');
  const darkQuery = window.matchMedia('(prefers-color-scheme: dark)');
  const idle = window.requestIdleCallback || ((f) => setTimeout(f, 600));

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

  // ------------------------------------------------------------- helpers
  function formatBytes(n) {
    if (!Number.isFinite(n)) return '';
    const u = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    while (n >= 1000 && i < u.length - 1) { n /= 1000; i++; }
    return (i === 0 ? n : n.toFixed(n < 10 ? 1 : 0)) + ' ' + u[i];
  }
  const ext = (name) => (name.includes('.') ? name.split('.').pop().toLowerCase() : '');
  const stem = (name) => (name.includes('.') ? name.slice(0, name.lastIndexOf('.')) : name);
  const shellQuote = (s) => (/^[\w@%+=:,./-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`);

  // The CLI is generated from tool ids: image.strip_metadata -> dogubako image strip-metadata
  const cliName = (id) => { const [cat, rest] = id.split('.'); return `dogubako ${cat} ${rest.replace(/_/g, '-')}`; };

  // ------------------------------------------------------------- bench drawings
  // Same deterministic waveform as the app, so a file always draws the same way.
  function bars(seed, n) {
    let x = 2166136261;
    for (let i = 0; i < seed.length; i++) x = Math.imul(x ^ seed.charCodeAt(i), 16777619);
    const out = [];
    for (let i = 0; i < n; i++) {
      x = Math.imul(x ^ (x >>> 13), 1274126177);
      const r = ((x >>> 0) % 1000) / 1000;
      out.push(Math.round(8 + r * 36 * (0.45 + 0.55 * Math.sin(i / 4) ** 2)));
    }
    return out;
  }
  function fillWave(el, seed) {
    el.replaceChildren(...bars(seed, 42).map((v) => { const s = document.createElement('span'); s.style.height = v + 'px'; return s; }));
  }
  $$('.bo-wave[data-seed]').forEach((el) => fillWave(el, el.dataset.seed));

  const KIND_BY_EXT = {
    image: 'jpg jpeg png gif webp avif heic heif tif tiff bmp svg ico raw cr2 nef dng',
    pdf: 'pdf',
    video: 'mp4 mov mkv webm avi m4v wmv flv mts',
    audio: 'mp3 wav flac m4a aac ogg opus aiff wma',
    sheet: 'csv tsv xlsx xls ods',
    json: 'json',
    text: 'txt md yaml yml toml xml html htm css js ts tsx jsx py rs go java c h cpp sql srt vtt log ini env sh',
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
      if (mime === 'application/json') return 'json';
      if (mime.startsWith('text/')) return 'text';
    }
    return 'other';
  }

  // ------------------------------------------------------------- tools that run in this tab
  // Each returns { file } (a new File), { text } or { table }. They are small,
  // honest stand-ins: the app's versions do more (oxipng, AVIF, lossless metadata removal).
  const BROWSER_IMAGE = /^image\/(png|jpeg|gif|webp|bmp|avif)$/;
  async function decode(file) {
    if (!BROWSER_IMAGE.test(file.type) && !/\.(png|jpe?g|gif|webp|bmp|avif)$/i.test(file.name)) {
      throw new Error('This browser can’t open that kind of image. The app can (HEIC, RAW, TIFF and more).');
    }
    const bmp = await createImageBitmap(file);
    const c = document.createElement('canvas');
    c.width = bmp.width; c.height = bmp.height;
    c.getContext('2d').drawImage(bmp, 0, 0);
    bmp.close && bmp.close();
    return c;
  }
  const encode = (canvas, type, q) => new Promise((res, rej) => canvas.toBlob((b) => (b ? res(b) : rej(new Error('Encoding failed'))), type, q));
  const EXT_OF = { 'image/jpeg': 'jpg', 'image/png': 'png', 'image/webp': 'webp' };
  const asFile = (blob, name) => new File([blob], name, { type: blob.type });

  const MAGIC = [
    [[0x89, 0x50, 0x4e, 0x47], 0, 'PNG image', 'image/png'],
    [[0xff, 0xd8, 0xff], 0, 'JPEG image', 'image/jpeg'],
    [[0x47, 0x49, 0x46, 0x38], 0, 'GIF image', 'image/gif'],
    [[0x57, 0x45, 0x42, 0x50], 8, 'WebP image', 'image/webp'],
    [[0x57, 0x41, 0x56, 0x45], 8, 'WAV audio', 'audio/wav'],
    [[0x41, 0x56, 0x49, 0x20], 8, 'AVI video', 'video/x-msvideo'],
    [[0x25, 0x50, 0x44, 0x46], 0, 'PDF document', 'application/pdf'],
    [[0x50, 0x4b, 0x03, 0x04], 0, 'ZIP archive (also DOCX, XLSX, EPUB, JAR)', 'application/zip'],
    [[0x1f, 0x8b], 0, 'gzip archive', 'application/gzip'],
    [[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c], 0, '7-Zip archive', 'application/x-7z-compressed'],
    [[0x52, 0x61, 0x72, 0x21], 0, 'RAR archive', 'application/vnd.rar'],
    [[0x28, 0xb5, 0x2f, 0xfd], 0, 'Zstandard archive', 'application/zstd'],
    [[0xfd, 0x37, 0x7a, 0x58, 0x5a], 0, 'xz archive', 'application/x-xz'],
    [[0x66, 0x74, 0x79, 0x70], 4, 'MP4 / QuickTime / HEIF container', 'video/mp4'],
    [[0x1a, 0x45, 0xdf, 0xa3], 0, 'Matroska / WebM video', 'video/webm'],
    [[0x49, 0x44, 0x33], 0, 'MP3 audio (ID3 tagged)', 'audio/mpeg'],
    [[0x66, 0x4c, 0x61, 0x43], 0, 'FLAC audio', 'audio/flac'],
    [[0x4f, 0x67, 0x67, 0x53], 0, 'Ogg audio', 'audio/ogg'],
    [[0x7f, 0x45, 0x4c, 0x46], 0, 'ELF executable (Linux)', 'application/x-elf'],
    [[0x4d, 0x5a], 0, 'Windows executable', 'application/x-msdownload'],
    [[0xcf, 0xfa, 0xed, 0xfe], 0, 'Mach-O executable (macOS)', 'application/x-mach-binary'],
    [[0x53, 0x51, 0x4c, 0x69, 0x74, 0x65], 0, 'SQLite database', 'application/vnd.sqlite3'],
    [[0x42, 0x4d], 0, 'BMP image', 'image/bmp'],
    [[0x49, 0x49, 0x2a, 0x00], 0, 'TIFF image', 'image/tiff'],
    [[0x4d, 0x4d, 0x00, 0x2a], 0, 'TIFF image', 'image/tiff'],
    [[0x00, 0x00, 0x01, 0x00], 0, 'Windows icon', 'image/x-icon'],
    [[0xd0, 0xcf, 0x11, 0xe0], 0, 'Old Office document (DOC, XLS, PPT)', 'application/x-ole-storage'],
  ];
  const head = async (file, n) => new Uint8Array(await file.slice(0, n).arrayBuffer());
  const tooBig = (file, max, what) => { if (file.size > max) throw new Error(`That file is ${formatBytes(file.size)}. This page ${what} up to ${formatBytes(max)}; the app has separate resource limits.`); };

  function parseCsv(text, delim) {
    const rows = []; let row = []; let cell = ''; let q = false;
    for (let i = 0; i < text.length && rows.length < 9; i++) {
      const c = text[i];
      if (q) { if (c === '"') { if (text[i + 1] === '"') { cell += '"'; i++; } else q = false; } else cell += c; }
      else if (c === '"') q = true;
      else if (c === delim) { row.push(cell); cell = ''; }
      else if (c === '\n' || c === '\r') { if (c === '\r' && text[i + 1] === '\n') i++; row.push(cell); rows.push(row); row = []; cell = ''; }
      else cell += c;
    }
    if (cell || row.length) { row.push(cell); rows.push(row); }
    return rows.filter((r) => r.some((c) => c !== ''));
  }

  const RUN = {
    async 'image.compress'(file) {
      const c = await decode(file);
      const type = file.type === 'image/jpeg' ? 'image/jpeg' : 'image/webp';
      const out = await encode(c, type, 0.75);
      // Like the app's "keep the original if smaller" setting.
      if (out.size >= file.size) return { file, note: 'It was already as small as quality 75 gets, so you keep the original.' };
      return { file: asFile(out, `${stem(file.name)}-small.${EXT_OF[type]}`), note: 'Quality 75. The app also squeezes PNGs losslessly and can write AVIF.' };
    },
    async 'image.convert'(file) {
      const c = await decode(file);
      const type = file.type === 'image/webp' ? 'image/png' : 'image/webp';
      const out = await encode(c, type, 0.85);
      return { file: asFile(out, `${stem(file.name)}.${EXT_OF[type]}`), note: `Converted to ${EXT_OF[type].toUpperCase()}. The app reads and writes about 20 formats, including HEIC, AVIF and JPEG XL.` };
    },
    async 'image.strip_metadata'(file) {
      const c = await decode(file);
      const type = file.type === 'image/jpeg' ? 'image/jpeg' : 'image/png';
      const out = await encode(c, type, 0.92);
      return { file: asFile(out, `${stem(file.name)}-clean.${EXT_OF[type]}`), note: 'Drawn again without EXIF, GPS or comments. The app removes them without re-encoding the pixels.' };
    },
    async 'files.hash'(file) {
      tooBig(file, 512e6, 'hashes files');
      const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
      const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
      return { text: `SHA-256  ${hex}`, note: 'The app also does BLAKE3, SHA-1, SHA-512, MD5 and CRC32, and checks them against a checksum file.' };
    },
    async 'files.type_detect'(file) {
      const b = await head(file, 64);
      const hit = MAGIC.find(([sig, off]) => sig.every((v, i) => b[off + i] === v));
      let what = hit ? `${hit[2]}\n${hit[3]}` : '';
      if (!hit) {
        const sample = await head(file, 4096);
        const text = !sample.includes(0) && (() => { try { new TextDecoder('utf-8', { fatal: true }).decode(sample); return true; } catch (e) { return false; } })();
        what = text ? 'Plain text (UTF-8)\ntext/plain' : 'Unknown binary data';
      }
      const claimed = ext(file.name);
      return { text: `${what}\nfrom the first bytes, not the name${claimed ? ` (.${claimed})` : ''}`, note: 'The app knows hundreds of formats and flags files whose name doesn’t match their contents.' };
    },
    async 'files.hexdump'(file) {
      const b = await head(file, 160);
      const lines = [];
      for (let o = 0; o < b.length; o += 16) {
        const row = Array.from(b.slice(o, o + 16));
        const hex = row.map((v) => v.toString(16).padStart(2, '0')).join(' ').padEnd(47, ' ');
        const asc = row.map((v) => (v >= 32 && v < 127 ? String.fromCharCode(v) : '.')).join('');
        lines.push(`${o.toString(16).padStart(8, '0')}  ${hex}  ${asc}`);
      }
      return { text: lines.join('\n') || '(empty file)', wide: true, note: 'The first 160 bytes. The app pages through the whole file.' };
    },
    async 'text.count'(file) {
      tooBig(file, 20e6, 'counts text');
      const t = await file.text();
      const words = (t.match(/\S+/g) || []).length;
      const lines = t ? t.split(/\r\n|\r|\n/).length : 0;
      const chars = Array.from(t).length;
      const minutes = Math.max(1, Math.round(words / 230));
      return { text: `${words.toLocaleString('en')} words\n${chars.toLocaleString('en')} characters\n${lines.toLocaleString('en')} lines\nabout ${minutes} min to read` };
    },
    async 'text.sort_lines'(file) {
      tooBig(file, 20e6, 'sorts text');
      const lines = (await file.text()).split(/\r\n|\r|\n/);
      const sorted = lines.slice().sort(new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' }).compare);
      return { file: asFile(new Blob([sorted.join('\n')], { type: 'text/plain' }), `${stem(file.name)}-sorted.${ext(file.name) || 'txt'}`), note: `${lines.length.toLocaleString('en')} lines, sorted naturally (2 before 10).` };
    },
    async 'dev.json'(file) {
      tooBig(file, 20e6, 'formats JSON');
      const t = await file.text();
      let v;
      try { v = JSON.parse(t); } catch (e) { throw new Error('Not valid JSON: ' + e.message.replace(/^JSON\.parse: /, '')); }
      const out = JSON.stringify(v, null, 2) + '\n';
      return { file: asFile(new Blob([out], { type: 'application/json' }), `${stem(file.name)}.formatted.json`), preview: out.split('\n').length > 9 ? out.split('\n').slice(0, 8).join('\n') + '\n…' : out.trimEnd(), note: 'Valid. Indented with two spaces.' };
    },
    async 'data.csv_view'(file) {
      tooBig(file, 50e6, 'opens CSV');
      const t = await file.slice(0, 64e3).text();
      const delim = ext(file.name) === 'tsv' ? '\t' : (t.split('\n')[0].split(';').length > t.split('\n')[0].split(',').length ? ';' : ',');
      return { table: parseCsv(t, delim), note: 'The first rows. The app sorts, filters and handles millions of rows.' };
    },
  };

  // Likeliest tools per kind of file, in the order the app's drop screen ranks them.
  // [tool id, name, what it does]. Ids in RUN run right here; the rest need the app.
  const SUGGEST = {
    image: [['image.convert', 'Convert Images', 'PNG, JPEG, WebP, AVIF, HEIC and more'], ['image.compress', 'Compress Images', 'Smaller files, same look'], ['image.redact', 'Redact Regions', 'Blur, pixelate or black out parts of the image'], ['image.strip_metadata', 'Remove Image Metadata', 'Strip EXIF, GPS location and comments']],
    pdf: [['pdf.compress', 'Compress PDF', 'Shrink scans and exports'], ['pdf.merge', 'Merge PDFs', 'Combine files into one'], ['pdf.ocr', 'OCR PDF', 'Make scanned pages searchable'], ['pdf.extract_text', 'Extract Text', 'Get the words out of the pages']],
    video: [['video.compress', 'Compress Video', 'Hit a target size, keep it watchable'], ['video.trim', 'Trim Video', 'Cut to the part that matters'], ['video.to_gif', 'Video to GIF', 'Loop a moment'], ['video.extract_audio', 'Extract Audio from Video', 'Keep only the sound']],
    audio: [['speech.transcribe', 'Transcribe Audio / Video', 'Offline speech to text, SRT or VTT'], ['audio.convert', 'Convert Audio', 'MP3, FLAC, WAV, Opus and more'], ['audio.normalize', 'Normalize Audio', 'Even out the loudness (EBU R128)'], ['audio.remove_silence', 'Remove Silence', 'Cut the dead air']],
    sheet: [['data.csv_view', 'CSV Viewer', 'Open it as a table'], ['data.stats', 'Column Statistics', 'Counts, types and ranges per column'], ['data.csv_filter_sort', 'Filter & Sort CSV', 'Find the rows you need'], ['data.clean', 'Clean CSV Data', 'Trim, dedupe and tidy']],
    json: [['dev.json', 'JSON Formatter & Validator', 'Check it and indent it'], ['data.json_to_csv', 'JSON to CSV', 'Flatten it into a table'], ['text.count', 'Word & Character Count', 'Characters, words and lines'], ['files.hash', 'File Hash', 'SHA-256 and friends']],
    text: [['text.count', 'Word & Character Count', 'Characters, words and lines'], ['text.sort_lines', 'Sort Lines', 'Alphabetical, natural or by length'], ['text.diff', 'Compare Text (Diff)', 'See what changed'], ['text.replace', 'Find & Replace', 'Change words or patterns']],
    doc: [['pdf.convert_document', 'Convert Office Documents', 'To PDF and other formats'], ['text.extract', 'Extract Text from Document', 'Plain text out of the file'], ['files.type_detect', 'Detect File Type', 'What is this, really?'], ['files.hash', 'File Hash', 'SHA-256 and friends']],
    archive: [['files.archive_list', 'List Archive Contents', 'See what’s inside before extracting'], ['files.archive_extract', 'Extract Archive', 'Safely, with bomb protection'], ['files.hash', 'File Hash', 'SHA-256 and friends'], ['files.type_detect', 'Detect File Type', 'What is this, really?']],
    other: [['files.type_detect', 'Detect File Type', 'What is this, really?'], ['files.hash', 'File Hash', 'SHA-256 and friends'], ['files.hexdump', 'Hex Viewer', 'Look at the bytes'], ['files.archive_create', 'Create Archive', 'zip, tar or 7z']],
  };
  const EXTRA_ARGS = { 'pdf.merge': ' more.pdf -o merged.pdf', 'text.diff': ' other.txt', 'video.trim': ' --start 0:30 --end 1:15', 'speech.transcribe': ' --formats srt,txt' };

  // ------------------------------------------------------------- the bench
  const shelf = $('#bench-shelf');
  const list = $('#intent-list');
  const fileLabel = $('#intent-file');
  const out = $('#intent-out');
  const files = new WeakMap(); // figure -> File
  const objectUrls = [];
  const keepUrl = (u) => (objectUrls.push(u), u);
  window.addEventListener('pagehide', () => objectUrls.forEach((u) => URL.revokeObjectURL(u)));
  let current = null; // the figure the intent panel is about

  function select(fig) {
    current = fig;
    const name = fig.dataset.name;
    const kind = fig.dataset.kind;
    fileLabel.textContent = name;
    out.replaceChildren();
    list.replaceChildren(...SUGGEST[kind].map(([id, title, d], i) => {
      const li = h('li', 'is-new');
      const live = id in RUN && (files.has(fig) || fig.dataset.sample === 'lazy');
      const b = h('button', live ? 'intent-btn' : 'intent-btn is-app');
      b.type = 'button';
      b.dataset.tool = id;
      b.append(h('span', 'n', String(i + 1)), h('span', 'intent-text'));
      b.lastChild.append(h('b', null, title), h('span', 'd', d));
      b.append(h('span', live ? 'tag tag-run' : 'tag', live ? 'Run here' : 'In the app'));
      li.append(b);
      return li;
    }));
    $$('.bench-object', shelf).forEach((o) => o.classList.toggle('is-on', o === fig));
  }

  async function fileFor(fig) {
    if (files.has(fig)) return files.get(fig);
    const make = SAMPLES[fig.dataset.name];
    if (!make) return null;
    const f = await make();
    files.set(fig, f);
    $('.bo-meta', fig).textContent = 'Sample · ' + formatBytes(f.size);
    return f;
  }

  function showApp(id, name) {
    const box = h('div', 'out-app');
    box.append(h('p', null, 'This one runs in the app. Some tools need separately installed engines or downloaded models; dogubako deps shows requirements. From a terminal it’s:'));
    const pre = h('pre', 'code code-sm');
    pre.innerHTML = `<code><span class="p">$</span> ${esc(cliName(id))} ${esc(shellQuote(name))}${esc(EXTRA_ARGS[id] || '')}</code>`;
    box.append(pre);
    const a = h('a', 'link-arrow', 'Alpha status and source'); a.href = '#download';
    box.append(a);
    out.replaceChildren(box);
  }

  async function run(btn) {
    if (!current || btn.disabled) return;
    const id = btn.dataset.tool;
    const fig = current;
    const name = fig.dataset.name;
    $$('.intent-btn', list).forEach((b) => b.classList.toggle('is-picked', b === btn));
    if (!(id in RUN) || btn.classList.contains('is-app')) return showApp(id, name);
    btn.disabled = true;
    btn.classList.add('is-busy');
    out.replaceChildren(h('p', 'out-busy', 'Working…'));
    const t0 = performance.now();
    try {
      const file = await fileFor(fig);
      if (!file) return showApp(id, name);
      const r = await RUN[id](file);
      const ms = Math.max(1, Math.round(performance.now() - t0));
      if (current !== fig) return;
      const box = h('div', 'out-done');
      if (r.file && r.file !== file) {
        const pct = Math.round((1 - r.file.size / file.size) * 100);
        const line = h('p', 'out-line');
        line.append(h('span', 'mono', `${formatBytes(file.size)} → ${formatBytes(r.file.size)}`));
        if (pct > 0) line.append(h('b', 'out-gain', `${pct}% smaller`));
        line.append(h('span', 'mono soft-3', `${ms} ms`));
        box.append(line);
        if (r.preview) { const p = h('pre', 'out-text'); p.textContent = r.preview; box.append(p); }
        const a = h('a', 'btn btn-ink btn-sm', 'Download ' + r.file.name);
        a.href = keepUrl(URL.createObjectURL(r.file)); a.download = r.file.name;
        box.append(a);
        const added = putOnBench([r.file], { result: true, select: false })[0];
        added && added.classList.add('is-result');
      } else if (r.file) {
        box.append(h('p', 'out-line', `${formatBytes(file.size)}, unchanged`));
      } else if (r.text) {
        const p = h('pre', r.wide ? 'out-text is-wide' : 'out-text');
        p.textContent = r.text;
        box.append(p);
        if (navigator.clipboard) {
          const c = h('button', 'btn btn-paper btn-sm', 'Copy');
          c.type = 'button';
          c.addEventListener('click', () => navigator.clipboard.writeText(r.text).then(() => { c.textContent = 'Copied'; }, () => {}));
          box.append(c);
        }
      } else if (r.table) {
        const wrap = h('div', 'out-table');
        const table = h('table');
        r.table.forEach((row, i) => {
          const tr = h('tr');
          row.slice(0, 6).forEach((cell) => tr.append(h(i ? 'td' : 'th', null, cell)));
          table.append(tr);
        });
        wrap.append(table);
        box.append(wrap);
      }
      if (r.note) box.append(h('p', 'out-note', r.note));
      out.replaceChildren(box);
    } catch (e) {
      if (current === fig) out.replaceChildren(h('p', 'out-err', e.message || String(e)));
    } finally {
      btn.disabled = false;
      btn.classList.remove('is-busy');
    }
  }
  list.addEventListener('click', (e) => { const b = e.target.closest('.intent-btn'); if (b) run(b); });

  // Number keys pick a suggestion while you're in the demo, like the app's drop screen.
  const demo = $('#drop-demo');
  demo.addEventListener('keydown', (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !/^[1-4]$/.test(e.key) || e.target.matches('input, textarea')) return;
    const b = $$('.intent-btn', list)[Number(e.key) - 1];
    if (b) { e.preventDefault(); b.focus(); run(b); }
  });

  shelf.addEventListener('click', (e) => { const o = e.target.closest('.bench-object'); if (o) select(o); });
  shelf.addEventListener('keydown', (e) => {
    if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('.bench-object')) { e.preventDefault(); select(e.target); }
  });
  $$('.bench-object', shelf).forEach((o) => { o.tabIndex = 0; o.setAttribute('role', 'button'); });

  function drawThing(kind, file, url) {
    const thing = h('span', 'bo-thing');
    if (kind === 'image' && url) {
      const img = h('img', 'bo-photo'); img.alt = ''; img.src = url; thing.append(img);
    } else if (kind === 'video') {
      thing.innerHTML = '<span class="bo-film" aria-hidden="true"><span class="bo-sprockets"></span><span class="bo-frame"></span><span class="bo-sprockets"></span></span>';
      if (url) {
        const v = h('video', 'bo-frame'); v.muted = true; v.playsInline = true; v.preload = 'metadata'; v.src = url + '#t=0.1';
        thing.querySelector('span.bo-frame').replaceWith(v);
      }
    } else if (kind === 'audio') {
      const w = h('span', 'bo-wave'); w.setAttribute('aria-hidden', 'true'); fillWave(w, file.name); thing.append(w);
    } else if (kind === 'pdf' || kind === 'sheet') {
      thing.innerHTML = $(`.bench-object[data-kind="${kind}"] .bo-thing`).innerHTML;
    } else if (kind === 'text' || kind === 'json' || kind === 'doc') {
      thing.innerHTML = '<span class="bo-sheet bo-text" aria-hidden="true"><span class="bo-code bo-w60"></span><span class="bo-code bo-w80 bo-in"></span><span class="bo-code bo-w70 bo-in"></span><span class="bo-code bo-w50 bo-in"></span><span class="bo-code bo-w30"></span></span>';
    } else {
      thing.innerHTML = '<span class="bo-card" aria-hidden="true"><span class="bo-ext"></span></span>';
      $('.bo-ext', thing).textContent = (ext(file.name) || 'file').toUpperCase().slice(0, 5);
    }
    return thing;
  }

  function putOnBench(fileList, { result = false, select: pick = true } = {}) {
    const added = [];
    for (const file of Array.from(fileList).slice(0, 6)) {
      const kind = kindOf(file.name, file.type);
      // Only previewable media gets an object URL; it never leaves this tab.
      const url = (kind === 'image' || kind === 'video') && file.size < 200e6 ? keepUrl(URL.createObjectURL(file)) : '';
      const fig = h('figure', 'bench-object is-new');
      fig.dataset.kind = kind; fig.dataset.name = file.name;
      fig.tabIndex = 0; fig.setAttribute('role', 'button');
      const cap = h('figcaption');
      cap.append(h('span', 'bo-name', file.name), h('span', 'bo-meta mono', (result ? 'Result · ' : 'Just now · ') + formatBytes(file.size)));
      fig.append(drawThing(kind, file, url), cap);
      files.set(fig, file);
      shelf.prepend(fig);
      added.push(fig);
    }
    // Keep the shelf to a tidy handful, but never drop the one being worked on.
    $$('.bench-object', shelf).filter((o) => o !== current).slice(9).forEach((o) => o.remove());
    if (added.length) {
      shelf.scrollTo({ left: 0, behavior: 'smooth' });
      if (pick) select(added[added.length - 1]);
    }
    return added;
  }

  $('#demo-file').addEventListener('change', (e) => { putOnBench(e.target.files); e.target.value = ''; });

  // Drop anywhere on the page, like the app.
  let depth = 0;
  const hasFiles = (e) => e.dataTransfer && Array.from(e.dataTransfer.types || []).includes('Files');
  window.addEventListener('dragenter', (e) => { if (!hasFiles(e)) return; depth++; demo.classList.add('is-over'); });
  window.addEventListener('dragleave', () => { depth = Math.max(0, depth - 1); if (!depth) demo.classList.remove('is-over'); });
  window.addEventListener('dragover', (e) => { if (hasFiles(e)) e.preventDefault(); });
  window.addEventListener('drop', (e) => {
    if (!hasFiles(e)) return;
    e.preventDefault(); depth = 0; demo.classList.remove('is-over');
    putOnBench(e.dataTransfer.files);
    if (Math.abs(demo.getBoundingClientRect().top) > window.innerHeight / 2) demo.scrollIntoView({ behavior: 'smooth', block: 'start' });
  });

  // Two of the sample files are real: the photo is drawn from its SVG, the CSV is generated.
  const SAMPLES = {
    'dune.jpg': async () => {
      const svg = $('.bench-object[data-name="dune.jpg"] svg');
      const src = new XMLSerializer().serializeToString(svg).replace('<svg', '<svg width="1800" height="1205"');
      const url = URL.createObjectURL(new Blob([src], { type: 'image/svg+xml' }));
      try {
        const img = new Image();
        await new Promise((res, rej) => { img.onload = res; img.onerror = () => rej(new Error('Could not draw the sample photo')); img.src = url; });
        const c = document.createElement('canvas');
        c.width = 1800; c.height = 1205;
        const g = c.getContext('2d');
        g.drawImage(img, 0, 0, c.width, c.height);
        // A little film grain so it compresses like a photo, not a flat drawing.
        const d = g.getImageData(0, 0, c.width, c.height);
        let s = 7;
        for (let i = 0; i < d.data.length; i += 4) { s = (s * 1103515245 + 12345) & 0x7fffffff; const n = (s % 17) - 8; d.data[i] += n; d.data[i + 1] += n; d.data[i + 2] += n; }
        g.putImageData(d, 0, 0);
        return asFile(await encode(c, 'image/jpeg', 0.95), 'dune.jpg');
      } finally { URL.revokeObjectURL(url); }
    },
    'orders.csv': async () => {
      const names = ['Aiko Tanaka', 'Ben Okafor', 'Clara Ruiz', 'Dev Patel', 'Emma Lind', 'Farid Haddad', 'Grace Kim', 'Hugo Martin'];
      const items = ['Notebook', 'Fountain pen', 'Ink, blue', 'Pencil case', 'Washi tape', 'Desk lamp'];
      const rows = ['order_id,date,customer,item,qty,total'];
      for (let i = 0; i < 1200; i++) {
        const q = 1 + (i * 7) % 4;
        rows.push(`${10400 + i},2026-${String(1 + (i % 10)).padStart(2, '0')}-${String(1 + (i * 3) % 28).padStart(2, '0')},${names[(i * 5) % names.length]},"${items[(i * 3) % items.length]}",${q},${(q * (4.5 + (i % 6) * 3.25)).toFixed(2)}`);
      }
      return new File([rows.join('\n') + '\n'], 'orders.csv', { type: 'text/csv' });
    },
  };
  Object.keys(SAMPLES).forEach((n) => { const f = $(`.bench-object[data-name="${n}"]`); if (f) f.dataset.sample = 'lazy'; });

  select($('.bench-object', shelf));

  // ------------------------------------------------------------- the cabinet
  const cabinet = $('#cabinet');
  const drawers = $$('.drawer', cabinet);
  cabinet.classList.add('is-live');
  // The open drawer's tray lies across the whole cabinet under its row; the
  // drawer reserves room for it so the rows below move down.
  function fitTray() {
    drawers.forEach((d) => { d.style.marginBottom = ''; });
    const open = drawers.find((d) => d.open);
    if (open) open.style.marginBottom = $('.drawer-tray', open).offsetHeight + 14 + 'px';
  }
  drawers.forEach((d) => d.addEventListener('toggle', () => {
    if (d.open) drawers.forEach((o) => { if (o !== d) o.open = false; });
    fitTray();
  }));
  let lastW = 0;
  new ResizeObserver(() => { if (cabinet.clientWidth !== lastW) { lastW = cabinet.clientWidth; fitTray(); } }).observe(cabinet);
  fitTray();

  const find = $('#tool-find');
  const findStatus = $('#find-status');
  $('.cabinet-find').hidden = false;
  find.addEventListener('input', () => {
    const q = find.value.trim().toLowerCase();
    let total = 0; let inDrawers = 0; let firstHit = null;
    drawers.forEach((d) => {
      let n = 0;
      $$('li', d).forEach((li) => { const hit = !q || li.textContent.toLowerCase().includes(q); li.hidden = !hit; li.classList.toggle('is-hit', !!q && hit); if (hit) n++; });
      const count = $('.drawer-count', d);
      count.firstElementChild.nextSibling.textContent = q ? `${n}` : `${$$('li', d).length}`;
      d.classList.toggle('is-empty', !!q && !n);
      if (q && n) { total += n; inDrawers++; firstHit = firstHit || d; }
    });
    if (q) {
      const open = drawers.find((d) => d.open);
      if (firstHit && (!open || open.classList.contains('is-empty'))) firstHit.open = true;
      findStatus.textContent = total ? `${total} ${total === 1 ? 'tool' : 'tools'} in ${inDrawers} ${inDrawers === 1 ? 'drawer' : 'drawers'}` : 'No tool by that name. The docs search descriptions too.';
    } else findStatus.textContent = '';
    fitTray();
  });

  // ------------------------------------------------------------- one tool, every way in
  // One description per tool; the four views below are generated from it, the
  // way the app, CLI, HTTP API and MCP server are generated from the tool spec.
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
  const kebab = (k) => k.replace(/_/g, '-');
  const fileIds = (p) => p.v.map((_, i) => `f_${i + 1}`);
  const args = (t) => Object.fromEntries(t.params.map((p) => [p.k, p.type === 'files' ? fileIds(p) : p.v]));
  // JSON with syntax spans; arrays stay on one line.
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
      return `<pre class="code code-sm"><code><span class="p">$</span> ${pipe}${esc(cliName(id))} ${esc(parts.join(' '))}</code></pre>`;
    },
    api(id, t) {
      const f = t.params.find((p) => p.type === 'files');
      const up = f ? f.v.map((n, i) => `<span class="p">PUT</span>  /api/v1/files/raw?name=${esc(n)}  <span class="c"># → f_${i + 1}</span>\n`).join('') : '';
      return `<pre class="code code-sm"><code>${up}<span class="p">POST</span> /api/v1/tools/${esc(id)}/run\n${jsonHtml({ params: args(t), wait_seconds: 60 })}</code></pre>`;
    },
    mcp(id, t) {
      return `<pre class="code code-sm"><code><span class="c"># tools/call</span> <span class="p">dogubako_run_tool</span>\n${jsonHtml({ tool: id, arguments: args(t) })}</code></pre>`;
    },
  };
  const waysTools = $$('#ways-pick button');
  function showWay(id) {
    const t = WAYS[id];
    waysTools.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.tool === id)));
    $('#way-id').textContent = id;
    for (const [k, render] of Object.entries(VIEWS)) {
      const el = $(`#way-${k} .way-body`);
      el.innerHTML = render(id, t);
      el.classList.remove('is-new'); void el.offsetWidth; el.classList.add('is-new');
    }
  }
  waysTools.forEach((b) => b.addEventListener('click', () => showWay(b.dataset.tool)));
  showWay('image.compress');
  window.__dgbWays = { WAYS, VIEWS }; // used by scripts that pre-render the no-JS markup

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
    tourImg.alt = tab.textContent + ' screen';
    $('#tour-caption').textContent = tab.dataset.caption;
    showShot(tab.dataset.shot);
  }));
  idle(() => { tourTabs.forEach((t) => { const i = new Image(); i.src = `screenshots/${t.dataset.shot}-light.webp`; }); });
  showShot('home');

  // Tabs with arrow-key support.
  function tabs(container) {
    const ts = $$('[role="tab"]', container);
    const pick = (tab) => ts.forEach((t) => { const on = t === tab; t.setAttribute('aria-selected', String(on)); t.tabIndex = on ? 0 : -1; });
    pick(ts.find((t) => t.getAttribute('aria-selected') === 'true') || ts[0]);
    ts.forEach((t, i) => {
      t.addEventListener('click', () => pick(t));
      t.addEventListener('keydown', (e) => {
        const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
        if (!d) return;
        e.preventDefault();
        const n = ts[(i + d + ts.length) % ts.length];
        n.focus(); n.click();
      });
    });
  }
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
      $('#hero-download').textContent = 'Alpha status for ' + $('.dl-os', card).textContent;
    }
  }
})();
