/* bento.js: each tile is a tiny live demo of one real tool.
 * Every tile renders from a time value, loops quietly while on screen, and shows a composed
 * still (its "done" moment) when the visitor prefers reduced motion. */
(function () {
  'use strict';
  const { inViewLoop, reduceMotion, EASE } = window.DGB;
  const $ = (s, r) => (r || document).querySelector(s);
  const still = reduceMotion();
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const prog = (t, a, b, e) => (e || EASE.lin)(clamp((t - a) / (b - a), 0, 1));
  const lerp = (a, b, p) => a + (b - a) * p;
  function rng(seed) { let s = seed >>> 0; return () => { s = Math.imul(s ^ (s >>> 15), 2246822507); s = Math.imul(s ^ (s >>> 13), 3266489909); s ^= s >>> 16; return (s >>> 0) / 4294967296; }; }

  function tile(name, spec) {
    const root = document.querySelector(`[data-tile="${name}"]`);
    if (!root) return;
    const vis = $('.tile-vis', root);
    const api = spec(vis);
    const measure = () => api.measure && api.measure();
    measure();
    if ('ResizeObserver' in window) new ResizeObserver(() => { measure(); api.render(lastT); }).observe(vis);
    let lastT = still ? api.still : 0;
    api.render(lastT);
    if (still) return;
    inViewLoop(vis, (t) => { lastT = t % api.period; api.render(lastT); });
  }

  /* ------------------------------------------------ speech.transcribe → SRT */
  tile('transcribe', (vis) => {
    const CUES = [
      [400, 2600, 'So, how did the move to Lisbon go?'],
      [2950, 5900, 'Honestly, better than I expected. The first week was chaos,'],
      [6150, 8100, 'but the studio was ready by Friday.'],
    ];
    const AUDIO = 8400, PERIOD = 11000;
    const wave = $('#trWave'), head = $('#trHead'), wordsEl = $('#trWords'), srt = $('#trSrtB'), time = $('#trTime');
    // waveform: loud in speech, quiet in the gaps
    const r = rng(7), N = 74, bars = [];
    for (let i = 0; i < N; i++) {
      const at = (i + 0.5) / N * AUDIO;
      const speaking = CUES.some((c) => at >= c[0] - 60 && at <= c[1] + 60);
      const h = speaking ? 14 + r() * 38 * (0.55 + 0.45 * Math.abs(Math.sin(i / 3.1))) : 4 + r() * 6;
      const b = document.createElement('span');
      b.className = 'b'; b.style.height = Math.round(h) + 'px';
      wave.insertBefore(b, head); bars.push(b);
    }
    // words with times spread across their cue
    const words = [];
    CUES.forEach((c, ci) => {
      const ws = c[2].split(' ');
      ws.forEach((w, i) => {
        const s = document.createElement('span');
        s.className = 'w'; s.textContent = w;
        wordsEl.appendChild(s); wordsEl.appendChild(document.createTextNode(' '));
        words.push({ el: s, t: c[0] + (c[1] - c[0]) * (i / ws.length), on: false, now: false });
      });
      if (ci < CUES.length - 1) { /* same paragraph */ }
    });
    const fmt = (ms) => { const s = Math.floor(ms / 1000), m = Math.floor(s / 60); return `00:${String(m).padStart(2, '0')}:${String(s % 60).padStart(2, '0')},${String(ms % 1000).padStart(3, '0')}`; };
    const cues = CUES.map((c, i) => {
      const d = document.createElement('div');
      d.className = 'cue';
      d.innerHTML = `<b>${i + 1}</b><br><b>${fmt(c[0])} --> ${fmt(c[1])}</b><br><span>${c[2]}</span>`;
      srt.appendChild(d);
      return { el: d, t: c[1], on: false };
    });
    let W = 0, lastBar = -1, lastSec = -1;
    return {
      period: PERIOD, still: 9000,
      measure() { W = wave.clientWidth - 24; },
      render(t) {
        const reset = t > PERIOD - 700;
        const pt = Math.min(t, AUDIO);
        head.style.transform = `translateX(${(pt / AUDIO * W).toFixed(2)}px)`;
        head.style.opacity = reset ? '0' : '';
        const nb = reset ? -1 : Math.floor(pt / AUDIO * N);
        if (nb !== lastBar) { bars.forEach((b, i) => b.classList.toggle('on', i < nb)); lastBar = nb; }
        words.forEach((w) => {
          const on = !reset && t >= w.t;
          const now = on && t - w.t < 360 && t < AUDIO;
          if (on !== w.on) { w.on = on; w.el.classList.toggle('on', on); }
          if (now !== w.now) { w.now = now; w.el.classList.toggle('now', now); }
        });
        cues.forEach((c) => { const on = !reset && t >= c.t; if (on !== c.on) { c.on = on; c.el.classList.toggle('on', on); } });
        const sec = Math.floor(pt / 1000);
        if (sec !== lastSec) { lastSec = sec; time.textContent = `00:0${sec} / 00:08`; }
      },
    };
  });

  /* ------------------------------------------------ dev.json */
  tile('json', () => {
    const minified = '{"name":"dogubako","version":"0.1.0","tools":249,"interfaces":["desktop","web","cli","http","mcp"],"local_first":true,"license":"AGPL-3.0-or-later","engines":{"native":156,"external":77,"model":6,"network":10}}';
    $('#jsMin').textContent = minified;
    const obj = JSON.parse(minified);
    // pretty-print by hand so each line can be colored and revealed on its own
    const lines = [];
    const esc = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;');
    const val = (v) => typeof v === 'string' ? `<span class="s">"${esc(v)}"</span>` : typeof v === 'boolean' ? `<span class="b">${v}</span>` : `<span class="n">${v}</span>`;
    (function walk(v, ind, key, last) {
      const pad = '  '.repeat(ind);
      const k = key != null ? `<span class="k">"${key}"</span><span class="p">: </span>` : '';
      const comma = last ? '' : '<span class="p">,</span>';
      if (Array.isArray(v)) {
        lines.push(`${pad}${k}<span class="p">[</span>`);
        v.forEach((x, i) => walk(x, ind + 1, null, i === v.length - 1));
        lines.push(`${pad}<span class="p">]</span>${comma}`);
      } else if (v && typeof v === 'object') {
        lines.push(`${pad}${k}<span class="p">{</span>`);
        const ks = Object.keys(v);
        ks.forEach((kk, i) => walk(v[kk], ind + 1, kk, i === ks.length - 1));
        lines.push(`${pad}<span class="p">}</span>${comma}`);
      } else {
        lines.push(`${pad}${k}${val(v)}${comma}`);
      }
    })(obj, 0, null, true);
    const out = $('#jsOut');
    out.innerHTML = lines.map((l) => `<span class="l">${l}</span>`).join('');
    const ls = Array.from(out.children).map((el) => ({ el, on: false }));
    const stats = $('#jsStats');
    let statsOn = false;
    const T0 = 700, STEP = 75, PERIOD = 7600;
    return {
      period: PERIOD, still: 5000,
      render(t) {
        const reset = t > PERIOD - 600;
        ls.forEach((l, i) => { const on = !reset && t >= T0 + i * STEP; if (on !== l.on) { l.on = on; l.el.classList.toggle('on', on); } });
        const s = !reset && t >= T0 + ls.length * STEP + 200;
        if (s !== statsOn) { statsOn = s; stats.classList.toggle('on', s); }
      },
    };
  });

  /* ------------------------------------------------ pdf.merge */
  tile('merge', (vis) => {
    const mg = $('#mg'), count = $('#mgCount');
    const PAGES = [['a', 1], ['a', 2], ['a', 3], ['b', 1], ['b', 2]];
    const pages = PAGES.map(([doc, n], i) => {
      const el = document.createElement('div');
      el.className = 'mg-pg' + (doc === 'b' ? ' b-doc' : '');
      el.innerHTML = '<i></i><i></i><i style="width:86%"></i><i></i><i style="width:70%"></i><i></i><i style="width:54%"></i><span class="pn"></span>';
      el.style.zIndex = String(10 - i);
      mg.appendChild(el);
      return { el, doc, n, k: i, pn: el.querySelector('.pn'), lastPn: '' };
    });
    let W = 0, H = 0, done = false;
    const PERIOD = 6400;
    let PW = 64, PH = 84;
    function start(p) { // fanned groups: a.pdf left, b.pdf right
      const g = p.doc === 'a' ? 0.25 : 0.75;
      const idx = p.doc === 'a' ? p.n - 1 : p.n - 1;
      const cnt = p.doc === 'a' ? 3 : 2;
      const off = idx - (cnt - 1) / 2;
      return { x: W * g - PW / 2 + off * 12, y: H * 0.42 - PH / 2 + Math.abs(off) * 4, r: off * 7 };
    }
    function row(p) { const gap = Math.min(PW * 0.8, (W - 40 - PW) / 4); return { x: W / 2 - PW / 2 + (p.k - 2) * gap, y: H * 0.42 - PH / 2, r: 0 }; }
    function stack(p) { return { x: W / 2 - PW / 2 + p.k * 2.5, y: H * 0.42 - PH / 2 + p.k * 2.5, r: (p.k - 2) * 0.8 }; }
    return {
      period: PERIOD, still: 3600,
      measure() {
        W = vis.clientWidth; H = vis.clientHeight;
        PH = Math.round(Math.max(84, Math.min(124, H * 0.48))); PW = Math.round(PH * 0.77);
        pages.forEach((p) => { p.el.style.width = PW + 'px'; p.el.style.height = PH + 'px'; });
      },
      render(t) {
        let landed = 0;
        pages.forEach((p) => {
          const a = start(p), b = row(p), c = stack(p);
          const t1 = 700 + p.k * 140, t2 = t1 + 620;
          const u = prog(t, t1, t2, EASE.out);
          const v = prog(t, 2500, 3100, EASE.inOut);
          const back = prog(t, PERIOD - 700, PERIOD - 150, EASE.inOut);
          let x = lerp(a.x, b.x, u), y = lerp(a.y, b.y, u) - Math.sin(Math.PI * u) * 34, r = lerp(a.r, b.r, u);
          x = lerp(x, c.x, v); y = lerp(y, c.y, v); r = lerp(r, c.r, v);
          let o = 1;
          if (back > 0) { o = back < 0.5 ? 1 - back * 2 : (back - 0.5) * 2; if (back >= 0.5) { x = a.x; y = a.y; r = a.r; } }
          p.el.style.transform = `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px) rotate(${r.toFixed(2)}deg)`;
          p.el.style.opacity = o === 1 ? '' : o.toFixed(3);
          if (u >= 1) landed++;
          const pn = u >= 1 && back < 0.5 ? String(p.k + 1) : String(p.n);
          if (pn !== p.lastPn) { p.lastPn = pn; p.pn.textContent = pn; }
        });
        const d = t >= 3000 && t < PERIOD - 700;
        if (d !== done) { done = d; mg.classList.toggle('is-done', d); }
        const c = String(t < PERIOD - 700 ? landed : 0);
        if (count.textContent !== c) count.textContent = c;
      },
    };
  });

  /* ------------------------------------------------ video.compress, target size */
  tile('video', () => {
    const frames = $('#vcFrames');
    frames.innerHTML = Array.from({ length: 10 }, () => '<span><svg viewBox="0 0 224 150" preserveAspectRatio="xMidYMid slice"><use href="#ph-dune"/></svg></span>').join('');
    const fill = $('#vcFill'), target = $('#vcTarget'), to = $('#vcTo');
    const SRC = 182, OUT = 24.6, PERIOD = 6400;
    target.style.left = (25 / SRC * 100).toFixed(2) + '%';
    const spr = document.querySelectorAll('[data-tile="video"] .vc-spr');
    let last = '';
    return {
      period: PERIOD, still: 4200,
      render(t) {
        const shift = (t * 0.045) % 96;
        frames.style.transform = `translateX(${-shift.toFixed(2)}px)`;
        spr.forEach((s) => { s.style.transform = `translateX(${-((t * 0.045) % 14).toFixed(2)}px)`; });
        const p = prog(t, 900, 3600, EASE.inOut) * (1 - prog(t, PERIOD - 700, PERIOD - 100, EASE.inOut));
        const size = lerp(SRC, OUT, p);
        fill.style.transform = `scaleX(${(size / SRC).toFixed(4)})`;
        const txt = size.toFixed(1) + ' MB';
        if (txt !== last) { last = txt; to.textContent = txt; }
      },
    };
  });

  /* ------------------------------------------------ files.hash */
  tile('hash', () => {
    const r = rng(42);
    const hex = (n) => Array.from({ length: n }, () => '0123456789abcdef'[Math.floor(r() * 16)]).join('');
    const A = hex(64), B = hex(64);
    const elA = $('#hsA'), elB = $('#hsB'), bar = $('#hsBar'), sz = $('#hsSz');
    const PERIOD = 6600, T1 = 3800;
    let lastStep = -1;
    const scramble = (n, step, salt) => { const rr = rng(step * 31 + salt); return Array.from({ length: n }, () => '0123456789abcdef'[Math.floor(rr() * 16)]).join(''); };
    return {
      period: PERIOD, still: 5000,
      render(t) {
        const back = t > PERIOD - 500;
        const p = back ? 0 : prog(t, 300, T1, EASE.inOut);
        bar.style.transform = `scaleX(${p.toFixed(4)})`;
        const step = back ? -2 : Math.floor(t / 70);
        if (step === lastStep) return;
        lastStep = step;
        sz.textContent = `${(p * 4.7).toFixed(1)} / 4.7 GB`;
        const k = Math.floor(p * 64);
        const done = p >= 1;
        elA.innerHTML = done ? A : A.slice(0, k) + `<span class="x">${scramble(64 - k, step, 1)}</span>`;
        elB.innerHTML = done ? B : B.slice(0, k) + `<span class="x">${scramble(64 - k, step, 2)}</span>`;
      },
    };
  });

  /* ------------------------------------------------ image.redact */
  tile('redact', () => {
    const photo = $('#rdPhoto'), box = $('#rdBox'), cur = $('#rdCur'), mosaic = $('#rdMosaic'), info = $('#rdInfo');
    // the address block on the shipping label, in the drawing's own coordinates (320 x 200)
    const R = { x0: 121, y0: 99, x1: 229, y1: 137 };
    const COLS = 14, ROWS = 5;
    const rr = rng(3);
    mosaic.style.gridTemplateColumns = `repeat(${COLS}, 1fr)`;
    mosaic.innerHTML = Array.from({ length: COLS * ROWS }, (_, i) => {
      const row = Math.floor(i / COLS), col = i % COLS;
      const ink = (row >= 1 && row <= 3 && col < 11 - row) ? 0.35 + rr() * 0.35 : rr() * 0.12;
      const v = Math.round(250 - ink * 170);
      return `<i style="background:rgb(${v},${v - 2},${v - 6})"></i>`;
    }).join('');
    let m = { s: 1, ox: 0, oy: 0 }, W = 0, H = 0, lastInfo = '';
    const PERIOD = 6800;
    const px = (x, y) => [m.ox + x * m.s, m.oy + y * m.s];
    return {
      period: PERIOD, still: 4200,
      measure() {
        W = photo.clientWidth; H = photo.clientHeight;
        const s = Math.max(W / 320, H / 200);
        m = { s, ox: (W - 320 * s) / 2, oy: (H - 200 * s) / 2 };
        const [x0, y0] = px(R.x0, R.y0), [x1, y1] = px(R.x1, R.y1);
        mosaic.style.left = x0 + 'px'; mosaic.style.top = y0 + 'px';
        mosaic.style.width = (x1 - x0) + 'px'; mosaic.style.height = (y1 - y0) + 'px';
        box.style.left = x0 + 'px'; box.style.top = y0 + 'px';
        box.style.width = (x1 - x0) + 'px'; box.style.height = (y1 - y0) + 'px';
      },
      render(t) {
        const [x0, y0] = px(R.x0, R.y0), [x1, y1] = px(R.x1, R.y1);
        const enter = prog(t, 200, 900, EASE.inOut);
        const drag = prog(t, 1100, 2100, EASE.inOut);
        const leave = prog(t, 2500, 3300, EASE.inOut);
        let cx = lerp(W + 30, x0, enter), cy = lerp(H + 20, y0, enter);
        cx = lerp(cx, x1, drag); cy = lerp(cy, y1, drag);
        cx = lerp(cx, W * 0.86, leave); cy = lerp(cy, H * 0.84, leave);
        const press = t > 1050 && t < 2200 ? 0.88 : 1;
        cur.style.transform = `translate(${cx.toFixed(2)}px, ${cy.toFixed(2)}px) scale(${press})`;
        const fadeOut = 1 - prog(t, PERIOD - 800, PERIOD - 300, EASE.ease);
        cur.style.opacity = (t < 150 ? 0 : fadeOut).toFixed(3);
        // the box grows with the drag (scale from its corner; the border is thin enough to read)
        const bw = Math.max(0.02, drag), bo = t >= 1100 ? fadeOut : 0;
        box.style.transform = `scale(${bw.toFixed(4)}, ${bw.toFixed(4)})`;
        box.style.opacity = bo.toFixed(3);
        const mo = prog(t, 2300, 2900, EASE.ease) * fadeOut;
        mosaic.style.opacity = mo.toFixed(3);
        const txt = t >= 2150 && fadeOut > 0.5 ? '1 region · 1600×1200px' : 'Drag on the image to add a box';
        if (txt !== lastInfo) { lastInfo = txt; info.textContent = txt; }
      },
    };
  });

  /* ------------------------------------------------ data.clean */
  tile('csv', () => {
    const table = $('#cvTable'), ops = Array.from($('#cvOps').children);
    const ws = (s) => s.replace(/^( +)|( +)$/g, (m) => `<span class="ws">${m}</span>`);
    const nul = (s) => `<span class="nul">${s}</span>`;
    // [raw, after trim, after nulls, after dates]
    const ROWS = [
      [['name'], ['joined'], ['city']],
      [[ws('  Ana Ruiz '), 'Ana Ruiz'], ['03/15/2024', null, null, '2024-03-15'], ['Porto']],
      [['Ben Ito'], [nul('N/A'), null, ''], [ws(' Osaka'), 'Osaka']],
      [[''], [''], ['']],
      [['Chloé Martin'], ['07/30/2025', null, null, '2025-07-30'], [nul('NULL'), null, '']],
    ];
    const rows = ROWS.map((cells, ri) => {
      const r = document.createElement('div');
      r.className = 'cv-row' + (ri === 0 ? ' hd' : '');
      r.style.top = ri * 30 + 'px';
      const cs = cells.map((states) => {
        const c = document.createElement('div');
        c.className = 'cv-c';
        c.innerHTML = states[0];
        r.appendChild(c);
        return { el: c, states, cur: 0 };
      });
      table.appendChild(r);
      return { el: r, cs };
    });
    table.style.height = rows.length * 30 + 'px';
    const STEPS = [900, 1900, 2900, 3900]; // trim, nulls, dates, empty rows
    const PERIOD = 7400;
    let lastStage = -1, lastOps = -1;
    const stateFor = (states, stage) => { let k = 0; for (let i = 1; i <= stage && i < states.length; i++) if (states[i] != null) k = i; return k; };
    return {
      period: PERIOD, still: 5200,
      render(t) {
        const reset = t > PERIOD - 500;
        let stage = 0;
        STEPS.forEach((s, i) => { if (t >= s) stage = i + 1; });
        if (reset) stage = 0;
        const on = stage;
        if (on !== lastOps) { lastOps = on; ops.forEach((o, i) => o.classList.toggle('on', i < on)); }
        if (stage !== lastStage) {
          const forward = stage > lastStage && lastStage >= 0;
          lastStage = stage;
          rows.forEach((r) => r.cs.forEach((c) => {
            const k = stateFor(c.states, Math.min(stage, 3));
            if (k !== c.cur) {
              c.cur = k; c.el.innerHTML = c.states[k];
              if (forward && !still) { c.el.classList.remove('flash'); void c.el.offsetWidth; c.el.classList.add('flash'); }
            }
          }));
        }
        // empty row folds away; the last row moves up
        const fold = stage >= 4 ? prog(t, STEPS[3], STEPS[3] + 450, EASE.out) : 0;
        const emp = rows[3], last = rows[4];
        emp.el.style.opacity = (1 - fold).toFixed(3);
        emp.el.style.transform = `scaleY(${(1 - fold * 0.6).toFixed(3)})`;
        last.el.style.transform = `translateY(${(-30 * fold).toFixed(2)}px)`;
        const tableO = reset ? 0.0 : t < 250 ? prog(t, 0, 250) : 1;
        table.style.opacity = tableO === 1 ? '' : tableO.toFixed(3);
      },
    };
  });
})();
