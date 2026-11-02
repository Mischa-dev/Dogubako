/* Brush: hand-built stroke-order paths for 道具箱, turned into variable-width
   brush outlines (with ink pooling and dry-brush streaks) and revealed stroke
   by stroke through per-stroke masks. No libraries. */
(function () {
  'use strict';

  /* ---------------------------------------------------------------- helpers
     Every stroke is a list of [x, y, width] control points in a 100×100 box.
     H / V / T build horizontals, verticals and horizontal-then-down turns with
     a consistent brush entry (the 45° press) and ending (a small button). */
  function H(x0, y0, x1, y1, w, end) {
    const pts = [
      [x0 - 1.7, y0 - 2.1, w * 0.42],
      [x0 + 0.6, y0 + 0.4, w * 1.14],
      [x0 + 4.4, y0 + 0.35, w * 0.94],
      [(x0 + x1) / 2, (y0 + y1) / 2 + 0.3, w * 0.8],
    ];
    if (end === 'open') pts.push([x1 - 3, y1 + 0.05, w * 0.86], [x1, y1, w * 0.94]);
    else {
      pts.push([x1 - 4.5, y1 + 0.05, w * 0.88], [x1 - 1.0, y1 - 0.35, w * 1.0],
        [x1 + 0.8, y1 + 0.75, w * 1.1], [x1 + 0.1, y1 + 1.7, w * 0.66]);
    }
    return pts;
  }
  function V(x0, y0, x1, y1, w, end) {
    const pts = [
      [x0 - 2.0, y0 - 2.2, w * 0.44],
      [x0 + 0.2, y0 + 0.9, w * 1.16],
      [x0 + 0.35, y0 + 5.5, w * 0.98],
      [(x0 + x1) / 2 + 0.15, (y0 + y1) / 2, w * 0.86],
    ];
    if (end === 'stop') pts.push([x1, y1 - 1.6, w * 0.92], [x1 + 0.15, y1 + 0.4, w * 1.02], [x1 - 0.25, y1 + 1.3, w * 0.64]);
    else pts.push([x1, y1, w * 0.92]);
    return pts;
  }
  function T(x0, y0, xc, yc, y1, wh, wv, end) {
    // horizontal from (x0,y0) to the shoulder at (xc,yc), then down to y1
    const pts = [
      [x0 - 0.8, y0 - 1.4, wh * 0.5],
      [x0 + 1.2, y0 + 0.3, wh * 1.04],
      [x0 + 5, y0 + 0.1, wh * 0.92],
      [(x0 + xc) / 2, (y0 + yc) / 2 + 0.2, wh * 0.8],
      [xc - 3.4, yc + 0.1, wh * 0.86],
      [xc - 0.8, yc - 1.0, wh * 1.04],
      [xc + 1.3, yc + 0.5, wv * 1.12],
      [xc + 0.9, yc + 4.8, wv * 0.98],
      [xc + 0.6, (yc + y1) / 2, wv * 0.9],
    ];
    if (end === 'stop') pts.push([xc + 0.3, y1 - 1.4, wv * 0.92], [xc + 0.4, y1 + 0.5, wv * 1.02], [xc + 0.05, y1 + 1.4, wv * 0.64]);
    else pts.push([xc + 0.3, y1, wv * 0.92]);
    return pts;
  }
  const P = (...a) => { const o = []; for (let i = 0; i < a.length; i += 3) o.push([a[i], a[i + 1], a[i + 2]]); return o; };

  /* ---------------------------------------------------------------- glyphs
     Stroke order follows the Japanese school order.
     k: h horizontal · v vertical · t turn · p left-falling sweep · n right
        sweep · d dot · z zigzag. Sweeps get dry-brush streaks; stops pool. */
  const GLYPHS = {
    // 道: 首 (9) then 辶 (3)
    '道': [
      { k: 'd', p: P(45.2, 5.8, 1.6, 47.6, 8.4, 4.2, 50.6, 12.2, 6.2, 52.2, 15.0, 6.6, 51.2, 16.6, 4.2) },
      { k: 'p', p: P(76.8, 4.4, 5.4, 75.8, 7.6, 6.6, 73.2, 11.6, 5.2, 70.0, 15.2, 2.8, 68.0, 16.9, 0.9) },
      { k: 'h', p: H(36.5, 21.6, 88.6, 19.4, 4.8) },
      { k: 'p', p: P(65.4, 24.6, 4.6, 64.6, 26.8, 5.0, 62.2, 29.6, 3.8, 59.0, 32.2, 2.2, 57.0, 33.6, 0.9) },
      { k: 'v', p: V(48.0, 35.6, 49.0, 71.4, 5.4) },
      { k: 't', p: T(50.2, 36.0, 80.6, 33.8, 71.4, 4.2, 6.8, 'stop') },
      { k: 'h', p: H(52.4, 48.0, 79.4, 47.0, 3.7, 'open') },
      { k: 'h', p: H(52.4, 59.8, 79.4, 58.9, 3.7, 'open') },
      { k: 'h', p: H(51.8, 71.4, 80.2, 70.6, 4.4, 'open') },
      { k: 'd', p: P(12.2, 12.8, 1.8, 14.6, 15.4, 4.6, 18.0, 19.2, 6.8, 19.8, 22.4, 6.8, 18.8, 24.2, 4.2) },
      { k: 'z', p: P(6.8, 34.4, 3.6, 9.0, 35.9, 5.4, 13.2, 35.6, 4.6, 18.6, 34.8, 4.8, 21.0, 35.0, 6.0, 21.6, 36.6, 6.0,
          19.8, 39.2, 5.2, 16.0, 43.4, 4.2, 13.0, 46.6, 3.8, 13.4, 48.0, 4.6, 16.4, 48.0, 5.4, 19.4, 48.4, 6.0,
          19.6, 50.4, 5.6, 17.4, 54.2, 4.6, 14.2, 58.4, 3.4, 11.6, 61.4, 2.2) },
      { k: 'n', p: P(12.6, 60.6, 4.8, 13.8, 64.2, 6.0, 15.0, 71.8, 6.2, 17.2, 79.2, 5.8, 21.6, 84.8, 5.0, 29.6, 87.8, 4.4,
          46.0, 88.8, 4.4, 64.0, 89.4, 5.4, 79.6, 90.6, 7.6, 88.6, 92.0, 8.4, 94.2, 93.2, 6.0, 98.2, 94.2, 2.4, 100.6, 94.6, 0.6) },
    ],
    // 具: 目-like box (5), long horizontal, 八 (2)
    '具': [
      { k: 'v', p: V(30.6, 10.4, 31.4, 63.2, 5.4) },
      { k: 't', p: T(32.6, 10.8, 68.6, 8.4, 63.2, 4.2, 6.8) },
      { k: 'h', p: H(34.2, 24.0, 67.4, 22.9, 3.6, 'open') },
      { k: 'h', p: H(34.2, 37.6, 67.4, 36.5, 3.6, 'open') },
      { k: 'h', p: H(34.2, 51.0, 67.4, 50.0, 3.6, 'open') },
      { k: 'h', p: H(9.2, 65.6, 89.8, 62.6, 5.0) },
      { k: 'p', p: P(38.8, 71.8, 5.2, 38.2, 75.0, 6.4, 35.6, 80.0, 5.8, 30.2, 86.0, 4.6, 23.2, 91.4, 2.8, 17.0, 94.8, 0.9) },
      { k: 'd', p: P(60.6, 73.0, 1.8, 62.8, 75.6, 4.6, 67.8, 80.6, 7.2, 72.2, 85.6, 8.4, 73.6, 88.8, 7.2, 71.6, 90.4, 4.2) },
    ],
    // 箱: ⺮ (6), 木 (4), 目 (5)
    '箱': [
      { k: 'p', p: P(20.8, 2.4, 5.2, 20.0, 5.4, 6.0, 17.0, 9.8, 4.8, 12.4, 14.4, 2.8, 8.6, 17.4, 0.9) },
      { k: 'h', p: H(17.2, 12.0, 40.6, 10.4, 3.9) },
      { k: 'd', p: P(20.6, 17.0, 1.6, 22.4, 18.8, 4.2, 25.2, 22.6, 6.0, 26.0, 25.4, 5.6, 24.8, 26.8, 3.4) },
      { k: 'p', p: P(66.6, 2.4, 5.2, 65.8, 5.4, 6.0, 62.8, 9.8, 4.8, 58.2, 14.4, 2.8, 54.6, 17.4, 0.9) },
      { k: 'h', p: H(63.2, 12.0, 91.6, 9.8, 3.9) },
      { k: 'd', p: P(66.8, 17.0, 1.6, 68.6, 18.8, 4.2, 71.6, 22.6, 6.0, 72.4, 25.4, 5.6, 71.2, 26.8, 3.4) },
      { k: 'h', p: H(6.6, 46.0, 39.6, 44.2, 4.2, 'open') },
      { k: 'v', p: V(23.4, 32.2, 24.2, 95.6, 6.0, 'stop') },
      { k: 'p', p: P(22.6, 47.8, 4.4, 21.4, 51.8, 5.0, 17.6, 59.2, 4.4, 12.4, 66.4, 3.0, 6.8, 72.2, 1.6, 3.6, 74.8, 0.6) },
      { k: 'd', p: P(27.6, 54.2, 1.6, 29.6, 56.0, 4.0, 33.0, 59.4, 5.6, 34.2, 61.8, 5.0, 33.2, 63.2, 3.0) },
      { k: 'v', p: V(50.4, 32.0, 51.4, 95.4, 5.4) },
      { k: 't', p: T(52.4, 32.4, 86.8, 30.2, 95.0, 4.2, 6.8, 'stop') },
      { k: 'h', p: H(54.4, 53.4, 85.6, 52.2, 3.6, 'open') },
      { k: 'h', p: H(54.4, 74.0, 85.6, 72.9, 3.6, 'open') },
      { k: 'h', p: H(53.8, 95.4, 86.4, 94.6, 4.2, 'open') },
    ],
    // leaf numerals for the folded book
    '一': [{ k: 'h', p: H(12, 52, 88, 48.4, 6.4) }],
    '二': [{ k: 'h', p: H(24, 34, 76, 31.4, 5.4) }, { k: 'h', p: H(8, 72, 92, 68.4, 6.4) }],
    '三': [{ k: 'h', p: H(22, 22, 78, 19.6, 5.0) }, { k: 'h', p: H(28, 49, 72, 47.0, 4.6) }, { k: 'h', p: H(8, 80, 92, 76.4, 6.4) }],
    '四': [
      { k: 'v', p: V(10.6, 24.4, 11.6, 80.4, 6.8) },
      { k: 't', p: T(13.2, 25.2, 87.4, 22.6, 80.4, 5.4, 7.8) },
      { k: 'p', p: P(39.4, 26.6, 5.6, 39.0, 33.0, 6.0, 38.0, 42.0, 5.4, 35.0, 49.4, 4.0, 30.6, 55.0, 2.2, 27.6, 57.4, 0.9) },
      { k: 'v', p: P(57.6, 25.0, 3.4, 59.6, 27.8, 6.0, 59.8, 36.0, 5.6, 59.8, 49.0, 5.4, 60.8, 54.0, 5.4, 64.6, 55.6, 5.2, 70.6, 55.0, 5.2, 72.6, 54.2, 6.0, 73.0, 56.2, 3.8) },
      { k: 'h', p: H(13.6, 79.6, 86.0, 78.6, 5.8, 'open') },
    ],
  };

  /* ---------------------------------------------------------------- geometry */
  function hash(n) { n = Math.sin(n * 127.1 + 311.7) * 43758.5453; return n - Math.floor(n); }
  function noise(x, seed) { // smooth 1-D value noise in [-1, 1]
    const i = Math.floor(x), f = x - i, u = f * f * (3 - 2 * f);
    return (hash(i + seed * 17.3) * (1 - u) + hash(i + 1 + seed * 17.3) * u) * 2 - 1;
  }

  // Centripetal Catmull-Rom through [x, y, w] points; returns dense samples.
  function spline(pts, step) {
    const n = pts.length;
    if (n < 2) return pts.slice();
    const ext = [
      [2 * pts[0][0] - pts[1][0], 2 * pts[0][1] - pts[1][1], pts[0][2]],
      ...pts,
      [2 * pts[n - 1][0] - pts[n - 2][0], 2 * pts[n - 1][1] - pts[n - 2][1], pts[n - 1][2]],
    ];
    const out = [];
    const d = (a, b) => Math.max(1e-4, Math.pow(Math.hypot(b[0] - a[0], b[1] - a[1]), 0.5));
    for (let i = 1; i < ext.length - 2; i++) {
      const p0 = ext[i - 1], p1 = ext[i], p2 = ext[i + 1], p3 = ext[i + 2];
      const t0 = 0, t1 = t0 + d(p0, p1), t2 = t1 + d(p1, p2), t3 = t2 + d(p2, p3);
      const chord = Math.hypot(p2[0] - p1[0], p2[1] - p1[1]);
      const steps = Math.max(2, Math.ceil(chord / step));
      for (let s = 0; s < steps; s++) {
        const t = t1 + (t2 - t1) * (s / steps);
        const pt = [0, 0, 0];
        for (let c = 0; c < 3; c++) {
          const A1 = ((t1 - t) * p0[c] + (t - t0) * p1[c]) / (t1 - t0);
          const A2 = ((t2 - t) * p1[c] + (t - t1) * p2[c]) / (t2 - t1);
          const A3 = ((t3 - t) * p2[c] + (t - t2) * p3[c]) / (t3 - t2);
          const B1 = ((t2 - t) * A1 + (t - t0) * A2) / (t2 - t0);
          const B2 = ((t3 - t) * A2 + (t - t1) * A3) / (t3 - t1);
          pt[c] = ((t2 - t) * B1 + (t - t1) * B2) / (t2 - t1);
        }
        out.push(pt);
      }
    }
    out.push(pts[n - 1].slice());
    return out;
  }

  const f2 = (v) => Math.round(v * 100) / 100;

  /* Build one stroke: brush outline, the centerline used to reveal it,
     dry-brush streaks for the mask, and where ink pools. */
  function buildStroke(stroke, seed) {
    const s = spline(stroke.p, 0.55);
    const n = s.length;
    const len = [0];
    for (let i = 1; i < n; i++) len.push(len[i - 1] + Math.hypot(s[i][0] - s[i - 1][0], s[i][1] - s[i - 1][1]));
    const L = len[n - 1];
    const nx = [], ny = [], tx = [], ty = [];
    for (let i = 0; i < n; i++) {
      const a = s[Math.max(0, i - 1)], b = s[Math.min(n - 1, i + 1)];
      let dx = b[0] - a[0], dy = b[1] - a[1];
      const m = Math.hypot(dx, dy) || 1; dx /= m; dy /= m;
      tx.push(dx); ty.push(dy); nx.push(-dy); ny.push(dx);
    }
    const left = [], right = [];
    let wMax = 0;
    for (let i = 0; i < n; i++) {
      const t = len[i];
      const w = Math.max(0.35, s[i][2] * (1 + 0.045 * noise(t * 0.35, seed)));
      wMax = Math.max(wMax, w);
      const eL = 0.1 * noise(t * 0.9, seed + 3) + 0.03 * noise(t * 3.3, seed + 5);
      const eR = 0.1 * noise(t * 0.9, seed + 7) + 0.03 * noise(t * 3.3, seed + 9);
      left.push([s[i][0] + nx[i] * (w / 2 + eL), s[i][1] + ny[i] * (w / 2 + eL)]);
      right.push([s[i][0] - nx[i] * (w / 2 + eR), s[i][1] - ny[i] * (w / 2 + eR)]);
      s[i][2] = w;
    }
    // outline: left side forward, round end cap, right side back, round start cap
    let d = 'M' + f2(left[0][0]) + ' ' + f2(left[0][1]);
    for (let i = 1; i < n; i++) d += 'L' + f2(left[i][0]) + ' ' + f2(left[i][1]);
    const cap = (i, back) => {
      const r = s[i][2] / 2, k = 7;
      for (let j = 1; j < k; j++) {
        const th = (Math.PI * j) / k, c = Math.cos(th), sn = Math.sin(th);
        const ax = back ? -nx[i] : nx[i], ay = back ? -ny[i] : ny[i];
        const bx = back ? -tx[i] : tx[i], by = back ? -ty[i] : ty[i];
        d += 'L' + f2(s[i][0] + r * (c * ax + sn * bx)) + ' ' + f2(s[i][1] + r * (c * ay + sn * by));
      }
    };
    cap(n - 1, false);
    for (let i = n - 1; i >= 0; i--) d += 'L' + f2(right[i][0]) + ' ' + f2(right[i][1]);
    cap(0, true);
    d += 'Z';

    // fill points where the turn is tight enough that the inner edge folds over
    const plugs = [];
    for (let i = 2; i < n - 2; i++) {
      const turn = Math.abs(Math.atan2(tx[i - 2] * ty[i + 2] - ty[i - 2] * tx[i + 2], tx[i - 2] * tx[i + 2] + ty[i - 2] * ty[i + 2]));
      const span = len[i + 2] - len[i - 2];
      if (turn / span > 1 / Math.max(1, s[i][2] * 0.5)) plugs.push([s[i][0], s[i][1], s[i][2] / 2]);
    }
    let plugD = '';
    for (const [x, y, r] of plugs) plugD += 'M' + f2(x - r) + ' ' + f2(y) + 'a' + f2(r) + ' ' + f2(r) + ' 0 1 0 ' + f2(2 * r) + ' 0a' + f2(r) + ' ' + f2(r) + ' 0 1 0 ' + f2(-2 * r) + ' 0Z';

    // centerline, extended at both ends so butt-capped reveal covers the caps
    const e0 = s[0][2] * 0.7 + 0.8, e1 = s[n - 1][2] * 0.7 + 0.8;
    let c = 'M' + f2(s[0][0] - tx[0] * e0) + ' ' + f2(s[0][1] - ty[0] * e0);
    for (let i = 0; i < n; i += 2) c += 'L' + f2(s[i][0]) + ' ' + f2(s[i][1]);
    c += 'L' + f2(s[n - 1][0]) + ' ' + f2(s[n - 1][1]);
    c += 'L' + f2(s[n - 1][0] + tx[n - 1] * e1) + ' ' + f2(s[n - 1][1] + ty[n - 1] * e1);
    const cLen = L + e0 + e1;

    // dry-brush streaks: short broken gaps that open up where a fast sweep runs out of ink
    let streaks = '';
    const sweep = (stroke.k === 'p' || stroke.k === 'n') && L > 18;
    const lines = sweep
      ? [[-0.4, 0.62, 0.16], [-0.24, 0.7, 0.22], [-0.06, 0.8, 0.14], [0.15, 0.66, 0.24], [0.33, 0.58, 0.18], [0.44, 0.74, 0.12]]
      : (stroke.k === 'h' && L > 44 ? [[0.3, 0.42, 0.14], [-0.34, 0.5, 0.1]] : []);
    lines.forEach(([off, from, wd], li) => {
      let seg = '', on = false;
      const stop = sweep ? 1.01 : 0.74;
      for (let i = 0; i < n; i++) {
        const u = len[i] / L;
        const ramp = sweep ? (u - from) / (1 - from) : 0.3;
        const open = u > from && u < stop && noise(len[i] * 0.9 + li * 7, seed + li * 11) > (sweep ? 0.5 - ramp * 1.1 : 0.55);
        if (open) {
          const o = off * s[i][2];
          seg += (on ? 'L' : 'M') + f2(s[i][0] + nx[i] * o) + ' ' + f2(s[i][1] + ny[i] * o);
          on = true;
        } else on = false;
      }
      if (seg) streaks += '<path d="' + seg + '" stroke-width="' + wd + '"/>';
    });

    const last = s[n - 1];
    // ink pools a little where the brush stops and presses
    let pool = null;
    if (stroke.k !== 'p' && stroke.k !== 'n' && stroke.k !== 'z') {
      const j = Math.max(0, n - 1 - Math.round(1.6 / 0.55));
      pool = [s[j][0] + tx[j] * 0.2, s[j][1] + ty[j] * 0.2, Math.max(1, s[j][2] * 0.36)];
    }
    return { d, plugD, c, cLen, L, wMax, streaks, pool, k: stroke.k, start: s[0], end: last };
  }

  /* ---------------------------------------------------------------- render
     Each glyph is its own <svg>, so only the one being written repaints. */
  let uid = 0;
  function renderGlyph(svg, ch, opts) {
    opts = opts || {};
    const strokes = GLYPHS[ch];
    const id = 'b' + (uid++);
    let defs = '', body = '';
    const built = strokes.map((st, i) => buildStroke(st, (ch.charCodeAt(0) % 97) + i * 3.7));
    built.forEach((b, i) => {
      const mw = f2(b.wMax * 1.5 + 2.4);
      defs += '<mask id="' + id + 'm' + i + '" maskUnits="userSpaceOnUse" x="-10" y="-10" width="125" height="125">' +
        '<path class="rv" d="' + b.c + '" fill="none" stroke="#fff" stroke-width="' + mw + '" stroke-linecap="butt" stroke-linejoin="round"' +
        ' stroke-dasharray="' + f2(b.cLen + 1) + ' ' + f2(b.cLen + 40) + '" stroke-dashoffset="' + (opts.drawn ? 0 : f2(b.cLen + 1)) + '"/>' +
        (b.streaks ? '<g fill="none" stroke="#000" stroke-linecap="round">' + b.streaks + '</g>' : '') +
        '</mask>';
      body += '<g class="st" mask="url(#' + id + 'm' + i + ')">' +
        '<path class="ink" d="' + b.d + '"/>' +
        (b.plugD ? '<path class="ink" d="' + b.plugD + '"/>' : '') +
        (b.pool ? '<circle class="pool" cx="' + f2(b.pool[0]) + '" cy="' + f2(b.pool[1]) + '" r="' + f2(b.pool[2]) + '"/>' : '') +
        '</g>';
    });
    svg.setAttribute('viewBox', '-2 -2 104 104');
    svg.innerHTML = '<defs>' + defs + '</defs>' + body;
    const reveals = svg.querySelectorAll('.rv');
    return built.map((b, i) => ({ el: reveals[i], len: b.cLen + 1, L: b.L, k: b.k, start: b.start, end: b.end }));
  }

  // plain outlines (no masks), for the seal's carved characters
  function outlines(ch) {
    return GLYPHS[ch].map((st, i) => { const b = buildStroke(st, (ch.charCodeAt(0) % 97) + i * 3.7); return b.d + b.plugD; });
  }

  window.Brush = { GLYPHS, renderGlyph, buildStroke, spline, noise, outlines };
})();
