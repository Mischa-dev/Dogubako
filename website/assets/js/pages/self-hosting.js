/* self-hosting.js: the install sequence (terminal → token → phone), the clients-and-server
 * diagram and the audit log. */
(function () {
  'use strict';
  const { $, reduce, pauseButton, watchView, typeKeys, show } = window.DGBP;
  const still = reduce();

  /* ---------------------------------------------------------------- install sequence */
  const TOKEN = '4f1c9a0e7d2b6c85e3a1f09b72d4c6e8';
  const D = 17000;
  const hold = (pairs) => [[0, pairs[0][1] ? 0 : 1], ...pairs.map(([t, v]) => [t, v, 'hold'])];
  const appear = (t) => [[0, 0], [t, 1, 'hold']];

  function build() {
    const tl = new window.DGB.Timeline(D);
    const e = (id) => document.getElementById(id);
    // the terminal fades in at the start of each loop and out at the end; it steps back a little for the phone
    tl.to(e('shTerm'), 'o', [[0, 0], [300, 1], [16300, 1], [16900, 0]]);
    tl.to(e('shTerm'), 's', [[10200, 1], [10800, 0.97], [16900, 0.97]]);

    // 1. docker compose up
    tl.text(e('shC1'), typeKeys(400, 'docker compose up -d --build', 45));
    tl.to(e('shK1'), 'o', [[0, 1], [1900, 0, 'hold']]);
    tl.to(e('shO1a'), 'o', appear(1900));
    tl.to(e('shBar'), 'sx', [[0, 0], [1900, 0], [3500, 1, 'inOut']]);
    tl.to(e('shBuilt'), 'o', appear(3500));
    tl.to(e('shO1b'), 'o', appear(3700));
    tl.to(e('shO1c'), 'o', appear(3900));
    tl.to(e('shO1d'), 'o', appear(4100));

    // 2. tailscale serve
    tl.to(e('shL2'), 'o', appear(4700));
    tl.text(e('shC2'), typeKeys(4900, 'tailscale serve --bg 8420', 45));
    tl.to(e('shK2'), 'o', hold([[4700, 1], [6200, 0]]));
    tl.to(e('shO2a'), 'o', appear(6400));
    tl.to(e('shO2b'), 'o', appear(6550));
    tl.to(e('shO2c'), 'o', appear(6700));

    // 3. the setup token
    tl.to(e('shL3'), 'o', appear(7200));
    tl.text(e('shC3'), typeKeys(7400, 'docker compose logs dogubako | grep "Setup token"', 32));
    tl.to(e('shK3'), 'o', hold([[7200, 1], [9100, 0]]));
    tl.to(e('shO3'), 'o', appear(9300));

    // 4. the phone: setup, then home
    tl.to(e('shPhone'), 'o', [[0, 0], [10200, 0], [10800, 1, 'out'], [16300, 1], [16900, 0]]);
    tl.to(e('shPhone'), 'y', [[10200, 70], [11000, 0, 'out']]);
    tl.text(e('shFTok'), typeKeys(11100, TOKEN, 22));
    tl.to(e('shFTokK'), 'o', hold([[10900, 1], [11950, 0]]));
    tl.text(e('shFUser'), typeKeys(12200, 'alex', 90));
    tl.to(e('shFUserK'), 'o', hold([[12100, 1], [12700, 0]]));
    tl.text(e('shFPass'), typeKeys(12900, '••••••••••••', 50));
    tl.to(e('shFPassK'), 'o', hold([[12800, 1], [13600, 0]]));
    tl.to(e('shFBtn'), 's', [[13900, 1], [14000, 0.96], [14200, 1]]);
    tl.to(e('shTap'), 'o', [[0, 0], [13900, 0], [13960, 0.9], [14450, 0]]);
    tl.to(e('shTap'), 's', [[13900, 0.3], [14450, 1.5, 'out']]);
    tl.to(e('shSetup'), 'o', [[0, 1], [14500, 1], [14800, 0]]);
    tl.to(e('shHome'), 'o', [[0, 0], [14600, 0], [15100, 1]]);
    tl.to(e('shHome'), 'y', [[14600, 12], [15300, 0, 'out']]);
    return tl;
  }

  show({
    root: $('#shShow'),
    duration: D,
    build,
    stillChapter: 3,
    chapters: [
      { label: 'Start', t: 0, still: 4400, cap: 'docker compose up builds the image with its engines and starts the server, listening only on 127.0.0.1:8420.' },
      { label: 'Tailscale', t: 4700, still: 6900, cap: 'tailscale serve puts it on your tailnet with HTTPS. No port is opened to the internet.' },
      { label: 'Token', t: 7200, still: 9900, cap: 'On first start, the server prints a one-time setup token.' },
      { label: 'Phone', t: 10200, still: 13700, cap: 'Open the address on your phone, enter the token and create the owner account.' },
    ],
  });

  /* ---------------------------------------------------------------- clients and the server */
  const net = $('#net');
  if (net && !still) {
    let vis = false, paused = false;
    const sync = () => {
      net.classList.toggle('is-on', vis || net.classList.contains('is-on'));
      net.classList.toggle('is-paused', paused || !vis || document.hidden);
    };
    watchView(net, (v) => { vis = v; sync(); }, '60px 0px');
    document.addEventListener('visibilitychange', sync);
    pauseButton($('.net-play'), 'the animation', (p) => { paused = p; sync(); });
  } else if (net) {
    const b = $('.net-play');
    if (b) b.hidden = true;
  }

  /* ---------------------------------------------------------------- audit log */
  const log = $('#auditLog');
  if (log && !still) {
    const items = Array.from(log.children);
    log.classList.add('is-live');
    let i = 0, timer = 0, vis = false, paused = false;
    const step = () => {
      timer = 0;
      if (!vis || paused || document.hidden) return;
      if (i >= items.length) {
        items.forEach((li) => li.classList.remove('on'));
        i = 0;
        timer = setTimeout(step, 700);
        return;
      }
      items[i++].classList.add('on');
      timer = setTimeout(step, i >= items.length ? 3600 : i < 3 ? 160 : 1100); // the first three arrive together
    };
    const kick = () => { if (vis && !paused && !document.hidden && !timer) timer = setTimeout(step, 400); };
    watchView($('#audit'), (v) => { vis = v; kick(); });
    document.addEventListener('visibilitychange', kick);
    pauseButton($('.audit-play'), 'the audit log animation', (p) => {
      paused = p;
      if (p) { clearTimeout(timer); timer = 0; items.forEach((li) => li.classList.add('on')); i = items.length; }
      else kick();
    });
  } else if (log) {
    const b = $('.audit-play');
    if (b) b.hidden = true;
  }
})();
