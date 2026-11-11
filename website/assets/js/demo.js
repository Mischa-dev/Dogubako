/* demo.js: the app window plays itself.
 *
 * 1. Builds the window's dynamic bits, measures where things are, and lays the whole demo out as one
 *    timeline (timeline.js). Steps are named times on that timeline; the list next to the window
 *    follows it and seeks it.
 * 2. On wide screens the window starts in the hero, tilted, and the scroll position flies it into its
 *    place in the demo section, flat. The demo starts playing once it has landed.
 * Reduced motion: no flight, no autoplay; the steps seek to composed stills. */
(function () {
  'use strict';
  const { Timeline, Player, EASE, reduceMotion } = window.DGB;
  const $ = (s, r) => (r || document).querySelector(s);
  const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  const win = $('#win');
  if (!win) return;
  const ws = $('#ws'), fly = $('#fly'), stage = $('#demoStage'), sec = $('#demo'), heroSlot = $('#heroSlot'), cue = $('#cue');
  const root = document.documentElement;
  const CATS = window.DGB_CATS, TOOLS = window.DGB_TOOLS;
  const still = reduceMotion();
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const lerp = (a, b, p) => a + (b - a) * p;

  /* ------------------------------------------------------------ static content */
  const icon = (n) => `<svg class="i"><use href="#i-${n}"/></svg>`;
  const counts = CATS.map((_, i) => TOOLS.filter((t) => t[0] === i).length);
  $('#sbCats').insertAdjacentHTML('beforeend', CATS.map((c, i) => `<div class="a-nav">${icon(c[2])}<span>${c[1]}</span><span class="a-count">${counts[i]}</span></div>`).join(''));

  // waveform bars, drawn the way BenchObject.tsx draws them (deterministic per file name)
  (function bars(seed, n) {
    let h = 2166136261;
    for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
    let html = '';
    for (let i = 0; i < n; i++) {
      h = Math.imul(h ^ (h >>> 13), 1274126177);
      const r = ((h >>> 0) % 1000) / 1000;
      html += `<span style="height:${Math.round(8 + r * 34 * (0.45 + 0.55 * Math.sin(i / 4) ** 2))}px"></span>`;
    }
    $('#benchWave').innerHTML = html;
  })('standup.m4a', 35);

  // command palette: real tools, filtered the way you'd expect for each keystroke
  const byName = new Map(TOOLS.map((t) => [t[1] + '|' + t[0], t]));
  const tool = (name, cat) => byName.get(name + '|' + cat);
  const shortHint = (d) => d.split(/:\s/)[0];
  function search(q) {
    const re = new RegExp('(^|[\\s(/&-])' + q, 'i');
    return TOOLS.filter((t) => re.test(t[1]))
      .map((t, i) => ({ t, i, s: t[1].toLowerCase().startsWith(q) ? 0 : 1 }))
      .sort((a, b) => a.s - b.s || a.i - b.i)
      .slice(0, 7)
      .map((x) => x.t);
  }
  const hl = (name, q) => (q ? name.replace(new RegExp('(^|[\\s(/&-])(' + q + ')', 'ig'), '$1<b>$2</b>') : name);
  const PAL_Q = 'subtitle';
  const PAL_STATES = [{ q: '', rows: [tool('Compress Images', 0), tool('Remove Image Metadata', 0), tool('Compress PDF', 4), tool('Normalize Audio', 2), tool('Trim Video', 1), tool('Clean CSV Data', 8), tool('Convert Images', 0)] }]
    .concat(Array.from(PAL_Q, (_, i) => ({ q: PAL_Q.slice(0, i + 1), rows: search(PAL_Q.slice(0, i + 1)) })));
  const lastRows = PAL_STATES[PAL_STATES.length - 1].rows;
  const PAL_PICK = Math.min(4, Math.max(0, lastRows.findIndex((t) => t[2] === 'speech.subtitles')));
  const palRows = $('#palRows');
  const rowEls = [];
  for (let i = 0; i < 7; i++) {
    const el = document.createElement('div');
    el.className = 'a-pal-row';
    el.style.top = i * 50 + 'px';
    palRows.appendChild(el);
    rowEls.push(el);
  }
  const rowHTML = (t, q) => (t ? `<span class="a-pal-ic">${icon(CATS[t[0]][2])}</span><span class="a-pal-t"><span class="a-pal-l">${hl(t[1], q)}</span><span class="a-pal-s">${shortHint(t[3])}</span></span><span class="a-pal-c">${CATS[t[0]][1]}</span>` : '');

  /* ------------------------------------------------------------ steps (times on the timeline) */
  const D = 32600;
  const CH = [
    { t: 0, still: 950 },
    { t: 3300, still: 6150 },
    { t: 7300, still: 12400 },
    { t: 13400, still: 24900 },
    { t: 26300, still: 29300 },
  ];
  const SENT = 'remove the location from these and make them smaller for email';

  /* ------------------------------------------------------------ measuring */
  function pos(el) {
    let x = 0, y = 0, e = el;
    while (e && e !== ws) {
      x += e.offsetLeft; y += e.offsetTop;
      const p = e.offsetParent;
      if (p && p !== ws) { x += p.clientLeft; y += p.clientTop; }
      e = p;
    }
    return { x, y, w: el.offsetWidth, h: el.offsetHeight, cx: x + el.offsetWidth / 2, cy: y + el.offsetHeight / 2 };
  }

  /* ------------------------------------------------------------ the timeline */
  function build(phone) {
    const tl = new Timeline(D);
    const dw = phone ? 390 : 1200;
    const E = (id) => document.getElementById(id);

    const sHome = E('sHome'), sIntent = E('sIntent'), sTool = E('sTool'), sMany = E('sMany'), sDo = E('sDo');
    const cursor = E('cursor'), ring = E('cursorRing'), carry = E('carry'), carry2 = E('carry2');
    const badge = $('.a-carry-badge', carry), badge2 = $('.a-carry-badge', carry2);

    const fade = (el, keys) => tl.to(el, 'o', keys);
    const blink = (el, from, to, solid) => { // caret: solid while typing, then blinks
      const ks = [[0, 0, 'hold'], [from, 1, 'hold']];
      let t = solid || from, on = true;
      while (t < to) { t += 530; on = !on; if (t < to) ks.push([t, on ? 1 : 0, 'hold']); }
      ks.push([to, 0, 'hold']);
      tl.to(el, 'o', ks);
    };
    const typeKeys = (t0, str, per) => { const ks = [[0, '']]; for (let i = 1; i <= str.length; i++) ks.push([t0 + (i - 1) * per, str.slice(0, i)]); return ks; };
    const cursorScale = [[0, 1]], ringO = [[0, 0]], ringS = [[0, 0.5]];
    const press = (t) => { // pointer press + ripple
      cursorScale.push([t - 60, 1], [t + 40, 0.86, 'ease'], [t + 220, 1, 'out']);
      ringO.push([t, 0, 'hold'], [t + 10, 0.55, 'hold'], [t + 460, 0, 'out']);
      ringS.push([t, 0.5, 'hold'], [t + 460, 1.5, 'out']);
    };
    const taps = [];

    // ---------------- screens
    fade(sHome, [[0, 1], [2180, 1], [2400, 0, 'ease'], [30700, 0], [31200, 1, 'ease']]);
    tl.to(sHome, 'y', [[0, 0], [2180, 0], [2400, -6, 'ease'], [30700, 10], [31200, 0, 'out']]);
    fade(sIntent, [[0, 0], [2380, 0], [2720, 1, 'ease'], [7150, 1], [7330, 0, 'ease']]);
    tl.to(sIntent, 'y', [[0, 0], [2350, 12], [2850, 0, 'out'], [7150, 0], [7400, -8, 'ease']]);
    fade(sTool, [[0, 0], [7320, 0], [7650, 1, 'ease'], [15000, 1], [15200, 0, 'ease']]);
    tl.to(sTool, 'y', [[0, 0], [7250, 12], [7800, 0, 'out']]);
    fade(sMany, [[0, 0], [15050, 0], [15350, 1, 'ease'], [19550, 1], [19750, 0, 'ease']]);
    tl.to(sMany, 'y', [[0, 0], [15000, 12], [15500, 0, 'out'], [19550, 0], [19800, -8, 'ease']]);
    fade(sDo, [[0, 0], [19700, 0], [20050, 1, 'ease'], [30600, 1], [30850, 0, 'ease']]);
    tl.to(sDo, 'y', [[0, 0], [19650, 12], [20200, 0, 'out']]);

    // ---------------- 1. drop a photo
    const opts = pos(E('opts'));
    const fig = pos(E('figPrint'));
    const start = phone ? [dw + 120, 470] : [dw + 70, 560];
    const dropAt = phone ? [205, 400] : [700, 400];
    const carryW = phone ? 160 : 192;
    tl.path(carry, [[0, start], [300, start], [1650, dropAt, 'inOut', 0.08], [2150, [dropAt[0] + 6, dropAt[1] + 4], 'ease', 0], [2780, [fig.cx, fig.cy + 0.5], 'out', -0.06]]);
    tl.to(carry, 's', [[0, 1], [2150, 1], [2780, fig.w / carryW, 'out']]);
    tl.to(carry, 'r', [[0, -9], [300, -9], [1650, -4, 'inOut'], [2150, -3], [2780, 0, 'out']]);
    fade(carry, [[0, 0], [260, 0], [420, 1, 'ease'], [2780, 1, 'hold'], [2790, 0, 'hold']]);
    fade(badge, [[0, 1], [2150, 1], [2320, 0, 'ease']]);
    fade(E('figPrint'), [[0, 0], [2780, 0, 'hold'], [2790, 1, 'hold']]);
    // the drop overlay shows twice: for the photo, then for the four
    const drop = E('dropOv'), dropCard = $('.a-drop-card', drop);
    const dropKeys = [[0, 0], [820, 0], [1020, 1, 'ease'], [2150, 1], [2380, 0, 'ease'], [14150, 0], [14350, 1, 'ease'], [15250, 1], [15480, 0, 'ease']];
    fade(drop, dropKeys);
    // the app behind it softens (a filter on the layers, not a backdrop-filter, which smears mirrored text at the edges once the window is scaled)
    const under = $$('.a-topbar, .a-sb, .a-main', E('app'));
    tl.fn(dropKeys, (v) => { const f = v > 0.01 ? `blur(${(v * 5).toFixed(2)}px)` : ''; under.forEach((el) => { el.style.filter = f; }); }, 100);
    tl.to(dropCard, 's', [[0, 0.96], [820, 0.96], [1100, 1, 'out'], [2150, 1], [2380, 0.98, 'ease'], [14150, 0.96], [14430, 1, 'out'], [15250, 1], [15480, 0.98, 'ease']]);
    tl.to(E('traySheet'), 'y', [[0, -26], [1000, -26], [1500, 0, 'inOut'], [1950, -24, 'inOut'], [2150, -12, 'inOut'], [14300, -26], [14800, 0, 'inOut'], [15250, -14, 'inOut']]);

    // ---------------- 2. say what should happen
    const ROW = 58;
    const slotY = (i) => i * ROW;
    const opt = (k) => $(`.a-opt[data-o="${k}"]`, sIntent);
    const oConvert = opt('convert'), oCompress = opt('compress'), oRedact = opt('redact'), oResize = opt('resize'), oMeta = opt('meta');
    const R0 = 5300, R1 = 5800; // re-rank window
    const appear = (el, i, t0, more) => { const t = t0 + i * 70; fade(el, [[0, 0], [t, 0], [t + 320, 1, 'ease']].concat(more || [])); };
    appear(E('optHl'), 0, 2750);
    appear(oConvert, 0, 2750); appear(oCompress, 1, 2750); appear(oRedact, 2, 2750, [[R0, 1], [R0 + 260, 0, 'ease']]); appear(oResize, 3, 2750);
    fade(oMeta, [[0, 0], [R0 + 120, 0], [R1, 1, 'ease']]);
    const slide = (el, a, b, i) => tl.to(el, 'y', [[0, slotY(a) + 8], [2750 + i * 70, slotY(a) + 8], [3150 + i * 70, slotY(a), 'out'], [R0, slotY(a)], [R1, slotY(b), 'out']]);
    slide(oConvert, 0, 2, 0); slide(oCompress, 1, 0, 1); slide(oRedact, 2, 2, 2); slide(oResize, 3, 1, 3);
    tl.to(oMeta, 'y', [[0, slotY(3) + 10], [R0 + 120, slotY(3) + 10], [R1, slotY(3), 'out']]);
    tl.text($('.a-num', oConvert), [[0, '1'], [R0 + 200, '3']]);
    tl.text($('.a-num', oCompress), [[0, '2'], [R0 + 200, '1']]);
    tl.text($('.a-num', oResize), [[0, '4'], [R0 + 200, '2']]);
    tl.cls(oConvert, 'is-active', [[0, true], [R0, false]]);
    tl.cls(oCompress, 'is-active', [[0, false], [R1 - 120, true]]);
    const tClick = 4050, tType = 4350, word = 'smaller', per = 92;
    press(tClick); taps.push(tClick);
    tl.cls(E('intentInput'), 'is-focus', [[0, false], [tClick + 40, true], [7300, false]]);
    fade(E('intentPh'), [[0, 1], [tType - 10, 1, 'hold'], [tType, 0, 'hold']]);
    fade(E('intentKeys'), [[0, 1], [tType - 10, 1, 'hold'], [tType, 0, 'hold']]);
    tl.text(E('intentTyped'), typeKeys(tType, word, per));
    blink(E('intentCaret'), tClick + 40, 7300, tType + word.length * per);
    const pick = [opts.x + (phone ? 150 : 190), opts.y + slotY(0) + 28];
    const tPick = 6500;
    press(tPick); taps.push(tPick);
    tl.to(E('optHl'), 's', [[0, 1], [tPick - 40, 1], [tPick + 60, 0.985, 'ease'], [tPick + 260, 1, 'out']]);

    // ---------------- 3. compare the result
    const run = pos(E('runBtn'));
    const rest = phone ? [run.cx + 70, run.cy + 120] : [run.cx + 40, run.cy + 170];
    const tRun = 8800, tP0 = 8950, tP1 = 10350, tWipe = 10400, tWipe1 = 11050;
    press(tRun); taps.push(tRun);
    tl.text(E('runLbl'), [[0, 'Run'], [tRun + 60, 'Running…'], [tP1 + 60, 'Run']]);
    tl.to(E('runBtn'), 's', [[0, 1], [tRun - 20, 1], [tRun + 80, 0.96, 'ease'], [tRun + 280, 1, 'out']]);
    tl.text(E('resWaitT'), [[0, 'Run to see the result'], [tRun + 60, 'Compressing dune.jpg…']]);
    fade(E('resProg'), [[0, 0], [tRun + 60, 0], [tRun + 200, 1, 'ease']]);
    fade(E('resPct'), [[0, 0], [tRun + 60, 0], [tRun + 200, 1, 'ease']]);
    tl.to(E('resBar'), 'sx', [[0, 0], [tP0, 0], [tP0 + 380, 0.32, 'out'], [tP0 + 900, 0.58, 'inOut'], [tP1, 1, 'inOut']]);
    tl.fn([[0, 0], [tP0, 0], [tP0 + 380, 32, 'out'], [tP0 + 900, 58, 'inOut'], [tP1, 100, 'inOut']], (v) => { E('resPct').textContent = Math.round(v) + '%'; }, 1);
    fade(E('resWait'), [[0, 1], [tP1, 1], [tP1 + 180, 0, 'ease']]);
    const resImg = E('resImg');
    tl.fn([[0, 100], [tWipe, 100], [tWipe1, 0, 'out']], (v) => { resImg.style.clipPath = v >= 100 ? 'inset(0 100% 0 0)' : v <= 0 ? '' : `inset(0 ${v}% 0 0)`; }, 10);
    tl.to(E('resSweep'), 'x', [[0, 0], [tWipe, 0], [tWipe1, pos(resImg).w, 'out']]);
    fade(E('resSweep'), [[0, 0], [tWipe, 0, 'hold'], [tWipe + 10, 1, 'hold'], [tWipe1 - 120, 1], [tWipe1 + 60, 0, 'ease']]);
    fade(E('resMeta'), [[0, 0], [tWipe1 - 150, 0], [tWipe1 + 150, 1, 'ease']]);
    const tCo = 11150, co = E('callout');
    fade(co, [[0, 0], [tCo, 0], [tCo + 260, 1, 'ease']]);
    tl.to(co, 's', [[0, 0.9], [tCo, 0.9], [tCo + 520, 1, 'back']]);
    tl.to(co, 'y', [[0, 12], [tCo, 12], [tCo + 520, 0, 'out']]);
    tl.fn([[0, 4.2], [tCo + 100, 4.2], [tCo + 1100, 1.1, 'out']], (v) => { E('coTo').textContent = v.toFixed(1) + ' MB'; }, 10);
    tl.fn([[0, 0], [tCo + 100, 0], [tCo + 1100, 74, 'out']], (v) => { E('coPct').textContent = String(Math.round(v)); }, 1);

    // ---------------- 4. four photos, one sentence
    const start2 = phone ? [dw + 120, 500] : [dw + 70, 600];
    const dropAt2 = phone ? [200, 410] : [690, 410];
    const fan = pos(E('fan'));
    const fanPrintW = phone ? 168 : 236, fanPrintH = phone ? 112 : 158;
    const fanAt = [fan.cx, fan.y + fan.h - (phone ? 4 : 8) - fanPrintH / 2];
    const tPick2 = 13800, tDrop2 = 15250, tLand2 = 15700;
    tl.path(carry2, [[0, start2], [tPick2, start2], [14950, dropAt2, 'inOut', 0.08], [tDrop2, [dropAt2[0] + 6, dropAt2[1] + 4], 'ease', 0], [tLand2, fanAt, 'out', -0.06]]);
    tl.to(carry2, 's', [[0, 1], [tDrop2, 1], [tLand2, fanPrintW / carryW, 'out']]);
    tl.to(carry2, 'r', [[0, -9], [tPick2, -9], [14950, -4, 'inOut'], [tDrop2, -3], [tLand2, 0, 'out']]);
    fade(carry2, [[0, 0], [tPick2 - 120, 0], [tPick2 + 60, 1, 'ease'], [tLand2, 1, 'hold'], [tLand2 + 10, 0, 'hold']]);
    fade(badge2, [[0, 1], [tDrop2, 1], [tDrop2 + 170, 0, 'ease']]);
    fade(E('fan'), [[0, 0], [tLand2, 0, 'hold'], [tLand2 + 10, 1, 'hold']]);

    const mopt = (k) => $(`.a-opt[data-m="${k}"]`, sMany);
    const mConvert = mopt('convert'), mCompress = mopt('compress'), mResize = mopt('resize'), mMeta = mopt('meta'), mDo = E('optDo');
    const A2 = 15750;
    appear(E('optHl2'), 0, A2);
    const tClick2 = 16300, tType2 = 16550, per2 = 40;
    const t3 = tType2 + 'remove the location'.length * per2; // three words: the sentence goes first
    const tEnter = 19450;
    appear(mConvert, 0, A2, [[t3, 1], [t3 + 220, 0, 'ease']]);
    appear(mCompress, 1, A2);
    appear(mResize, 2, A2, [[t3, 1], [t3 + 220, 0, 'ease']]);
    appear(mMeta, 3, A2);
    const mslide = (el, a, b, i) => tl.to(el, 'y', [[0, slotY(a) + 8], [A2 + i * 70, slotY(a) + 8], [A2 + 400 + i * 70, slotY(a), 'out'], [t3, slotY(a)], [t3 + 500, slotY(b), 'out']]);
    mslide(mConvert, 0, 0, 0); mslide(mCompress, 1, 2, 1); mslide(mResize, 2, 2, 2); mslide(mMeta, 3, 1, 3);
    fade(mDo, [[0, 0], [t3 + 60, 0], [t3 + 340, 1, 'ease']]);
    tl.to(mDo, 'y', [[0, -8], [t3 + 60, -8], [t3 + 460, 0, 'out']]);
    [mConvert, mCompress, mResize, mMeta].forEach((el) => fade($('.a-num', el), [[0, 1], [tType2 - 10, 1, 'hold'], [tType2, 0, 'hold']]));
    tl.cls(mConvert, 'is-active', [[0, true], [t3, false]]);
    tl.cls(mDo, 'is-active', [[0, false], [t3 + 60, true]]);
    tl.to(E('optHl2'), 's', [[0, 1], [t3 + 40, 1], [t3 + 140, 0.985, 'ease'], [t3 + 360, 1, 'out'], [tEnter - 40, 1], [tEnter + 60, 0.985, 'ease'], [tEnter + 260, 1, 'out']]);
    press(tClick2); taps.push(tClick2);
    tl.cls(E('manyInput'), 'is-focus', [[0, false], [tClick2 + 40, true], [19700, false]]);
    fade(E('manyPh'), [[0, 1], [tType2 - 10, 1, 'hold'], [tType2, 0, 'hold']]);
    fade(E('manyKeys'), [[0, 1], [tType2 - 10, 1, 'hold'], [tType2, 0, 'hold']]);
    tl.text(E('manyTyped'), typeKeys(tType2, SENT, per2));
    tl.text(E('doQ'), typeKeys(tType2, SENT, per2));
    blink(E('manyCaret'), tClick2 + 40, 19700, tType2 + SENT.length * per2);
    if (phone) { press(tEnter); taps.push(tEnter); }

    // the request, carried out
    const think = E('dThink'), thinkT = E('dThinkT');
    const st1 = pos(E('dStep1')), st2 = pos(E('dStep2')), sdo = pos(sDo);
    think.style.left = (st1.x - sdo.x + 13) + 'px';
    think.style.top = (st1.y - sdo.y + (st1.h - 24) / 2) + 'px';
    const tT1 = 20200, tS1 = 20850, tS1d = 21650, tT2 = 21700, tS2 = 22150, tS2p = 22250, tS2d = 23450, tOut = 23750;
    fade(think, [[0, 0], [tT1, 0], [tT1 + 200, 1, 'ease'], [tS1 - 120, 1], [tS1, 0, 'ease'], [tT2, 0], [tT2 + 200, 1, 'ease'], [tS2 - 120, 1], [tS2, 0, 'ease']]);
    tl.to(think, 'y', [[0, 0], [tT2 - 10, 0, 'hold'], [tT2, st2.y - st1.y, 'hold']]);
    tl.text(thinkT, [[0, 'Working out what to do…'], [tT2, 'Working on the next step…']]);
    tl.to($('.i', think), 'o', [[0, 1], [tT1, 1], [tT1 + 700, 0.35, 'inOut'], [tT1 + 1400, 1, 'inOut'], [tT2 + 300, 0.35, 'inOut'], [tT2 + 1000, 1, 'inOut']]);
    const stepIn = (el, t) => { fade(el, [[0, 0], [t, 0], [t + 240, 1, 'ease']]); tl.to(el, 'y', [[0, 8], [t, 8], [t + 420, 0, 'out']]); };
    const stepRun = (spin, ok, tg, t0, t1) => {
      tl.to(spin, 'r', [[0, 0], [t0, 0], [t1, (t1 - t0) / 700 * 360, 'lin']]);
      fade(spin, [[0, 1], [t1, 1], [t1 + 120, 0, 'ease']]);
      fade(ok, [[0, 0], [t1, 0], [t1 + 160, 1, 'ease']]);
      tl.to(ok, 's', [[0, 0.6], [t1, 0.6], [t1 + 420, 1, 'back']]);
      fade(tg, [[0, 0], [t1 + 100, 0], [t1 + 360, 1, 'ease']]);
    };
    stepIn(E('dStep1'), tS1); stepRun(E('dSpin1'), E('dOk1'), E('dTg1'), tS1, tS1d);
    stepIn(E('dStep2'), tS2); stepRun(E('dSpin2'), E('dOk2'), E('dTg2'), tS2, tS2d);
    fade(E('dProg2'), [[0, 0], [tS2p - 60, 0], [tS2p, 1, 'ease'], [tS2d, 1], [tS2d + 200, 0, 'ease']]);
    tl.to(E('dBar2'), 'sx', [[0, 0], [tS2p, 0], [tS2p + 300, 0.22, 'out'], [tS2p + 800, 0.6, 'inOut'], [tS2d - 50, 1, 'inOut']]);
    fade(E('dOut'), [[0, 0], [tOut, 0], [tOut + 300, 1, 'ease']]);
    tl.to(E('dOut'), 'y', [[0, 12], [tOut, 12], [tOut + 600, 0, 'out']]);
    const undo = pos(E('dUndo'));
    const nearUndo = phone ? [undo.cx, undo.cy] : [undo.x - 30, undo.cy + 26];

    // ---------------- 5. ⌘K finds any tool
    const tHud = 26400, tKey = 26650, tOpen = 26800, tTypeP = 27300, perP = 110;
    const tDown = Array.from({ length: PAL_PICK }, (_, i) => 28500 + i * 230);
    const tEsc = 30350, tClose = 30500;
    const hud = E('hud'), hudK = E('hudK');
    if (!phone) {
      tl.html(hudK, [[0, ''], [tEnter - 200, '<span>↵</span>'], [tHud, '<span>⌘</span><span>K</span>'], [28350, '<span>↓</span>'], [tEsc - 150, '<span>Esc</span>']]);
      const lastDown = tDown.length ? tDown[tDown.length - 1] : 28500;
      fade(hud, [[0, 0], [tEnter - 200, 0], [tEnter - 60, 1, 'ease'], [tEnter + 400, 1], [tEnter + 600, 0, 'ease'],
        [tHud, 0], [tHud + 150, 1, 'ease'], [tOpen + 350, 1], [tOpen + 550, 0, 'ease'],
        [28350, 0], [28470, 1, 'ease'], [lastDown + 500, 1], [lastDown + 700, 0, 'ease'],
        [tEsc - 150, 0], [tEsc - 30, 1, 'ease'], [tEsc + 400, 1], [tEsc + 600, 0, 'ease']]);
      const pressAt = [tEnter, tKey].concat(tDown, [tEsc]);
      const ks = [[0, 0, 'hold']];
      pressAt.forEach((t) => ks.push([t, 1, 'hold'], [t + 130, 0, 'hold']));
      tl.fn(ks, (v) => { hudK.querySelectorAll('span').forEach((k) => { k.style.transform = v ? 'translateY(1.5px)' : ''; k.style.boxShadow = v ? 'inset 0 0 0 1px rgba(255,255,255,.12)' : ''; }); }, 1);
    } else {
      press(tKey); taps.push(tKey);
    }
    const scrim = E('palScrim'), pal = E('pal');
    fade(scrim, [[0, 0], [tOpen - 50, 0], [tOpen + 150, 1, 'ease'], [tClose, 1], [tClose + 250, 0, 'ease']]);
    fade(pal, [[0, 0], [tOpen, 0], [tOpen + 130, 1, 'ease'], [tClose, 1], [tClose + 200, 0, 'ease']]);
    tl.to(pal, 's', [[0, 0.97], [tOpen, 0.97], [tOpen + 260, 1, 'out'], [tClose, 1], [tClose + 200, 0.98, 'ease']]);
    tl.to(pal, 'y', [[0, -6], [tOpen, -6], [tOpen + 260, 0, 'out']]);
    fade(E('palPh'), [[0, 1], [tTypeP - 10, 1, 'hold'], [tTypeP, 0, 'hold']]);
    tl.text(E('palTyped'), typeKeys(tTypeP, PAL_Q, perP));
    blink(E('palCaret'), tOpen + 100, tClose + 200, tTypeP + PAL_Q.length * perP);
    tl.text($('.a-pal-sec', pal), [[0, 'Recent'], [tTypeP, 'Tools']]);
    rowEls.forEach((el, i) => {
      tl.html(el, PAL_STATES.map((st, k) => [k === 0 ? 0 : tTypeP + (k - 1) * perP, rowHTML(st.rows[i], st.q)]));
      fade(el, PAL_STATES.map((st, k) => [k === 0 ? 0 : tTypeP + (k - 1) * perP, st.rows[i] ? 1 : 0, 'hold']));
    });
    const hlKeys = [[0, 0]];
    tDown.forEach((t, i) => hlKeys.push([t, i * 50], [t + 150, (i + 1) * 50, 'out']));
    tl.to(E('palHl'), 'y', hlKeys);

    // ---------------- the pointer
    const inputAt = (el) => { const p = pos(el); return [p.x + (phone ? 120 : 150), p.cy]; };
    const tbs = pos($('[data-a="tb-search"]'));
    const doRow = [opts.x + (phone ? 150 : 210), pos(E('opts2')).y + 28];
    const path = [[0, start], [300, start], [1650, dropAt, 'inOut', 0.08], [2150, [dropAt[0] + 6, dropAt[1] + 4], 'ease', 0],
      [3900, inputAt(E('intentInput')), 'inOut', -0.1],
      [5750, inputAt(E('intentInput'))],
      [6400, pick, 'inOut', 0.12],
      [7500, pick], [8350, [run.cx - 4, run.cy + 2], 'inOut', -0.12], [8900, [run.cx - 4, run.cy + 2]],
      [9800, rest, 'inOut', 0.1],
      [13000, rest], [tPick2 - 60, start2, 'inOut', 0.1], [tPick2, start2],
      [14950, dropAt2, 'inOut', 0.08], [tDrop2, [dropAt2[0] + 6, dropAt2[1] + 4], 'ease', 0],
      [tClick2 - 100, inputAt(E('manyInput')), 'inOut', -0.1], [tClick2 + 200, inputAt(E('manyInput'))]];
    if (phone) path.push([tEnter - 500, inputAt(E('manyInput'))], [tEnter - 80, doRow, 'inOut', 0.1], [tEnter + 300, doRow], [24000, nearUndo, 'inOut', 0.1], [tKey - 500, nearUndo], [tKey - 80, [tbs.cx, tbs.cy], 'inOut', 0.1]);
    else path.push([tEnter + 400, inputAt(E('manyInput'))], [24700, nearUndo, 'inOut', 0.12]);
    tl.path(cursor, path);
    tl.to(cursor, 's', cursorScale);
    tl.to(ring, 'o', ringO);
    tl.to(ring, 's', ringS);
    if (phone) {
      // a finger only shows while it touches: the drags, then each tap
      const ks = [[0, 0], [260, 0], [420, 1, 'ease'], [2150, 1], [2400, 0, 'ease'], [tPick2 - 200, 0], [tPick2 + 60, 1, 'ease'], [tDrop2, 1], [tDrop2 + 250, 0, 'ease']];
      taps.sort((a, b) => a - b).forEach((t) => ks.push([t - 220, 0], [t - 80, 1, 'ease'], [t + 260, 1], [t + 460, 0, 'ease']));
      fade(cursor, ks);
    } else {
      fade(cursor, [[0, 0], [260, 0], [420, 1, 'ease'], [26000, 1], [26200, 0, 'ease']]);
    }
    return tl;
  }

  /* ------------------------------------------------------------ fitting the window */
  const phoneMq = window.matchMedia('(max-width: 639px)');
  let phone = null, tl = null;
  function rebuild() { const t = player.t; if (tl) tl.clear(); tl = build(phone); tl.invalidate(); player.seek(t); }
  function fit() {
    const wantPhone = phoneMq.matches;
    if (wantPhone !== phone) {
      phone = wantPhone;
      win.classList.toggle('is-phone', phone);
      root.classList.toggle('demo-phone', phone);
      if (tl) tl.clear();
      tl = null;
    }
    const dw = phone ? 390 : 1200;
    const avail = stage.clientWidth;
    const s = Math.min(1, avail / dw);
    win.style.setProperty('--s', s.toFixed(5));
    win.style.width = phone ? dw * s + 'px' : '';
    if (!tl) { tl = build(phone); tl.invalidate(); }
    player.render(player.t);
  }

  /* ------------------------------------------------------------ the steps beside the window */
  const items = $$('#stepList .step');
  const btns = items.map((li) => $('.step-btn', li));
  const capT = $('#stepCapT'), capD = $('#stepCapD'), playBtn = $('#demoPlay'), playL = $('#demoPlayL');
  const titles = items.map((li) => $('.step-t', li).textContent);
  const texts = items.map((li) => $('.step-d p', li).textContent);
  let current = -1;
  const chapterAt = (t) => { let k = 0; for (let i = 0; i < CH.length; i++) if (t >= CH[i].t) k = i; return k; };
  function onTime(t) {
    const k = chapterAt(t);
    if (k !== current) {
      current = k;
      items.forEach((li, i) => li.classList.toggle('is-on', i === k));
      btns.forEach((b, i) => { if (i === k) b.setAttribute('aria-current', 'step'); else b.removeAttribute('aria-current'); });
      capT.textContent = titles[k];
      capD.textContent = texts[k];
    }
    const end = k + 1 < CH.length ? CH[k + 1].t : D;
    const p = userPaused ? 1 : clamp((t - CH[k].t) / (end - CH[k].t), 0, 1);
    items[k].style.setProperty('--p', p.toFixed(4));
  }

  let userPaused = true, started = false;
  const player = new Player({ duration: D, render: (t) => tl && tl.render(t), onTime });

  function setPaused(v) {
    userPaused = v;
    playBtn.classList.toggle('is-paused', v);
    playL.textContent = v ? 'Play' : 'Pause';
    playBtn.setAttribute('aria-label', v ? 'Play the demo' : 'Pause the demo');
    if (v) player.pause(); else player.play();
    onTime(player.t);
  }
  function startDemo() { // first time the window has landed (or is properly in view)
    if (started) return;
    started = true;
    if (!still) { player.seek(0); setPaused(false); }
  }
  function jump(i) {
    started = true;
    current = -1;
    if (userPaused) player.seek(CH[i].still);
    else player.seek(CH[i].t + 1);
  }
  btns.forEach((b, i) => b.addEventListener('click', () => jump(i)));
  playBtn.addEventListener('click', () => { started = true; setPaused(!userPaused); });

  fit();
  player.seek(still ? CH[2].still : 0);
  setPaused(true);
  if (still) started = true;

  // pause when off screen or back up in the hero, and start the first time it is properly in view
  // (when it doesn't fly)
  let onScreen = true, inHero = false;
  const syncVisible = () => player.setVisible(onScreen && !inHero);
  // Scrolling back to the top carries the window back into the hero: it holds its frame there
  // and picks up where it left off once it has landed again.
  function setInHero(v) { if (v !== inHero) { inHero = v; syncVisible(); } }
  new IntersectionObserver((es) => { for (const e of es) onScreen = e.isIntersecting; syncVisible(); }).observe(win);
  new IntersectionObserver((es) => {
    for (const e of es) if (e.intersectionRatio >= 0.55 && !flyOn) startDemo();
  }, { threshold: [0, 0.55, 0.8] }).observe(stage);

  /* ------------------------------------------------------------ the flight from the hero */
  const flyMq = window.matchMedia('(min-width: 1000px) and (prefers-reduced-motion: no-preference)');
  let flyOn = flyMq.matches, M = null, ticking = false;
  const TILT = { rx: 26, ry: 14, rz: -10, persp: 1700 };
  function measure() {
    if (!flyOn) { M = null; return; }
    const sy = window.scrollY;
    const st = stage.getBoundingClientRect(), hs = heroSlot.getBoundingClientRect(), sc = sec.getBoundingClientRect();
    const W = win.offsetWidth, H = win.offsetHeight;
    const nx = st.left + (st.width - W) / 2 + W / 2, ny = st.top + sy + H / 2; // the window's centre in place (page coordinates)
    const hx = hs.left + hs.width / 2, hy = hs.top + sy + hs.height / 2; // the slot's centre in the hero
    M = { k: hs.width / W, nx, ny, hx, hy, end: Math.max(1, sc.top + sy) }; // end: scroll position where the demo section reaches the top
    // tilted, the window projects larger and off centre: shrink it until its projection fits the slot, then centre that
    const prev = fly.style.transform, lx = lean.x, ly = lean.y;
    lean.x = lean.y = 0; // measured at rest
    for (let i = 0; i < 2; i++) {
      fly.style.transform = tf(0, 0, M.k, 1);
      const r = fly.getBoundingClientRect();
      if (i === 0) M.k *= Math.min(hs.width / r.width, hs.height / r.height);
      else { M.hx = hx - (r.left + r.width / 2 - nx); M.hy = hy - (r.top + sy + r.height / 2 - ny); }
    }
    fly.style.transform = prev;
    lean.x = lx; lean.y = ly;
  }
  // lean: while it is tilted, the window turns a few degrees toward the pointer (-1..1 on each axis)
  const lean = { x: 0, y: 0, tx: 0, ty: 0, raf: 0 };
  const tf = (dx, dy, k, r) => `translate3d(${dx.toFixed(2)}px, ${dy.toFixed(2)}px, 0) scale(${k.toFixed(4)}) perspective(${TILT.persp}px) rotateX(${((TILT.rx - lean.y * 3) * r).toFixed(3)}deg) rotateY(${((TILT.ry + lean.x * 5) * r).toFixed(3)}deg) rotateZ(${(TILT.rz * r).toFixed(3)}deg)`;
  function place() {
    ticking = false;
    if (!M) { fly.style.transform = ''; fly.classList.remove('is-tilted'); sec.style.removeProperty('--land'); cue.classList.remove('is-away'); setInHero(false); return; }
    const sy = Math.max(0, window.scrollY);
    if (sy >= M.end - 0.5) {
      if (fly.style.transform) { fly.style.transform = ''; fly.classList.remove('is-tilted'); fly.style.setProperty('--tilt', '0'); sec.style.setProperty('--land', '1'); cue.classList.add('is-away'); }
      setInHero(false);
      startDemo();
      return;
    }
    const p = sy / M.end;
    if (p < 0.72) setInHero(true); // a small scroll above the landing doesn't stop it; leaving does
    const e = EASE.inOut(p);
    const r = 1 - EASE.inOut(clamp(p / 0.86, 0, 1)); // flattens a little before it lands
    const cx = lerp(M.hx, M.nx, EASE.inOut(clamp((p - 0.2) / 0.8, 0, 1))); // it drops first, then crosses once the hero text has gone
    const cyScreen = lerp(M.hy, M.ny - M.end, e); // where the centre should be on screen
    const dx = cx - M.nx, dy = cyScreen + sy - M.ny;
    const k = lerp(M.k, 1, e);
    fly.style.transform = tf(dx, dy, k, r);
    fly.style.setProperty('--tilt', r.toFixed(4));
    fly.classList.toggle('is-tilted', r > 0.001);
    sec.style.setProperty('--land', EASE.out(clamp((p - 0.62) / 0.38, 0, 1)).toFixed(4)); // the steps arrive once the window has cleared them
    cue.classList.toggle('is-away', p > 0.3);
  }
  const onScroll = () => { if (!ticking && M) { ticking = true; requestAnimationFrame(place); } };
  function leanStep() {
    lean.raf = 0;
    const dx = lean.tx - lean.x, dy = lean.ty - lean.y;
    if (Math.abs(dx) < 0.002 && Math.abs(dy) < 0.002) { lean.x = lean.tx; lean.y = lean.ty; place(); return; }
    lean.x += dx * 0.09; lean.y += dy * 0.09;
    place();
    lean.raf = requestAnimationFrame(leanStep);
  }
  if (window.matchMedia('(pointer: fine)').matches) {
    window.addEventListener('pointermove', (e) => {
      if (!M || window.scrollY >= M.end) return; // only while it is in the air
      lean.tx = clamp((e.clientX / window.innerWidth) * 2 - 1, -1, 1);
      lean.ty = clamp((e.clientY / window.innerHeight) * 2 - 1, -1, 1);
      if (!lean.raf) lean.raf = requestAnimationFrame(leanStep);
    }, { passive: true });
    document.documentElement.addEventListener('pointerleave', () => { lean.tx = lean.ty = 0; if (M && !lean.raf) lean.raf = requestAnimationFrame(leanStep); });
  }
  window.addEventListener('scroll', onScroll, { passive: true });
  function relayout() {
    flyOn = flyMq.matches;
    root.classList.toggle('fly', flyOn);
    fit();
    measure();
    place();
  }
  if (flyMq.addEventListener) flyMq.addEventListener('change', relayout);
  window.addEventListener('resize', () => { cancelAnimationFrame(relayout.r); relayout.r = requestAnimationFrame(relayout); });
  relayout();
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(() => { rebuild(); measure(); place(); });
  window.addEventListener('load', () => { measure(); place(); });

  /* ------------------------------------------------------------ "See it work": scroll down for the visitor */
  const title = $('#demo-title');
  let scrollAnim = 0;
  function target() {
    if (flyOn && M) return M.end;
    const r = stage.getBoundingClientRect();
    const navH = 60;
    const avail = window.innerHeight - navH;
    const block = r.height + (phone ? 190 : 140); // the window plus the steps and caption under it
    return Math.max(0, r.top + window.scrollY - navH - Math.max(12, (avail - block) / 2));
  }
  function glide(to) {
    cancelAnimationFrame(scrollAnim);
    const from = window.scrollY, dist = to - from;
    if (still || Math.abs(dist) < 2) { window.scrollTo(0, to); return Promise.resolve(); }
    const dur = clamp(Math.abs(dist) * 0.9, 700, 1400), t0 = performance.now();
    const stop = () => { cancelAnimationFrame(scrollAnim); off(); };
    const off = () => { ['wheel', 'touchstart', 'keydown', 'mousedown'].forEach((ev) => window.removeEventListener(ev, stop)); };
    ['wheel', 'touchstart', 'keydown', 'mousedown'].forEach((ev) => window.addEventListener(ev, stop, { passive: true, once: true }));
    return new Promise((done) => {
      const step = (now) => {
        const p = clamp((now - t0) / dur, 0, 1);
        window.scrollTo(0, from + dist * EASE.inOut(p));
        if (p < 1) scrollAnim = requestAnimationFrame(step); else { off(); done(); }
      };
      scrollAnim = requestAnimationFrame(step);
    });
  }
  // tabbing into the steps mid-flight: finish the landing so the steps and the window are both in place
  $('#steps').addEventListener('focusin', () => { if (flyOn && M && window.scrollY < M.end - 1) glide(M.end); });
  cue.addEventListener('click', () => {
    glide(target()).then(() => { title.focus({ preventScroll: true }); });
  });

  window.DGB.demo = { player, CH, seek: (t) => player.seek(t), setPaused, glide, measure: () => { measure(); place(); } };
})();
