// Canvas painters for every surface in the scene. Everything is drawn here at load time,
// so the page needs no image files for the 3D bench: paper grain, the hinoki bench top,
// the dune print, PDF pages, film, the waveform card, the CSV sheet, the JSON card,
// drawer labels and the lid lining.

export const SANS = '"Instrument Sans Variable", "Instrument Sans", system-ui, sans-serif';
export const MONO = '"DM Mono", ui-monospace, Menlo, monospace';
const CJK = '"Hiragino Mincho ProN", "Yu Mincho", "Noto Serif CJK JP", "IPAMincho", "Hiragino Sans", "Noto Sans CJK JP", "IPAGothic", serif';

export const INK = '#1a1916';
export const PAPER = '#fdfcf9';
const LINE = '#dcd8cf';

export function canvas(w, h) {
  const c = document.createElement('canvas');
  c.width = w; c.height = h;
  return c;
}

/** Small deterministic PRNG, so every surface draws the same way on every visit. */
export function rng(seed) {
  let h = 2166136261;
  for (const ch of String(seed)) h = Math.imul(h ^ ch.charCodeAt(0), 16777619);
  return () => {
    h = Math.imul(h ^ (h >>> 15), 2246822507);
    h = Math.imul(h ^ (h >>> 13), 3266489909);
    h ^= h >>> 16;
    return (h >>> 0) / 4294967296;
  };
}

