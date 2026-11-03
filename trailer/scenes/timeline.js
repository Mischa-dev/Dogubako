/*
 * Dogubako alpha trailer: the timeline.
 *
 * window.render(t) puts every element where it belongs at time t (seconds).
 * Nothing depends on wall-clock time, so the renderer can step frame by frame.
 *
 * The music (tools/music.py) is 96 BPM: a bar is 2.5 s and every scene starts
 * on a bar line. Keep the two files in step when you move things.
 */
(() => {
  const BAR = 2.5;
  const BEAT = BAR / 4;
  const DURATION = 72;
  const CAP = '../build/captures/';
  const ASSET = '../assets/';
  const BOXES = window.BOXES || {};

  // ------------------------------------------------------------ helpers
  const $ = (s) => document.querySelector(s);
  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const lerp = (a, b, p) => a + (b - a) * p;
  const prog = (t, a, b) => clamp((t - a) / (b - a));
  const E = {
    lin: (p) => p,
    out: (p) => 1 - Math.pow(1 - p, 3),
    out5: (p) => 1 - Math.pow(1 - p, 5),
    in: (p) => p * p * p,
    inOut: (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2),
    sine: (p) => -(Math.cos(Math.PI * p) - 1) / 2,
    expo: (p) => (p >= 1 ? 1 : 1 - Math.pow(2, -10 * p)),
    back: (p) => { const c1 = 1.4, c3 = c1 + 1; return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2); },
  };
  const ease = (t, a, b, fn = E.out) => fn(prog(t, a, b));
  /** 0→1 over [a, a+fi], 1, then 1→0 over [b-fo, b]. */
  const win = (t, a, b, fi = 0.5, fo = 0.5, fn = E.sine) =>
    Math.min(fn(prog(t, a, a + fi)), 1 - fn(prog(t, b - fo, b)));

  function put(el, o) {
    if (!el) return;
    const tf = [];
    if (o.x !== undefined || o.y !== undefined) tf.push(`translate3d(${(o.x ?? 0).toFixed(2)}px, ${(o.y ?? 0).toFixed(2)}px, 0)`);
    if (o.cx) tf.push(`translate(-50%, -50%)`);
    if (o.persp) tf.push(`perspective(${o.persp}px)`);
    if (o.ry !== undefined) tf.push(`rotateY(${o.ry}deg)`);
    if (o.rx !== undefined) tf.push(`rotateX(${o.rx}deg)`);
    if (o.r !== undefined) tf.push(`rotate(${o.r}deg)`);
    if (o.s !== undefined) tf.push(`scale(${o.s})`);
    if (o.sx !== undefined || o.sy !== undefined) tf.push(`scale(${o.sx ?? 1}, ${o.sy ?? 1})`);
    el.style.transform = tf.join(' ');
    if (o.o !== undefined) {
      el.style.opacity = clamp(o.o).toFixed(4);
      el.style.visibility = o.o <= 0.001 ? 'hidden' : 'visible';
    }
    if (o.blur !== undefined) el.style.filter = o.blur > 0.05 ? `blur(${o.blur.toFixed(2)}px)` : 'none';
  }
  const show = (el, on) => { el.style.display = on ? 'block' : 'none'; };
  const icon = (name, extra = '') =>
    `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" ${extra}>${window.ICONS[name]}</svg>`;
  const el = (html) => { const d = document.createElement('div'); d.innerHTML = html.trim(); return d.firstChild; };
  /** Split text into word spans for staggered reveals. */
  function words(node, text) {
    node.innerHTML = text.split(' ').map((w) => `<span class="word">${w}</span>`).join(' ');
    return [...node.querySelectorAll('.word')];
  }
  function revealWords(ws, t, start, step = 0.07, dur = 0.55, rise = 34) {
    ws.forEach((w, i) => {
      const p = ease(t, start + i * step, start + i * step + dur, E.out5);
      put(w, { y: (1 - p) * rise, o: p, blur: (1 - p) * 8 });
    });
  }
  function measure(node) { return { w: node.offsetWidth, h: node.offsetHeight }; }

  // images: captures from the real app and the painted sample photos
  document.querySelectorAll('img[data-src]').forEach((img) => {
    const n = img.dataset.src;
    img.src = n.includes('.') ? ASSET + n : CAP + n + '.png';
  });
  const B = (key, fallback) => BOXES[key] || fallback;

  // ------------------------------------------------------------ ambient
  const ambient = $('#ambient');
  const blobs = [
    { light: '#f4d8c6', dark: '#8a6f7d', x: 0.18, y: 0.12, r: 900, sp: 0.07 },
    { light: '#dcd7ef', dark: '#4a5a78', x: 0.9, y: 0.2, r: 820, sp: 0.05 },
    { light: '#f1e2cf', dark: '#b88a74', x: 0.55, y: 1.05, r: 1000, sp: 0.06 },
  ].map((b) => {
    const light = el(`<div class="blob" style="width:${b.r}px;height:${b.r}px;background:${b.light}"></div>`);
    const dark = el(`<div class="blob" style="width:${b.r}px;height:${b.r}px;background:${b.dark}"></div>`);
    ambient.append(light, dark);
    return { ...b, lightEl: light, darkEl: dark };
  });
  function renderAmbient(t, dark, strength) {
    blobs.forEach((b, i) => {
      const x = b.x * 1920 + Math.sin(t * b.sp * 2 * Math.PI * 0.35 + i * 2) * 120 - b.r / 2;
      const y = b.y * 1080 + Math.cos(t * b.sp * 2 * Math.PI * 0.3 + i) * 80 - b.r / 2;
      put(b.lightEl, { x, y, o: (1 - dark) * 0.75 * strength });
      put(b.darkEl, { x, y, o: dark * 0.42 * strength });
    });
  }

  // dark/light ground per moment: [time, darkness]
  const GROUND = [
    [0, 1], [4.6, 1], [5.3, 0],             // logo dark → slips paper
    [27.2, 0], [27.9, 1],                    // montage dark
    [39.6, 1], [40.3, 0],                    // interfaces paper
    [47.2, 0], [47.9, 1],                    // local-first dark
    [54.7, 1], [55.3, 0],                    // themes paper…
    [56.5, 0], [57.3, 1],                    // …flip to dark with the app
    [72, 1],
  ];
  function ground(t) {
    for (let i = 0; i < GROUND.length - 1; i++) {
      const [a, va] = GROUND[i], [b, vb] = GROUND[i + 1];
      if (t >= a && t <= b) return lerp(va, vb, E.inOut(prog(t, a, b)));
    }
    return 1;
  }

  // ============================================================ 1. logo
  const S1 = { root: $('#s-logo'), mark: $('#logo-mark'), box: $('#lm-box'), seam: $('#lm-seam'), latch: $('#lm-latch'), word: $('#logo-word'), kanji: $('#logo-kanji') };
  S1.word.innerHTML = 'Dogubako'.split('').map((c) => `<span class="word">${c}</span>`).join('');
  const s1Letters = [...S1.word.querySelectorAll('.word')];
  function logo(t) {
    const v = win(t, -1, 5.1, 0.01, 0.7);
    show(S1.root, v > 0);
    if (v <= 0) return;
    S1.box.setAttribute('stroke-dashoffset', (1 - ease(t, 0.15, 1.25, E.inOut)).toFixed(4));
    S1.seam.setAttribute('stroke-dashoffset', (1 - ease(t, 0.95, 1.4, E.inOut)).toFixed(4));
    // the latch drops in and lands exactly on the click in the music (1.6 s)
    const lp = prog(t, 1.32, 1.6);
    const ly = t < 1.6 ? -34 * (1 - lp * lp) : -5 * Math.exp(-(t - 1.6) * 14) * Math.abs(Math.sin((t - 1.6) * 30));
    S1.latch.setAttribute('transform', `translate(0 ${ly.toFixed(2)})`);
    S1.latch.style.opacity = clamp(lp * 4);

    const slide = ease(t, 1.95, 2.75, E.inOut);
    const ms = lerp(1, 0.72, slide);
    const markCx = lerp(960, 640, slide);
    const out = ease(t, 4.3, 5.1, E.in);
    put(S1.mark, { x: markCx - 150, y: 470 - 150 + lerp(0, -10, slide), s: ms * (1 + out * 0.08), o: v });
    // glow pulse when the latch lands
    S1.mark.style.filter = `drop-shadow(0 0 ${(28 * Math.exp(-Math.max(0, t - 1.6) * 2.5) * (t > 1.6 ? 1 : 0)).toFixed(1)}px rgba(224,83,61,.55))`;
    const wx = 790;
    s1Letters.forEach((c, i) => {
      const p = ease(t, 2.6 + i * 0.045, 3.2 + i * 0.045, E.out5);
      put(c, { y: (1 - p) * 60, o: p, blur: (1 - p) * 10 });
    });
    put(S1.word, { x: wx, y: 470 - 92 - 6, o: v, s: 1 + out * 0.08 });
    const kp = ease(t, 3.3, 3.95);
    put(S1.kanji, { x: wx + 8, y: 470 + 82 + (1 - kp) * 14, o: kp * v });
  }

  // ============================================================ 2. slips
  const S2 = { root: $('#s-slips'), h1: $('#slips-h1'), h2: $('#slips-h2'), host: $('#slips'), box: $('#box-big'), lid: $('#box-lid'), latch: $('#box-latch') };
  const s2w1 = words(S2.h1, 'Every little tool you reach for…');
  const s2w2 = words(S2.h2, '…in one box.');
  const TOOLS = [
    ['image', 'HEIC → JPG'], ['pdf', 'Merge PDFs'], ['speech', 'Transcribe a call'], ['dev', 'Format JSON'], ['image', 'Remove background'],
    ['video', 'Trim a clip'], ['files', 'Checksum'], ['dev', 'Decode a JWT'], ['data', 'CSV → Excel'], ['pdf', 'OCR a scan'],
    ['design', 'Contrast check'], ['image', 'Blur faces'], ['audio', 'Normalize loudness'], ['dev', 'QR code'], ['dev', 'Regex tester'],
    ['network', 'DNS lookup'], ['files', 'Batch rename'], ['image', 'Strip metadata'], ['video', 'Video → GIF'], ['speech', 'Subtitles'],
    ['files', 'Unzip safely'], ['text', 'Compare text'], ['pdf', 'Compress PDF'], ['audio', 'Cut silence'], ['network', 'TLS certificate'],
    ['image', 'Resize 400 photos'], ['dev', 'Base64'], ['system', 'Battery health'], ['text', 'Markdown → HTML'], ['design', 'Palette from photo'],
  ];
  // a loose cloud with depth; deterministic pseudo-random layout
  let seed = 11;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  const slips = TOOLS.map(([ic, name], i) => {
    const node = el(`<div class="abs slip">${icon(ic)}<span>${name}</span></div>`);
    S2.host.append(node);
    const col = i % 6, row = Math.floor(i / 6);
    return {
      node,
      x: 120 + col * 290 + rnd() * 120 - 40,
      y: 300 + row * 120 + rnd() * 50 - (col % 2) * 30,
      z: 0.9 + rnd() * 0.32,
      r: (rnd() - 0.5) * 12,
      enter: 5.25 + rnd() * 1.6,
      leave: 7.55 + (5 - Math.abs(col - 2.5)) * 0.07 + row * 0.08 + rnd() * 0.12,
      drift: (rnd() - 0.5) * 70,
    };
  });
  function slipsScene(t) {
    const v = win(t, 4.85, 10.0, 0.4, 0.45);
    show(S2.root, v > 0);
    if (v <= 0) return;
    const h1out = ease(t, 8.0, 8.4, E.in);
    revealWords(s2w1, t, 5.2, 0.075);
    put(S2.h1, { x: 960 - measure(S2.h1).w / 2, y: 120, o: v * (1 - h1out), blur: h1out * 10 });
    revealWords(s2w2, t, 8.45, 0.09);
    put(S2.h2, { x: 960 - measure(S2.h2).w / 2, y: 120, o: v });

    // box rises, lid open; slips pour in; lid slams on the downbeat of bar 4 (10 s)? no: at 9.05 s
    const rise = ease(t, 7.0, 7.7, E.out5);
    const boxX = 700, boxY = lerp(1120, 590, rise);
    const lidOpen = 1 - ease(t, 8.85, 9.12, E.in);
    S2.lid.setAttribute('transform', `rotate(${(-24 * lidOpen * ease(t, 7.3, 7.8)).toFixed(2)} 5 33)`);
    S2.latch.style.opacity = t > 9.1 ? 1 : 0;
    const thump = t > 9.12 ? Math.exp(-(t - 9.12) * 10) * Math.sin((t - 9.12) * 40) * 0.025 : 0;
    put(S2.box, { x: boxX, y: boxY, sy: 1 - thump, sx: 1 + thump, o: v * rise });

    const mouthX = 960, mouthY = boxY + 33 * 4 + 20;
    slips.forEach((s, i) => {
      const ein = ease(t, s.enter, s.enter + 0.7, E.out5);
      const fly = prog(t, s.leave, s.leave + 0.6);
      const f = E.in(fly);
      const floatY = Math.sin(t * 0.9 + i) * 8;
      const w = s.node.offsetWidth, h = s.node.offsetHeight;
      const x0 = s.x + s.drift * prog(t, 5, 8) - w / 2, y0 = s.y + floatY - h / 2 + (1 - ein) * 40;
      // curve: swing out a little, then dive into the box
      const x = lerp(x0, mouthX - w / 2, f) + Math.sin(fly * Math.PI) * (s.x < 960 ? -40 : 40);
      const y = lerp(y0, mouthY - h / 2, f) - Math.sin(fly * Math.PI) * 60;
      const sc = s.z * lerp(lerp(0.85, 1, ein), 0.25, f);
      put(s.node, { x, y, s: sc, r: s.r * (1 - f) + fly * (s.x < 960 ? 30 : -30), o: v * ein * (1 - prog(fly, 0.82, 1)) });
      s.node.style.zIndex = Math.round(s.z * 100);
    });
  }

  // ============================================================ 3. bench
  const S3 = { root: $('#s-bench'), shelf: $('#bench-shelf'), h: $('#bench-h'), sub: $('#bench-sub'), host: $('#bench-objs') };
  const s3w = words(S3.h, 'Drop a file.');
  const holes = '<div class="holes" style="top:6px">' + '<i></i>'.repeat(11) + '</div><div class="holes" style="bottom:6px">' + '<i></i>'.repeat(11) + '</div>';
  const wave = Array.from({ length: 30 }, (_, i) => `<i style="height:${(14 + Math.abs(Math.sin(i * 0.7) * Math.cos(i * 0.31)) * 80).toFixed(0)}px"></i>`).join('');
  const OBJS = [
    { html: `<div class="photo"><img src="${ASSET}dune.jpg"></div>`, label: 'dune.jpg', meta: 'Image · 152 KB', r: -4 },
    { html: `<div style="position:relative"><div class="doc-under" style="transform:rotate(5deg) translate(8px,0)"></div><div class="doc"><div class="ln h"></div>${'<div class="ln"></div>'.repeat(4)}<div class="ln" style="width:60%"></div>${'<div class="ln"></div>'.repeat(3)}</div></div>`, label: 'lease.pdf', meta: 'PDF · 14 pages', r: 3 },
    { html: `<div class="film">${holes}<img src="${ASSET}dusk.png"></div>`, label: 'trip.mov', meta: 'Video · 2:14', r: -2 },
    { html: `<div class="tape">${wave}</div>`, label: 'interview.m4a', meta: 'Audio · 41 min', r: 2.5 },
    { html: `<div class="code mono">{\n  <span class="k">"name"</span>: <span class="s">"tent"</span>,\n  <span class="k">"size"</span>: <span class="n">4</span>,\n  <span class="k">"tags"</span>: [\n    <span class="s">"camp"</span>,\n    <span class="s">"rain"</span>\n  ]\n}</div>`, label: 'order.json', meta: 'JSON · 2 KB', r: -3 },
    { html: `<div class="sheet">${Array.from({ length: 32 }, (_, i) => (i % 4 === 0 || i < 4 ? '<b></b>' : i === 9 || i === 14 || i === 23 ? '<span class="f"></span>' : '<span></span>')).join('')}</div>`, label: 'budget.csv', meta: 'CSV · 1,204 rows', r: 2 },
  ];
  const benchObjs = OBJS.map((o, i) => {
    const node = el(`<div class="abs obj"><div class="thing"><div class="shadow"></div>${o.html}</div><div class="label">${o.label}</div><div class="meta mono">${o.meta}</div></div>`);
    S3.host.append(node);
    return { node, thing: node.querySelector('.thing > :last-child'), shadow: node.querySelector('.shadow'), r: o.r, at: 10.05 + i * BEAT / 2, cx: 215 + i * 298 };
  });
  function bench(t) {
    const v = win(t, 9.6, 15.3, 0.45, 0.55);
    show(S3.root, v > 0);
    if (v <= 0) return;
    const drift = lerp(30, -30, prog(t, 10, 15.3));
    revealWords(s3w, t, 10.1, 0.12, 0.6, 50);
    put(S3.h, { x: 140, y: 190, o: v });
    const sp = ease(t, 10.6, 11.3);
    put(S3.sub, { x: 146, y: 308 + (1 - sp) * 16, o: v * sp });
    put(S3.shelf, { y: lerp(80, 0, ease(t, 9.7, 10.4, E.out5)), o: v });
    const out = ease(t, 14.6, 15.3, E.in);
    benchObjs.forEach((b, i) => {
      const fall = prog(t, b.at, b.at + 0.34);
      const u = Math.max(0, t - b.at - 0.34);
      let y;
      if (fall < 1) y = -640 * (1 - fall * fall);
      else y = -26 * Math.exp(-u * 9) * Math.abs(Math.sin(u * 15));
      const squash = fall >= 1 ? Math.exp(-u * 18) * 0.06 : 0;
      const tilt = fall < 1 ? b.r * 3 : b.r + b.r * 2 * Math.exp(-u * 7) * Math.cos(u * 14);
      put(b.thing, { y, r: tilt, sy: 1 - squash, sx: 1 + squash * 0.6 });
      const height = clamp(-y / 400);
      b.shadow.style.opacity = (fall > 0 ? 1 - height * 0.9 : 0).toFixed(3);
      b.shadow.style.transform = `scale(${(1 + height * 0.5).toFixed(3)})`;
      const lp = ease(t, b.at + 0.35, b.at + 0.85);
      b.node.querySelector('.label').style.opacity = lp;
      b.node.querySelector('.meta').style.opacity = lp;
      put(b.node, { x: b.cx - 135 + drift, y: 560 + out * (i === 0 ? 0 : 30), o: v * (fall > 0 ? 1 : 0) });
    });
  }

  // ============================================================ 4–5. flow
  const S4 = {
    root: $('#s-flow'), cam: $('#flow-cam'), win: $('#flow-win'), home: $('#flow-home'), intent: $('#flow-intent'), tool: $('#flow-tool'), result: $('#flow-result'),
    overlay: $('#flow-overlay'), card: $('#flow-dropcard'), photo: $('#flow-photo'), cursor: $('#flow-cursor'), ripple: $('#flow-ripple'), cap: $('#flow-cap'), ring: $('#flow-ring'),
  };
  const TYPE_AT = 18.45, TYPE_STEP = 0.105, TYPED = 'smaller';
  const intentFrames = [];
  for (let k = 1; k <= TYPED.length; k++) {
    const img = document.createElement('img');
    img.className = 'shot';
    img.src = CAP + `intent-${k}.png`;
    S4.intent.after(img);
    intentFrames.push(img);
  }
  /** camera: scale s, keep window point (fx, fy) at stage point (sx, sy) */
  function camera(node, s, fx, fy, sx = 960, sy = 540, extra = {}) {
    put(node, { x: sx - fx * s, y: sy - fy * s, s, ...extra });
    node.style.transformOrigin = '0 0';
  }
  const path = (t, keys) => {
    // keys: [[time, x, y], …] eased between
    if (t <= keys[0][0]) return keys[0].slice(1);
    for (let i = 0; i < keys.length - 1; i++) {
      const [a, ax, ay] = keys[i], [b, bx, by] = keys[i + 1];
      if (t <= b) { const p = E.inOut(prog(t, a, b)); return [lerp(ax, bx, p), lerp(ay, by, p)]; }
    }
    return keys[keys.length - 1].slice(1);
  };
  function caption(node, t, a, b, html, dark = false) {
    const v = win(t, a, b, 0.45, 0.35, E.out);
    if (node.dataset.html !== html) { node.innerHTML = html; node.dataset.html = html; }
    node.className = `abs caption ${dark ? 'glass-dark' : 'glass'}`;
    return v;
  }
  function flow(t) {
    const v = win(t, 14.8, 27.6, 0.6, 0.4);
    show(S4.root, v > 0);
    if (v <= 0) return;
    const row1 = B('intentRow1', [1050, 221]);
    const intentInput = B('intentInput', [1000, 152]);
    const run = B('runButton', [1098, 531]);
    const stageC = B('stage', [820, 280]);
    const badge = B('sizeBadge', [1290, 464]);

    // camera keyframes [t, scale, fx, fy]
    const camKeys = [
      [14.8, 0.84, 720, 470], [15.7, 0.9, 720, 450], [17.3, 0.9, 720, 450],
      [18.1, 1.32, intentInput[0] - 40, 330], [19.95, 1.32, intentInput[0] - 40, 340],
      [20.7, 0.9, 720, 450], [22.6, 0.9, 720, 450],
      [23.9, 1.42, stageC[0], stageC[1] + 30], [25.0, 1.42, stageC[0] + 40, stageC[1] + 30],
      [26.2, 1.85, badge[0] - 260, badge[1] - 110], [27.7, 1.95, badge[0] - 250, badge[1] - 105],
    ];
    let s = camKeys[0][1], fx = camKeys[0][2], fy = camKeys[0][3];
    for (let i = 0; i < camKeys.length - 1; i++) {
      const [a, sa, xa, ya] = camKeys[i], [b, sb, xb, yb] = camKeys[i + 1];
      if (t >= a && t <= b) { const p = E.inOut(prog(t, a, b)); s = lerp(sa, sb, p); fx = lerp(xa, xb, p); fy = lerp(ya, yb, p); break; }
      if (t > b) { s = sb; fx = xb; fy = yb; }
    }
    const enter = ease(t, 14.8, 15.7, E.out5);
    camera(S4.cam, s, fx, fy, 960, 540 + (1 - enter) * 120, { o: v });

    // screens
    const intentOn = ease(t, 17.05, 17.4);
    const toolOn = ease(t, 20.05, 20.35);
    const resultOn = ease(t, 21.95, 22.3);
    put(S4.home, { o: 1 });
    put(S4.intent, { o: intentOn * (1 - toolOn) });
    const typed = clamp(Math.floor((t - TYPE_AT) / TYPE_STEP) + 1, 0, TYPED.length);
    intentFrames.forEach((img, k) => put(img, { o: (k === typed - 1 ? 1 : 0) * (1 - toolOn) }));
    put(S4.tool, { o: toolOn });
    put(S4.result, { o: resultOn });

    // drag-in photo and the drop overlay
    const carry = path(t, [[15.5, 1700, -260], [16.95, 720, 405]]);
    const dropped = prog(t, 16.95, 17.25);
    const over = ease(t, 16.15, 16.5) * (1 - ease(t, 16.95, 17.3));
    S4.overlay.style.opacity = over;
    put(S4.card, { s: lerp(0.94, 1, over) + Math.sin(t * 8) * 0.004 * over });
    put(S4.photo, { x: carry[0] - 127, y: carry[1] - 94, r: lerp(8, -3, prog(t, 15.5, 16.95)) * (1 - dropped), s: lerp(1.05, 0.4, E.in(dropped)), o: (t > 15.45 ? 1 : 0) * (1 - dropped) });

    // the cursor
    const cur = path(t, [
      [15.5, 1700 + 60, -260 + 60], [16.95, 720 + 60, 405 + 60],
      [17.6, 880, 600], [18.2, intentInput[0] + 160, intentInput[1] + 70],
      [19.25, intentInput[0] + 160, intentInput[1] + 70], [19.85, row1[0] - 120, row1[1] + 4],
      [20.9, row1[0] - 120, row1[1] + 4], [21.65, run[0], run[1] + 2],
      [22.5, run[0] + 60, run[1] + 160],
    ]);
    const press = (at) => Math.exp(-Math.pow((t - at) / 0.07, 2));
    const pr = Math.max(press(19.9), press(21.75));
    put(S4.cursor, { x: cur[0] - 6, y: cur[1] - 2, s: 1 - pr * 0.18, o: (t > 15.45 && t < 22.9 ? 1 : 0) * (1 - ease(t, 22.4, 22.9)) });
    const rip = (at) => { const p = prog(t, at, at + 0.5); return p > 0 && p < 1 ? p : 0; };
    const rp = rip(19.9) || rip(21.75);
    const rpos = t < 21 ? [row1[0] - 120, row1[1] + 4] : run;
    put(S4.ripple, { x: rpos[0], y: rpos[1], s: 0.3 + E.out(rp) * 1.1, o: rp > 0 ? (1 - rp) * 0.9 : 0 });

    // a soft ring around the size change once we're close enough to read it
    const hl = win(t, 25.9, 27.6, 0.45, 0.4, E.out);
    put(S4.ring, { x: badge[0] - 120, y: badge[1] - 22, s: lerp(1.08, 1, hl), o: hl * v });
    S4.ring.style.transformOrigin = '50% 50%';

    // captions
    let cv = 0;
    if (t < 20.1) cv = caption(S4.cap, t, 17.6, 20.0, 'Say what you want <small>“smaller”, “jpg”, “no location” — or press <span class="key">1</span>–<span class="key">6</span></small>');
    else if (t < 23) cv = caption(S4.cap, t, 20.4, 22.7, 'The right tool, already filled in <small>Settings you can see. One click to run.</small>');
    else if (t < 25.5) cv = caption(S4.cap, t, 23.2, 25.4, 'Before and after, side by side <small>Compare it before you keep it.</small>');
    else cv = caption(S4.cap, t, 25.6, 27.5, 'Half the size. Same photo. <small>And it never left your computer.</small>');
    const cw = S4.cap.offsetWidth;
    put(S4.cap, { x: 960 - cw / 2, y: 900 + (1 - cv) * 24, o: cv * v });
  }

  // ============================================================ 6. montage
  const S6 = { root: $('#s-montage') };
  const MONTAGE = [
    { src: 'json-dark', title: 'Format JSON as you type', sub: 'Errors point at the exact spot.', zoom: 1.25, fx: 880, fy: 380 },
    { src: 'redact-dark', title: 'Redact what’s private', sub: 'Blur, pixelate or black out, by hand.', zoom: 1.3, fx: 880, fy: 330 },
    { src: 'palette-dark', title: 'Any tool, two keys away', sub: '⌘K or Ctrl K searches every tool and file.', zoom: 1.75, fx: 780, fy: 230 },
    { src: 'system-dark', title: 'Know your machine', sub: 'Battery health, disks, memory, heat.', zoom: 1.3, fx: 900, fy: 330 },
  ];
  const M0 = 27.5, MSTEP = 1.875;
  const mItems = MONTAGE.map((m) => {
    const frame = el(`<div class="mframe"><img src="${CAP}${m.src}.png"></div>`);
    const cap = el(`<div class="abs mcap">${m.title}<small>${m.sub}</small></div>`);
    S6.root.append(frame, cap);
    return { frame, cap, img: frame.querySelector('img'), m };
  });
  function montage(t) {
    const v = win(t, M0 - 0.3, M0 + MONTAGE.length * MSTEP + 0.45, 0.5, 0.5);
    show(S6.root, v > 0);
    if (v <= 0) return;
    mItems.forEach((m, i) => {
      const a = M0 + i * MSTEP, b = a + MSTEP;
      const pin = ease(t, a - 0.25, a + 0.45, E.out5);
      const pout = ease(t, b - 0.25, b + 0.3, E.in);
      const last = i === MONTAGE.length - 1;
      const exitK = last ? 0 : pout;
      const sc = 0.86;
      const x = 640 + (1 - pin) * 520 - exitK * 700;
      const y = 540 - 450 * sc + Math.sin((t - a) * 0.8) * 4;
      put(m.frame, { x, y, s: sc * lerp(1.04, 1, pin), persp: 2400, ry: lerp(-24, -7, pin) + exitK * 24, o: v * pin * (1 - exitK) * (last ? 1 - ease(t, b, b + 0.4) : 1) });
      m.frame.style.transformOrigin = '0 0';
      m.frame.style.zIndex = i;
      // slow push-in on the part of the screen worth reading
      const z = lerp(m.m.zoom * 0.94, m.m.zoom, E.sine(prog(t, a - 0.25, b + 0.3)));
      m.img.style.transformOrigin = '0 0';
      m.img.style.transform = `translate(${(720 - m.m.fx * z).toFixed(1)}px, ${(450 - m.m.fy * z).toFixed(1)}px) scale(${z.toFixed(4)})`;
      const cin = ease(t, a, a + 0.5, E.out5), cout = ease(t, b - 0.3, b, E.in);
      put(m.cap, { x: 110, y: 540 - m.cap.offsetHeight / 2 + (1 - cin) * 40 - cout * 30, o: v * cin * (1 - cout), blur: (1 - cin) * 8 + cout * 8 });
    });
  }

  // ============================================================ 7. count
  const S7 = { root: $('#s-count'), num: $('#count-num'), label: $('#count-label'), host: $('#cats') };
  const CATS = [
    ['image', 'Images', 30], ['video', 'Video', 27], ['audio', 'Audio', 21], ['speech', 'Speech & voice', 9],
    ['pdf', 'PDF & documents', 27], ['files', 'Files', 23], ['text', 'Text', 27], ['dev', 'Developer', 34],
    ['data', 'Data', 12], ['network', 'Network', 18], ['design', 'Design', 9], ['system', 'System', 12],
  ];
  const TOTAL = CATS.reduce((a, c) => a + c[2], 0);
  const cats = CATS.map(([ic, name, n], i) => {
    const node = el(`<div class="abs cat glass-dark">${icon(ic)}<span>${name}</span><span class="n">${n}</span></div>`);
    S7.host.append(node);
    return { node, col: i % 4, row: Math.floor(i / 4) };
  });
  function count(t) {
    const v = win(t, 34.7, 40.2, 0.4, 0.55);
    show(S7.root, v > 0);
    if (v <= 0) return;
    const c = ease(t, 35.0, 36.7, E.expo);
    S7.num.textContent = Math.round(c * TOTAL);
    const up = ease(t, 36.3, 37.1, E.inOut);
    if (!S7.finalW) { const keep = S7.num.textContent; S7.num.textContent = String(TOTAL); S7.finalW = S7.num.offsetWidth; S7.num.textContent = keep; }
    S7.num.style.width = `${S7.finalW}px`;
    S7.num.style.textAlign = 'right';
    const nw = S7.finalW, lw = S7.label.offsetWidth;
    const total = nw + 36 + lw;
    const pin = ease(t, 34.9, 35.5, E.out5);
    const ny = lerp(370, 150, up);
    put(S7.num, { x: 960 - total / 2, y: ny + (1 - pin) * 50, o: v * pin, s: 1 + Math.exp(-Math.max(0, t - 36.7) * 6) * 0.03 * (t > 36.7 ? 1 : 0) });
    const lp = ease(t, 36.0, 36.6);
    put(S7.label, { x: 960 - total / 2 + nw + 36, y: ny + 222 + (1 - lp) * 16, o: v * lp });
    cats.forEach((k, i) => {
      const at = 36.75 + (k.row * 4 + k.col) * 0.055;
      const p = ease(t, at, at + 0.6, E.out5);
      const x = 264 + k.col * 354, y = 560 + k.row * 116;
      put(k.node, { x, y: y + (1 - p) * 50, s: lerp(0.9, 1, p), o: v * p });
    });
  }

  // ============================================================ 8. interfaces
  const S8 = { root: $('#s-iface'), h: $('#iface-h'), spec: $('#iface-spec'), specId: $('#iface-spec-id'), wires: $('#wires'), host: $('#ifaces') };
  S8.specId.innerHTML = icon('braces') + '<span>image.convert</span>';
  const s8w = words(S8.h, 'Write a tool once. Use it everywhere.');
  const CMD = 'dogubako image convert photo.heic --format webp';
  const IFACES = [
    { ic: 'laptop', t: 'Desktop app', b: 'macOS on Apple silicon.\nNo account. Works offline.', x: 90, y: 300 },
    { ic: 'server', t: 'Self-hosted web', b: 'One server for your house or team.\nPhones and laptops just open it.', x: 90, y: 640 },
    { ic: 'terminal', t: 'Command line', term: true, x: 1410, y: 300 },
    { ic: 'braces', t: 'HTTP API', term: true, b: '<span class="p">POST</span> /api/v1/tools/\n     image.convert/run', x: 1410, y: 640 },
    { ic: 'bot', t: 'MCP, for AI agents', b: 'Each agent gets its own token,\nlimited to the tools you allow.', x: 750, y: 820 },
  ];
  const ifaces = IFACES.map((f, i) => {
    const node = el(`<div class="abs iface glass"><div class="t">${icon(f.ic)}<span>${f.t}</span></div><div class="b ${f.term ? 'term' : ''}">${f.b || ''}</div></div>`);
    S8.host.append(node);
    const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    p.setAttribute('pathLength', '1');
    S8.wires.append(p);
    return { ...f, node, body: node.querySelector('.b'), wire: p, at: 41.1 + i * BEAT };
  });
  const SPEC = { x: 680, y: 380, w: 560 };
  function iface(t) {
    const v = win(t, 39.7, 47.6, 0.5, 0.55);
    show(S8.root, v > 0);
    if (v <= 0) return;
    revealWords(s8w, t, 40.15, 0.07);
    put(S8.h, { x: 960 - measure(S8.h).w / 2, y: 110, o: v });
    const sp = ease(t, 40.4, 41.0, E.out5);
    put(S8.spec, { x: SPEC.x, y: SPEC.y + (1 - sp) * 40, s: lerp(0.94, 1, sp), o: v * sp });
    const sh = S8.spec.offsetHeight;
    const specC = [SPEC.x + SPEC.w / 2, SPEC.y + sh / 2];
    ifaces.forEach((f) => {
      const w = f.node.offsetWidth, h = f.node.offsetHeight;
      const cx = f.x + w / 2, cy = f.y + h / 2;
      // wire: from the spec card's edge to the interface card's edge
      let x1, y1, x2, y2;
      if (f.x < SPEC.x) { x1 = SPEC.x; y1 = specC[1] + (cy - specC[1]) * 0.25; x2 = f.x + w; y2 = cy; }
      else if (f.x > SPEC.x + SPEC.w) { x1 = SPEC.x + SPEC.w; y1 = specC[1] + (cy - specC[1]) * 0.25; x2 = f.x; y2 = cy; }
      else { x1 = specC[0]; y1 = SPEC.y + sh; x2 = cx; y2 = f.y; }
      const mx = (x1 + x2) / 2;
      const d = Math.abs(x2 - x1) > 50
        ? `M${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`
        : `M${x1} ${y1} C ${x1} ${(y1 + y2) / 2}, ${x2} ${(y1 + y2) / 2}, ${x2} ${y2}`;
      f.wire.setAttribute('d', d);
      f.wire.setAttribute('stroke-dashoffset', (1 - ease(t, f.at - 0.35, f.at + 0.1, E.inOut)).toFixed(4));
      f.wire.style.opacity = v;
      const p = ease(t, f.at, f.at + 0.55, E.out5);
      put(f.node, { x: f.x + (1 - p) * (f.x < 960 ? -30 : 30), y: f.y + (f.y > 800 ? (1 - p) * 30 : 0), s: lerp(0.95, 1, p), o: v * p });
      if (f.term && !f.b) {
        const n = clamp(Math.floor((t - (f.at + 0.3)) / 0.032), 0, CMD.length);
        const caret = (Math.floor(t * 2.5) % 2 === 0 || n < CMD.length) ? '▍' : ' ';
        const txt = CMD.slice(0, n);
        const wrapped = txt.length > 23 ? txt.slice(0, 23) + '\n  ' + txt.slice(23) : txt;
        f.body.innerHTML = `<span class="p">$</span> ${wrapped}${caret}`;
      }
    });
  }

  // ============================================================ 9. local
  const S9 = { root: $('#s-local'), ring: $('#local-ring'), orbs: $('#local-orbs'), icon: $('#local-icon'), h: $('#local-h'), nevers: $('#local-nevers'), label: $('#local-engines-label'), engines: $('#local-engines') };
  S9.icon.innerHTML = icon('laptop', 'width="150" height="150" style="stroke:#edeae4;stroke-width:1.2"');
  S9.h.innerHTML = '<span class="word">Nothing</span> <span class="word">leaves</span><br><span class="word">your</span> <span class="word">computer.</span>';
  S9.h.style.fontSize = '84px';
  const s9w = [...S9.h.querySelectorAll('.word')];
  const NEVERS = ['No account needed.', 'No telemetry.', 'No paywalls.', 'Open source, AGPL-3.0.'];
  const nevers = NEVERS.map((n) => {
    const node = el(`<div class="abs never" style="display:flex;align-items:center;gap:18px">${icon('shield', 'width="40" height="40" style="stroke:#5fcb8e"')}<span>${n}</span></div>`);
    S9.nevers.append(node);
    return node;
  });
  const orbs = [
    `<div class="orb"><img src="${ASSET}dune.jpg"></div>`,
    `<div class="orb doclike">${'<i></i>'.repeat(5)}</div>`,
    `<div class="orb"><img src="${ASSET}meadow.jpg"></div>`,
    `<div class="orb doclike" style="background:#1f1e1b">${'<i style="background:#3d3a35"></i>'.repeat(5)}</div>`,
    `<div class="orb"><img src="${ASSET}dusk.png"></div>`,
    `<div class="orb doclike">${'<i></i>'.repeat(5)}</div>`,
  ].map((h) => { const n = el(h); n.classList.add('abs'); S9.orbs.append(n); return n; });
  const ENGINES = ['FFmpeg', 'ImageMagick', 'libvips', 'ExifTool', 'qpdf', 'Poppler', 'Ghostscript', 'Tesseract', 'whisper.cpp', 'Piper', 'LibreOffice', 'Pandoc', '7-Zip', 'Rust'];
  S9.engines.innerHTML = [...ENGINES, ...ENGINES].map((e) => `<div class="chip-dark">${e}</div>`).join('');
  S9.engines.style.maskImage = S9.engines.style.webkitMaskImage = 'none';
  function local(t) {
    const v = win(t, 47.2, 55.0, 0.6, 0.5);
    show(S9.root, v > 0);
    if (v <= 0) return;
    const C = [520, 470], R = 290;
    const rin = ease(t, 47.4, 48.4, E.out5);
    put(S9.ring, { x: C[0] - R, y: C[1] - R, s: lerp(0.8, 1, rin), o: v * rin, r: t * 4 });
    S9.ring.style.width = S9.ring.style.height = `${2 * R}px`;
    put(S9.icon, { x: C[0] - 75, y: C[1] - 80, o: v * rin, s: lerp(0.9, 1, rin) });
    orbs.forEach((o, i) => {
      const a = (i / orbs.length) * Math.PI * 2 + t * 0.32;
      const p = ease(t, 47.6 + i * 0.12, 48.4 + i * 0.12, E.out5);
      const r = R * lerp(0.4, 1, p);
      // a gentle tug outward that the boundary always wins
      const tug = Math.max(0, Math.sin(t * 1.3 + i * 1.7)) * 14;
      put(o, { x: C[0] + Math.cos(a) * (r + tug) - 55, y: C[1] + Math.sin(a) * (r + tug) * 0.98 - 42, r: Math.sin(a) * 8, o: v * p });
    });
    revealWords(s9w, t, 48.1, 0.11, 0.7, 40);
    put(S9.h, { x: 1010, y: 220, o: v });
    nevers.forEach((n, i) => {
      const at = 49.4 + i * BEAT;
      const p = ease(t, at, at + 0.55, E.out5);
      put(n, { x: 1014 + (1 - p) * 30, y: 480 + i * 74, o: v * p });
    });
    const ep = ease(t, 51.9, 52.7);
    put(S9.label, { x: 960 - S9.label.offsetWidth / 2, y: 862 + (1 - ep) * 12, o: v * ep });
    const half = S9.engines.scrollWidth / 2;
    const ex = -((t - 51.9) * 90 % half);
    put(S9.engines, { x: ex, y: 920, o: v * ep });
  }

  // ============================================================ 10. themes
  const S10 = { root: $('#s-theme'), cam: $('#theme-cam'), dark: $('#theme-dark-shot'), phone: $('#theme-phone'), cap: $('#theme-cap'), phoneImg: $('#theme-phone img') };
  S10.phoneImg.src = CAP + 'phone-home-dark.png';
  function themes(t) {
    const v = win(t, 54.85, 60.0, 0.5, 0.45);
    show(S10.root, v > 0);
    if (v <= 0) return;
    const toggle = B('themeToggle', [223, 43]);
    const enter = ease(t, 54.85, 55.7, E.out5);
    const shift = ease(t, 57.6, 58.5, E.inOut);
    const s = lerp(0.8, 0.72, shift);
    camera(S10.cam, s, 720, 450, lerp(960, 730, shift), 520 + (1 - enter) * 100, { o: v });
    const rp = ease(t, 56.35, 57.25, E.inOut);
    S10.dark.style.clipPath = `circle(${(rp * 1750).toFixed(1)}px at ${toggle[0]}px ${toggle[1]}px)`;
    const pp = ease(t, 57.7, 58.6, E.out5);
    put(S10.phone, { x: 1420 + (1 - pp) * 500, y: 130 + (1 - pp) * 40, r: (1 - pp) * 6, o: v * pp });
    let cv;
    if (t < 57.6) cv = caption(S10.cap, t, 55.5, 57.5, 'Light or dark <small>Sidebar or none. Pin what you use.</small>', t > 56.8);
    else cv = caption(S10.cap, t, 57.9, 60.1, 'Your desk, your server, your phone <small>The same app everywhere.</small>', true);
    put(S10.cap, { x: (t < 57.6 ? 960 : 760) - S10.cap.offsetWidth / 2, y: 920 + (1 - cv) * 24, o: cv * v });
  }

  // ============================================================ 11. end
  const S11 = { root: $('#s-end'), tile: $('#end-tile'), latch: $('#end-latch'), word: $('#end-word'), tag: $('#end-tag'), alpha: $('#end-alpha'), meta: $('#end-meta'), url: $('#end-url') };
  S11.word.innerHTML = 'Dogubako'.split('').map((c) => `<span class="word">${c}</span>`).join('');
  const s11Letters = [...S11.word.querySelectorAll('.word')];
  function end(t) {
    const v = win(t, 59.7, 80, 0.4, 0.1);
    show(S11.root, v > 0);
    if (v <= 0) return;
    const tp = ease(t, 59.7, 60.4, E.back);
    put(S11.tile, { x: 960 - 100, y: 200, s: lerp(0.6, 1, tp), o: v * ease(t, 59.7, 60.0) });
    S11.tile.style.transformOrigin = '50% 50%';
    S11.tile.style.filter = `drop-shadow(0 30px 50px rgba(0,0,0,.6)) drop-shadow(0 0 ${(40 * Math.exp(-Math.max(0, t - 60.02) * 2) * (t > 60.02 ? 1 : 0)).toFixed(1)}px rgba(224,83,61,.5))`;
    const lp = prog(t, 59.8, 60.02);
    const ly = t < 60.02 ? -26 * (1 - lp * lp) : -3 * Math.exp(-(t - 60.02) * 14) * Math.abs(Math.sin((t - 60.02) * 30));
    S11.latch.setAttribute('transform', `translate(0 ${ly.toFixed(2)})`);
    S11.latch.style.opacity = clamp(lp * 3);
    s11Letters.forEach((c, i) => {
      const p = ease(t, 60.5 + i * 0.05, 61.2 + i * 0.05, E.out5);
      put(c, { y: (1 - p) * 70, o: p, blur: (1 - p) * 12 });
    });
    const push = 1 + E.sine(prog(t, 60, 72)) * 0.04;
    S11.root.style.transformOrigin = '50% 52%';
    S11.root.style.transform = `scale(${push.toFixed(4)})`;
    put(S11.word, { x: 960 - S11.word.offsetWidth / 2, y: 430, o: v });
    const items = [[S11.tag, 61.3, 640], [S11.alpha, 61.9, 730], [S11.url, 62.4, 820], [S11.meta, 62.8, 890]];
    items.forEach(([n, at, y]) => {
      const p = ease(t, at, at + 0.7, E.out5);
      put(n, { x: 960 - n.offsetWidth / 2, y: y + (1 - p) * 20, o: v * p, blur: (1 - p) * 6 });
    });
  }

  // ============================================================ frame
  const bgDark = $('#bg-dark'), vignette = $('#vignette'), fade = $('#fade');
  window.render = (t) => {
    const g = ground(t);
    bgDark.style.opacity = g.toFixed(4);
    vignette.style.opacity = (g * 0.9).toFixed(4);
    renderAmbient(t, g, t < 0.6 ? ease(t, 0, 0.6) : 1);
    logo(t); slipsScene(t); bench(t); flow(t); montage(t); count(t); iface(t); local(t); themes(t); end(t);
    fade.style.opacity = Math.max(1 - ease(t, 0, 0.35), ease(t, 69.6, 71.8, E.sine)).toFixed(4);
  };
  window.DURATION = DURATION;

  // ------------------------------------------------------------ preview helpers
  const q = new URLSearchParams(location.search);
  const ready = Promise.all([document.fonts.ready, ...[...document.images].map((i) => i.decode().catch(() => null))]);
  window.trailerReady = ready.then(() => true);
  ready.then(() => {
    if (q.has('play')) {
      const audio = new Audio('../build/music.wav');
      const t0 = performance.now() - (parseFloat(q.get('play')) || 0) * 1000;
      audio.currentTime = (performance.now() - t0) / 1000;
      audio.play().catch(() => {});
      const loop = () => { const t = (performance.now() - t0) / 1000; window.render(t); if (t < DURATION) requestAnimationFrame(loop); };
      loop();
    } else {
      window.render(parseFloat(q.get('t') || '0'));
    }
  });
})();
