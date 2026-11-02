/* Dogubako, "Open the box".
   Scroll-scrubbed scenes built from CSS 3D transforms, the Web Animations API and one small canvas.
   Every scene is a pure function of its scroll progress p (0..1), so the same code also renders
   the still frames shown to people who prefer reduced motion. No libraries. */
(() => {
  'use strict';

  const doc = document.documentElement;
  const RM = doc.classList.contains('rm');
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => Array.from(r.querySelectorAll(s));
  const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
  const lerp = (a, b, t) => a + (b - a) * t;
  const seg = (p, a, b) => clamp((p - a) / (b - a));
  const E = {
    lin: (t) => t,
    in3: (t) => t * t * t,
    out3: (t) => 1 - Math.pow(1 - t, 3),
    out4: (t) => 1 - Math.pow(1 - t, 4),
    io2: (t) => (t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2),
    io3: (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2),
    io4: (t) => (t < 0.5 ? 8 * t * t * t * t : 1 - Math.pow(-2 * t + 2, 4) / 2),
    outExpo: (t) => (t >= 1 ? 1 : 1 - Math.pow(2, -10 * t)),
    outBack: (t, s = 1.5) => 1 + (s + 1) * Math.pow(t - 1, 3) + s * Math.pow(t - 1, 2),
    smooth: (t) => t * t * (3 - 2 * t),
  };
  const fmt = (n, d = 2) => Math.round(n * Math.pow(10, d)) / Math.pow(10, d);

  /* Piecewise keyframes of objects: [[p, {a, b}], ...] with eased segments. */
  function track(p, keys, ease = E.io3) {
    if (p <= keys[0][0]) return { ...keys[0][1] };
    for (let i = 1; i < keys.length; i++) {
      if (p <= keys[i][0]) {
        const [p0, a] = keys[i - 1], [p1, b] = keys[i];
        const t = ease((p - p0) / (p1 - p0));
        const o = {};
        for (const k in a) o[k] = lerp(a[k], b[k], t);
        return o;
      }
    }
    return { ...keys[keys.length - 1][1] };
  }

  /* Captions: each .cap has data-range="in out" in scene progress. Opacity and a short slide. */
  function capsCtl(box) {
    if (!box) return () => {};
    const caps = $$('.cap', box).map((el) => {
      const [a, b] = el.dataset.range.split(' ').map(Number);
      return { el, a, b, last: -2 };
    });
    const F = 0.03;
    return (p) => {
      for (const c of caps) {
        const i = c.a < 0 ? 1 : E.smooth(seg(p, c.a, c.a + F));
        const o = c.b > 1 ? 0 : E.smooth(seg(p, c.b - F, c.b));
        const v = Math.min(i, 1 - o);
        const q = fmt(v, 3);
        if (q === c.last) continue;
        c.last = q;
        const y = (1 - i) * 26 - o * 26;
        c.el.style.opacity = q;
        c.el.style.transform = `translate3d(0,${fmt(y, 1)}px,0)`;
      }
    };
  }

  /* ---------------------------------------------------------------- the box, in CSS 3D */
  const U = 4;
  const BW = 88 * U, BH = 66 * U, BD = 60 * U, BR = 14 * U, BS = 8 * U;
  const LID_H = 26 * U, YT = -BH / 2, YS = YT + LID_H, YB = BH / 2;
  const DW = 200, DH = 56, DD = 220;
  const CATS = [
    ['Images', 30], ['Video', 27], ['Audio', 21], ['Speech & Voice', 9], ['PDF & Documents', 27], ['Files', 23],
    ['Text', 27], ['Developer', 34], ['Data', 12], ['Network & Web', 18], ['Design', 9], ['System', 12],
  ];
  const LIGHT = (() => { const v = [-0.42, -0.78, 0.46]; const m = Math.hypot(...v); return v.map((x) => x / m); })();
  const shade = (nx, ny, nz) => fmt(0.18 + 0.82 * Math.max(0, nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2]), 3);

  function div(cls, parent, html) {
    const d = document.createElement('div');
    d.className = cls;
    if (html) d.innerHTML = html;
    parent.appendChild(d);
    return d;
  }
  function face(parent, w, h, tf, cls, l) {
    const f = div('f ' + cls, parent);
    f.style.cssText = `width:${fmt(w, 2)}px;height:${fmt(h, 2)}px;left:${fmt(-w / 2, 2)}px;top:${fmt(-h / 2, 2)}px;transform:${tf}` + (l != null ? `;--l:${l}` : '');
    return f;
  }
  /* Extrude a 2D outline (screen coords, clockwise) along z. Each edge becomes one face. */
  function extrude(parent, pts, depth, specials) {
    for (let i = 0; i < pts.length - 1; i++) {
      const [x1, y1] = pts[i], [x2, y2] = pts[i + 1];
      const L = Math.hypot(x2 - x1, y2 - y1);
      const th = Math.atan2(y2 - y1, x2 - x1);
      const tf = `translate3d(${fmt((x1 + x2) / 2)}px,${fmt((y1 + y2) / 2)}px,0) rotateZ(${fmt(th * 180 / Math.PI, 3)}deg) rotateX(90deg)`;
      const sp = specials && specials[i];
      if (sp) face(parent, L + 1.2, depth, tf, sp);
      else face(parent, L + 1.6, depth + 2, tf, 'mat edge', shade(Math.sin(th), -Math.cos(th), 0));
    }
  }
  function arc(cx, cy, r, a0, a1, n) {
    const out = [];
    for (let i = 0; i <= n; i++) {
      const a = (a0 + (a1 - a0) * (i / n)) * Math.PI / 180;
      out.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
    }
    return out;
  }

  function buildBox(host, { n = 5, drawers = true } = {}) {
    host.textContent = '';
    const bx = div('p3 bx', host);
    face(bx, 640, 420, `translate3d(0,${YB + 1}px,0) rotateX(90deg)`, 'floor');
    face(bx, 470, 330, `translate3d(0,${YB + 1.5}px,0) rotateX(90deg)`, 'floor core');
    face(bx, 900, 640, `translate3d(0,${YB + 2}px,0) rotateX(90deg)`, 'floorglow');

    const hx = BW / 2, cxl = -hx + BR, cxr = hx - BR, cyt = YT + BR, cyb = YB - BR;
    // body: right side down, bottom corners, left side up, then the open top
    const body = div('p3 body', bx);
    const bodyPts = [[hx, YS], ...arc(cxr, cyb, BR, 0, 90, n), ...arc(cxl, cyb, BR, 90, 180, n), [-hx, YS], [hx, YS]];
    extrude(body, bodyPts, BD, { [bodyPts.length - 2]: 'well' });
    const bodyH = YB - YS;
    const bf = face(body, BW, bodyH + 1.5, `translate3d(0,${YS + bodyH / 2 - 0.75}px,${BD / 2}px)`, 'mat capf', shade(0, 0, 1));
    bf.style.setProperty('--r', `0 0 ${BR}px ${BR}px`);
    div('panel', bf).style.cssText = `left:${BS}px;right:${BS}px;top:${BS / 2 + 1.5}px;bottom:${BS}px;border-radius:0 0 ${BR - BS}px ${BR - BS}px`;
    const bb = face(body, BW, bodyH, `translate3d(0,${YS + bodyH / 2}px,${-BD / 2}px) rotateY(180deg)`, 'mat capf back', shade(0, 0, -1));
    bb.style.setProperty('--r', `0 0 ${BR}px ${BR}px`);

    // lid: hinged at the back of the seam
    const lid = div('p3 lid', bx);
    lid.style.transformOrigin = `0px ${YS}px ${-BD / 2}px`;
    const lidPts = [[-hx, YS], ...arc(cxl, cyt, BR, 180, 270, n), ...arc(cxr, cyt, BR, 270, 360, n), [hx, YS], [-hx, YS]];
    extrude(lid, lidPts, BD, { [lidPts.length - 2]: 'lidin' });
    const lf = face(lid, BW, LID_H + 1.5, `translate3d(0,${YT + LID_H / 2 + 0.75}px,${BD / 2 + 0.4}px)`, 'mat capf', shade(0, 0, 1));
    lf.style.setProperty('--r', `${BR}px ${BR}px 0 0`);
    div('panel', lf).style.cssText = `left:${BS}px;right:${BS}px;top:${BS}px;bottom:${BS / 2 + 1.5}px;border-radius:${BR - BS}px ${BR - BS}px 0 0`;
    const lb = face(lid, BW, LID_H, `translate3d(0,${YT + LID_H / 2}px,${-BD / 2}px) rotateY(180deg)`, 'mat capf back', shade(0, 0, -1));
    lb.style.setProperty('--r', `${BR}px ${BR}px 0 0`);

    // the latch: vermilion, hinged on its top edge, straddling the seam
    const lw = 18 * U, lh = 14 * U, lt = 9;
    const latch = div('p3 latch', lid);
    face(latch, lw, lh, `translate3d(0,${lh / 2}px,${lt}px)`, 'latch');
    face(latch, lw, lh, `translate3d(0,${lh / 2}px,0) rotateY(180deg)`, 'latch-b');
    face(latch, lw - 6, lt, `translate3d(0,${lh}px,${lt / 2}px) rotateX(-90deg)`, 'latch-e');
    face(latch, lw - 6, lt, `translate3d(0,0,${lt / 2}px) rotateX(90deg)`, 'latch-e');
    face(latch, lt, lh - 6, `translate3d(${-lw / 2}px,${lh / 2}px,${lt / 2}px) rotateY(-90deg)`, 'latch-e');
    face(latch, lt, lh - 6, `translate3d(${lw / 2}px,${lh / 2}px,${lt / 2}px) rotateY(90deg)`, 'latch-e');

    if (drawers) {
      const cab = div('p3 cab', bx);
      CATS.forEach(([name, count]) => {
        const d = div('p3 dr', cab);
        const fr = face(d, DW, DH, `translateZ(${DD / 2}px)`, 'dr-front');
        fr.innerHTML = `<span class="dn">${name.replace('&', '&amp;')}</span><span class="dc">${count}</span><i class="dp"></i>`;
        face(d, DW, DD, `translate3d(0,${-DH / 2}px,0) rotateX(90deg)`, 'dr-top');
        face(d, DD, DH, `translate3d(${DW / 2}px,0,0) rotateY(90deg)`, 'dr-side');
        face(d, DD, DH, `translate3d(${-DW / 2}px,0,0) rotateY(-90deg)`, 'dr-side');
      });
    }
    return bx;
  }

  function bindBox(world) {
    const bx = $('.bx', world);
    return {
      bx,
      lid: $('.lid', bx),
      latch: $('.latch', bx),
      well: $('.well', bx),
      floor: $('.floor', bx),
      floorglow: $('.floorglow', bx),
      drawers: $$('.dr', bx).map((el) => ({ el, faces: Array.from(el.children), op: null })),
    };
  }

  /* Cabinet slot (2 columns x 6 rows, standing on the box) and the hidden start inside the box. */
  const CAB_S = 0.81;
  function cabSlot(i) {
    const c = i % 2, r = (i / 2) | 0;
    return { x: (c - 0.5) * 166, y: YS - 27 - (5 - r) * 48, z: 0 };
  }

  function setLatch(b, a) { b.latch.style.transform = `translate3d(0,${YS - 7 * U}px,${BD / 2}px) rotateX(${fmt(a, 2)}deg)`; }
  function setLid(b, a) { b.lid.style.transform = `rotateX(${fmt(a, 2)}deg)`; }
  function setDrawer(d, x, y, z, ry, rx, rz, s) {
    d.el.style.transform = `translate3d(${fmt(x, 1)}px,${fmt(y, 1)}px,${fmt(z, 1)}px) rotateY(${fmt(ry, 2)}deg) rotateX(${fmt(rx, 2)}deg) rotateZ(${fmt(rz, 2)}deg) scale3d(${fmt(s, 4)},${fmt(s, 4)},${fmt(s, 4)})`;
  }
  function setDrawerOpacity(d, o) {
    o = fmt(o, 3);
    if (o === d.op) return;
    d.op = o;
    for (const f of d.faces) f.style.opacity = o;
  }

  const PERSP = 1600;
  function worldTf(c) {
    return `translate3d(${fmt(c.x, 1)}px,${fmt(c.y, 1)}px,${fmt(c.z || 0, 1)}px) rotateX(${fmt(c.rx, 3)}deg) rotateY(${fmt(c.ry, 3)}deg) scale3d(${fmt(c.s, 4)},${fmt(c.s, 4)},${fmt(c.s, 4)})`;
  }
  /* Project a point in box space to stage pixels, matching the CSS above. */
  function project(px, py, pz, c, W, H) {
    let x = px * c.s, y = py * c.s, z = pz * c.s;
    const ry = c.ry * Math.PI / 180, rx = c.rx * Math.PI / 180;
    const cy = Math.cos(ry), sy = Math.sin(ry);
    [x, z] = [x * cy + z * sy, -x * sy + z * cy];
    const cx = Math.cos(rx), sx = Math.sin(rx);
    [y, z] = [y * cx - z * sx, y * sx + z * cx];
    x += c.x; y += c.y; z += c.z || 0;
    const ox = W / 2 + c.x, oy = H / 2 + c.y;
    const k = PERSP / (PERSP - z);
    return [ox + (W / 2 + x - ox) * k, oy + (H / 2 + y - oy) * k, k];
  }

  /* ---------------------------------------------------------------- dust in the light */
  class Dust {
    constructor(canvas, n) {
      this.c = canvas; this.ctx = canvas.getContext('2d'); this.ps = [];
      for (let i = 0; i < n; i++) this.ps.push(this.spawn(true));
      this.t = 0;
    }
    spawn(init) {
      return { x: Math.random() * 2 - 1, y: init ? Math.random() : 0, v: 0.04 + Math.random() * 0.07, r: 0.6 + Math.random() * 1.6, a: 0.25 + Math.random() * 0.6, f: 0.5 + Math.random() * 1.2, ph: Math.random() * 6.28 };
    }
    size(W, H) {
      const dpr = Math.min(2, window.devicePixelRatio || 1);
      this.W = W; this.H = H; this.dpr = dpr;
      this.c.width = Math.round(W * dpr); this.c.height = Math.round(H * dpr);
    }
    draw(dt, amt, ox, oy, half, height) {
      const ctx = this.ctx;
      ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
      ctx.clearRect(0, 0, this.W, this.H);
      if (amt < 0.02) return;
      this.t += dt;
      for (const p of this.ps) {
        p.y += p.v * dt;
        if (p.y > 1) Object.assign(p, this.spawn(false));
        const spread = half * (1 + p.y * 0.9);
        const x = ox + (p.x * spread) + Math.sin(this.t * p.f + p.ph) * 10;
        const y = oy - p.y * height;
        const life = Math.sin(Math.PI * p.y);
        ctx.globalAlpha = amt * p.a * life;
        ctx.fillStyle = '#fff6e6';
        ctx.beginPath(); ctx.arc(x, y, p.r, 0, 6.283); ctx.fill();
      }
      ctx.globalAlpha = 1;
    }
  }

  /* ---------------------------------------------------------------- hero: open the box */
  class Hero {
    constructor(root, opts = {}) {
      this.root = root;
      this.still = !!opts.still;
      this.stage = $('.hero-stage', root);
      this.v3d = $('.v3d', this.stage);
      this.world = $('.world', this.stage);
      this.dim = $('.stage-dim', this.stage);
      this.vig = $('.stage-vig', this.stage);
      this.shaft = $('.stage-shaft', this.stage);
      this.pool = $('.stage-pool', this.stage);
      this.flash = $('.stage-flash', this.stage);
      this.face = $('.flash-face', this.stage);
      this.copy = this.still ? null : $('.hero-copy', root);
      this.capBox = this.still ? null : $('.hero-caps', root);
      this.caps = capsCtl(this.capBox);
      this.cue = this.still ? null : $('.scroll-cue', root);
      if (!opts.built) buildBox(this.world, { n: innerWidth < 760 ? 3 : 5 });
      this.b = bindBox(this.world);
      const canvas = $('.dust', this.stage);
      this.dust = !this.still && !RM && canvas ? new Dust(canvas, innerWidth < 760 ? 26 : 56) : null;
      this.intro0 = this.still || RM ? -1 : performance.now() / 1000 + 0.25;
      this.navDark = false;
    }
    layout() {
      const W = this.stage.clientWidth, H = this.stage.clientHeight;
      this.W = W; this.H = H;
      this.narrow = W < 760;
      this.base = this.narrow ? Math.min(W / 520, H / 880) : clamp(Math.min(W / 1440, H / 900) * 1.02, 0.5, 1.2);
      if (this.dust) this.dust.size(W, H);
      this.keys = this.narrow ? [
        [0.00, { x: 0, y: 0.25, rx: -13, ry: -25, s: 0.86 }],
        [0.10, { x: -0.02, y: 0.17, rx: -15, ry: -29, s: 0.95 }],
        [0.21, { x: -0.04, y: 0.15, rx: -19, ry: -32, s: 0.97 }],
        [0.36, { x: -0.03, y: 0.22, rx: -25, ry: -27, s: 0.95 }],
        [0.47, { x: 0, y: 0.27, rx: -20, ry: -24, s: 0.92 }],
        [0.62, { x: 0, y: 0.34, rx: -12, ry: -20, s: 0.8 }],
        [0.80, { x: 0, y: 0.355, rx: -6, ry: -8, s: 0.8 }],
        [0.86, { x: 0, y: 0.355, rx: -5, ry: -7, s: 0.8 }],
        [0.95, { x: 0, y: 0.02, rx: 0, ry: 0, s: 0.8 }],
        [1.00, { x: 0, y: 0, rx: 0, ry: 0, s: 0.8 }],
      ] : [
        [0.00, { x: 0.2, y: 0.05, rx: -12, ry: -26, s: 1.0 }],
        [0.10, { x: 0.17, y: 0.06, rx: -15, ry: -30, s: 1.12 }],
        [0.21, { x: 0.16, y: 0.08, rx: -19, ry: -33, s: 1.16 }],
        [0.36, { x: 0.15, y: 0.16, rx: -25, ry: -27, s: 1.06 }],
        [0.47, { x: 0.15, y: 0.2, rx: -20, ry: -25, s: 0.98 }],
        [0.62, { x: 0.16, y: 0.25, rx: -12, ry: -22, s: 0.86 }],
        [0.80, { x: 0.17, y: 0.27, rx: -7, ry: -11, s: 0.86 }],
        [0.86, { x: 0.17, y: 0.27, rx: -6, ry: -9, s: 0.86 }],
        [0.95, { x: 0.01, y: 0.02, rx: 0, ry: 0, s: 0.86 }],
        [1.00, { x: 0, y: 0, rx: 0, ry: 0, s: 0.86 }],
      ];
      if (this.still) {
        this.keys = this.keys.map(([p, v]) => [p, { ...v, x: 0, y: v.y * 0.8 + (this.narrow ? -0.04 : 0.04), s: v.s * (this.narrow ? 1.05 : 1.28) }]);
      }
    }
    fanSlot(i) {
      if (this.narrow) {
        const c = i % 2, r = (i / 2) | 0;
        return { x: (c - 0.5) * 262, y: -626 + r * 84, z: 70, ry: (c - 0.5) * -10, s: 1.22 };
      }
      const c = i % 3, r = (i / 3) | 0;
      const x = (c - 1) * 236;
      const phi = x / 1100;
      return { x, y: -452 + r * 80, z: 80 + 1100 * (1 - Math.cos(phi)) + ((i * 37) % 5) * 9, ry: -phi * 180 / Math.PI, s: 1 };
    }
    render(p, time, dt) {
      const W = this.W, H = this.H;
      if (!W) return false;
      let needs = false;
      // camera
      const k = track(p, this.keys);
      const cam = { x: k.x * W, y: k.y * H, z: 0, rx: k.rx, ry: k.ry, s: k.s * this.base };
      if (this.intro0 > 0) {
        const it = clamp((time - this.intro0) / 2.6);
        if (it < 1) needs = true; else this.intro0 = -1;
        const e = E.io4(it);
        cam.rx = lerp(0, cam.rx, e);
        cam.ry = lerp(0, cam.ry, e);
        cam.s *= lerp(0.9, 1, E.out3(it));
        cam.y += lerp(H * 0.03, 0, E.out3(it));
        this.v3d.style.opacity = fmt(E.out3(clamp(it * 3)), 3);
      }
      if (!this.still && !RM && p < 0.06) {
        const idle = 1 - seg(p, 0.02, 0.06);
        cam.ry += Math.sin(time * 0.55) * 1.6 * idle;
        cam.rx += Math.sin(time * 0.8 + 1) * 0.6 * idle;
        cam.y += Math.sin(time * 0.9) * 4 * idle;
        needs = true;
      }
      this.world.style.transform = worldTf(cam);
      this.v3d.style.perspectiveOrigin = `${fmt(W / 2 + cam.x, 1)}px ${fmt(H / 2 + cam.y, 1)}px`;

      // latch, then the lid (with a small jolt as the latch lets go)
      const la = seg(p, 0.115, 0.2);
      setLatch(this.b, 168 * E.outBack(la, 1.2));
      const jolt = Math.sin(Math.PI * seg(p, 0.185, 0.215)) * 3.5;
      const lo = E.io3(seg(p, 0.215, 0.36));
      setLid(this.b, jolt + 112 * lo);

      // light: the room dims from the edges in, the box glows, a shaft of light rises
      const g = E.smooth(seg(p, 0.2, 0.33)) * (1 - 0.35 * seg(p, 0.62, 0.8));
      const d = E.smooth(seg(p, 0.205, 0.37));
      const fl = E.smooth(seg(p, 0.955, 1));
      const fr = seg(p, 0.925, 0.99);
      const [ox, oy, ok] = project(0, YS, 0, cam, W, H);
      const [bx0, by0] = project(0, 0, 0, cam, W, H);
      this.dim.style.opacity = fmt(E.in3(d) * 0.96 * (1 - fl), 3);
      if (this.vig) {
        this.vig.style.opacity = fmt(E.out3(d) * (1 - fl), 3);
        this.vig.style.transform = `translate3d(${fmt(bx0, 1)}px,${fmt(by0, 1)}px,0) scale(${fmt(lerp(4.2, 3.2, d), 3)})`;
      }
      this.b.well.style.setProperty('--g', fmt(0.25 + g * 0.75, 3));
      this.b.floorglow.style.opacity = fmt(g * 0.9, 3);
      if (this.shaft) {
        const sh = g * (1 - E.smooth(seg(p, 0.52, 0.74)));
        const sc = cam.s * ok * lerp(0.92, 1.04, g);
        this.shaft.style.opacity = fmt(sh, 3);
        this.shaft.style.transform = `translate3d(${fmt(ox, 1)}px,${fmt(oy, 1)}px,0) scale(${fmt(sc, 4)},${fmt(sc * lerp(0.7, 1, g), 4)})`;
      }
      if (this.pool) {
        this.pool.style.opacity = fmt(g, 3);
        this.pool.style.transform = `translate3d(${fmt(ox, 1)}px,${fmt(oy, 1)}px,0) scale(${fmt(cam.s * 1.1, 3)})`;
      }
      if (this.dust) {
        const amt = g * (1 - seg(p, 0.66, 0.8));
        if (amt > 0.02 || this.dustOn) {
          this.dust.draw(dt || 0.016, amt, ox, oy, BW * 0.4 * cam.s * ok, H * 0.55);
          this.dustOn = amt > 0.02;
          if (this.dustOn) needs = true;
        }
      }

      // drawers: rise out of the box row by row, fan out into a wall, then Images comes to you
      const dv = E.io3(seg(p, 0.86, 0.975));
      const away = E.smooth(seg(p, 0.84, 0.895));
      const diveZ = (PERSP * 0.6) / Math.max(0.2, cam.s);
      this.b.drawers.forEach((dr, i) => {
        const r = (i / 2) | 0;
        const cs = cabSlot(i);
        const rise = E.io3(seg(p, 0.37 + r * 0.026, 0.47 + r * 0.026));
        const fan = E.io3(seg(p, 0.6 + i * 0.007, 0.72 + i * 0.007));
        const fs = this.fanSlot(i);
        let x = cs.x, y = lerp(YS + 70, cs.y, rise), z = cs.z, ry = 0, rx = 0, rz = 0, s = CAB_S;
        if (fan > 0) {
          const arcT = Math.sin(Math.PI * fan);
          x = lerp(cs.x, fs.x, fan);
          y = lerp(cs.y, fs.y, fan) - arcT * 40;
          z = lerp(cs.z, fs.z, fan) + arcT * 170;
          ry = lerp(0, fs.ry, fan);
          rx = -arcT * 8;
          rz = arcT * (i % 2 ? 5 : -5);
          s = lerp(CAB_S, fs.s, fan);
        }
        let o = rise > 0.001 ? 1 : 0;
        if (dv > 0) {
          if (i === 0) {
            x = lerp(x, 0, dv); y = lerp(y, 0, dv); z = lerp(z, diveZ, E.in3(dv) * 0.85 + dv * 0.15);
            ry = lerp(ry, 0, dv); rx = lerp(rx, 0, dv); rz = lerp(rz, 0, dv); s = lerp(s, 1, dv);
          }
        }
        if (away > 0 && i !== 0) {
          z -= away * 600; y += away * 80;
          o *= 1 - away;
        }
        setDrawer(dr, x, y, z, ry, rx, rz, s);
        setDrawerOpacity(dr, o);
        if (i === 0) this.d0 = [x, y, z, s];
      });
      if (this.flash) {
        // the Images drawer front comes right up to you and opens out into the page
        if (fr > 0) {
          const [x0, y0, z0, s0] = this.d0;
          const zf = z0 + (DD / 2) * s0;
          const [ax, ay] = project(x0 - (DW / 2) * s0, y0 - (DH / 2) * s0, zf, cam, W, H);
          const [bx, by] = project(x0 + (DW / 2) * s0, y0 + (DH / 2) * s0, zf, cam, W, H);
          const e = E.io3(fr);
          const t = lerp(ay, -2, e), b = lerp(H - by, -2, e), l = lerp(ax, -2, e), r = lerp(W - bx, -2, e);
          const rad = lerp(5 * (bx - ax) / DW, 0, e);
          this.flash.style.clipPath = `inset(${fmt(t, 1)}px ${fmt(r, 1)}px ${fmt(b, 1)}px ${fmt(l, 1)}px round ${fmt(rad, 1)}px)`;
          this.flash.style.background = `color-mix(in oklab, var(--bg) ${fmt(E.smooth(seg(fr, 0.45, 1)) * 100, 1)}%, var(--drawer))`;
          if (this.face) {
            const rw = W - r - l, rh = H - b - t;
            const k = Math.min((bx - ax) / DW * lerp(1, 1.6, e), rw / DW);
            const fx = l + rw / 2 - DW * k / 2, fy = t + rh / 2 - DH * k / 2;
            this.face.style.transform = `translate3d(${fmt(fx, 1)}px,${fmt(fy, 1)}px,0) scale(${fmt(k, 4)})`;
            this.face.style.opacity = fmt(1 - E.smooth(seg(fr, 0.12, 0.5)), 3);
          }
        }
        this.flash.style.opacity = fmt(fr > 0 ? 1 : 0, 3);
      }

      // copy and captions
      if (this.copy) {
        const c = seg(p, 0.015, 0.085);
        this.copy.style.opacity = fmt(1 - E.smooth(c), 3);
        this.copy.style.transform = this.narrow ? `translate3d(0,${fmt(-c * 40, 1)}px,0)` : `translate3d(0,calc(-46% - ${fmt(c * 40, 1)}px),0)`;
        this.copy.style.visibility = c >= 1 ? 'hidden' : '';
      }
      if (this.cue) this.cue.style.opacity = fmt(1 - seg(p, 0, 0.03), 3);
      this.caps(p);
      const dark = d > 0.45 && fl < 0.5;
      if (this.capBox && dark !== this.dark) { this.capBox.classList.toggle('on-dark', dark); }
      this.dark = dark;
      this.navDark = dark;
      return needs;
    }
  }

  /* ---------------------------------------------------------------- scene 1: the app window */
  function offsetIn(el, anc) {
    let x = 0, y = 0, n = el;
    while (n && n !== anc) { x += n.offsetLeft; y += n.offsetTop; n = n.offsetParent; }
    return { x, y, w: el.offsetWidth, h: el.offsetHeight };
  }
  function waveBars(el, seed, n) {
    let h = 2166136261;
    for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
    let html = '';
    for (let i = 0; i < n; i++) {
      h = Math.imul(h ^ (h >>> 13), 1274126177);
      const r = ((h >>> 0) % 1000) / 1000;
      html += `<span style="height:${Math.round(6 + r * 30 * (0.45 + 0.55 * Math.sin(i / 4) ** 2))}px"></span>`;
    }
    el.innerHTML = html;
  }

  class App {
    constructor(root, opts = {}) {
      this.root = root; this.still = !!opts.still;
      this.stage = $('.app-stage', root);
      this.fit = $('.app-fit', root);
      this.win = $('.win', root);
      this.wm = $('.wm', root);
      this.caps = capsCtl(this.still ? null : $('.side-caps', root));
      this.home = $('.v-home', root); this.intent = $('.v-intent', root); this.tool = $('.v-tool', root);
      this.photo = $('.photo', root); this.pShadow = $('.photo-shadow', root);
      this.pResult = $('.photo-result', root); this.wipe = $('.photo-wipe', root);
      this.veil = $('.drop-veil', root); this.cursor = $('.cursor', root);
      this.benchCap = $('.vh-cap-photo', root);
      this.opts = $$('.vi-opts li', root);
      this.typed = $('.vi-typed', root); this.caret = $('.vi-caret', root); this.ph = $('.vi-ph', root);
      this.size = $('.vt-size', root); this.delta = $('.vt-delta', root);
      this.prog = $('.vt-progress i', root); this.runBtn = $('.vt-btn', root);
      const wave = $('.bo-wave', root);
      if (wave && !wave.children.length) waveBars(wave, 'interview.m4a', 40);
      this.lastActive = -1; this.lastNum = -1; this.lastChars = -1;
    }
    layout() {
      const sw = this.stage.clientWidth, sh = this.stage.clientHeight;
      this.narrow = (this.still ? sw : innerWidth) < 900 && sw < 700;
      this.root.classList.toggle('narrow', this.narrow);
      const dw = this.narrow ? 340 : 980, dh = this.narrow ? 590 : 640;
      this.scale = Math.min(sw / dw, sh / dh, this.narrow ? 1.12 : 1.08) * (this.narrow ? 0.94 : 0.96);
      this.fit.style.transform = `translate(-50%,-50%) scale(${fmt(this.scale, 4)})`;
      // measure the slots the photo travels between
      const r = (s) => offsetIn($(s, this.root), this.wm);
      this.sBench = r('.slot-bench'); this.sIntent = r('.slot-intent'); this.sTool = r('.slot-tool');
      const base = this.sTool;
      this.base = base;
      this.photo.style.width = base.w + 'px'; this.photo.style.height = base.h + 'px';
      this.wmW = this.wm.clientWidth; this.wmH = this.wm.clientHeight;
      this.optH = this.opts[1] ? this.opts[1].offsetTop - this.opts[0].offsetTop : 56;
      this.optPos = this.opts.map((li) => offsetIn(li, this.wm));
      this.runPos = offsetIn(this.runBtn, this.wm);
    }
    place(rect, rot, lift) {
      const s = rect.w / this.base.w;
      const cx = rect.x + rect.w / 2, cy = rect.y + rect.h / 2;
      this.photo.style.transform = `translate3d(${fmt(cx, 1)}px,${fmt(cy - lift * 14, 1)}px,0) rotate(${fmt(rot, 2)}deg) scale(${fmt(s * (1 + lift * 0.05), 4)}) translate(${fmt(-this.base.w / 2, 1)}px,${fmt(-this.base.h / 2, 1)}px)`;
      this.pShadow.style.opacity = fmt(0.65 + lift * 0.35, 3);
      this.pShadow.style.transform = `translate3d(0,${fmt(lift * 18, 1)}px,0) scale(${fmt(1 - lift * 0.04, 3)})`;
    }
    render(p) {
      if (!this.base) return false;
      // the window materialises out of the light we dove into, then settles
      const ent = E.out4(seg(p, 0, 0.1));
      const fadeIn = E.smooth(seg(p, 0, 0.045));
      const out = E.io2(seg(p, 0.94, 1));
      this.win.style.opacity = fmt(fadeIn, 3);
      this.win.style.transform = `perspective(1600px) translate3d(0,${fmt((1 - ent) * 70 - out * 10, 1)}px,0) rotateX(${fmt((1 - ent) * 14, 2)}deg) scale(${fmt(lerp(1.32, 1, ent) - out * 0.03, 4)})`;

      // views
      const homeOut = E.io2(seg(p, 0.32, 0.36));
      const intIn = E.out3(seg(p, 0.34, 0.4)), intOut = E.io2(seg(p, 0.615, 0.655));
      const toolIn = E.out3(seg(p, 0.635, 0.69));
      this.home.style.opacity = fmt(1 - homeOut, 3);
      this.home.style.transform = `translate3d(0,${fmt(-homeOut * 16, 1)}px,0)`;
      this.intent.style.opacity = fmt(intIn * (1 - intOut), 3);
      this.intent.style.transform = `translate3d(${fmt(-intOut * 24, 1)}px,${fmt((1 - intIn) * 16, 1)}px,0)`;
      this.tool.style.opacity = fmt(toolIn, 3);
      this.tool.style.transform = `translate3d(${fmt((1 - toolIn) * 28, 1)}px,0,0)`;
      this.home.style.visibility = homeOut >= 1 ? 'hidden' : '';
      this.intent.style.visibility = intIn <= 0 || intOut >= 1 ? 'hidden' : '';
      this.tool.style.visibility = toolIn <= 0 ? 'hidden' : '';

      // drag and drop: the photo comes in from outside the window, carried by the cursor
      const T0 = 0.16;
      const carry = E.io3(seg(p, T0, 0.255));
      const drop = E.out3(seg(p, 0.262, 0.302));
      const veil = E.smooth(seg(p, 0.2, 0.235)) * (1 - E.smooth(seg(p, 0.275, 0.305)));
      this.veil.style.opacity = fmt(veil, 3);
      const bench = this.sBench;
      const off = { x: this.wmW + 60, y: -bench.h * 1.4, w: bench.w * 1.08, h: bench.h * 1.08 };
      const hover = { x: bench.x + 6, y: bench.y - 8, w: bench.w * 1.06, h: bench.h * 1.06 };
      let rect, rot = 0, lift = 0, op = 1;
      if (p < T0) { op = 0; rect = off; }
      else if (p < 0.262) {
        rect = { x: lerp(off.x, hover.x, carry), y: lerp(off.y, hover.y, carry) - Math.sin(Math.PI * carry) * 40, w: lerp(off.w, hover.w, carry), h: lerp(off.h, hover.h, carry) };
        rot = lerp(12, -3, carry); lift = 1; op = E.smooth(seg(p, T0, T0 + 0.025));
      } else if (p < 0.335) {
        const sq = Math.sin(Math.PI * seg(p, 0.29, 0.318)) * 0.025;
        rect = { x: lerp(hover.x, bench.x, drop), y: lerp(hover.y, bench.y, drop), w: lerp(hover.w, bench.w, drop) * (1 + sq), h: lerp(hover.h, bench.h, drop) * (1 - sq) };
        rot = lerp(-3, 0, drop); lift = 1 - drop;
      } else if (p < 0.63) {
        const m = E.io3(seg(p, 0.335, 0.4));
        const a = bench, b = this.sIntent;
        rect = { x: lerp(a.x, b.x, m), y: lerp(a.y, b.y, m) - Math.sin(Math.PI * m) * 20, w: lerp(a.w, b.w, m), h: lerp(a.h, b.h, m) };
        lift = Math.sin(Math.PI * m) * 0.6;
      } else {
        const m = E.io3(seg(p, 0.63, 0.7));
        const a = this.sIntent, b = this.sTool;
        rect = { x: lerp(a.x, b.x, m), y: lerp(a.y, b.y, m) - Math.sin(Math.PI * m) * 24, w: lerp(a.w, b.w, m), h: lerp(a.h, b.h, m) };
        lift = Math.sin(Math.PI * m) * 0.6;
      }
      this.photo.style.opacity = fmt(op, 3);
      this.place(rect, rot, lift);
      this.benchCap.style.opacity = fmt(E.smooth(seg(p, 0.29, 0.32)), 3);

      // ranked tools: appear, "smaller" is typed, Compress lifts and moves to the top
      const chars = Math.round(7 * seg(p, 0.47, 0.52));
      if (chars !== this.lastChars) {
        this.typed.textContent = 'smaller'.slice(0, chars);
        this.ph.style.opacity = chars ? 0 : 1;
        this.lastChars = chars;
      }
      this.caret.style.opacity = p > 0.455 && p < 0.615 ? 1 : 0;
      const ro = E.io3(seg(p, 0.522, 0.575));
      const liftC = Math.sin(Math.PI * ro);
      const swapped = ro > 0.5;
      this.opts.forEach((li, i) => {
        const a = E.out3(seg(p, 0.37 + i * 0.02, 0.41 + i * 0.02));
        let y = 0, sc = 1, x = (1 - a) * 18;
        if (i === 0) { y = ro * this.optH; sc = 1 - liftC * 0.03; }
        if (i === 1) { y = -ro * this.optH; sc = 1 + liftC * 0.035; x -= liftC * 6; }
        if (i === 1) sc -= Math.sin(Math.PI * seg(p, 0.598, 0.616)) * 0.02;
        li.style.opacity = fmt(i === 0 ? a * (1 - liftC * 0.35) : a, 3);
        li.style.transform = `translate3d(${fmt(x, 1)}px,${fmt(y, 1)}px,0) scale(${fmt(sc, 4)})`;
      });
      const act = swapped ? 1 : 0;
      if (act !== this.lastActive) {
        this.opts.forEach((li, i) => li.classList.toggle('is-active', i === act));
        this.opts[0].querySelector('.vi-n').textContent = swapped ? '2' : '1';
        this.opts[1].querySelector('.vi-n').textContent = swapped ? '1' : '2';
        this.lastActive = act;
      }

      // run, then wipe from original to result while the size ticks down
      const runPress = Math.sin(Math.PI * seg(p, 0.725, 0.745));
      this.runBtn.style.transform = `scale(${fmt(1 - runPress * 0.06, 4)})`;
      this.prog.style.transform = `scaleX(${fmt(E.io2(seg(p, 0.735, 0.81)), 4)})`;
      const wf = E.io3(seg(p, 0.76, 0.89));
      const back = E.io3(seg(p, 0.9, 0.96));
      const f = wf - back * 0.5;
      const showWipe = p > 0.755;
      this.pResult.style.clipPath = `inset(0 0 0 ${fmt((1 - f) * 100, 2)}%)`;
      this.pResult.style.opacity = showWipe ? 1 : 0;
      this.wipe.style.opacity = fmt(showWipe ? Math.min(1, seg(p, 0.755, 0.77)) : 0, 3);
      this.wipe.style.left = fmt((1 - f) * 100, 2) + '%';
      const mb = 4.2 - 3.1 * wf;
      this.size.textContent = 'WebP · ' + mb.toFixed(1) + ' MB';
      this.delta.style.opacity = fmt(seg(p, 0.875, 0.9), 3);

      // the cursor
      let cx = 0, cy = 0, co = 0;
      if (p >= T0 && p < 0.31) {
        cx = rect.x + rect.w * 0.62; cy = rect.y + rect.h * 0.55 - lift * 14; co = E.smooth(seg(p, T0, T0 + 0.02)) * (1 - E.smooth(seg(p, 0.285, 0.31)));
      } else if (p >= 0.555 && p < 0.655) {
        const t = E.io3(seg(p, 0.56, 0.595));
        const li = this.optPos[0];
        const tx = li.x + Math.min(li.w * 0.55, 220), ty = li.y + li.h * 0.55;
        cx = lerp(tx + 120, tx, t); cy = lerp(ty + 140, ty, t);
        co = E.smooth(seg(p, 0.555, 0.57)) * (1 - E.smooth(seg(p, 0.635, 0.655)));
      } else if (p >= 0.68 && p < 0.79) {
        const t = E.io3(seg(p, 0.685, 0.72));
        const tx = this.runPos.x + this.runPos.w * 0.6, ty = this.runPos.y + this.runPos.h * 0.6;
        cx = lerp(tx - 140, tx, t); cy = lerp(ty + 160, ty, t);
        co = E.smooth(seg(p, 0.68, 0.695)) * (1 - E.smooth(seg(p, 0.77, 0.79)));
      }
      this.cursor.style.opacity = fmt(co, 3);
      this.cursor.style.transform = `translate3d(${fmt(cx, 1)}px,${fmt(cy, 1)}px,0)`;

      this.caps(p);
      return false;
    }
  }

  /* ---------------------------------------------------------------- scene 2: four ways in */
  class Ways {
    constructor(root, opts = {}) {
      this.root = root; this.still = !!opts.still;
      this.stage = $('.ways-stage', root);
      this.fit = $('.ways-fit', root);
      this.wrap = $('.layers', root);
      this.caps = capsCtl(this.still ? null : $('.side-caps', root));
      this.order = ['app', 'cli', 'http', 'mcp'];
      this.L = {};
      for (const k of ['spec', ...this.order]) {
        const el = $('.layer-' + k, root);
        this.L[k] = { el, body: $$(':scope > :not(.lh)', el) };
      }
      this.typers = $$('.type', root).map((el) => {
        const full = el.dataset.text;
        let ghost = el.parentNode.querySelector('.ghost');
        if (!ghost) {
          ghost = document.createElement('span');
          ghost.className = 'ghost';
          ghost.style.visibility = 'hidden';
          (el.nextElementSibling || el).after(ghost);
        }
        el.textContent = '';
        return { el, ghost, caret: el.nextElementSibling, full, n: -1 };
      });
    }
    layout() {
      const sw = this.stage.clientWidth, sh = this.stage.clientHeight;
      this.narrow = sw < 600;
      this.root.classList.toggle('narrow', this.narrow);
      const dw = this.narrow ? 330 : 640, dh = this.narrow ? 520 : 470;
      this.scale = Math.min(sw / dw, sh / dh, this.narrow ? 1.1 : 1.12) * 0.94;
      this.fit.style.transform = `translate(-50%,-50%) scale(${fmt(this.scale, 4)})`;
      this.gap = this.narrow ? 40 : 54;
    }
    type(t, f) {
      const n = Math.round(t.full.length * clamp(f));
      if (n === t.n) return;
      t.n = n;
      t.el.textContent = t.full.slice(0, n);
      t.ghost.textContent = t.full.slice(n);
    }
    render(p) {
      const N = this.narrow;
      const arrive = { app: -1, cli: 0.165, http: 0.385, mcp: 0.605 };
      const ex = E.io3(seg(p, 0.8, 0.94));
      const tilt = ex * (N ? 40 : 44);
      this.wrap.style.transform = `translateY(${fmt(ex * (N ? 20 : 34), 1)}px) rotateX(${fmt(tilt, 2)}deg)`;
      const got = {};
      for (const k of this.order) got[k] = arrive[k] < 0 ? 1 : E.out3(seg(p, arrive[k], arrive[k] + 0.09));
      const strip = N ? 40 : 46, zStep = N ? 50 : 62;
      this.order.forEach((k, idx) => {
        let depth = 0;
        for (let j = idx + 1; j < this.order.length; j++) depth += got[this.order[j]];
        const a = got[k];
        const { el, body } = this.L[k];
        // stacked: newest in front, earlier layers step back and up
        const sx = (1 - a) * (N ? 300 : 560);
        const sy = -depth * this.gap;
        const sz = -depth * (N ? 70 : 90);
        const ry = (1 - a) * -30;
        // exploded: every layer becomes a thin plate above the description
        const lvl = idx + 1;
        const x = lerp(sx, 0, ex), y = lerp(sy, N ? 10 : 30, ex), z = lerp(sz, lvl * zStep, ex);
        el.style.transform = `translate3d(${fmt(x, 1)}px,${fmt(y, 1)}px,${fmt(z, 1)}px) rotateY(${fmt(ry * (1 - ex), 2)}deg)`;
        el.style.opacity = fmt(Math.min(1, a * 1.5), 3);
        el.style.setProperty('--fog', fmt(Math.min(0.5, depth * 0.2) * (1 - ex), 3));
        el.style.zIndex = String(10 + idx);
        el.style.clipPath = ex > 0 ? `inset(0 0 calc(${fmt(100 * ex, 2)}% - ${fmt(strip * ex, 1)}px) 0 round 16px)` : '';
        const bo = fmt(1 - E.smooth(seg(ex, 0, 0.6)), 3);
        for (const b of body) b.style.opacity = bo;
      });
      const sp = this.L.spec.el;
      sp.style.transform = `translate3d(0,${fmt(lerp(N ? 60 : 90, N ? 10 : 30, ex), 1)}px,${fmt(lerp(-120, 0, ex), 1)}px)`;
      sp.style.opacity = fmt(E.smooth(seg(p, 0.8, 0.88)), 3);
      sp.style.zIndex = '5';
      // typing
      const [cli, http, mcp] = this.typers;
      this.type(cli, seg(p, 0.215, 0.33));
      this.type(http, seg(p, 0.435, 0.56));
      this.type(mcp, seg(p, 0.655, 0.77));
      cli.caret.style.opacity = p > 0.2 && p < 0.39 ? 1 : 0;
      http.caret.style.opacity = p > 0.42 && p < 0.61 ? 1 : 0;
      mcp.caret.style.opacity = p > 0.64 && p < 0.8 ? 1 : 0;
      this.caps(p);
      return false;
    }
  }

  /* ---------------------------------------------------------------- scene 3: nothing leaves */
  class Priv {
    constructor(root, opts = {}) {
      this.root = root; this.still = !!opts.still;
      this.stage = $('.priv-stage', root);
      this.fit = $('.priv-fit', root);
      this.caps = capsCtl(this.still ? null : $('.side-caps', root));
      this.box = $('.pv-box', root); this.lid = $('.pv-lid', root);
      this.file = $('.pv-file', root); this.wall = $('.pv-wall', root);
      this.dash = $('.pv-dash', root); this.stop = $('.pv-stop', root); this.zero = $('.pv-zero', root);
      this.cloud = $('.pv-cloud', root); this.pv = $('.pv', root); this.seal = $('.pv-seal', root);
    }
    layout() {
      const sw = this.stage.clientWidth, sh = this.stage.clientHeight;
      this.narrow = sw < 600;
      this.root.classList.toggle('narrow', this.narrow);
      const dw = this.narrow ? 340 : 960, dh = this.narrow ? 600 : 460;
      this.scale = Math.min(sw / dw, sh / dh, 1.1) * 0.94;
      this.fit.style.transform = `translate(-50%,-50%) scale(${fmt(this.scale, 4)})`;
      const b = offsetIn(this.box, this.pv), w = offsetIn(this.wall, this.pv);
      this.fw = this.file.offsetWidth; this.fh = this.file.offsetHeight;
      this.bc = { x: b.x + b.w / 2, y: b.y + b.h * 0.6 };
      this.exit = this.narrow ? { x: this.bc.x, y: b.y + b.h * 0.95 + this.fh / 2 } : { x: b.x + b.w + this.fw / 2 - 10, y: b.y + b.h * 0.42 };
      this.stopAt = this.narrow ? { x: this.bc.x, y: w.y - this.fh / 2 - 5 } : { x: w.x - this.fw / 2 - 5, y: this.exit.y };
      this.above = { x: this.bc.x, y: b.y - this.fh * 0.2 };
    }
    render(p) {
      if (!this.fw) return false;
      const N = this.narrow;
      // out of the box, along the line, into the wall, and back
      const lidOpen = E.out3(seg(p, 0.1, 0.17)) * (1 - E.io3(seg(p, 0.2, 0.26))) + E.out3(seg(p, 0.6, 0.66)) * (1 - E.io3(seg(p, 0.72, 0.8)));
      this.lid.style.transform = `translate3d(0,${fmt(-lidOpen * 10, 1)}px,0) rotate(${fmt(-lidOpen * 24, 2)}deg)`;
      const up = E.out3(seg(p, 0.12, 0.2));
      const go = E.in3(seg(p, 0.24, 0.44));
      const hit = seg(p, 0.44, 0.47);
      const ret = E.io3(seg(p, 0.48, 0.64));
      const drop = E.io3(seg(p, 0.65, 0.72));
      let x, y, s = 1, sx = 1, sy = 1, rot = 0, o = 1;
      if (p < 0.24) {
        const t = up;
        x = lerp(this.bc.x, this.above.x, t); y = lerp(this.bc.y, this.above.y, t);
        const t2 = E.io3(seg(p, 0.19, 0.24));
        x = lerp(x, this.exit.x, t2); y = lerp(y, this.exit.y, t2);
        s = lerp(0.55, 1, up); o = E.smooth(seg(p, 0.12, 0.15)); rot = lerp(-6, 0, t2);
      } else if (p < 0.48) {
        x = lerp(this.exit.x, this.stopAt.x, go); y = lerp(this.exit.y, this.stopAt.y, go);
        const sq = Math.sin(Math.PI * hit);
        if (N) { sy = 1 - sq * 0.14; sx = 1 + sq * 0.08; y -= sq * this.fh * 0.07; } else { sx = 1 - sq * 0.14; sy = 1 + sq * 0.08; x -= sq * this.fw * 0.07; }
      } else if (p < 0.65) {
        x = lerp(this.stopAt.x, this.above.x, ret); y = lerp(this.stopAt.y, this.above.y, ret) - Math.sin(Math.PI * ret) * (N ? 0 : 50);
        if (N) x += Math.sin(Math.PI * ret) * 60;
        rot = Math.sin(Math.PI * ret) * 6;
      } else {
        x = this.above.x; y = lerp(this.above.y, this.bc.y, drop); s = lerp(1, 0.55, drop); o = 1 - E.smooth(seg(p, 0.68, 0.72));
      }
      this.file.style.opacity = fmt(o, 3);
      this.file.style.transform = `translate3d(${fmt(x - this.fw / 2, 1)}px,${fmt(y - this.fh / 2, 1)}px,0) rotate(${fmt(rot, 2)}deg) scale(${fmt(s * sx, 4)},${fmt(s * sy, 4)})`;
      const flash = Math.max(Math.sin(Math.PI * seg(p, 0.43, 0.56)), seg(p, 0.56, 0.62) * 0.35 + (p > 0.62 ? 0.35 : 0) * (1 - seg(p, 0.8, 0.9)));
      this.wall.style.opacity = fmt(flash, 3);
      this.wall.style.transform = N ? `scaleY(${fmt(1 + Math.sin(Math.PI * hit) * 1.2, 3)})` : `scaleX(${fmt(1 + Math.sin(Math.PI * hit) * 1.2, 3)})`;
      const st = E.outBack(seg(p, 0.45, 0.5), 2) * (1 - seg(p, 0.78, 0.84));
      this.stop.style.opacity = fmt(clamp(st), 3);
      this.stop.style.transform = `translate3d(0,${fmt((1 - clamp(st)) * 10, 1)}px,0) scale(${fmt(0.9 + 0.1 * st, 3)})`;
      const dir = p < 0.48 ? 1 : -1;
      const off = (p * 600) * dir;
      this.dash.style.backgroundPosition = N ? `0 ${fmt(off % 16, 1)}px` : `${fmt(off % 16, 1)}px 0`;
      this.dash.style.opacity = fmt(0.55 * (1 - E.smooth(seg(p, 0.74, 0.86))) + 0.15, 3);
      this.zero.style.opacity = fmt(E.smooth(seg(p, 0.8, 0.88)), 3);
      this.seal.style.opacity = fmt(E.smooth(seg(p, 0.445, 0.48)) * (1 - 0.55 * E.smooth(seg(p, 0.8, 0.92))), 3);
      this.caps(p);
      return false;
    }
  }

  /* ---------------------------------------------------------------- closing: everything back in the box */
  class Close {
    constructor(root, opts = {}) {
      this.root = root;
      this.stage = $('.close-stage', root);
      this.v3d = $('.v3d', this.stage);
      this.world = $('.world', this.stage);
      this.copy = $('.close-copy', root);
      buildBox(this.world, { n: innerWidth < 760 ? 3 : 5 });
      this.b = bindBox(this.world);
    }
    layout() {
      const W = this.stage.clientWidth, H = this.stage.clientHeight;
      this.W = W; this.H = H; this.narrow = W < 760;
      this.base = this.narrow ? Math.min(W / 520, H / 880) : clamp(Math.min(W / 1440, H / 900), 0.5, 1.15);
      this.keys = this.narrow ? [
        [0.0, { x: 0, y: 0.3, rx: -18, ry: -24, s: 0.78 }],
        [0.4, { x: 0, y: 0.22, rx: -20, ry: -28, s: 0.92 }],
        [0.62, { x: 0, y: 0.2, rx: -14, ry: -22, s: 0.95 }],
        [0.9, { x: 0, y: 0.2, rx: 0, ry: 0, s: 0.95 }],
        [1.0, { x: 0, y: 0.2, rx: 0, ry: 0, s: 0.95 }],
      ] : [
        [0.0, { x: 0.18, y: 0.2, rx: -18, ry: -24, s: 0.86 }],
        [0.4, { x: 0.2, y: 0.08, rx: -20, ry: -28, s: 1.02 }],
        [0.62, { x: 0.21, y: 0.04, rx: -14, ry: -22, s: 1.08 }],
        [0.9, { x: 0.21, y: 0.02, rx: 0, ry: 0, s: 1.12 }],
        [1.0, { x: 0.21, y: 0.02, rx: 0, ry: 0, s: 1.12 }],
      ];
    }
    render(p) {
      if (!this.W) return false;
      const W = this.W, H = this.H;
      const k = track(p, this.keys);
      const cam = { x: k.x * W, y: k.y * H, z: 0, rx: k.rx, ry: k.ry, s: k.s * this.base };
      this.world.style.transform = worldTf(cam);
      this.v3d.style.perspectiveOrigin = `${fmt(W / 2 + cam.x, 1)}px ${fmt(H / 2 + cam.y, 1)}px`;
      this.b.drawers.forEach((dr, i) => {
        const r = (i / 2) | 0;
        const cs = cabSlot(i);
        const sink = E.io3(seg(p, 0.04 + (5 - r) * 0.04, 0.16 + (5 - r) * 0.04));
        setDrawer(dr, cs.x, lerp(cs.y, YS + 70, sink), 0, 0, 0, 0, CAB_S);
        setDrawerOpacity(dr, sink > 0.999 ? 0 : 1);
      });
      const close = E.io3(seg(p, 0.38, 0.56));
      const settle = Math.sin(Math.PI * seg(p, 0.56, 0.6)) * 2.5;
      setLid(this.b, 112 * (1 - close) + settle);
      setLatch(this.b, 168 * (1 - E.io3(seg(p, 0.57, 0.66))));
      const g = 1 - E.smooth(seg(p, 0.4, 0.56));
      this.b.well.style.setProperty('--g', fmt(0.25 + g * 0.75, 3));
      this.b.floorglow.style.opacity = fmt(g * 0.7, 3);
      const c = E.out3(seg(p, 0.62, 0.8));
      this.copy.style.opacity = fmt(c, 3);
      this.copy.style.transform = this.narrow ? `translate3d(0,${fmt((1 - c) * 24, 1)}px,0)` : `translate3d(0,calc(-50% + ${fmt((1 - c) * 24, 1)}px),0)`;
      return false;
    }
  }

  /* ---------------------------------------------------------------- scroll engine */
  const scenes = [];
  const nav = $('#nav');
  let hero = null;
  let vh = innerHeight, vw = innerWidth;

  function measure() {
    vh = innerHeight; vw = innerWidth;
    for (const s of scenes) {
      const r = s.el.getBoundingClientRect();
      s.top = r.top + scrollY; s.h = s.el.offsetHeight;
      s.inst.layout();
      s.cur = -1;
    }
  }
  let raf = 0, lastT = 0;
  function kick() { if (!raf) raf = requestAnimationFrame(frame); }
  function frame(now) {
    raf = 0;
    const dt = lastT ? Math.min(0.05, (now - lastT) / 1000) : 1 / 60;
    lastT = now;
    const y = scrollY;
    let active = false;
    const k = 1 - Math.exp(-dt * 11);
    for (const s of scenes) {
      const span = Math.max(1, s.h - vh);
      const raw = (y - s.top) / span;
      s.target = clamp(raw);
      if (s.cover) {
        const on = raw >= -0.0005;
        if (on !== s.on) { s.el.classList.toggle('is-on', on); s.on = on; }
      }
      const visible = y + vh > s.top - 2 && y < s.top + s.h + 2;
      if (!visible) { s.vis = false; continue; }
      if (!s.vis || s.cur < 0) { s.cur = s.target; s.vis = true; }
      else {
        s.cur += (s.target - s.cur) * k;
        if (Math.abs(s.target - s.cur) < 0.00015) s.cur = s.target; else active = true;
      }
      if (s.inst.render(s.cur, now / 1000, dt)) active = true;
    }
    nav.classList.toggle('is-scrolled', y > 8);
    const heroVis = hero && y < hero.top + hero.h - vh * 0.5;
    nav.classList.toggle('on-dark', !!(heroVis && hero.inst.navDark));
    if (active) kick(); else lastT = 0;
  }

  /* ---------------------------------------------------------------- reduced motion: the same story as still frames */
  function stills(sceneEl, Cls, list, { header = true, cls = '' } = {}) {
    const sticky = $('.sticky', sceneEl);
    const stage = $('.stage', sticky);
    const capBox = $('.caps', sticky);
    const caps = capBox ? $$('.cap', capBox) : [];
    const wrap = document.createElement('div');
    wrap.className = 'wrap stills ' + cls;
    if (header && caps[0]) {
      const head = document.createElement('div');
      head.className = 'stills-head';
      while (caps[0].firstChild) head.appendChild(caps[0].firstChild);
      wrap.appendChild(head);
    }
    const made = [];
    for (const st of list) {
      const fig = document.createElement('figure');
      fig.className = 'still';
      const cap = document.createElement('figcaption');
      st.caps.forEach((i) => {
        const c = caps[i];
        if (!c) return;
        c.style.opacity = ''; c.style.transform = ''; c.style.visibility = '';
        cap.appendChild(c);
      });
      const frame = document.createElement('div');
      frame.className = 'still-frame';
      frame.setAttribute('aria-hidden', 'true');
      frame.appendChild(stage.cloneNode(true));
      fig.append(cap, frame);
      wrap.appendChild(fig);
      made.push([frame, st.p]);
    }
    sceneEl.appendChild(wrap);
    for (const [frame, p] of made) {
      const inst = new Cls(frame, { still: true, built: true });
      inst.layout();
      inst.render(p, 0, 0);
    }
  }

  /* ---------------------------------------------------------------- all 249: pour out of the drawers */
  function setupTools() {
    const cards = $$('.dcard');
    const counter = $('#pour-count');
    let landed = 0;
    const total = cards.reduce((n, c) => n + c.querySelectorAll('li').length, 0);
    const poured = new WeakSet();
    if (!RM && 'IntersectionObserver' in window && Element.prototype.animate) {
      counter.textContent = `0 of ${total} out of the drawers`;
      cards.forEach((c) => c.querySelectorAll('li').forEach((li) => { li.style.opacity = '0'; }));
      const io = new IntersectionObserver((entries) => {
        for (const en of entries) {
          if (!en.isIntersecting || poured.has(en.target)) continue;
          poured.add(en.target);
          io.unobserve(en.target);
          pour(en.target);
        }
      }, { threshold: 0.12, rootMargin: '0px 0px -8% 0px' });
      cards.forEach((c) => io.observe(c));
    }
    function pour(card) {
      const face = $('.dc-face', card);
      const items = $$('li', card);
      const idx = cards.indexOf(card) % 4;
      const delay0 = idx * 110;
      face.animate([
        { transform: 'none' },
        { transform: 'translateY(-5px) rotateX(14deg)', offset: 0.35 },
        { transform: 'translateY(1px)', offset: 0.75 },
        { transform: 'none' },
      ], { duration: 760, delay: delay0, easing: 'cubic-bezier(.3,.6,.2,1)' });
      const fr = face.getBoundingClientRect();
      const mx = fr.left + fr.width / 2, my = fr.top + fr.height * 0.75;
      items.forEach((li, i) => {
        const r = li.getBoundingClientRect();
        const dx = mx - (r.left + r.width / 2) + (Math.random() - 0.5) * 40;
        const dy = my - (r.top + r.height / 2);
        const rot = (Math.random() - 0.5) * 26;
        li.style.opacity = '';
        const a = li.animate([
          { transform: `translate(${fmt(dx)}px,${fmt(dy)}px) rotate(${fmt(rot)}deg) scale(.55)`, opacity: 0, easing: 'cubic-bezier(.45,0,.85,.5)' },
          { transform: `translate(${fmt(dx * 0.35)}px,${fmt(dy * 0.4)}px) rotate(${fmt(rot * 0.6)}deg) scale(.85)`, opacity: 1, offset: 0.4, easing: 'cubic-bezier(.2,.7,.3,1)' },
          { transform: 'translate(0,2px) rotate(0deg) scale(1)', opacity: 1, offset: 0.82, easing: 'ease-out' },
          { transform: 'none', opacity: 1 },
        ], { duration: 820, delay: delay0 + 160 + i * 26, fill: 'backwards' });
        a.finished.then(() => {
          landed++;
          counter.textContent = landed >= total ? `${total} tools in 12 drawers. Every one runs on your machine.` : `${landed} of ${total} out of the drawers`;
        }).catch(() => {});
      });
    }
    // find
    const input = $('#find'), status = $('#find-status');
    const norm = (s) => s.toLowerCase().normalize('NFKD').replace(/[^\w\s.&/-]/g, '').trim();
    const data = cards.map((c) => ({
      c, cat: norm($('.dc-name', c).textContent), n: $('.dc-n', c), N: c.querySelectorAll('li').length,
      items: $$('li', c).map((li) => ({ li, t: norm(li.textContent) })),
    }));
    input.addEventListener('input', () => {
      const q = norm(input.value);
      let hits = 0;
      for (const d of data) {
        const catHit = q && d.cat.includes(q);
        let n = 0;
        for (const it of d.items) {
          const hit = !q || catHit || it.t.includes(q);
          if (hit) n++;
          it.li.classList.toggle('is-dim', !!q && !hit);
          it.li.classList.toggle('is-hit', !!q && hit);
        }
        hits += q ? n : 0;
        d.n.textContent = q ? `${n}/${d.N}` : String(d.N);
        d.c.classList.toggle('is-empty', !!q && n === 0);
      }
      status.textContent = q ? (hits ? `${hits} ${hits === 1 ? 'tool matches' : 'tools match'} “${input.value.trim()}”` : `Nothing matches “${input.value.trim()}” yet`) : '';
    });
  }

  /* ---------------------------------------------------------------- agents: keys open drawers */
  function setupAgents() {
    const tansu = $('#tansu');
    const slugs = ['image', 'video', 'audio', 'speech', 'pdf', 'files', 'text', 'dev', 'data', 'network', 'design', 'system'];
    tansu.innerHTML = CATS.map(([n, c], i) => `<div class="td" data-cat="${slugs[i]}"><span>${n.replace('&', '&amp;')}</span><span>${c}</span></div>`).join('');
    const tds = $$('.td', tansu);
    const keys = $$('.key');
    const status = $('#key-status');
    const text = {
      image: 'image-bot can open 1 of 12 drawers: Images, without Extract Text (OCR).',
      'video audio speech': 'media-bot can open 3 of 12 drawers: Video, Audio, and Speech & Voice.',
      all: 'The owner can open all 12 drawers, and decides what every other key opens.',
    };
    function pick(btn) {
      const allow = btn.dataset.allow;
      keys.forEach((k) => k.setAttribute('aria-pressed', String(k === btn)));
      tansu.classList.add('has-key');
      tds.forEach((td) => td.classList.toggle('is-open', allow === 'all' || allow.split(' ').includes(td.dataset.cat)));
      status.textContent = text[allow] || '';
    }
    let user = false, i = 0, timer = 0, inView = false;
    keys.forEach((k) => k.addEventListener('click', () => { user = true; clearInterval(timer); pick(k); }));
    pick(keys[0]);
    if (RM || !('IntersectionObserver' in window)) return;
    const cycle = () => { if (user || document.hidden || !inView) return; i = (i + 1) % keys.length; pick(keys[i]); };
    new IntersectionObserver((es) => {
      inView = es[0].isIntersecting;
      clearInterval(timer);
      if (inView && !user) timer = setInterval(cycle, 3200);
    }, { threshold: 0.35 }).observe(tansu);
  }

  /* ---------------------------------------------------------------- theme, trailer, intro */
  function setupTheme() {
    const btn = $('#theme-toggle');
    const sync = () => btn.setAttribute('aria-label', doc.dataset.theme === 'dark' ? 'Use light theme' : 'Use dark theme');
    sync();
    btn.addEventListener('click', () => {
      doc.dataset.theme = doc.dataset.theme === 'dark' ? 'light' : 'dark';
      try { localStorage.setItem('dgb-otb-theme', doc.dataset.theme); } catch (e) {}
      sync();
      $$('meta[name="theme-color"]').forEach((m) => m.setAttribute('content', doc.dataset.theme === 'dark' ? '#151412' : '#f7f6f3'));
    });
  }
  function setupTrailer() {
    const fig = $('#trailer');
    const src = fig && fig.dataset.src;
    if (!src) return;
    const v = document.createElement('video');
    v.controls = true; v.playsInline = true; v.preload = 'none'; v.src = src;
    v.setAttribute('aria-label', 'Dogubako alpha trailer');
    v.addEventListener('loadedmetadata', () => { const ph = $('.trailer-ph', fig); if (ph) ph.remove(); });
    fig.appendChild(v);
  }
  function introCopy() {
    if (RM || !Element.prototype.animate) return;
    const els = $$('.hero-copy > *');
    els.forEach((el, i) => el.animate([
      { opacity: 0, transform: 'translate3d(0,22px,0)' },
      { opacity: 1, transform: 'none' },
    ], { duration: 1100, delay: 280 + i * 110, easing: 'cubic-bezier(.16,1,.3,1)', fill: 'backwards' }));
    $$('.scroll-cue > *').forEach((el) => el.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 900, delay: 1600, fill: 'backwards' }));
  }

  /* ---------------------------------------------------------------- start */
  function start() {
    setupTheme();
    setupTools();
    setupAgents();
    setupTrailer();
    const heroEl = $('.hero'), appEl = $('.app-scene'), waysEl = $('.ways-scene'), privEl = $('.priv-scene'), closeEl = $('.close-scene');
    if (RM) {
      const h = new Hero(heroEl);
      h.layout(); h.render(0, 0, 0);
      stills(heroEl, Hero, [{ p: 0.43, caps: [0, 1] }, { p: 0.6, caps: [2] }, { p: 0.82, caps: [3] }], { header: false, cls: 'hero-stills' });
      stills(appEl, App, [{ p: 0.312, caps: [1] }, { p: 0.57, caps: [2] }, { p: 0.735, caps: [3] }, { p: 1, caps: [4] }], { cls: 'app-stills' });
      stills(waysEl, Ways, [{ p: 0.34, caps: [1] }, { p: 0.56, caps: [2] }, { p: 0.78, caps: [3] }, { p: 1, caps: [4] }], { cls: 'ways-stills' });
      stills(privEl, Priv, [{ p: 0.46, caps: [1] }, { p: 1, caps: [2] }], { cls: 'priv-stills' });
      const c = new Close(closeEl); c.layout(); c.render(1);
      const relayout = () => { h.layout(); h.render(0, 0, 0); c.layout(); c.render(1); };
      addEventListener('resize', relayout);
      addEventListener('scroll', () => nav.classList.toggle('is-scrolled', scrollY > 8), { passive: true });
      return;
    }
    const add = (el, inst) => { const s = { el, inst, top: 0, h: 0, cur: -1, vis: false }; scenes.push(s); return s; };
    hero = add(heroEl, new Hero(heroEl));
    add(appEl, new App(appEl)).cover = true;
    add(waysEl, new Ways(waysEl));
    add(privEl, new Priv(privEl));
    add(closeEl, new Close(closeEl));
    measure();
    introCopy();
    addEventListener('scroll', kick, { passive: true });
    let rt = 0, lastW = innerWidth, lastH = innerHeight;
    addEventListener('resize', () => {
      clearTimeout(rt);
      rt = setTimeout(() => {
        // Phones resize the viewport when the address bar slides; only re-measure for real changes.
        if (innerWidth === lastW && Math.abs(innerHeight - lastH) < 120) { vh = innerHeight; kick(); return; }
        lastW = innerWidth; lastH = innerHeight;
        measure(); kick();
      }, 120);
    });
    document.addEventListener('visibilitychange', () => { if (!document.hidden) { lastT = 0; kick(); } });
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(() => { measure(); kick(); });
    addEventListener('load', () => { measure(); kick(); });
    kick();
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
