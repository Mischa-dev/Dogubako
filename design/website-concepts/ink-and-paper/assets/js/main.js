/* Dogubako · Ink and paper. Vanilla JS, no dependencies. */
(function () {
  'use strict';
  const root = document.documentElement;
  const RM = root.classList.contains('rm');
  const $ = (s, c) => (c || document).querySelector(s);
  const $$ = (s, c) => Array.from((c || document).querySelectorAll(s));
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const io = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);

  /* cubic-bezier easing, as CSS defines it */
  function bezier(x1, y1, x2, y2) {
    const cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx;
    const cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
    const sx = (t) => ((ax * t + bx) * t + cx) * t;
    const sy = (t) => ((ay * t + by) * t + cy) * t;
    const dx = (t) => (3 * ax * t + 2 * bx) * t + cx;
    return (x) => {
      if (x <= 0) return 0; if (x >= 1) return 1;
      let t = x;
      for (let i = 0; i < 6; i++) { const e = sx(t) - x, d = dx(t); if (Math.abs(e) < 1e-5 || Math.abs(d) < 1e-6) break; t -= e / d; }
      return sy(clamp(t));
    };
  }
  // how the brush travels: presses in, moves, settles (stops) or flies off (sweeps)
  const EASE = {
    h: bezier(0.5, 0, 0.32, 1), v: bezier(0.5, 0, 0.3, 1), t: bezier(0.45, 0, 0.3, 1),
    d: bezier(0.35, 0, 0.25, 1), p: bezier(0.42, 0.04, 0.7, 0.62), n: bezier(0.4, 0, 0.62, 0.8), z: bezier(0.45, 0, 0.4, 1),
  };

  /* ------------------------------------------------------------ theme + paper grain */
  function makeGrain(dark) {
    try {
      const S = 320, c = document.createElement('canvas');
      c.width = c.height = S;
      const x = c.getContext('2d');
      const img = x.createImageData(S, S), d = img.data;
      const [r, g, b] = dark ? [255, 248, 235] : [70, 55, 35];
      for (let i = 0; i < d.length; i += 4) {
        const n = Math.random();
        d[i] = r; d[i + 1] = g; d[i + 2] = b;
        d[i + 3] = n > 0.985 ? 30 + Math.random() * 30 : n * n * n * 16;
      }
      x.putImageData(img, 0, 0);
      // fibres: short, faint, slightly curved, wrapped at the tile edges
      x.lineCap = 'round';
      for (let k = 0; k < 90; k++) {
        const x0 = Math.random() * S, y0 = Math.random() * S, a = Math.random() * Math.PI * 2, L = 6 + Math.random() * 22;
        x.strokeStyle = 'rgba(' + r + ',' + g + ',' + b + ',' + (0.05 + Math.random() * 0.06) + ')';
        x.lineWidth = 0.4 + Math.random() * 0.5;
        for (const ox of [-S, 0, S]) for (const oy of [-S, 0, S]) {
          x.beginPath(); x.moveTo(x0 + ox, y0 + oy);
          x.quadraticCurveTo(x0 + ox + Math.cos(a + 0.6) * L * 0.6, y0 + oy + Math.sin(a + 0.6) * L * 0.6, x0 + ox + Math.cos(a) * L, y0 + oy + Math.sin(a) * L);
          x.stroke();
        }
      }
      root.style.setProperty('--grain', 'url(' + c.toDataURL('image/png') + ')');
    } catch (e) { /* no canvas: plain paper */ }
  }
  const themeBtn = $('.theme');
  function syncTheme() {
    const dark = root.dataset.theme === 'dark';
    themeBtn.setAttribute('aria-pressed', String(dark));
    makeGrain(dark);
  }
  themeBtn.addEventListener('click', () => {
    root.dataset.theme = root.dataset.theme === 'dark' ? 'light' : 'dark';
    try { localStorage.setItem('dgb-ink-theme', root.dataset.theme); } catch (e) {}
    syncTheme();
  });
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => {
    let stored = null; try { stored = localStorage.getItem('dgb-ink-theme'); } catch (er) {}
    if (!stored) { root.dataset.theme = e.matches ? 'dark' : 'light'; syncTheme(); }
  });
  syncTheme();

  /* ------------------------------------------------------------ nav */
  const nav = $('#nav');
  const navBrand = $('.nav .brand');

  /* ------------------------------------------------------------ stroke timelines
     One small scheduler for every brush-written thing on the page. */
  function strokePlan(strokes, opts) {
    const plan = [];
    let T = opts.start || 0, prev = null;
    const tempo = opts.tempo || 1;
    strokes.forEach((s) => {
      if (prev) T += (18 + Math.hypot(s.start[0] - prev[0], s.start[1] - prev[1]) * 0.55) / tempo;
      const dur = ((26 + s.L * 1.3) * (s.k === 'd' ? 1.5 : s.k === 'n' ? 1.15 : 1)) / tempo;
      plan.push({ el: s.el, len: s.len, t0: T, dur, ease: EASE[s.k] || EASE.h });
      T += dur; prev = s.end;
    });
    return { plan, end: T };
  }
  function applyPlan(plan, t) {
    let done = true;
    for (const it of plan) {
      const q = clamp((t - it.t0) / it.dur);
      if (q < 1) done = false;
      if (it.q === q) continue;
      it.q = q;
      it.el.setAttribute('stroke-dashoffset', (it.len * (1 - it.ease(q))).toFixed(2));
    }
    return done;
  }
  function wrapForInk(svg) {
    const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
    g.setAttribute('class', 'all');
    $$('g.st', svg).forEach((x) => g.appendChild(x));
    svg.appendChild(g);
  }

  /* ------------------------------------------------------------ hero */
  const hero = $('.hero');
  const glyphFigs = $$('.glyph', hero);
  const glyphs = glyphFigs.map((fig) => {
    const svg = $('svg', fig);
    const strokes = Brush.renderGlyph(svg, fig.dataset.ch, { drawn: RM });
    wrapForInk(svg);
    return { fig, strokes };
  });
  $$('.wordmark span').forEach((s, i) => s.style.setProperty('--i', i));

  // the seal: a rough vermilion square with 道具箱 carved out of it
  const seal = $('.seal');
  const SEAL_IN = 3; // red square spans 3..97 of the seal's 100 box
  if (seal) {
    const n = Brush.noise;
    let d = '';
    const P = [];
    const edge = (x0, y0, x1, y1, k) => { for (let i = 0; i < 24; i++) { const t = i / 24; P.push([x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, k * 24 + i]); } };
    const a = SEAL_IN + 1.6, b = 100 - SEAL_IN - 1.6;
    edge(a, SEAL_IN, b, SEAL_IN, 0); edge(100 - SEAL_IN, a, 100 - SEAL_IN, b, 1); edge(b, 100 - SEAL_IN, a, 100 - SEAL_IN, 2); edge(SEAL_IN, b, SEAL_IN, a, 3);
    P.forEach(([x, y, i], j) => {
      const j1 = 0.55 * n(i * 0.7, 5), j2 = 0.55 * n(i * 0.7, 9);
      d += (j ? 'L' : 'M') + (x + j1).toFixed(2) + ' ' + (y + j2).toFixed(2);
    });
    d += 'Z';
    const carve = (ch, tf) => '<g transform="' + tf + '">' + Brush.outlines(ch).map((o) => '<path d="' + o + '"/>').join('') + '</g>';
    $('.seal-svg', seal).innerHTML =
      '<g filter="url(#seal-tex)"><path class="seal-red" d="' + d + '"/>' +
      '<g class="seal-cut" stroke="var(--paper)" stroke-width="2.4" stroke-linejoin="round">' +
      carve('道', 'translate(53 9) scale(0.38)') + carve('具', 'translate(53 53) scale(0.38)') + carve('箱', 'translate(10 9) scale(0.38 0.82)') +
      '</g><path d="M50 9V91" stroke="var(--paper)" stroke-width="1.6" opacity="0.0"/></g>';
    const sp = $('.specks', seal);
    for (let i = 0; i < 9; i++) {
      const ang = (i / 9) * Math.PI * 2 + Math.random() * 0.5, dist = 54 + Math.random() * 30, sz = 1.4 + Math.pow(Math.random(), 2) * 4.2;
      const s = document.createElement('i');
      s.style.left = 50 + Math.cos(ang) * dist + '%';
      s.style.top = 50 + Math.sin(ang) * dist + '%';
      s.style.width = s.style.height = sz.toFixed(1) + 'px';
      s.dataset.dx = (Math.cos(ang) * 10).toFixed(1);
      s.dataset.dy = (Math.sin(ang) * 10).toFixed(1);
      sp.appendChild(s);
    }
  }

  function finishHero() {
    hero.classList.add('written', 'stamped', 'resolved', 'copy-in');
    glyphFigs.forEach((f) => f.classList.add('done'));
    navBrand.classList.add('on');
    if (seal) seal.style.display = 'none';
  }

  if (RM) {
    finishHero();
  } else {
    // schedule: 道 deliberately, then a little quicker, like a hand warming up
    let T = 320;
    const plans = [];
    const tempos = [0.95, 1.25, 1.5];
    glyphs.forEach((g, gi) => {
      const { plan, end } = strokePlan(g.strokes, { start: T, tempo: tempos[gi] });
      plans.push(...plan);
      g.doneAt = end;
      T = end + 170;
    });
    const sealAt = T;
    const events = [];
    const on = (t, fn) => events.push({ t, fn });
    glyphs.forEach((g) => on(g.doneAt + 60, () => g.fig.classList.add('done')));
    on(sealAt - 120, () => hero.classList.add('written'));

    let speed = 1;
    const anims = [];
    const anim = (elm, kf, o) => { const a = elm.animate(kf, Object.assign({ fill: 'forwards' }, o)); a.playbackRate = speed; anims.push(a); return a; };
    const SX = 0.18 / ((100 - 2 * SEAL_IN) / 100 * 0.94), SY = 0.14 / ((100 - 2 * SEAL_IN) / 100 * 0.94);

    on(sealAt, () => {
      if (!seal) return;
      anim(seal, [
        { opacity: 0, transform: 'translate(3%, -16%) scale(1.34) rotate(-9deg)' },
        { opacity: 1, offset: 0.35 },
        { opacity: 1, transform: 'translate(0, 0) scale(1) rotate(-2deg)' },
      ], { duration: 300, easing: 'cubic-bezier(.55,0,1,.55)' });
      anim($('.seal-svg', seal), [
        { filter: 'drop-shadow(0 22px 14px rgba(40,20,10,.22))' },
        { filter: 'drop-shadow(0 0 0 rgba(40,20,10,0))' },
      ], { duration: 300, easing: 'cubic-bezier(.55,0,1,.55)' });
    });
    on(sealAt + 300, () => {
      if (!seal) return;
      hero.classList.add('thud');
      anim($('.seal-svg', seal), [{ transform: 'scale(1)' }, { transform: 'scale(0.982)' }, { transform: 'scale(1)' }], { duration: 160, easing: 'ease-out' });
      anim($('.seal-dent', seal), [
        { opacity: 0, transform: 'scale(.78)' }, { opacity: 0.85, transform: 'scale(.96)', offset: 0.18 }, { opacity: 0, transform: 'scale(1.16)' },
      ], { duration: 1100, easing: 'cubic-bezier(.2,0,0,1)' });
      $$('.specks i', seal).forEach((s, i) => anim(s, [
        { transform: 'translate(' + -s.dataset.dx + 'px,' + -s.dataset.dy + 'px) scale(0)' },
        { transform: 'translate(0,0) scale(1)' },
      ], { duration: 170, delay: i * 9, easing: 'cubic-bezier(.2,.8,.3,1)' }));
    });
    const R = sealAt + 300 + 560; // the seal resolves into the latch
    on(R, () => {
      if (!seal) return;
      anim($('.seal-cut', seal), [{ opacity: 1 }, { opacity: 0 }], { duration: 260, easing: 'ease-out' });
      anim($('.specks', seal), [{ opacity: 1 }, { opacity: 0 }], { duration: 420, easing: 'ease-out' });
      anim(seal, [
        { opacity: 1, transform: 'translate(0, 0) scale(1) rotate(-2deg)' },
        { opacity: 1, transform: 'translate(0, 0) scale(' + SX.toFixed(4) + ', ' + SY.toFixed(4) + ') rotate(0deg)' },
      ], { duration: 640, delay: 120, easing: 'cubic-bezier(.7,0,.2,1)' });
    });
    on(R + 300, () => hero.classList.add('stamped'));
    on(R + 770, () => { hero.classList.add('resolved'); navBrand.classList.add('on'); if (seal) seal.style.visibility = 'hidden'; });
    on(sealAt + 440, () => hero.classList.add('copy-in'));

    let vt = 0, last = 0, ei = 0;
    events.sort((a, b) => a.t - b.t);
    const finalAt = events[events.length - 1].t;
    function tick(now) {
      const dt = last ? Math.min(64, now - last) : 16;
      last = now;
      vt += dt * speed;
      applyPlan(plans, vt);
      while (ei < events.length && events[ei].t <= vt) events[ei++].fn();
      if (vt < finalAt + 50) requestAnimationFrame(tick);
    }
    // fonts first, so nothing reflows under the brush
    const go = () => requestAnimationFrame(tick);
    (document.fonts && document.fonts.ready ? Promise.race([document.fonts.ready, new Promise((r) => setTimeout(r, 900))]) : Promise.resolve()).then(go);
    // anyone in a hurry gets the rest of the intro at five times the speed
    const hurry = () => {
      if (speed > 1) return;
      speed = 5; anims.forEach((a) => { a.playbackRate = 5; });
    };
    addEventListener('wheel', hurry, { passive: true, once: true });
    addEventListener('touchmove', hurry, { passive: true, once: true });
    addEventListener('keydown', (e) => { if (['ArrowDown', 'PageDown', ' ', 'End', 'Tab'].includes(e.key)) hurry(); }, { once: true });
  }

  /* ------------------------------------------------------------ emaki: the scroll */
  const emaki = $('.emaki');
  const track = $('.emaki-track');
  const ground = $('.emaki-ground');
  const scenes = $$('.scene', emaki).map((art) => {
    const def = window.Scenes[art.dataset.scene];
    const svg = $('.scene-art svg', art);
    const state = def.build(svg);
    return { art, artEl: $('.scene-art', art), def, state, p: -1 };
  });
  const railFill = $('.rail-fill');
  const railSteps = $$('.rail-steps li');
  const desktopMQ = matchMedia('(min-width: 900px)');
  let emakiDesk = false, emakiTop = 0, emakiDist = 0, trackW = 0;

  function setSceneP(sc, p) {
    p = Math.round(p * 1000) / 1000;
    if (sc.p === p) return;
    sc.p = p;
    sc.def.update(sc.state, p);
    sc.art.classList.toggle('acting', p > 0.5);
  }

  function drawGround(W, H, edges) {
    // suyari-gasumi: long bands of mist with stepped ends, drawn the way an
    // emakimono separates one scene from the next
    let d = '';
    const band = (x, y, w, h) => {
      const r = h / 2, s = Math.min(46, w * 0.14), h2 = h * 0.5;
      d += 'M' + (x + r) + ' ' + y + 'H' + (x + w - s - r) +
        'a' + r + ' ' + h2 / 2 + ' 0 0 1 0 ' + h2 + 'H' + (x + w - r * 0.6) + 'a' + r * 0.6 + ' ' + (h - h2) / 2 + ' 0 0 1 0 ' + (h - h2) +
        'H' + (x + s + r) + 'a' + r + ' ' + (h - h2) / 2 + ' 0 0 1 0 -' + (h - h2) + 'H' + (x + r * 0.6) +
        'a' + r * 0.6 + ' ' + h2 / 2 + ' 0 0 1 0 -' + h2 + 'Z';
    };
    edges.forEach((x, i) => {
      band(x - 360 + (i % 2) * 80, 104 + (i % 3) * 14, 520 + (i % 2) * 140, 30);
      band(x - 140 - (i % 3) * 50, H - 196 + (i % 2) * 16, 460 + (i % 3) * 80, 26);
    });
    ground.setAttribute('viewBox', '0 0 ' + W + ' ' + H);
    ground.setAttribute('width', W);
    ground.setAttribute('height', H);
    ground.innerHTML = '<defs><pattern id="sunago" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(18)">' +
      '<circle cx="1.5" cy="1.5" r="0.75" fill="var(--ink-4)" fill-opacity="0.55"/><circle cx="5" cy="4.6" r="0.5" fill="var(--ink-4)" fill-opacity="0.35"/></pattern></defs>' +
      '<path d="' + d + '" fill="url(#sunago)" stroke="var(--rule)" stroke-width="1"/>' +
      '<path d="M0 ' + (H - 70.5) + 'H' + W + '" stroke="var(--rule-soft)" stroke-width="1"/>';
  }

  function layoutEmaki() {
    emakiDesk = !RM && desktopMQ.matches;
    if (!emakiDesk) {
      emaki.style.height = '';
      track.style.transform = '';
      return;
    }
    trackW = track.scrollWidth;
    const vw = innerWidth, vh = innerHeight;
    emakiDist = Math.max(0, trackW - vw);
    emaki.style.height = emakiDist + vh + 'px';
    emakiTop = emaki.getBoundingClientRect().top + scrollY;
    measureScenes();
    const edges = $$('.panel', track).slice(1).map((p) => p.offsetLeft);
    drawGround(trackW, vh, edges);
  }

  function measureScenes() {
    scenes.forEach((sc) => {
      sc.L = sc.art.offsetLeft; sc.W = sc.art.offsetWidth;
      sc.aL = sc.artEl.offsetLeft; sc.aW = sc.artEl.offsetWidth;
    });
  }
  function updateEmaki() {
    const vw = innerWidth, vh = innerHeight;
    if (emakiDesk) {
      const x = clamp(scrollY - emakiTop, 0, emakiDist);
      track.style.transform = 'translate3d(' + (-x).toFixed(1) + 'px,0,0)';
      ground.style.transform = 'translate3d(' + (x * 0.14).toFixed(1) + 'px,0,0)';
      if (railFill) railFill.style.transform = 'scaleX(' + (emakiDist ? x / emakiDist : 0).toFixed(4) + ')';
      let near = -1, best = 1e9;
      scenes.forEach((sc, i) => {
        const left = sc.L - x, w = sc.W;
        const dC = (left + w / 2 - vw / 2) / vw;
        if (Math.abs(dC) < best) { best = Math.abs(dC); near = i; }
        if (left > vw * 1.4 || left + w < -vw * 0.6) return;
        // follow the drawing, not the panel: ink goes down while the art slides in,
        // the tool acts as the scene settles in the middle of the screen
        const aw = sc.aW / vw;
        const A = (left + sc.aL + sc.aW / 2) / vw;
        const A0 = 1 + aw * 0.2, A1 = Math.max(0.97 - aw * 0.35, 0.62), A2 = 0.5 + (sc.aL + sc.aW / 2 - w / 2) / vw - 0.1;
        const p = A > A1 ? 0.47 * (A0 - A) / (A0 - A1) : 0.47 + 0.53 * (A1 - A) / Math.max(0.05, A1 - A2);
        setSceneP(sc, clamp(p));
      });
      railSteps.forEach((li, i) => li.classList.toggle('on', i === near && best < 0.6));
    } else {
      // read every position first, then write, so the browser lays out once
      const ps = scenes.map((sc) => {
        const r = sc.artEl.getBoundingClientRect();
        if (r.bottom < -vh || r.top > vh * 2) return null;
        const dC = (r.top + r.height / 2 - vh / 2) / vh;
        return clamp((0.55 - dC) / 0.7);
      });
      scenes.forEach((sc, i) => { if (ps[i] != null) setSceneP(sc, ps[i]); });
    }
  }
  if (RM) scenes.forEach((sc) => setSceneP(sc, 1));

  /* ------------------------------------------------------------ drawers: 249 tools, moving */
  const drawers = $$('.drawer').map((d, i) => {
    const ul = $('.drawer-list', d);
    if (!RM) {
      const clone = ul.cloneNode(true);
      Array.from(clone.children).forEach((li) => { li.setAttribute('aria-hidden', 'true'); ul.appendChild(li); });
    }
    const dir = i % 2 ? 1 : -1;
    const v = 11 + ((i * 7) % 12) * 1.1;
    const o = { d, ul, dir, v, y: 0, f: 1, target: 1, h: 0 };
    const slow = () => { o.target = 0.1; };
    const fast = () => { o.target = 1; };
    d.addEventListener('pointerenter', slow); d.addEventListener('pointerleave', fast);
    d.addEventListener('focusin', slow); d.addEventListener('focusout', fast);
    return o;
  });
  let drawersOn = false, dLast = 0;
  function measureDrawers() { drawers.forEach((o) => { o.h = o.ul.scrollHeight / 2; if (o.dir > 0 && o.y === 0) o.y = -o.h * ((o.v * 13) % 1); }); }
  function drawersTick(now) {
    if (!drawersOn || document.hidden) { dLast = 0; return; }
    const dt = dLast ? Math.min(50, now - dLast) / 1000 : 0;
    dLast = now;
    for (const o of drawers) {
      o.f += (o.target - o.f) * Math.min(1, dt * 4);
      o.y += o.dir * o.v * o.f * dt;
      if (o.h) { if (o.y <= -o.h) o.y += o.h; if (o.y > 0) o.y -= o.h; }
      o.ul.style.transform = 'translate3d(0,' + o.y.toFixed(2) + 'px,0)';
    }
    requestAnimationFrame(drawersTick);
  }
  if (!RM) {
    measureDrawers();
    new IntersectionObserver((es) => {
      const vis = es[0].isIntersecting;
      if (vis && !drawersOn) { drawersOn = true; requestAnimationFrame(drawersTick); }
      drawersOn = vis;
    }, { rootMargin: '100px' }).observe($('.drawers'));
    document.addEventListener('visibilitychange', () => { if (!document.hidden && drawersOn) requestAnimationFrame(drawersTick); });
  }

  /* ------------------------------------------------------------ the folded book */
  const book = $('.book');
  const leaves = $$('.leaf', book);
  const bookMQ = matchMedia('(min-width: 1180px)');
  // brushed numerals 一 二 三 四 on each leaf
  const numerals = leaves.map((leaf) => {
    const box = $('.leaf-no', leaf);
    const ch = box.textContent.trim();
    if (!window.Brush.GLYPHS[ch]) return null;
    box.textContent = '';
    box.classList.add('brushed');
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('class', 'glyph-svg');
    box.appendChild(svg);
    const strokes = Brush.renderGlyph(svg, ch, { drawn: RM });
    wrapForInk(svg);
    return { strokes, written: RM };
  });
  function writeNumeral(i) {
    const n = numerals[i];
    if (!n || n.written) return;
    n.written = true;
    const { plan, end } = strokePlan(n.strokes, { start: 0, tempo: 0.7 });
    let t0 = 0;
    const step = (now) => { if (!t0) t0 = now; applyPlan(plan, now - t0); if (now - t0 < end + 30) requestAnimationFrame(step); };
    requestAnimationFrame(step);
  }
  let bookDesk = false;
  function updateBook() {
    if (RM) return;
    const vh = innerHeight;
    const r = book.getBoundingClientRect();
    if (r.bottom < -200 || r.top > vh + 200) return;
    if (bookDesk) {
      const w = leaves[0].offsetWidth;
      const u = io(clamp((vh * 0.96 - r.top) / (vh * 0.6)));
      const a = (1 - u) * 84 * Math.PI / 180;
      const off = (4 * w - 4 * w * Math.cos(a)) / 2;
      leaves.forEach((leaf, i) => {
        const x = i * w * Math.cos(a) + off - i * w, z = i % 2 ? -w * Math.sin(a) : 0;
        const ry = (i % 2 ? -a : a) * 180 / Math.PI;
        leaf.style.transform = 'translate3d(' + x.toFixed(1) + 'px,0,' + z.toFixed(1) + 'px) rotateY(' + ry.toFixed(2) + 'deg)';
        leaf.style.setProperty('--shade-o', (Math.sin(a) * (i % 2 ? 1 : 0.55)).toFixed(3));
      });
      if (u > 0.82) leaves.forEach((_, i) => setTimeout(() => writeNumeral(i), i * 260));
    } else {
      leaves.forEach((leaf, i) => {
        const lr = leaf.getBoundingClientRect();
        const u = io(clamp((vh * 0.98 - lr.top) / (vh * 0.42)));
        leaf.style.transform = 'perspective(1400px) rotateX(' + (-(1 - u) * 72).toFixed(2) + 'deg)';
        leaf.style.setProperty('--shade-o', ((1 - u) * 0.9).toFixed(3));
        if (u > 0.85) writeNumeral(i);
      });
    }
  }
  function layoutBook() {
    bookDesk = bookMQ.matches;
    leaves.forEach((l) => { l.style.transform = ''; });
  }

  /* ------------------------------------------------------------ reveals, key tag, copy */
  if (!RM) {
    const rv = $$('.sec-head, .fact, .colophon, .platforms li, .alpha-side, .code-card, .foot-top');
    rv.forEach((e) => e.classList.add('rv-up'));
    const ob = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { e.target.classList.add('in'); ob.unobserve(e.target); } }), { rootMargin: '0px 0px -12% 0px' });
    rv.forEach((e) => ob.observe(e));
  }
  const tag = $('.keytag');
  if (tag) {
    const parts = $$('.tag-svg path, .tag-svg circle', tag);
    if (RM) { tag.classList.add('in'); parts.forEach((p) => { p.style.strokeDashoffset = '0'; }); }
    else {
      parts.forEach((p) => { p.style.strokeDashoffset = '1'; });
      new IntersectionObserver((es, ob) => {
        if (!es[0].isIntersecting) return;
        ob.disconnect();
        tag.classList.add('in');
        parts.forEach((p, i) => p.animate([{ strokeDashoffset: 1 }, { strokeDashoffset: 0 }], { duration: 900, delay: i * 160, easing: 'cubic-bezier(.45,0,.2,1)', fill: 'forwards' }));
        $('.tag-svg', tag).animate([
          { transform: 'rotate(0deg)' }, { transform: 'rotate(3.2deg)' }, { transform: 'rotate(-2deg)' }, { transform: 'rotate(1deg)' }, { transform: 'rotate(-0.4deg)' }, { transform: 'rotate(0deg)' },
        ], { duration: 2400, delay: 500, easing: 'ease-in-out' });
      }, { rootMargin: '0px 0px -20% 0px' }).observe(tag);
    }
  }
  $$('.copy').forEach((b) => b.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = 'Copied'; }
    catch (e) { b.textContent = 'Select'; }
    setTimeout(() => { b.textContent = 'Copy'; }, 1600);
  }));

  /* ------------------------------------------------------------ one scroll loop */
  let ticking = false;
  function onScroll() {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(() => {
      ticking = false;
      nav.classList.toggle('solid', scrollY > 24);
      if (!RM) { updateEmaki(); updateBook(); }
    });
  }
  function layout() {
    layoutEmaki();
    layoutBook();
    if (!RM) measureDrawers();
    nav.classList.toggle('solid', scrollY > 24);
    if (!RM) { updateEmaki(); updateBook(); }
  }
  addEventListener('scroll', onScroll, { passive: true });
  let rz;
  addEventListener('resize', () => { clearTimeout(rz); rz = setTimeout(layout, 120); });
  layout();
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(layout);
  addEventListener('load', layout);
})();
