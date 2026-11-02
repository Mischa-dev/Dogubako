/* timeline.js: a small keyframe-track timeline.
 *
 * Every visual state is a pure function of time t: render(t) evaluates each track and writes
 * only transform / opacity (plus the odd text or class) to the DOM. Pausing is "stop advancing t",
 * seeking is "render(t)", looping is "t mod duration", and a reduced-motion still is "render(a
 * chosen t) once". No CSS transitions run inside a timeline, so nothing drifts out of sync.
 *
 *   const tl = new DGB.Timeline(9000);
 *   tl.to(el, 'o', [[0, 0], [400, 1, 'out']]);        // opacity 0 -> 1 over 400ms
 *   tl.path(cursor, [[0, [10, 10]], [800, [300, 120], 'inOut', 0.2]]);
 *   tl.text(label, [[0, ''], [1200, 'Done']]);
 *   tl.render(t);
 */
(function (g) {
  'use strict';

  function bezier(x1, y1, x2, y2) {
    const cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx;
    const cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
    const sx = (t) => ((ax * t + bx) * t + cx) * t;
    const sy = (t) => ((ay * t + by) * t + cy) * t;
    const dx = (t) => (3 * ax * t + 2 * bx) * t + cx;
    return (x) => {
      if (x <= 0) return 0;
      if (x >= 1) return 1;
      let t = x;
      for (let i = 0; i < 8; i++) {
        const e = sx(t) - x;
        if (Math.abs(e) < 1e-6) break;
        const d = dx(t);
        if (Math.abs(d) < 1e-6) break;
        t -= e / d;
      }
      if (t < 0 || t > 1) { // bisection fallback
        let lo = 0, hi = 1; t = x;
        for (let i = 0; i < 24; i++) { if (sx(t) < x) lo = t; else hi = t; t = (lo + hi) / 2; }
      }
      return sy(t);
    };
  }

  const EASE = {
    lin: (t) => t,
    ease: bezier(0.2, 0, 0, 1),      // the app's --ease
    out: bezier(0.16, 1, 0.3, 1),    // long, soft landing
    inOut: bezier(0.65, 0, 0.35, 1), // pointer travel
    in: bezier(0.55, 0, 0.75, 0.2),
    back: bezier(0.3, 1.45, 0.55, 1),
    hold: (t) => (t < 1 ? 0 : 1),
  };
  const ez = (e) => (typeof e === 'function' ? e : EASE[e || 'ease'] || EASE.ease);

  function norm(keys) {
    return keys.map((k) => ({ t: k[0], v: k[1], e: ez(k[2]), b: k[3] || 0 })).sort((a, b) => a.t - b.t);
  }
  function sample(keys, t) {
    if (t <= keys[0].t) return keys[0].v;
    for (let i = 1; i < keys.length; i++) {
      const b = keys[i];
      if (t < b.t) {
        const a = keys[i - 1];
        const p = b.e((t - a.t) / (b.t - a.t));
        return a.v + (b.v - a.v) * p;
      }
    }
    return keys[keys.length - 1].v;
  }
  function sampleStep(keys, t) {
    let v = keys[0][1];
    for (let i = 0; i < keys.length; i++) { if (t >= keys[i][0]) v = keys[i][1]; else break; }
    return v;
  }

  const PROPS = ['x', 'y', 's', 'sx', 'sy', 'r', 'o'];
  const DEF = { x: 0, y: 0, s: 1, sx: 1, sy: 1, r: 0, o: 1 };

  class Timeline {
    constructor(duration) {
      this.d = duration;
      this.nodes = new Map();
      this.tracks = [];
    }
    node(el) {
      let n = this.nodes.get(el);
      if (!n) { n = { el, tf: null, op: null, vis: null, base: Object.assign({}, DEF) }; Object.assign(n, DEF); this.nodes.set(el, n); }
      return n;
    }
    /** Numeric keyframes on a transform/opacity channel: x, y (px), s, sx, sy (scale), r (deg), o (0..1). */
    to(el, prop, keys) {
      if (!el) return this;
      this.tracks.push({ k: 'n', n: this.node(el), prop, keys: norm(keys) });
      return this;
    }
    /** Move along points; each segment can bend (fraction of its length) for a natural pointer arc. */
    path(el, keys) {
      if (!el) return this;
      const ks = keys.map((k) => ({ t: k[0], p: k[1], e: ez(k[2] || 'inOut'), b: k[3] == null ? 0.12 : k[3] })).sort((a, b) => a.t - b.t);
      this.tracks.push({ k: 'p', n: this.node(el), keys: ks });
      return this;
    }
    /** Numeric keyframes with a custom writer, called only when the rounded value changes. */
    fn(keys, apply, precision) {
      this.tracks.push({ k: 'f', keys: norm(keys), apply, q: precision || 1000, last: NaN });
      return this;
    }
    /** Discrete values: the value of the last key at or before t. */
    set(keys, apply) {
      const ks = keys.slice().sort((a, b) => a[0] - b[0]);
      this.tracks.push({ k: 's', keys: ks, apply, last: undefined, first: true });
      return this;
    }
    text(el, keys) { return el ? this.set(keys, (v) => { el.textContent = v; }) : this; }
    html(el, keys) { return el ? this.set(keys, (v) => { el.innerHTML = v; }) : this; }
    cls(el, c, keys) { return el ? this.set(keys, (v) => { el.classList.toggle(c, !!v); }) : this; }

    render(t) {
      for (const n of this.nodes.values()) { for (const p of PROPS) n[p] = n.base[p]; }
      for (const tr of this.tracks) {
        if (tr.k === 'n') {
          tr.n[tr.prop] = sample(tr.keys, t);
        } else if (tr.k === 'p') {
          const ks = tr.keys;
          let x, y;
          if (t <= ks[0].t) { x = ks[0].p[0]; y = ks[0].p[1]; }
          else if (t >= ks[ks.length - 1].t) { const l = ks[ks.length - 1]; x = l.p[0]; y = l.p[1]; }
          else {
            for (let i = 1; i < ks.length; i++) {
              if (t < ks[i].t) {
                const a = ks[i - 1], b = ks[i];
                const u = b.e((t - a.t) / (b.t - a.t));
                const ax = a.p[0], ay = a.p[1], bx = b.p[0], by = b.p[1];
                const mx = (ax + bx) / 2, my = (ay + by) / 2;
                const dx = bx - ax, dy = by - ay;
                const cxp = mx - dy * b.b, cyp = my + dx * b.b; // control point bent sideways
                const iu = 1 - u;
                x = iu * iu * ax + 2 * iu * u * cxp + u * u * bx;
                y = iu * iu * ay + 2 * iu * u * cyp + u * u * by;
                break;
              }
            }
          }
          tr.n.x = x; tr.n.y = y;
        } else if (tr.k === 'f') {
          const v = Math.round(sample(tr.keys, t) * tr.q) / tr.q;
          if (v !== tr.last) { tr.last = v; tr.apply(v); }
        } else {
          const v = sampleStep(tr.keys, t);
          if (tr.first || v !== tr.last) { tr.first = false; tr.last = v; tr.apply(v); }
        }
      }
      for (const n of this.nodes.values()) {
        const x = Math.round(n.x * 100) / 100, y = Math.round(n.y * 100) / 100;
        let tf = '';
        if (x || y) tf += `translate(${x}px, ${y}px)`;
        if (n.s !== 1) tf += ` scale(${Math.round(n.s * 10000) / 10000})`;
        if (n.sx !== 1 || n.sy !== 1) tf += ` scale(${Math.round(n.sx * 10000) / 10000}, ${Math.round(n.sy * 10000) / 10000})`;
        if (n.r) tf += ` rotate(${Math.round(n.r * 100) / 100}deg)`;
        if (tf !== n.tf) { n.tf = tf; n.el.style.transform = tf; }
        const o = Math.round(Math.min(1, Math.max(0, n.o)) * 1000) / 1000;
        if (o !== n.op) { n.op = o; n.el.style.opacity = String(o); }
        const vis = o === 0 ? 'hidden' : '';
        if (vis !== n.vis) { n.vis = vis; n.el.style.visibility = vis; }
      }
    }
    /** Forget cached writes, so the next render writes everything (after a rebuild or theme change). */
    invalidate() {
      for (const n of this.nodes.values()) { n.tf = null; n.op = null; n.vis = null; }
      for (const tr of this.tracks) { tr.last = tr.k === 'f' ? NaN : undefined; tr.first = true; }
    }
    clear() {
      for (const n of this.nodes.values()) { n.el.style.transform = ''; n.el.style.opacity = ''; n.el.style.visibility = ''; }
    }
  }

  /** Plays a timeline in a loop. Advances only while playing, on screen and with the tab visible. */
  class Player {
    constructor(opts) {
      this.render = opts.render;
      this.onTime = opts.onTime || (() => {});
      this.d = opts.duration;
      this.t = 0;
      this.playing = false;
      this.visible = true;
      this.raf = 0;
      this.prev = 0;
      this._tick = this._tick.bind(this);
      document.addEventListener('visibilitychange', () => this._sync());
    }
    get active() { return this.playing && this.visible && !document.hidden; }
    play() { this.playing = true; this._sync(); }
    pause() { this.playing = false; this._sync(); }
    setVisible(v) { this.visible = v; this._sync(); }
    seek(t) { this.t = ((t % this.d) + this.d) % this.d; this.render(this.t); this.onTime(this.t); }
    _sync() {
      if (this.active && !this.raf) { this.prev = performance.now(); this.raf = requestAnimationFrame(this._tick); }
      else if (!this.active && this.raf) { cancelAnimationFrame(this.raf); this.raf = 0; }
    }
    _tick(now) {
      this.raf = 0;
      if (!this.active) return;
      const dt = Math.min(50, now - this.prev); // never jump after a stall
      this.prev = now;
      this.t = (this.t + dt) % this.d;
      this.render(this.t);
      this.onTime(this.t);
      this.raf = requestAnimationFrame(this._tick);
    }
  }

  /** Runs a callback every frame while an element is on screen and the tab is visible. */
  function inViewLoop(el, cb, opts) {
    let on = false, raf = 0, t0 = 0, acc = 0, prev = 0;
    const tick = (now) => {
      raf = 0;
      if (!on || document.hidden) return;
      acc += Math.min(50, now - prev); prev = now;
      cb(acc);
      raf = requestAnimationFrame(tick);
    };
    const sync = () => {
      const want = on && !document.hidden;
      if (want && !raf) { prev = performance.now(); raf = requestAnimationFrame(tick); }
      if (!want && raf) { cancelAnimationFrame(raf); raf = 0; }
    };
    const io = new IntersectionObserver((es) => { for (const e of es) on = e.isIntersecting; sync(); }, { rootMargin: (opts && opts.margin) || '80px 0px' });
    io.observe(el);
    document.addEventListener('visibilitychange', sync);
    return { reset() { acc = 0; t0 = 0; } };
  }

  const reduceMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  g.DGB = Object.assign(g.DGB || {}, { Timeline, Player, EASE, bezier, inViewLoop, reduceMotion });
})(window);
