/* hero.js: the app window plays itself.
 * Builds the window's dynamic bits, measures where things are, then lays the whole demo out
 * as one timeline (see timeline.js). Chapters are just named times on that timeline. */
(function () {
  'use strict';
  const { Timeline, Player, reduceMotion } = window.DGB;
  const $ = (s, r) => (r || document).querySelector(s);
  const win = $('#win');
  if (!win) return;
  const ws = $('#ws');
  const CATS = window.DGB_CATS, TOOLS = window.DGB_TOOLS;

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
  const PAL_STATES = [
    { q: '', sec: 'Recent', rows: [tool('Compress Images', 0), tool('Compress PDF', 4), tool('Normalize Audio', 2), tool('Trim Video', 1), tool('Clean CSV Data', 8), tool('Convert Images', 0)] },
    { q: 'p', sec: 'Tools', rows: search('p') },
    { q: 'pd', sec: 'Tools', rows: search('pd') },
    { q: 'pdf', sec: 'Tools', rows: search('pdf') },
  ];
  const MERGE_ROW = Math.max(0, PAL_STATES[3].rows.findIndex((t) => t[2] === 'pdf.merge'));
  const palRows = $('#palRows');
  const rowEls = [];
  for (let i = 0; i < 7; i++) {
    const el = document.createElement('div');
    el.className = 'a-pal-row';
    el.style.top = i * 50 + 'px';
    palRows.appendChild(el);
    rowEls.push(el);
  }
  const rowHTML = (t, q) => t ? `<span class="a-pal-ic">${icon(CATS[t[0]][2])}</span><span class="a-pal-t"><span class="a-pal-l">${hl(t[1], q)}</span><span class="a-pal-s">${shortHint(t[3])}</span></span><span class="a-pal-c">${CATS[t[0]][1]}</span>` : '';

  /* ------------------------------------------------------------ chapters */
  const D = 33000;
  const CH = [
    { label: 'Drop', t: 0, still: 2120, cap: 'Drag a photo onto the window. It lands on the bench, drawn as what it is: a print.' },
    { label: 'Choose', t: 3300, still: 6150, cap: 'Dogubako ranks the tools that fit. Type “smaller” and Compress Images moves to the top.' },
    { label: 'Result', t: 7300, still: 12400, cap: 'Run it. The original and the result sit side by side, with the size change.' },
    { label: 'Palette', t: 13400, still: 16500, cap: 'Ctrl K searches all 249 tools by name and by what they do.' },
    { label: 'Terminal', t: 17700, still: 23300, cap: 'Every tool is also a command, with the same options and the same engine.' },
    { label: 'Agent', t: 24300, still: 31200, cap: 'Agents run the same tools over MCP, limited to what you allow them.' },
  ];

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

    const sHome = E('sHome'), sIntent = E('sIntent'), sTool = E('sTool');
    const cursor = E('cursor'), ring = E('cursorRing'), carry = E('carry');
    const badge = $('.a-carry-badge', carry);

    // helpers
    const fade = (el, keys) => tl.to(el, 'o', keys);
    const show = (el, t0, t1, d = 220, d2 = 220) => fade(el, [[0, 0], [t0, 0], [t0 + d, 1, 'ease'], [t1, 1], [t1 + d2, 0, 'ease']]);
    const blink = (el, from, to, solid) => { // caret: solid while typing, then blinks
      const ks = [[0, 0, 'hold'], [from, 1, 'hold']];
      let t = solid || from, on = true;
      while (t < to) { t += 530; on = !on; if (t < to) ks.push([t, on ? 1 : 0, 'hold']); }
      ks.push([to, 0, 'hold']);
      tl.to(el, 'o', ks);
    };
    const typeKeys = (t0, str, per) => { const ks = [[0, '']]; for (let i = 1; i <= str.length; i++) ks.push([t0 + (i - 1) * per, str.slice(0, i)]); return ks; };
    const press = (t) => { // pointer press + ripple
      cursorScale.push([t - 60, 1], [t + 40, 0.86, 'ease'], [t + 220, 1, 'out']);
      ringO.push([t, 0, 'hold'], [t + 10, 0.55, 'hold'], [t + 460, 0, 'out']);
      ringS.push([t, 0.5, 'hold'], [t + 460, 1.5, 'out']);
    };
    const cursorScale = [[0, 1]], ringO = [[0, 0]], ringS = [[0, 0.5]];

    // ---------------- screens
    fade(sHome, [[0, 1], [2180, 1], [2400, 0, 'ease'], [32350, 0], [32850, 1, 'ease']]);
    tl.to(sHome, 'y', [[0, 0], [2180, 0], [2400, -6, 'ease'], [32350, 10], [32850, 0, 'out']]);
    fade(sIntent, [[0, 0], [2380, 0], [2720, 1, 'ease'], [7150, 1], [7330, 0, 'ease']]);
    tl.to(sIntent, 'y', [[0, 0], [2350, 12], [2850, 0, 'out'], [7150, 0], [7400, -8, 'ease']]);
    fade(sTool, [[0, 0], [7320, 0], [7650, 1, 'ease'], [32200, 1], [32400, 0, 'ease']]);
    tl.to(sTool, 'y', [[0, 0], [7250, 12], [7800, 0, 'out']]);

    // ---------------- 1. drop
    const opts = pos(E('opts'));
    const fig = pos(E('figPrint'));
    const start = phone ? [dw + 120, 470] : [dw + 150, 560];
    const dropAt = phone ? [205, 400] : [700, 400];
    const carryW = phone ? 160 : 192;
    tl.path(carry, [[0, start], [300, start], [1650, dropAt, 'inOut', 0.08], [2150, [dropAt[0] + 6, dropAt[1] + 4], 'ease', 0], [2780, [fig.cx, fig.cy + 0.5], 'out', -0.06]]);
    tl.to(carry, 's', [[0, 1], [2150, 1], [2780, fig.w / carryW, 'out']]);
    tl.to(carry, 'r', [[0, -9], [300, -9], [1650, -4, 'inOut'], [2150, -3], [2780, 0, 'out']]);
    fade(carry, [[0, 0], [260, 0], [420, 1, 'ease'], [2780, 1, 'hold'], [2790, 0, 'hold']]);
    fade(badge, [[0, 1], [2150, 1], [2320, 0, 'ease']]);
    fade(E('figPrint'), [[0, 0], [2780, 0, 'hold'], [2790, 1, 'hold']]);
    // drop overlay
    const drop = E('dropOv'), dropCard = $('.a-drop-card', drop);
    fade(drop, [[0, 0], [820, 0], [1020, 1, 'ease'], [2150, 1], [2380, 0, 'ease']]);
    tl.to(dropCard, 's', [[0, 0.96], [820, 0.96], [1100, 1, 'out'], [2150, 1], [2380, 0.98, 'ease']]);
    tl.to(E('traySheet'), 'y', [[0, -26], [1000, -26], [1500, 0, 'inOut'], [1950, -24, 'inOut'], [2150, -12, 'inOut']]);

    // ---------------- 2. choose
    const ROW = 58;
    const slotY = (i) => i * ROW;
    const opt = (k) => $(`.a-opt[data-o="${k}"]`, E('sIntent'));
    const oConvert = opt('convert'), oCompress = opt('compress'), oRedact = opt('redact'), oResize = opt('resize'), oMeta = opt('meta');
    const R0 = 5300, R1 = 5800; // re-rank window
    const appear = (el, i, from) => {
      const t = 2750 + i * 70;
      fade(el, [[0, 0], [t, 0], [t + 320, 1, 'ease']].concat(from || []));
    };
    appear(E('optHl'), 0);
    appear(oConvert, 0); appear(oCompress, 1); appear(oRedact, 2, [[R0, 1], [R0 + 260, 0, 'ease']]); appear(oResize, 3);
    fade(oMeta, [[0, 0], [R0 + 120, 0], [R1, 1, 'ease']]);
    const slide = (el, a, b, i) => tl.to(el, 'y', [[0, slotY(a) + 8], [2750 + i * 70, slotY(a) + 8], [3150 + i * 70, slotY(a), 'out'], [R0, slotY(a)], [R1, slotY(b), 'out']]);
    slide(oConvert, 0, 2, 0); slide(oCompress, 1, 0, 1); slide(oRedact, 2, 2, 2); slide(oResize, 3, 1, 3);
    tl.to(oMeta, 'y', [[0, slotY(3) + 10], [R0 + 120, slotY(3) + 10], [R1, slotY(3), 'out']]);
    tl.text($('.a-num', oConvert), [[0, '1'], [R0 + 200, '3']]);
    tl.text($('.a-num', oCompress), [[0, '2'], [R0 + 200, '1']]);
    tl.text($('.a-num', oResize), [[0, '4'], [R0 + 200, '2']]);
    tl.cls(oConvert, 'is-active', [[0, true], [R0, false]]);
    tl.cls(oCompress, 'is-active', [[0, false], [R1 - 120, true]]);
    // the input: click, type "smaller"
    const tClick = 4050, tType = 4350, word = 'smaller', per = 92;
    press(tClick);
    tl.cls(E('intentInput'), 'is-focus', [[0, false], [tClick + 40, true], [7300, false]]);
    fade(E('intentPh'), [[0, 1], [tType - 10, 1, 'hold'], [tType, 0, 'hold']]);
    fade(E('intentKeys'), [[0, 1], [tType - 10, 1, 'hold'], [tType, 0, 'hold']]);
    tl.text(E('intentTyped'), typeKeys(tType, word, per));
    blink(E('intentCaret'), tClick + 40, 7300, tType + word.length * per);
    // pick Compress Images (now number 1)
    const pick = [opts.x + (phone ? 150 : 190), opts.y + slotY(0) + 28];
    const tPick = 6500;
    // pointer: input -> compress -> run
    const run = pos(E('runBtn'));
    const rest = phone ? [run.cx + 70, run.cy + 120] : [run.cx + 40, run.cy + 170];
    tl.path(cursor, [[0, start], [300, start], [1650, dropAt, 'inOut', 0.08], [2150, [dropAt[0] + 6, dropAt[1] + 4], 'ease', 0],
      [3900, [pos(E('intentInput')).x + 96, pos(E('intentInput')).cy], 'inOut', -0.1],
      [5750, [pos(E('intentInput')).x + 96, pos(E('intentInput')).cy]],
      [6400, pick, 'inOut', 0.12],
      [7500, pick], [8350, [run.cx - 4, run.cy + 2], 'inOut', -0.12], [8900, [run.cx - 4, run.cy + 2]],
      [9800, rest, 'inOut', 0.1],
      [13100, rest],
      ...(phone ? [[13600, [pos($('[data-a="tb-search"]')).cx, pos($('[data-a="tb-search"]')).cy], 'inOut', 0.1]] : [[13600, [rest[0] - 120, rest[1] - 40], 'inOut', 0.1]]),
      [17600, phone ? [pos($('[data-a="tb-search"]')).cx, pos($('[data-a="tb-search"]')).cy] : [rest[0] - 120, rest[1] - 40]],
      [17601, start],
    ]);
    press(tPick);
    tl.to(E('optHl'), 's', [[0, 1], [tPick - 40, 1], [tPick + 60, 0.985, 'ease'], [tPick + 260, 1, 'out']]);

    // ---------------- 3. result
    const tRun = 8800, tP0 = 8950, tP1 = 10350, tWipe = 10400, tWipe1 = 11050;
    press(tRun);
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
    const paneW = pos(resImg).w;
    tl.to(E('resSweep'), 'x', [[0, 0], [tWipe, 0], [tWipe1, paneW, 'out']]);
    fade(E('resSweep'), [[0, 0], [tWipe, 0, 'hold'], [tWipe + 10, 1, 'hold'], [tWipe1 - 120, 1], [tWipe1 + 60, 0, 'ease']]);
    fade(E('resMeta'), [[0, 0], [tWipe1 - 150, 0], [tWipe1 + 150, 1, 'ease']]);
    const tCo = 11150;
    const co = E('callout');
    fade(co, [[0, 0], [tCo, 0], [tCo + 260, 1, 'ease']]);
    tl.to(co, 's', [[0, 0.9], [tCo, 0.9], [tCo + 520, 1, 'back']]);
    tl.to(co, 'y', [[0, 12], [tCo, 12], [tCo + 520, 0, 'out']]);
    tl.fn([[0, 4.2], [tCo + 100, 4.2], [tCo + 1100, 1.1, 'out']], (v) => { E('coTo').textContent = v.toFixed(1) + ' MB'; }, 10);
    tl.fn([[0, 0], [tCo + 100, 0], [tCo + 1100, 74, 'out']], (v) => { E('coPct').textContent = String(Math.round(v)); }, 1);

    // ---------------- 4. palette
    const tOpen = 13900, tTypeP = 14450, tDown = [15250, 15500, 15750];
    const hud = E('hud'), hudK = E('hudK');
    if (!phone) {
      tl.html(hudK, [[0, ''], [13500, '<span>Ctrl</span><span>K</span>'], [15100, '<span>↓</span>']]);
      fade(hud, [[0, 0], [13500, 0], [13650, 1, 'ease'], [14250, 1], [14450, 0, 'ease'], [15100, 0], [15220, 1, 'ease'], [16100, 1], [16300, 0, 'ease']]);
      const keyEls = () => hudK.querySelectorAll('span');
      tl.fn([[0, 0], [13750, 0, 'hold'], [13760, 1, 'hold'], [13900, 0, 'hold']], (v) => { keyEls().forEach((k) => { k.style.transform = v ? 'translateY(1.5px)' : ''; k.style.boxShadow = v ? 'inset 0 0 0 1px rgba(255,255,255,.12)' : ''; }); }, 1);
    } else {
      press(13650);
    }
    const scrim = E('palScrim'), pal = E('pal');
    fade(scrim, [[0, 0], [tOpen - 50, 0], [tOpen + 150, 1, 'ease'], [17600, 1], [17850, 0, 'ease']]);
    fade(pal, [[0, 0], [tOpen, 0], [tOpen + 130, 1, 'ease'], [17600, 1], [17800, 0, 'ease']]);
    tl.to(pal, 's', [[0, 0.97], [tOpen, 0.97], [tOpen + 260, 1, 'out'], [17600, 1], [17800, 0.98, 'ease']]);
    tl.to(pal, 'y', [[0, -6], [tOpen, -6], [tOpen + 260, 0, 'out']]);
    fade(E('palPh'), [[0, 1], [tTypeP - 10, 1, 'hold'], [tTypeP, 0, 'hold']]);
    tl.text(E('palTyped'), typeKeys(tTypeP, 'pdf', 150));
    blink(E('palCaret'), tOpen + 100, 17800, tTypeP + 3 * 150);
    tl.text($('.a-pal-sec', pal), [[0, 'Recent'], [tTypeP, 'Tools']]);
    rowEls.forEach((el, i) => {
      tl.html(el, PAL_STATES.map((st, k) => [k === 0 ? 0 : tTypeP + (k - 1) * 150, rowHTML(st.rows[i], st.q)]));
      fade(el, PAL_STATES.map((st, k) => [k === 0 ? 0 : tTypeP + (k - 1) * 150, st.rows[i] ? 1 : 0, 'hold']));
    });
    const hlKeys = [[0, 0]];
    for (let i = 0; i < MERGE_ROW && i < tDown.length; i++) hlKeys.push([tDown[i], i * 50], [tDown[i] + 150, (i + 1) * 50, 'out']);
    tl.to(E('palHl'), 'y', hlKeys);

    // ---------------- 5. terminal
    const term = E('term');
    const tT0 = 17750, tCmd0 = 18450, cmd = 'dogubako pdf merge a.pdf b.pdf -o merged.pdf', cPer = 36;
    const tEnter = tCmd0 + cmd.length * cPer + 260, tBar1 = tEnter + 1500;
    fade(term, [[0, 0], [tT0, 0], [tT0 + 260, 1, 'ease'], [24200, 1], [24500, 0, 'ease']]);
    tl.to(term, 'y', [[0, 70], [tT0, 70], [tT0 + 620, 0, 'out'], [24200, 0], [24500, 40, 'ease']]);
    tl.to(term, 's', [[0, 0.97], [tT0, 0.97], [tT0 + 620, 1, 'out']]);
    tl.text(E('tCmd'), typeKeys(tCmd0, cmd, cPer));
    fade(E('tCaret'), [[0, 1], [tEnter, 1, 'hold'], [tEnter + 10, 0, 'hold']]);
    const SPIN = '⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏', BAR = phone ? 14 : 30;
    tl.fn([[0, 0], [tEnter + 40, 0, 'hold'], [tEnter + 50, tEnter + 50, 'hold'], [tBar1, tBar1, 'lin'], [tBar1 + 10, 0, 'hold']], (tt) => {
      const el = E('tProg');
      if (!tt) { el.textContent = ''; return; }
      const p = Math.min(1, window.DGB.EASE.inOut((tt - tEnter - 50) / (tBar1 - tEnter - 50)));
      const n = Math.round(p * BAR);
      const msg = p < 0.42 ? 'a.pdf' : p < 0.88 ? 'b.pdf' : 'Writing merged PDF';
      el.innerHTML = `${SPIN[Math.floor(tt / 80) % 10]} Merge PDFs <span class="t-b">[</span>${'━'.repeat(n)}${' '.repeat(BAR - n)}<span class="t-b">]</span> ${String(Math.round(p * 100)).padStart(3, ' ')}% <span class="t-k">${msg}</span>`;
    }, 1);
    // after the bar is cleared, the result lands where it was
    tl.html(E('tProg'), [[0, ''], [tBar1 + 20, '<span class="t-k">files</span>  2']]);
    tl.html(E('tO1'), [[0, ''], [tBar1 + 90, '<span class="t-k">pages</span>  14']]);
    tl.text(E('tO2'), [[0, ''], [tBar1 + 170, 'saved merged.pdf (1.8 MB)']]);
    fade(E('tP2'), [[0, 0], [tBar1 + 280, 0, 'hold'], [tBar1 + 290, 1, 'hold']]);

    // ---------------- 6. agent
    const ag = E('agent');
    const tA0 = 24450;
    fade(ag, [[0, 0], [tA0, 0], [tA0 + 280, 1, 'ease'], [32000, 1], [32350, 0, 'ease']]);
    tl.to(ag, phone ? 'y' : 'x', [[0, phone ? 80 : -40], [tA0, phone ? 80 : -40], [tA0 + 700, 0, 'out'], [32000, 0], [32350, phone ? 40 : -24, 'ease']]);
    const msgIn = (el, t) => { fade(el, [[0, 0], [t, 0], [t + 260, 1, 'ease']]); tl.to(el, 'y', [[0, 8], [t, 8], [t + 420, 0, 'out']]); };
    msgIn(E('agU'), 25150);
    msgIn(E('agB1'), 25850);
    msgIn(E('agC1'), 26450);
    fade($('.ag-res', E('agC1')), [[0, 0], [26950, 0], [27150, 1, 'ease']]);
    const tC2 = 27350;
    msgIn(E('agC2'), tC2);
    $('#agJson').querySelectorAll('div').forEach((l, i) => fade(l, [[0, 0], [tC2 + 120 + i * 55, 0], [tC2 + 300 + i * 55, 1, 'ease']]));
    const tRun0 = 27950, tRun1 = 30000;
    tl.to(E('agBar'), 'sx', [[0, 0], [tRun0, 0], [tRun0 + 500, 0.3, 'out'], [tRun0 + 1300, 0.7, 'inOut'], [tRun1, 1, 'inOut']]);
    tl.text(E('agStT'), [[0, 'running'], [tRun1 + 40, 'done']]);
    tl.cls(E('agSt'), 'is-done', [[0, false], [tRun1 + 40, true]]);
    tl.fn([[0, 0], [tC2, 0], [tRun1, (tRun1 - tC2) / 1000 * 3, 'lin']], (v) => { E('agSpin').style.transform = `rotate(${Math.round(v * 360) % 360}deg)`; }, 50);
    fade(E('agOut'), [[0, 0], [tRun1 + 60, 0], [tRun1 + 300, 1, 'ease']]);
    msgIn(E('agB2'), 30600);

    // ---------------- pointer
    tl.to(cursor, 's', cursorScale);
    tl.to(ring, 'o', ringO);
    tl.to(ring, 's', ringS);
    if (phone) {
      // a finger only shows while it touches: the drag, then each tap
      const taps = [tClick, tPick, tRun, 13650];
      const ks = [[0, 0], [260, 0], [420, 1, 'ease'], [2150, 1], [2400, 0, 'ease']];
      taps.forEach((t) => ks.push([t - 220, 0], [t - 80, 1, 'ease'], [t + 260, 1], [t + 460, 0, 'ease']));
      fade(cursor, ks);
    } else {
      fade(cursor, [[0, 0], [260, 0], [420, 1, 'ease'], [17450, 1], [17650, 0, 'ease']]);
    }
    return tl;
  }

  /* ------------------------------------------------------------ fitting the window */
  let phone = null, tl = null;
  const demo = $('#demo');
  function fit() {
    const avail = win.parentElement.clientWidth;
    const wantPhone = avail < 640;
    if (wantPhone !== phone) {
      phone = wantPhone;
      win.classList.toggle('is-phone', phone);
      if (tl) tl.clear();
      tl = build(phone);
      tl.invalidate();
    }
    const dw = phone ? 390 : 1200;
    const s = Math.min(1, avail / dw);
    win.style.setProperty('--s', s.toFixed(5));
    if (phone) win.style.width = dw * s + 'px'; else win.style.width = '';
    if (player) player.render(player.t);
  }

  /* ------------------------------------------------------------ controls */
  const pips = $('#pips'), cap = $('#demoCap'), playBtn = $('#demoPlay');
  const pipEls = CH.map((c, i) => {
    const li = document.createElement('li');
    li.innerHTML = `<button type="button" class="pip"><span><span class="pip-n">${i + 1}</span><span class="pip-l">${c.label}</span></span><span class="pip-bar" aria-hidden="true"><i></i></span></button>`;
    pips.appendChild(li);
    const b = li.firstChild;
    b.addEventListener('click', () => jump(i));
    return b;
  });
  let current = -1;
  const chapterAt = (t) => { let k = 0; for (let i = 0; i < CH.length; i++) if (t >= CH[i].t) k = i; return k; };
  function onTime(t) {
    const k = chapterAt(t);
    if (k !== current) {
      current = k;
      pipEls.forEach((b, i) => { if (i === k) b.setAttribute('aria-current', 'step'); else b.removeAttribute('aria-current'); b.setAttribute('aria-label', `Chapter ${i + 1}: ${CH[i].label}`); });
      cap.textContent = CH[k].cap;
    }
    const end = k + 1 < CH.length ? CH[k + 1].t : D;
    const p = Math.max(0, Math.min(1, (t - CH[k].t) / (end - CH[k].t)));
    const bar = pipEls[k].querySelector('.pip-bar i');
    bar.style.transform = `scaleX(${userPaused || still ? 1 : p.toFixed(4)})`;
  }

  const still = reduceMotion();
  let userPaused = still;
  const player = new Player({ duration: D, render: (t) => tl && tl.render(t), onTime });

  function setPaused(v) {
    userPaused = v;
    playBtn.setAttribute('aria-pressed', String(v));
    playBtn.setAttribute('aria-label', v ? 'Play the demo' : 'Pause the demo');
    if (v) player.pause(); else player.play();
    onTime(player.t);
  }
  function jump(i) {
    current = -1;
    if (userPaused) player.seek(CH[i].still);
    else player.seek(CH[i].t + 1);
  }
  playBtn.addEventListener('click', () => setPaused(!userPaused));

  fit();
  window.addEventListener('resize', () => { cancelAnimationFrame(fit.r); fit.r = requestAnimationFrame(fit); });
  if (document.fonts && document.fonts.ready) document.fonts.ready.then(() => { const t = player.t; if (tl) tl.clear(); tl = build(phone); tl.invalidate(); player.seek(t); });

  // pause when off screen
  new IntersectionObserver((es) => { for (const e of es) player.setVisible(e.isIntersecting); }, { rootMargin: '0px' }).observe(demo);

  if (still) {
    // reduced motion: a composed still of the result; the chapter buttons step through stills
    player.seek(CH[2].still);
    setPaused(true);
  } else {
    // let the window settle in before it starts moving
    player.seek(0);
    setPaused(true);
    userPaused = false;
    playBtn.setAttribute('aria-pressed', 'false');
    playBtn.setAttribute('aria-label', 'Pause the demo');
    setTimeout(() => { if (!userPaused) setPaused(false); }, 900);
  }

  /* ------------------------------------------------------------ scroll tilt */
  if (!still) {
    let ticking = false;
    const tilt = () => {
      ticking = false;
      const r = demo.getBoundingClientRect();
      const vh = window.innerHeight;
      const p = Math.max(0, Math.min(1, (vh - r.top) / (vh * 0.62)));
      const ang = (1 - p) * 16;
      const sc = 0.94 + 0.06 * p;
      win.style.transform = ang < 0.05 ? '' : `rotateX(${ang.toFixed(2)}deg) scale(${sc.toFixed(4)})`;
    };
    window.addEventListener('scroll', () => { if (!ticking) { ticking = true; requestAnimationFrame(tilt); } }, { passive: true });
    tilt();
  }

  window.DGB.hero = { player, CH, seek: (t) => player.seek(t), setPaused };
})();