/** The app's deterministic waveform (web/src/components/BenchObject.tsx), so a file draws the same. */
export function bars(seed, n) {
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

function rr(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

/** Fine paper fibre noise, grey around 128. Used as a bump map. */
export function paperGrain(size = 256, seed = 'paper') {
  const c = canvas(size, size);
  const ctx = c.getContext('2d');
  const r = rng(seed);
  const img = ctx.createImageData(size, size);
  for (let i = 0; i < size * size; i++) {
    const v = 128 + (r() - 0.5) * 34;
    img.data[i * 4] = img.data[i * 4 + 1] = img.data[i * 4 + 2] = v;
    img.data[i * 4 + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
  ctx.globalAlpha = 0.09;
  ctx.lineWidth = 0.7;
  for (let i = 0; i < size * 1.4; i++) {
    const x = r() * size, y = r() * size, a = r() * Math.PI, l = 3 + r() * 12;
    ctx.strokeStyle = r() > 0.5 ? '#fff' : '#000';
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + Math.cos(a) * l, y + Math.sin(a) * l);
    ctx.stroke();
  }
  return c;
}

/** Pale hinoki bench top (or a dark oiled one), tileable, low contrast. */
export function benchWood(dark, size = 1024) {
  const c = canvas(size, size);
  const ctx = c.getContext('2d');
  const r = rng(dark ? 'walnut' : 'hinoki');
  ctx.fillStyle = dark ? '#262320' : '#f3f0ea';
  ctx.fillRect(0, 0, size, size);
  // broad colour drift
  for (let i = 0; i < 9; i++) {
    const x = r() * size, w = 60 + r() * 220;
    const g = ctx.createLinearGradient(x - w, 0, x + w, 0);
    const tone = dark ? `rgba(${r() > 0.5 ? '54,49,43' : '20,18,16'},` : `rgba(${r() > 0.5 ? '226,216,200' : '253,252,250'},`;
    g.addColorStop(0, tone + '0)');
    g.addColorStop(0.5, tone + (dark ? 0.35 : 0.42) + ')');
    g.addColorStop(1, tone + '0)');
    ctx.fillStyle = g;
    for (const dx of [-size, 0, size]) { ctx.save(); ctx.translate(dx, 0); ctx.fillRect(x - w, 0, w * 2, size); ctx.restore(); }
  }
  // grain lines, periodic in y so the texture tiles
  const lines = 150;
  for (let i = 0; i < lines; i++) {
    const x0 = r() * size;
    const amp = 2 + r() * 9, ph = r() * Math.PI * 2, k = (1 + Math.floor(r() * 3)) * (Math.PI * 2) / size;
    const a = dark ? 0.05 + r() * 0.1 : 0.04 + r() * 0.09;
    ctx.strokeStyle = dark ? `rgba(12,10,8,${a})` : `rgba(150,132,108,${a * 0.7})`;
    ctx.lineWidth = 0.6 + r() * 1.6;
    for (const dx of [-size, 0, size]) {
      ctx.beginPath();
      for (let y = 0; y <= size; y += 8) {
        const x = x0 + dx + Math.sin(y * k + ph) * amp + Math.sin(y * k * 3 + ph * 2) * amp * 0.2;
        if (y === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
  }
  // pores
  const img = ctx.getImageData(0, 0, size, size);
  const d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const n = (r() - 0.5) * (dark ? 7 : 6);
    d[i] += n; d[i + 1] += n; d[i + 2] += n;
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

/** The dune sunset from the site's SVG print, painted on canvas. */
export function dunePhoto(w = 1024) {
  const h = Math.round(w * 150 / 224);
  const c = canvas(w, h);
  const ctx = c.getContext('2d');
  ctx.scale(w / 224, h / 150);
  const sky = ctx.createLinearGradient(0, 0, 0, 150);
  sky.addColorStop(0, '#6b5079'); sky.addColorStop(0.55, '#b98a9c'); sky.addColorStop(0.75, '#e2b4a6'); sky.addColorStop(1, '#e2b4a6');
  ctx.fillStyle = sky; ctx.fillRect(0, 0, 224, 150);
  const glow = ctx.createRadialGradient(98, 86, 0, 98, 86, 34);
  glow.addColorStop(0, 'rgba(255,241,220,0.9)'); glow.addColorStop(1, 'rgba(255,227,204,0)');
  ctx.fillStyle = glow; ctx.beginPath(); ctx.arc(98, 86, 34, 0, Math.PI * 2); ctx.fill();
  ctx.fillStyle = '#fff4e4'; ctx.beginPath(); ctx.arc(98, 86, 8.5, 0, Math.PI * 2); ctx.fill();
  const dunes = [
    ['M0 98 C30 92 52 84 78 88 S120 80 148 86 S196 92 224 88 V150 H0Z', '#c4898a'],
    ['M0 112 C40 104 70 108 104 102 S170 98 224 108 V150 H0Z', '#9c6873'],
    ['M0 124 C50 118 92 126 140 120 S200 118 224 122 V150 H0Z', '#5e3e4f'],
    ['M0 138 C60 132 120 136 224 134 V150 H0Z', '#2f2028'],
  ];
  for (const [p, f] of dunes) { ctx.fillStyle = f; ctx.fill(new Path2D(p)); }
  return c;
}

/** A visitor's own photo, cover-fitted to the print. */
export function photoFrom(img, w = 1024) {
  const ar = (img.naturalWidth || img.width) / (img.naturalHeight || img.height) || 1.5;
  const h = Math.round(w / Math.min(2.2, Math.max(0.5, ar)));
  const c = canvas(w, h);
  c.getContext('2d').drawImage(img, 0, 0, w, h);
  return c;
}

/** One page of a PDF: an ink title, grey lines, a page number. */
export function pdfPage(seed, n = 1) {
  const W = 512, H = 724;
  const c = canvas(W, H);
  const ctx = c.getContext('2d');
  const r = rng(seed + n);
  ctx.fillStyle = PAPER; ctx.fillRect(0, 0, W, H);
  const m = 54;
  let y = 70;
  if (n === 1) {
    ctx.fillStyle = INK; ctx.fillRect(m, y, W * (0.45 + r() * 0.2), 16); y += 34;
    ctx.fillStyle = '#a9a59c'; ctx.fillRect(m, y, W * 0.28, 8); y += 46;
  }
  ctx.fillStyle = LINE;
  while (y < H - 110) {
    const para = 3 + Math.floor(r() * 5);
    for (let i = 0; i < para && y < H - 110; i++) {
      const last = i === para - 1;
      ctx.fillRect(m, y, (W - m * 2) * (last ? 0.35 + r() * 0.4 : 0.9 + r() * 0.1), 7);
      y += 21;
    }
    y += 18;
  }
  ctx.fillStyle = '#b9b5ac';
  ctx.font = `500 18px ${MONO}`;
  ctx.textAlign = 'center';
  ctx.fillText(String(n), W / 2, H - 44);
  return c;
}

/** A visitor's PDF: first page with its name set as the title. */
export function pdfNamed(name) {
  const c = pdfPage(name, 2);
  const ctx = c.getContext('2d');
  ctx.fillStyle = PAPER; ctx.fillRect(40, 40, 432, 90);
  ctx.fillStyle = INK; ctx.font = `600 30px ${SANS}`;
  ctx.fillText(fit(ctx, name, 400), 54, 96);
  return c;
}

function fit(ctx, text, max) {
  if (ctx.measureText(text).width <= max) return text;
  let t = text;
  while (t.length > 3 && ctx.measureText(t + '…').width > max) t = t.slice(0, -1);
  return t + '…';
}

/** Talk frames: a soft backdrop and a speaker silhouette, nudged per frame. */
function talkFrame(ctx, x, y, w, h, i) {
  const g = ctx.createLinearGradient(x, y, x + w * 0.6, y + h);
  g.addColorStop(0, '#4a5a78'); g.addColorStop(0.55, '#8a6f7d'); g.addColorStop(1, '#d8a98e');
  ctx.fillStyle = g; ctx.fillRect(x, y, w, h);
  const glow = ctx.createRadialGradient(x + w * 0.72, y + h * 0.3, 0, x + w * 0.72, y + h * 0.3, w * 0.5);
  glow.addColorStop(0, 'rgba(255,236,214,0.55)'); glow.addColorStop(1, 'rgba(255,236,214,0)');
  ctx.fillStyle = glow; ctx.fillRect(x, y, w, h);
  const cx = x + w * (0.42 + Math.sin(i * 1.7) * 0.05), base = y + h;
  ctx.fillStyle = 'rgba(28,24,30,0.82)';
  ctx.beginPath(); ctx.arc(cx, y + h * 0.42, h * 0.14, 0, Math.PI * 2); ctx.fill();
  ctx.beginPath();
  ctx.moveTo(cx - w * 0.26, base); ctx.quadraticCurveTo(cx - w * 0.22, y + h * 0.6, cx, y + h * 0.6);
  ctx.quadraticCurveTo(cx + w * 0.22, y + h * 0.6, cx + w * 0.26, base); ctx.fill();
  ctx.fillStyle = 'rgba(40,34,30,0.7)';
  ctx.fillRect(x + w * 0.78, y + h * 0.62, w * 0.14, h * 0.38); // lectern
}

/** Film strip with real sprocket holes (alpha), frames from a painter or a video still. */
export function filmStrip(frames = 7, still = null) {
  const fw = 176, gap = 14, H = 236;
  const W = frames * (fw + gap) + gap;
  const c = canvas(W, H);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#1c1b19'; ctx.fillRect(0, 0, W, H);
  // frames
  const fy = 44, fh = H - 88;
  for (let i = 0; i < frames; i++) {
    const x = gap + i * (fw + gap);
    if (still) {
      const ar = still.width / still.height, tw = fw, th = fh;
      let sw = still.width, sh = still.height, sx = 0, sy = 0;
      if (ar > tw / th) { sw = still.height * (tw / th); sx = (still.width - sw) / 2; } else { sh = still.width / (tw / th); sy = (still.height - sh) / 2; }
      ctx.drawImage(still, sx, sy, sw, sh, x, fy, fw, fh);
    } else talkFrame(ctx, x, fy, fw, fh, i);
    ctx.strokeStyle = 'rgba(0,0,0,0.5)'; ctx.lineWidth = 2; ctx.strokeRect(x + 1, fy + 1, fw - 2, fh - 2);
  }
  // sprocket holes: punch alpha
  ctx.globalCompositeOperation = 'destination-out';
  const hw = 18, hh = 14, step = 34;
  for (let x = 10; x < W - hw; x += step) {
    rr(ctx, x, 15, hw, hh, 3); ctx.fill();
    rr(ctx, x, H - 15 - hh, hw, hh, 3); ctx.fill();
  }
  ctx.globalCompositeOperation = 'source-over';
  return c;
}

/**
 * Waveform card. `p` (0..1) morphs the bars into transcript lines, the way
 * Transcribe Audio / Video turns a recording into subtitles.
 */
export function waveCard(target, seed, p = 0, lines = null, realBars = null) {
  const c = target || canvas(1024, 372);
  const ctx = c.getContext('2d');
  const W = c.width, H = c.height;
  ctx.globalAlpha = 1;
  ctx.fillStyle = PAPER; ctx.fillRect(0, 0, W, H);
  const b = realBars || bars(seed, 42);
  const n = b.length, pad = 56, bw = 7, step = (W - pad * 2) / n;
  for (let i = 0; i < n; i++) {
    const local = Math.min(1, Math.max(0, p * 1.8 - (i / n) * 0.8));
    const h = (b[i] / 44) * (H * 0.62) * (1 - local);
    if (h < 1) continue;
    ctx.globalAlpha = 1 - local * 0.6;
    ctx.fillStyle = '#2a2a28';
    rr(ctx, pad + i * step + (step - bw) / 2, H / 2 - h / 2, bw, h, 3); ctx.fill();
  }
  ctx.globalAlpha = 1;
  const t = lines || [
    ['1  00:00:01,200 --> 00:00:04,100', 0],
    ['So, where did it start?', 1],
    ['2  00:00:04,300 --> 00:00:07,900', 0],
    ['With a folder full of HEIC files.', 1],
  ];
  if (p > 0.25) {
    const q = (p - 0.25) / 0.75;
    let y = 96;
    t.forEach(([s, kind], i) => {
      const show = Math.min(1, Math.max(0, q * t.length * 1.2 - i));
      if (show <= 0) return;
      const chars = Math.round(s.length * show);
      ctx.fillStyle = kind ? INK : '#8c887f';
      ctx.font = kind ? `500 40px ${SANS}` : `400 26px ${MONO}`;
      ctx.fillText(s.slice(0, chars), pad, y);
      y += kind ? 76 : 46;
    });
  }
  return c;
}

/** CSV sheet with real-looking rows. `p` highlights then removes duplicate rows. */
export const SHEET_ROWS = [
  ['1041', '2026-09-28', 'Tote, linen', '2', '64.00'],
  ['1042', '2026-09-28', 'Notebook A5', '5', '42.50'],
  ['1043', '2026-09-29', 'Pen set', '1', '18.00'],
  ['1044', '2026-09-30', 'Tote, linen', '1', '32.00'],
  ['1044', '2026-09-30', 'Tote, linen', '1', '32.00'],
  ['1045', '2026-10-01', 'Desk lamp', '1', '89.00'],
  ['1046', '2026-10-01', 'Notebook A5', '3', '25.50'],
  ['1046', '2026-10-01', 'Notebook A5', '3', '25.50'],
  ['1047', '2026-10-02', 'Ink, black', '6', '27.00'],
  ['1048', '2026-10-03', 'Pen set', '2', '36.00'],
  ['1049', '2026-10-03', 'Paper, A4', '4', '22.00'],
];
const DUPES = new Set([4, 7]);
export function sheet(target, p = 0, rows = SHEET_ROWS, head = ['id', 'date', 'item', 'qty', 'total'], dupes = DUPES) {
  const c = target || canvas(1024, 760);
  const ctx = c.getContext('2d');
  const W = c.width, H = c.height;
  ctx.fillStyle = PAPER; ctx.fillRect(0, 0, W, H);
  const cols = head.length;
  const colW = [0.13, 0.25, 0.3, 0.1, 0.22].slice(0, cols);
  if (cols !== 5) colW.fill(1 / cols);
  const hh = 64, rh = 58;
  ctx.fillStyle = '#f0ede6'; ctx.fillRect(0, 0, W, hh);
  ctx.font = `500 26px ${MONO}`;
  ctx.fillStyle = '#6a6862';
  let x = 0;
  head.forEach((h, i) => { ctx.fillText(fitText(ctx, String(h), colW[i] * W - 30), x + 18, 42); x += colW[i] * W; });
  // rows, collapsing the duplicates as p goes 0.45 -> 1
  const hl = Math.min(1, p / 0.35), collapse = Math.min(1, Math.max(0, (p - 0.45) / 0.45));
  let y = hh;
  rows.forEach((row, ri) => {
    const isDup = dupes.has(ri);
    const h = isDup ? rh * (1 - collapse) : rh;
    if (h < 0.5) return;
    ctx.save();
    ctx.beginPath(); ctx.rect(0, y, W, h); ctx.clip();
    if (isDup && hl > 0) { ctx.fillStyle = `rgba(246,227,166,${0.9 * hl})`; ctx.fillRect(0, y, W, h); }
    ctx.globalAlpha = isDup ? 1 - collapse : 1;
    ctx.fillStyle = '#2a2925';
    ctx.font = `500 27px ${MONO}`;
    let cx = 0;
    row.forEach((v, i) => { ctx.fillText(fitText(ctx, String(v), colW[i] * W - 30), cx + 18, y + 38); cx += colW[i] * W; });
    ctx.restore();
    y += h;
    ctx.fillStyle = '#e6e3dc'; ctx.fillRect(0, y - 1, W, 2);
  });
  x = 0;
  ctx.fillStyle = '#e6e3dc';
  for (let i = 0; i < cols - 1; i++) { x += colW[i] * W; ctx.fillRect(x - 1, 0, 2, H); }
  return c;
}
function fitText(ctx, t, max) { return fit(ctx, t, Math.max(20, max)); }

/** JSON on an index card. `p` goes from minified to formatted with syntax colours. */
const JSON_MIN = '{"name":"dogubako","tools":249,"categories":12,"local":true,"telemetry":false,"license":"AGPL-3.0-or-later"}';
export function jsonCard(target, p = 0, text = JSON_MIN) {
  const c = target || canvas(760, 1000);
  const ctx = c.getContext('2d');
  const W = c.width, H = c.height;
  ctx.fillStyle = PAPER; ctx.fillRect(0, 0, W, H);
  ctx.fillStyle = '#ece9e2';
  for (let y = 150; y < H - 40; y += 54) ctx.fillRect(48, y, W - 96, 2);
  ctx.fillStyle = '#9c988f'; ctx.font = `500 26px ${MONO}`;
  ctx.fillText('config.json', 48, 84);
  // minified
  const a = 1 - Math.min(1, p * 2.2);
  if (a > 0) {
    ctx.globalAlpha = a; ctx.fillStyle = '#2a2925'; ctx.font = `500 32px ${MONO}`;
    const per = Math.floor((W - 96) / 19.3);
    for (let i = 0, y = 140; i * per < text.length; i++, y += 54) ctx.fillText(text.slice(i * per, (i + 1) * per), 48, y);
    ctx.globalAlpha = 1;
  }
  if (p > 0.3) {
    const q = (p - 0.3) / 0.7;
    const lines = [
      [['{', 'p']],
      [['  "name"', 'k'], [': ', 'p'], ['"dogubako"', 's'], [',', 'p']],
      [['  "tools"', 'k'], [': ', 'p'], ['249', 'n'], [',', 'p']],
      [['  "categories"', 'k'], [': ', 'p'], ['12', 'n'], [',', 'p']],
      [['  "local"', 'k'], [': ', 'p'], ['true', 'b'], [',', 'p']],
      [['  "telemetry"', 'k'], [': ', 'p'], ['false', 'b'], [',', 'p']],
      [['  "license"', 'k'], [': ', 'p'], ['"AGPL-3.0-', 's']],
      [['     or-later"', 's']],
      [['}', 'p']],
    ];
    const col = { p: '#6a6862', k: INK, s: '#7a5a1e', n: '#2f5f8a', b: '#9a3b2b' };
    ctx.font = `500 32px ${MONO}`;
    lines.forEach((ln, i) => {
      const show = Math.min(1, Math.max(0, q * lines.length * 1.1 - i));
      if (show <= 0) return;
      ctx.globalAlpha = show;
      let x = 48 + (1 - show) * 24;
      for (const [s, k] of ln) { ctx.fillStyle = col[k]; ctx.fillText(s, x, 140 + i * 54); x += ctx.measureText(s).width; }
    });
    ctx.globalAlpha = 1;
  }
  return c;
}

/** A visitor's text file: its first lines in mono, on a card. */
export function textCard(name, text) {
  const c = canvas(760, 1000);
  const ctx = c.getContext('2d');
  ctx.fillStyle = PAPER; ctx.fillRect(0, 0, 760, 1000);
  ctx.fillStyle = '#9c988f'; ctx.font = `500 26px ${MONO}`;
  ctx.fillText(fit(ctx, name, 660), 48, 84);
  ctx.fillStyle = '#3c3a36'; ctx.font = `400 26px ${MONO}`;
  const lines = String(text).replace(/\t/g, '  ').split(/\r?\n/).slice(0, 16);
  lines.forEach((l, i) => ctx.fillText(fit(ctx, l, 664), 48, 150 + i * 50));
  return c;
}

/** Any other file: a plain card with its extension. */
export function extCard(name) {
  const c = canvas(512, 640);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#ffffff'; ctx.fillRect(0, 0, 512, 640);
  const ext = name.includes('.') ? name.split('.').pop().toUpperCase().slice(0, 5) : 'FILE';
  ctx.fillStyle = '#6a6862'; ctx.font = `500 54px ${MONO}`;
  ctx.fillText(ext, 44, 580);
  ctx.fillStyle = '#9c988f'; ctx.font = `400 26px ${MONO}`;
  ctx.fillText(fit(ctx, name, 420), 44, 84);
  return c;
}

/** Paper plate on a drawer front: category name and its real tool count. */
export function drawerLabel(name, count) {
  const W = 512, H = 160;
  const c = canvas(W, H);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#f4f0e7'; ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = 'rgba(60,45,30,0.18)'; ctx.lineWidth = 4; ctx.strokeRect(10, 10, W - 20, H - 20);
  ctx.fillStyle = INK;
  let size = 52;
  ctx.font = `600 ${size}px ${SANS}`;
  while (ctx.measureText(name).width > W - 150 && size > 30) { size -= 2; ctx.font = `600 ${size}px ${SANS}`; }
  ctx.textBaseline = 'middle';
  ctx.fillText(name, 34, H / 2 + 2);
  ctx.font = `500 44px ${MONO}`; ctx.fillStyle = '#6a6862'; ctx.textAlign = 'right';
  ctx.fillText(String(count), W - 32, H / 2 + 2);
  return c;
}

/** Inside of the lid: the name, printed on the lining. Top of the texture is the lid's front edge. */
export function lidLining() {
  const W = 1024, H = 600;
  const c = canvas(W, H);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#efe9de'; ctx.fillRect(0, 0, W, H);
  const r = rng('lid');
  for (let i = 0; i < 2600; i++) { ctx.fillStyle = `rgba(120,100,70,${r() * 0.05})`; ctx.fillRect(r() * W, r() * H, 1 + r() * 2, 1); }
  ctx.fillStyle = '#1c1b18';
  ctx.textAlign = 'center';
  ctx.font = `400 150px ${CJK}`;
  ctx.fillText('道具箱', W / 2, 300);
  ctx.font = `500 34px ${MONO}`; ctx.fillStyle = '#5a5751';
  ctx.fillText('dōgu-bako · toolbox', W / 2, 392);
  ctx.font = `400 26px ${MONO}`; ctx.fillStyle = '#8a867d';
  ctx.fillText('249 tools · 12 drawers', W / 2, 446);
  return c;
}

/** Floor of the top tray, seen from above when the lid is open. */
export function trayLining() {
  const W = 1024, H = 600;
  const c = canvas(W, H);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#ebe4d7'; ctx.fillRect(0, 0, W, H);
  const r = rng('tray');
  for (let i = 0; i < 3000; i++) { ctx.fillStyle = `rgba(110,90,60,${r() * 0.05})`; ctx.fillRect(r() * W, r() * H, 1 + r() * 2, 1); }
  // soft inner shadow so it reads as recessed
  const shade = (x0, y0, x1, y1) => { const g = ctx.createLinearGradient(x0, y0, x1, y1); g.addColorStop(0, 'rgba(60,45,30,0.28)'); g.addColorStop(1, 'rgba(60,45,30,0)'); return g; };
  ctx.fillStyle = shade(0, 0, 0, 90); ctx.fillRect(0, 0, W, 90);
  ctx.fillStyle = shade(0, 0, 70, 0); ctx.fillRect(0, 0, 70, H);
  ctx.fillStyle = shade(W, 0, W - 70, 0); ctx.fillRect(W - 70, 0, 70, H);
  ctx.fillStyle = shade(0, H, 0, H - 50); ctx.fillRect(0, H - 50, W, 50);
  ctx.fillStyle = 'rgba(80,70,55,0.55)'; ctx.font = `400 22px ${MONO}`; ctx.textAlign = 'right';
  ctx.fillText('AGPL-3.0-or-later · no telemetry', W - 64, H - 54);
  return c;
}

/** Floor of a drawer: its tools as little index cards. Top of the canvas is the back of the drawer. */
export function drawerFloor(tools) {
  const W = 512, H = 1024;
  const c = canvas(W, H);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#ebe4d7'; ctx.fillRect(0, 0, W, H);
  const top = H * 0.36, bottom = H - 26;
  const colsN = tools.length > 14 ? 2 : 1;
  const rows = Math.ceil(tools.length / colsN);
  const rh = (bottom - top) / rows;
  const cw = (W - 36) / colsN;
  const fs = Math.max(12, Math.min(26, rh * 0.5));
  ctx.textBaseline = 'middle';
  tools.forEach((t, i) => {
    const col = Math.floor(i / rows), row = i % rows;
    const x = 18 + col * cw, y = top + row * rh;
    ctx.fillStyle = '#fbf9f4';
    rr(ctx, x + 3, y + 2, cw - 6, rh - 4, 4); ctx.fill();
    ctx.fillStyle = 'rgba(60,45,30,0.12)'; ctx.fillRect(x + 3, y + rh - 3, cw - 6, 1.5);
    ctx.fillStyle = '#2a2925'; ctx.font = `500 ${fs}px ${SANS}`;
    ctx.fillText(fit(ctx, t, cw - 26), x + 13, y + rh / 2);
  });
  const g = ctx.createLinearGradient(0, 0, 0, top);
  g.addColorStop(0, 'rgba(40,30,20,0.35)'); g.addColorStop(1, 'rgba(40,30,20,0)');
  ctx.fillStyle = g; ctx.fillRect(0, 0, W, top);
  return c;
}

/** Soft rounded-rectangle shadow, alpha in the red channel's luminance (white = shadow). */
export function blob(size = 128, inset = 0.24, soft = 0.22) {
  const c = canvas(size, size);
  const ctx = c.getContext('2d');
  const img = ctx.createImageData(size, size);
  const half = 0.5 - inset, r = 0.08;
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const px = Math.abs((x + 0.5) / size - 0.5), py = Math.abs((y + 0.5) / size - 0.5);
      const qx = Math.max(px - (half - r), 0), qy = Math.max(py - (half - r), 0);
      const d = Math.hypot(qx, qy) - r + Math.min(Math.max(px - (half - r), py - (half - r)), 0);
      const a = 1 - Math.min(1, Math.max(0, (d + soft * 0.15) / soft));
      const v = Math.round(255 * a * a * (3 - 2 * a));
      const k = (y * size + x) * 4;
      img.data[k] = img.data[k + 1] = img.data[k + 2] = v;
      img.data[k + 3] = 255;
    }
  }
  ctx.putImageData(img, 0, 0);
  return c;
}
