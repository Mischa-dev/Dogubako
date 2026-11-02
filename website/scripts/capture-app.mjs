// Re-capture the app screenshots in docs/screenshots from a running server.
//
// Start a throwaway server with a fresh data dir, then run this from the repo root:
//
//   python3 website/scripts/samples.py /tmp/dgb-samples      # sample photos and a CSV
//   target/debug/dogubako server start --no-auth --data-dir /tmp/dgb-data --bind 127.0.0.1:8420 &
//   node website/scripts/capture-app.mjs http://127.0.0.1:8420 /tmp/dgb-samples
//   python3 website/scripts/screenshots.py                    # WebP copies for the website
//
// It seeds a few runs through the HTTP API so the bench and history have something
// on them, then walks the UI like a person would. Needs Playwright with Chromium.
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

// require() so a global install works too (NODE_PATH=$(npm root -g)).
const { chromium } = createRequire(import.meta.url)('playwright');

const BASE = process.argv[2] ?? 'http://127.0.0.1:8420';
const SAMPLES = process.argv[3] ?? '/tmp/dgb-samples';
const OUT = new URL('../../docs/screenshots/', import.meta.url).pathname;
const DESKTOP = { width: 1360, height: 860 };
const PHONE = { width: 390, height: 844 };
const PINS = ['image.convert', 'dev.json', 'pdf.compress', 'text.diff', 'image.redact'];

async function api(path, init = {}) {
  const res = await fetch(BASE + '/api/v1' + path, init);
  const body = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(`${path}: ${res.status} ${JSON.stringify(body)}`);
  return body;
}
async function upload(name) {
  const buf = await readFile(join(SAMPLES, name));
  return (await api(`/files/raw?name=${encodeURIComponent(name)}`, { method: 'PUT', body: buf })).id;
}
async function run(tool, params) {
  const job = await api(`/tools/${tool}/run`, {
    method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ params, wait_seconds: 60 }),
  });
  if (job.status !== 'succeeded') console.warn(`  ${tool}: ${job.status} ${JSON.stringify(job.error ?? '')}`);
  return job;
}

async function seed() {
  const dune = await upload('dune.jpg');
  const dusk = await upload('dusk.png');
  const orders = await upload('orders.csv');
  // Oldest first: the bench shows the newest outputs first.
  await run('pdf.from_images', { input: [dune, dusk] });
  await run('data.clean', { file: orders, operations: ['trim', 'empty_rows'] });
  await run('image.convert', { input: [dusk], format: 'webp' });
  await run('image.compress', { input: [dusk] });
  await run('image.convert', { input: [dune], format: 'png' });
  await run('image.compress', { input: [dune] });
}

