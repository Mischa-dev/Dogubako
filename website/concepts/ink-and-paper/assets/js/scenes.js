/* Scenes for the scroll (emaki). Each scene draws a file in ink line-art,
   then the tool acts on it. update(p) takes 0..1: roughly 0-0.45 draws the
   file, 0.5-0.92 is the tool at work. Everything is scrubbed by scroll, so it
   runs backwards too. */
(function () {
  'use strict';
  const NS = 'http://www.w3.org/2000/svg';
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const seg = (p, a, b) => clamp((p - a) / (b - a));
  const io = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
  const out = (t) => 1 - Math.pow(1 - t, 3);
  const lerp = (a, b, t) => a + (b - a) * t;

  function el(tag, attrs, parent, text) {
    const e = document.createElementNS(NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (text != null) e.textContent = text;
    if (parent) parent.appendChild(e);
    return e;
  }
  // a line that draws itself: pathLength=1 so dashoffset 1 → 0 draws it
  function ln(parent, d, cls) {
    return el('path', { d, class: 'ln drawn ' + (cls || ''), pathLength: 1, 'stroke-dashoffset': 1 }, parent);
  }
  function setDraw(e, t) { // t: 0 hidden → 1 drawn; negative lifts the ink from the start
    const v = t >= 0 ? 1 - t : t;
    const s = v.toFixed(4);
    if (e._d !== s) { e._d = s; e.style.strokeDashoffset = s; }
  }
  function setOp(e, o) { const s = o.toFixed(3); if (e._o !== s) { e._o = s; e.style.opacity = s; } }
  function setTf(e, tf) { if (e._t !== tf) { e._t = tf; e.setAttribute('transform', tf); } }
  function setText(e, s) { if (e._s !== s) { e._s = s; e.textContent = s; } }
  // stagger a list of drawn lines across [a, b]
  function drawSeq(list, p, a, b, overlap = 0.55) {
    const n = list.length, span = (b - a) / (n - (n - 1) * overlap);
    list.forEach((e, i) => { const s = a + i * span * (1 - overlap); setDraw(e, out(seg(p, s, s + span))); });
  }
  function hash(i) { const x = Math.sin(i * 91.7 + 13.1) * 43758.5453; return x - Math.floor(x); }

  /* ================================================================ photo */
  function buildPhotoArt(g) {
    const lines = [];
    const card = el('rect', { x: 150, y: 64, width: 330, height: 252, class: 'sheetfill' }, g);
    lines.push(ln(g, 'M150 64H480V316H150Z'));
    lines.push(ln(g, 'M166 80H464V280H166Z', 't'));
    const clipId = 'pc' + Math.random().toString(36).slice(2, 7);
    const cp = el('clipPath', { id: clipId }, g);
    el('rect', { x: 166, y: 80, width: 298, height: 200 }, cp);
    const pic = el('g', { 'clip-path': 'url(#' + clipId + ')' }, g);
    lines.push(ln(pic, 'M404 152a24 24 0 1 0 0.1 0', 't'));
    lines.push(ln(pic, 'M330 118q6-6 12 0q6-6 12 0M356 106q4-4 8 0q4-4 8 0', 't'));
    lines.push(ln(pic, 'M160 214C196 190 226 178 262 196S318 170 352 186S420 202 470 176', 'f'));
    lines.push(ln(pic, 'M160 240C214 212 262 204 300 226S384 252 470 212'));
    const hatch = [];
    for (let i = 0; i < 9; i++) hatch.push('M' + (304 + i * 13) + ' ' + (232 + (i % 3)) + 'l10 15');
    lines.push(ln(pic, hatch.join(''), 'h'));
    lines.push(ln(pic, 'M160 270C236 248 300 258 352 270S432 262 470 252', 't'));
    lines.push(ln(pic, 'M190 262q8-14 4-30M198 264q10-12 12-26M206 266q2-10 10-18', 't'));
    return { card, lines };
  }
  const photo = {
    build(svg) {
      const s = {};
      const wid = 'pw' + Math.random().toString(36).slice(2, 7);
      const rg = el('radialGradient', { id: wid }, el('defs', {}, svg));
      el('stop', { offset: 0, 'stop-color': 'currentColor', 'stop-opacity': 1 }, rg);
      el('stop', { offset: 0.55, 'stop-color': 'currentColor', 'stop-opacity': 0.45 }, rg);
      el('stop', { offset: 1, 'stop-color': 'currentColor', 'stop-opacity': 0 }, rg);
      s.wash = el('ellipse', { cx: 315, cy: 190, rx: 250, ry: 170, fill: 'url(#' + wid + ')', opacity: 0 }, svg);
      s.wash.style.color = 'var(--brush)';
      s.ghost = el('rect', { x: 150, y: 64, width: 330, height: 252, class: 'ghost', opacity: 0, transform: 'rotate(-3 150 316)' }, svg);
      s.A = el('g', {}, svg);
      s.a = buildPhotoArt(s.A);
      s.B = el('g', {}, svg);
      s.b = buildPhotoArt(s.B);
      s.name = el('text', { x: 150, y: 356, class: 'lbl b' }, svg, 'shot.png');
      s.meta = el('text', { x: 150, y: 378, class: 'lbl s' }, svg, '2400 × 1600 · 4.2 MB');
      s.pct = el('text', { x: 480, y: 356, class: 'lbl b', 'text-anchor': 'end', opacity: 0 }, svg, '−74%');
      s.dim = el('path', { d: 'M150 404H480M150 398v12M480 398v12', class: 'ln h', opacity: 0 }, svg);
      s.dimT = el('text', { x: 315, y: 428, class: 'lbl s', 'text-anchor': 'middle', opacity: 0 }, svg, 'same 2400 px across');
      return s;
    },
    update(s, p) {
      // draw the print
      drawSeq(s.a.lines, p, 0.02, 0.42, 0.45);
      setOp(s.a.card, seg(p, 0.02, 0.14));
      setOp(s.name, seg(p, 0.28, 0.38));
      setOp(s.meta, seg(p, 0.32, 0.42));
      // the tool acts: the ink lifts off, the print is re-inked smaller
      const lift = seg(p, 0.5, 0.66);
      const sc = lerp(1, 0.6, io(seg(p, 0.52, 0.74)));
      setTf(s.A, 'rotate(-3 150 316)');
      s.a.lines.forEach((e, i) => { if (lift > 0) setDraw(e, -io(clamp(lift * 1.6 - i * 0.07))); });
      setOp(s.A, 1 - seg(p, 0.6, 0.7));
      setTf(s.B, 'rotate(-3 150 316) translate(150 316) scale(' + sc.toFixed(4) + ') translate(-150 -316)');
      setOp(s.B, p > 0.54 ? 1 : 0);
      drawSeq(s.b.lines, p, 0.58, 0.88, 0.5);
      setOp(s.b.card, seg(p, 0.56, 0.64));
      const w = seg(p, 0.48, 0.62), wf = seg(p, 0.62, 0.84);
      setOp(s.wash, 0.11 * out(w) * (1 - wf));
      setTf(s.wash, 'translate(315 190) scale(' + (0.6 + 0.5 * out(w)).toFixed(3) + ') translate(-315 -190)');
      setOp(s.ghost, seg(p, 0.62, 0.8));
      const mb = lerp(4.2, 1.1, io(seg(p, 0.56, 0.86)));
      setText(s.meta, '2400 × 1600 · ' + mb.toFixed(1) + ' MB');
      setText(s.name, p > 0.7 ? 'shot-small.png' : 'shot.png');
      setOp(s.pct, seg(p, 0.84, 0.92));
      setOp(s.dim, seg(p, 0.86, 0.94));
      setOp(s.dimT, seg(p, 0.88, 0.96));
    },
  };

  /* ================================================================ pdf */
  function buildStack(g, sheets, seed) {
    const parts = { sheets: [], lines: [] };
    for (let i = sheets - 1; i >= 0; i--) {
      const o = i * 6;
      const r = el('rect', { x: o, y: o, width: 120, height: 156, class: 'sheetfill' }, g);
      parts.sheets.push(r);
      parts.lines.push(ln(g, 'M' + o + ' ' + o + 'h120v156h-120z', i ? 't' : ''));
    }
    let t = 'M14 22h' + (46 + (seed % 3) * 8);
    for (let k = 0; k < 9; k++) t += 'M14 ' + (42 + k * 11) + 'h' + Math.round(70 + hash(seed * 10 + k) * 22);
    parts.lines.push(ln(g, t, 'h'));
    return parts;
  }
  const pdf = {
    build(svg) {
      const s = { stacks: [] };
      const defs = [['a.pdf', 2, 70, 92, -5], ['b.pdf', 3, 258, 80, 2], ['c.pdf', 1, 446, 98, -2]];
      const pages = { 'a.pdf': 2, 'b.pdf': 3, 'c.pdf': 1 };
      s.merged = el('g', { opacity: 0 }, svg);
      s.mParts = buildStack(s.merged, 5, 7);
      s.bind = ln(svg, 'M246 74V246', '');
      s.bind.style.strokeWidth = '3.4';
      defs.forEach(([name, n, x, y, rot], i) => {
        const g = el('g', {}, svg);
        const inner = el('g', {}, g);
        const parts = buildStack(inner, n, i + 2);
        const lab = el('text', { x: x, y: y + 156 + n * 6 + 30, class: 'lbl b' }, svg, name);
        const sub = el('text', { x: x, y: y + 156 + n * 6 + 50, class: 'lbl s' }, svg, pages[name] + (pages[name] > 1 ? ' pages' : ' page'));
        s.stacks.push({ g, inner, parts, x, y, rot, n, lab, sub });
      });
      s.mLab = el('text', { x: 246, y: 300, class: 'lbl b', opacity: 0 }, svg, 'merged.pdf');
      s.mSub = el('text', { x: 246, y: 320, class: 'lbl s', opacity: 0 }, svg, '6 pages · a, b, c in order');
      return s;
    },
    update(s, p) {
      const mx = 246, my = 74; // where the merged stack sits
      s.stacks.forEach((st, i) => {
        const a = 0.02 + i * 0.1;
        drawSeq(st.parts.lines, p, a, a + 0.24, 0.4);
        st.parts.sheets.forEach((r) => setOp(r, seg(p, a, a + 0.08)));
        const lab = seg(p, 0.3 + i * 0.03, 0.4 + i * 0.03) * (1 - seg(p, 0.5, 0.58));
        setOp(st.lab, lab); setOp(st.sub, lab);
        // slide to the middle, then fold shut like a page turning
        const m = io(seg(p, 0.5 + i * 0.02, 0.66 + i * 0.02));
        const fold = io(seg(p, 0.64 + i * 0.015, 0.73 + i * 0.015));
        const x = lerp(st.x, mx + (2 - i) * 2, m), y = lerp(st.y, my + i * 2, m);
        const cx = 60 + st.n * 3;
        setTf(st.g, 'translate(' + x.toFixed(2) + ' ' + y.toFixed(2) + ') rotate(' + (st.rot * (1 - m)).toFixed(2) + ' 60 78)');
        setTf(st.inner, 'translate(' + cx + ' 0) scale(' + Math.max(0.02, 1 - fold).toFixed(3) + ' 1) translate(' + -cx + ' 0)');
        setOp(st.g, fold > 0.98 ? 0 : 1);
      });
      const un = io(seg(p, 0.72, 0.84));
      setOp(s.merged, p > 0.72 ? 1 : 0);
      setTf(s.merged, 'translate(' + mx + ' ' + my + ') translate(72 0) scale(' + Math.max(0.02, un).toFixed(3) + ' 1) translate(-72 0)');
      s.mParts.lines.forEach((e) => setDraw(e, p > 0.72 ? 1 : 0));
      s.mParts.sheets.forEach((r) => setOp(r, p > 0.72 ? 1 : 0));
      setDraw(s.bind, out(seg(p, 0.8, 0.9)));
      setOp(s.mLab, seg(p, 0.84, 0.92));
      setOp(s.mSub, seg(p, 0.86, 0.94));
    },
  };

  /* ================================================================ film */
  function frameArt(g, i, x, y, detail) {
    const L = [];
    // projection screen behind the speaker
    L.push(ln(g, 'M' + (x + 52) + ' ' + (y + 14) + 'h56v36h-56z', 't'));
    L.push(ln(g, 'M' + (x + 60) + ' ' + (y + 24) + 'h' + (26 + i * 4) + 'M' + (x + 60) + ' ' + (y + 32) + 'h' + (34 - i * 3) + 'M' + (x + 60) + ' ' + (y + 40) + 'h18', 'h'));
    // speaker: head, shoulders, an arm that moves frame to frame
    const hx = x + 34 + i * 1.5, hy = y + 34;
    L.push(ln(g, 'M' + hx + ' ' + (hy - 7) + 'a7 7 0 1 0 0.1 0', 't'));
    L.push(ln(g, 'M' + (hx - 14) + ' ' + (y + 82) + 'q2-26 14-30q12 4 14 30', 't'));
    const arm = [[16, -10], [20, -20], [12, -24], [18, -4]][i];
    L.push(ln(g, 'M' + (hx + 8) + ' ' + (y + 54) + 'l' + arm[0] + ' ' + arm[1], 't'));
    // lectern
    L.push(ln(g, 'M' + (x + 8) + ' ' + (y + 60) + 'h34l-4 28h-26z', 't'));
    if (detail) {
      let h = '';
      for (let k = 0; k < 6; k++) h += 'M' + (x + 12 + k * 5) + ' ' + (y + 64) + 'l-3 22';
      for (let k = 0; k < 5; k++) h += 'M' + (x + 56 + k * 10) + ' ' + (y + 54) + 'l8 30';
      L.push(ln(g, h, 'h'));
    }
    return L;
  }
  const film = {
    build(svg) {
      const s = {};
      const X = 40, Y = 104, W = 560, H = 168;
      const cp = 'fc' + Math.random().toString(36).slice(2, 7);
      const defs = el('defs', {}, svg);
      const c1 = el('clipPath', { id: cp + 'a' }, defs);
      s.clipA = el('rect', { x: 0, y: 0, width: 640, height: 460 }, c1);
      const grad = el('linearGradient', { id: cp + 'g', x1: 0, x2: 1, y1: 0, y2: 0 }, defs);
      el('stop', { offset: 0, 'stop-color': 'currentColor', 'stop-opacity': 0 }, grad);
      el('stop', { offset: 0.85, 'stop-color': 'currentColor', 'stop-opacity': 0.14 }, grad);
      el('stop', { offset: 1, 'stop-color': 'currentColor', 'stop-opacity': 0 }, grad);
      el('rect', { x: X, y: Y, width: W, height: H, class: 'sheetfill' }, svg);
      s.strip = [ln(svg, 'M' + X + ' ' + Y + 'h' + W + 'v' + H + 'h-' + W + 'z'), ln(svg, 'M' + X + ' ' + (Y + 28) + 'h' + W + 'M' + X + ' ' + (Y + H - 28) + 'h' + W, 't')];
      let holes = '';
      for (let k = 0; k < 24; k++) {
        const hx = X + 10 + k * 23;
        holes += 'M' + hx + ' ' + (Y + 9) + 'h10v10h-10z' + 'M' + hx + ' ' + (Y + H - 19) + 'h10v10h-10z';
      }
      s.holes = ln(svg, holes, 'h');
      s.frames = []; s.detail = [];
      const det = el('g', { 'clip-path': 'url(#' + cp + 'a)' }, svg);
      for (let i = 0; i < 4; i++) {
        const fx = X + 14 + i * 136, fy = Y + 36;
        s.frames.push(ln(svg, 'M' + fx + ' ' + fy + 'h124v96h-124z', 't'));
        s.frames.push(...frameArt(svg, i, fx, fy, false));
        s.detail.push(...frameArt(det, i, fx, fy, true).slice(-1));
      }
      s.wipe = el('rect', { x: 0, y: Y - 10, width: 70, height: H + 20, fill: 'url(#' + cp + 'g)', opacity: 0 }, svg);
      s.wipe.style.color = 'var(--brush)';
      s.wipeLine = el('path', { d: 'M0 ' + (Y - 14) + 'v' + (H + 28), class: 'ln t', opacity: 0 }, svg);
      s.name = el('text', { x: X, y: 318, class: 'lbl b' }, svg, 'talk.mov');
      s.meta = el('text', { x: X + W, y: 318, class: 'lbl s', 'text-anchor': 'end' }, svg, '418 MB');
      s.track = el('path', { d: 'M' + X + ' 340H' + (X + W), class: 'ln h' }, svg);
      s.bar = el('rect', { x: X, y: 337, width: W, height: 6, rx: 3, class: 'tick' }, svg);
      s.X = X; s.W = W;
      s.done = el('text', { x: X, y: 372, class: 'lbl s', opacity: 0 }, svg, 'same length, same frames, a sixth of the weight');
      return s;
    },
    update(s, p) {
      drawSeq(s.strip, p, 0.02, 0.14, 0.3);
      setDraw(s.holes, out(seg(p, 0.08, 0.2)));
      drawSeq(s.frames, p, 0.1, 0.38, 0.75);
      drawSeq(s.detail, p, 0.26, 0.42, 0.5);
      setOp(s.name, seg(p, 0.3, 0.4)); setOp(s.meta, seg(p, 0.32, 0.42));
      setOp(s.track, seg(p, 0.3, 0.42)); setOp(s.bar, seg(p, 0.34, 0.44));
      // the encoder's pass: a wash moves frame by frame, leaving lighter ink
      const w = seg(p, 0.5, 0.88);
      const wx = s.X - 20 + w * (s.W + 40);
      s.clipA.setAttribute('x', wx.toFixed(1));
      const on = p > 0.5 && p < 0.92 ? 1 : 0;
      setOp(s.wipe, on * (1 - seg(p, 0.86, 0.92)));
      setTf(s.wipe, 'translate(' + (wx - 70).toFixed(1) + ' 0)');
      setOp(s.wipeLine, on * 0.6 * (1 - seg(p, 0.86, 0.92)));
      setTf(s.wipeLine, 'translate(' + wx.toFixed(1) + ' 0)');
      const mb = lerp(418, 64, w);
      setText(s.meta, Math.round(mb) + ' MB');
      s.bar.setAttribute('width', (s.W * mb / 418).toFixed(1));
      setOp(s.done, seg(p, 0.88, 0.96));
    },
  };

  /* ================================================================ speech */
  function bars(seed, n) {
    let h = 2166136261;
    for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
    const o = [];
    for (let i = 0; i < n; i++) {
      h = Math.imul(h ^ (h >>> 13), 1274126177);
      const r = ((h >>> 0) % 1000) / 1000;
      o.push(Math.round(8 + r * 36 * (0.45 + 0.55 * Math.sin(i / 4) ** 2)));
    }
    return o;
  }
  const SRT = [
    ['1', 'num'], ['00:00:00,000 --> 00:00:03,180', 'time'], ['Thanks for coming in today.', 'txt'], ['', ''],
    ['2', 'num'], ['00:00:03,180 --> 00:00:06,420', 'time'], ['So, where should we start?', 'txt'],
  ];
  const speech = {
    build(svg) {
      const s = {};
      el('rect', { x: 70, y: 40, width: 500, height: 130, rx: 14, class: 'sheetfill' }, svg);
      s.card = ln(svg, 'M84 40H556Q570 40 570 54V156Q570 170 556 170H84Q70 170 70 156V54Q70 40 84 40Z');
      const hs = bars('interview.m4a', 56);
      s.bars = hs.map((h, i) => {
        const x = 96 + i * 8.2;
        const e = el('path', { d: 'M' + x.toFixed(1) + ' ' + (105 - h / 2) + 'V' + (105 + h / 2), class: 'ln drawn', pathLength: 1, 'stroke-dashoffset': 1 }, svg);
        e.style.strokeWidth = '3.2';
        return e;
      });
      s.drops = hs.map((h, i) => {
        const x = 96 + i * 8.2;
        const e = el('path', { d: 'M' + x.toFixed(1) + ' 101V109', class: 'ln', opacity: 0 }, svg);
        e.style.strokeWidth = '2.6';
        return e;
      });
      s.cursor = el('path', { d: 'M0 48V162', class: 'ln t', opacity: 0 }, svg);
      s.name = el('text', { x: 70, y: 198, class: 'lbl b' }, svg, 'interview.m4a');
      s.meta = el('text', { x: 570, y: 198, class: 'lbl s', 'text-anchor': 'end' }, svg, '34 MB');
      // the subtitle sheet
      s.sheet = el('rect', { x: 70, y: 226, width: 500, height: 206, class: 'sheetfill', opacity: 0 }, svg);
      s.sheetLn = ln(svg, 'M70 226H570V432H70Z', 't');
      s.fname = el('text', { x: 556, y: 250, class: 'lbl s', 'text-anchor': 'end', opacity: 0 }, svg, 'interview.srt');
      s.lines = SRT.map(([t, k], i) => el('text', { x: 92, y: 258 + i * 23, class: 'code-t' + (k === 'time' || k === 'num' ? ' dim' : '') }, svg, ''));
      return s;
    },
    update(s, p) {
      setDraw(s.card, out(seg(p, 0.02, 0.16)));
      s.bars.forEach((b, i) => setDraw(b, out(seg(p, 0.08 + i * 0.004, 0.16 + i * 0.004))));
      setOp(s.name, seg(p, 0.28, 0.38)); setOp(s.meta, seg(p, 0.3, 0.4));
      setOp(s.sheet, seg(p, 0.44, 0.52));
      setDraw(s.sheetLn, out(seg(p, 0.44, 0.56)));
      setOp(s.fname, seg(p, 0.5, 0.58));
      // whisper listens left to right; each bar it passes drops onto the page as a word
      const c = seg(p, 0.52, 0.86);
      const cx = 92 + c * 466;
      setTf(s.cursor, 'translate(' + cx.toFixed(1) + ' 0)');
      setOp(s.cursor, p > 0.52 && p < 0.9 ? 0.7 : 0);
      const half = s.bars.length / 2;
      s.bars.forEach((b, i) => {
        const bx = 96 + i * 8.2;
        const passed = cx > bx;
        setOp(b, passed ? 0.28 : 1);
        const t0 = 0.52 + ((bx - 92) / 466) * 0.34;
        const q = seg(p, t0, t0 + 0.07);
        const d = s.drops[i];
        if (q <= 0 || q >= 1) { setOp(d, 0); return; }
        const cue = i < half ? 2 : 6;
        const ty = 258 + cue * 23 - 5;
        const tx = 92 + ((i % half) / half) * 230;
        const e = io(q);
        setTf(d, 'translate(' + lerp(0, tx - bx, e).toFixed(1) + ' ' + lerp(0, ty - 105, e).toFixed(1) + ') rotate(' + (90 * e).toFixed(1) + ' ' + bx.toFixed(1) + ' 105)');
        setOp(d, 1 - seg(q, 0.7, 1));
      });
      // type the cues in step with the cursor
      const chars = SRT.map((l) => l[0].length);
      const firstHalf = [0, 1, 2], second = [4, 5, 6];
      const fill = (idxs, t) => {
        const total = idxs.reduce((a, i) => a + chars[i], 0);
        let n = Math.round(total * t);
        idxs.forEach((i) => { const k = Math.min(chars[i], n); n -= k; setText(s.lines[i], SRT[i][0].slice(0, k)); });
      };
      fill(firstHalf, seg(c, 0.04, 0.5));
      fill(second, seg(c, 0.52, 0.98));
    },
  };

  /* ================================================================ json */
  const IN = '{"b":1,"a":[1,2]}';
  const OUT = ['{', '  "a": [', '    1,', '    2', '  ],', '  "b": 1', '}'];
  const json = {
    build(svg) {
      const s = { chars: [] };
      const cw = 10.2, lh = 32, ox = 250, oy = 112;
      // target slots for every visible character in the formatted output
      const slots = [];
      OUT.forEach((line, r) => { [...line].forEach((ch, c) => { if (ch !== ' ') slots.push({ ch, x: ox + c * cw, y: oy + r * lh }); }); });
      const used = new Array(slots.length).fill(false);
      s.scribble = ln(svg, 'M214 206c-46-80 70-138 138-110s110 80 46 120-172 34-172-28 80-92 138-68 46 80-8 92-104 10-86-34', 't');
      s.scribble.style.opacity = '0.55';
      [...IN].forEach((ch, i) => {
        let k = slots.findIndex((sl, j) => !used[j] && sl.ch === ch);
        used[k] = true;
        const ang = (hash(i + 3) - 0.5) * 70;
        const r = 30 + hash(i + 9) * 70, th = hash(i + 17) * Math.PI * 2;
        const sx = 300 + Math.cos(th) * r * 1.35, sy = 186 + Math.sin(th) * r * 0.8;
        const t = el('text', { class: 'code-t', x: 0, y: 0, 'text-anchor': 'start' }, svg, ch);
        s.chars.push({ t, sx, sy, ang, s0: 1.1 + hash(i + 31) * 0.9, tx: slots[k].x, ty: slots[k].y, i });
      });
      s.inLbl = el('text', { x: 120, y: 392, class: 'lbl b' }, svg, 'stdin');
      s.inSub = el('text', { x: 120, y: 414, class: 'lbl s' }, svg, '1 line · 17 characters');
      s.nums = OUT.map((_, r) => el('text', { x: 216, y: oy + r * lh, class: 'lbl s', 'text-anchor': 'end', opacity: 0 }, svg, String(r + 1)));
      s.guides = [ln(svg, 'M' + (ox + 2) + ' ' + (oy + 10) + 'V' + (oy + 5 * lh - 16), 'h'), ln(svg, 'M' + (ox + 2 * cw + 2) + ' ' + (oy + 2 * lh - 16) + 'V' + (oy + 3 * lh + 6), 'h')];
      s.rule = ln(svg, 'M230 ' + (oy - 28) + 'V' + (oy + 6 * lh + 10), 'h');
      s.outLbl = el('text', { x: 250, y: 392, class: 'lbl b', opacity: 0 }, svg, 'stdout');
      s.outSub = el('text', { x: 250, y: 414, class: 'lbl s', opacity: 0 }, svg, '7 lines · keys sorted · valid JSON');
      return s;
    },
    update(s, p) {
      setDraw(s.scribble, p < 0.6 ? out(seg(p, 0.02, 0.3)) : -io(seg(p, 0.6, 0.78)));
      s.chars.forEach((c) => {
        const a = seg(p, 0.06 + c.i * 0.015, 0.16 + c.i * 0.015);
        const m = io(seg(p, 0.5 + c.i * 0.011, 0.7 + c.i * 0.011));
        const x = lerp(c.sx, c.tx, m), y = lerp(c.sy, c.ty, m);
        const sc = lerp(c.s0 * (0.6 + 0.4 * out(a)), 1, m);
        setOp(c.t, a);
        setTf(c.t, 'translate(' + x.toFixed(1) + ' ' + y.toFixed(1) + ') rotate(' + (c.ang * (1 - m)).toFixed(1) + ') scale(' + sc.toFixed(3) + ')');
      });
      const lbl = seg(p, 0.3, 0.4) * (1 - seg(p, 0.5, 0.58));
      setOp(s.inLbl, lbl); setOp(s.inSub, lbl);
      s.nums.forEach((n, i) => setOp(n, seg(p, 0.76 + i * 0.012, 0.84 + i * 0.012)));
      drawSeq(s.guides, p, 0.8, 0.92, 0.3);
      setDraw(s.rule, out(seg(p, 0.74, 0.86)));
      setOp(s.outLbl, seg(p, 0.84, 0.92)); setOp(s.outSub, seg(p, 0.86, 0.94));
    },
  };

  window.Scenes = { photo, pdf, film, speech, json };
})();