async function settle(page) {
  await page.waitForLoadState('networkidle');
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(400);
}
async function shot(page, name) {
  await settle(page);
  await page.screenshot({ path: join(OUT, name + '.png') });
  console.log('  ' + name);
}
async function open(browser, scheme, viewport = DESKTOP, scale = 1) {
  const ctx = await browser.newContext({ viewport, colorScheme: scheme, deviceScaleFactor: scale, reducedMotion: 'reduce' });
  // The pinned tools from the old screenshots, so the sidebar isn't empty.
  await ctx.addInitScript((pins) => {
    try { if (!localStorage.getItem('dgb.favorites')) localStorage.setItem('dgb.favorites', JSON.stringify(pins)); } catch {}
  }, PINS);
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.warn('  page error:', e.message));
  return page;
}
async function go(page, hash) {
  await page.goto(BASE + '/' + hash);
  await settle(page);
}
async function chooseFile(page, name) {
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.getByRole('button', { name: 'Choose files' }).first().click()]);
  await chooser.setFiles(join(SAMPLES, name));
  await page.waitForURL(/#\/open/);
}
async function compressResult(page) {
  await go(page, '#/');
  await chooseFile(page, 'dune.jpg');
  await page.locator('main').getByText('Compress Images', { exact: true }).first().click();
  await page.waitForURL(/#\/tool\/image\.compress/);
  await settle(page);
  await page.getByRole('button', { name: /^Run/ }).first().click();
  await page.getByText('Result', { exact: true }).first().waitFor({ timeout: 60_000 });
  // Wait until both previews have actually decoded.
  await page.waitForFunction(() => {
    const imgs = [...document.querySelectorAll('main .stage-img')];
    return imgs.length >= 2 && imgs.every((i) => i.complete && i.naturalWidth > 0);
  }, null, { timeout: 30_000 }).catch(() => console.warn('  previews did not finish loading'));
}
async function liveJson(page) {
  await go(page, '#/tool/dev.json');
  const input = page.locator('textarea').first();
  await input.fill('{"name":"dogubako","version":"0.1.0","tools":249,"interfaces":["desktop","web","cli","http","mcp"],"local_first":true,"license":"AGPL-3.0-or-later","engines":{"native":156,"external":77,"model":6,"network":10}}');
  await page.waitForTimeout(800);
}
async function redact(page) {
  await go(page, '#/');
  await chooseFile(page, 'dune.jpg');
  await page.locator('main').getByText('Redact Regions', { exact: true }).first().click();
  await page.waitForURL(/#\/tool\/image\.redact/);
  await settle(page);
  // Draw one region over the sun once the editor's image is up.
  await page.waitForFunction(() => [...document.querySelectorAll('main img')].some((i) => i.complete && i.getBoundingClientRect().width > 300), null, { timeout: 15_000 }).catch(() => {});
  await page.waitForTimeout(500);
  // The editor's image is the biggest picture on the page.
  const boxes = await page.locator('main img').evaluateAll((els) => els.map((e) => { const r = e.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; }));
  const box = boxes.sort((a, b) => b.width * b.height - a.width * a.height)[0];
  if (box) {
    await page.mouse.move(box.x + box.width * 0.36, box.y + box.height * 0.5);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.68, { steps: 8 });
    await page.mouse.up();
  }
}

const browser = await chromium.launch();
try {
  console.log('seeding runs');
  await seed();

  for (const scheme of ['light', 'dark']) {
    console.log(scheme);
    let page = await open(browser, scheme);
    await go(page, '#/');
    await shot(page, `home-${scheme}`);
    await compressResult(page);
    await shot(page, `tool-image-result-${scheme}`);
    await liveJson(page);
    await shot(page, `tool-live-json-${scheme}`);
    await redact(page);
    await shot(page, `redact-editor-${scheme}`);
    await page.context().close();
  }

  console.log('light extras');
  let page = await open(browser, 'light');
  await go(page, '#/');
  await chooseFile(page, 'dune.jpg');
  await shot(page, 'drop-screen-light');
  await go(page, '#/');
  await page.keyboard.press('Control+K');
  await page.keyboard.type('pdf');
  await shot(page, 'command-palette-light');
  await page.keyboard.press('Escape');
  await go(page, '#/');
  await page.getByRole('button', { name: 'Customize' }).click();
  await shot(page, 'customize-home-light');
  await page.context().close();

  console.log('dark extras');
  page = await open(browser, 'dark');
  await go(page, '#/settings/dependencies');
  await shot(page, 'settings-dependencies-dark');
  await go(page, '#/tool/system.overview');
  await page.waitForTimeout(1500);
  await shot(page, 'system-dashboard-dark');
  await page.context().close();

  console.log('phone');
  page = await open(browser, 'light', PHONE, 1.5);
  await go(page, '#/');
  await shot(page, 'phone-home-light');
  await chooseFile(page, 'dune.jpg');
  await shot(page, 'phone-drop-screen-light');
  await page.context().close();
  page = await open(browser, 'dark', PHONE, 1.5);
  await go(page, '#/tool/dev.regex');
  await page.locator('[data-param="pattern"] input, [data-param="pattern"] textarea').first().fill('(\\w+)@(\\w+)\\.app');
  await page.locator('#regex-text').fill('Write to hello@dogubako.app or team@dogubako.app,\nnot spam@example.com.');
  await page.waitForTimeout(800);
  await shot(page, 'phone-regex-dark');
  await page.context().close();
} finally {
  await browser.close();
}
