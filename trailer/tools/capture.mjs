// Capture the real app for the trailer: drives a running Dogubako server in
// headless Chromium and saves 2x screenshots plus the positions of the
// controls the trailer's cursor needs (build/captures/boxes.js).
//
//   target/debug/dogubako server start --no-auth --data-dir /tmp/dgb --bind 127.0.0.1:8420
//   node trailer/tools/capture.mjs [only-these,shots]
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BASE = process.env.DGB_URL || 'http://127.0.0.1:8420';
const OUT = join(root, 'build', 'captures');
const ASSETS = join(root, 'assets');
mkdirSync(OUT, { recursive: true });
const only = process.argv[2] ? new Set(process.argv[2].split(',')) : null;
const want = (n) => !only || only.has(n);
const boxesFile = join(OUT, 'boxes.json');
const boxes = existsSync(boxesFile) ? JSON.parse(readFileSync(boxesFile, 'utf8')) : {};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------------------------------------------------------------- seed
async function api(path, init = {}) {
  const r = await fetch(BASE + '/api/v1' + path, init);
  if (!r.ok) throw new Error(`${path}: ${r.status} ${await r.text()}`);
  return r.json();
}
async function upload(name) {
  return (await api(`/files/raw?name=${encodeURIComponent(name)}`, { method: 'PUT', body: readFileSync(join(ASSETS, name)) })).id;
}
async function runTool(id, params) {
  return api(`/tools/${id}/run`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ params, wait_seconds: 60 }) });
}
if (want('seed')) {
  const seeded = await api('/jobs').then((j) => (j.jobs ?? j.items ?? j).length > 0).catch(() => false);
  if (!seeded) {
    const meadow = await upload('meadow.jpg');
    const dusk = await upload('dusk.png');
    await runTool('image.convert', { input: [meadow], format: 'webp' }).catch((e) => console.warn(e.message));
    await runTool('image.compress', { input: [dusk] }).catch((e) => console.warn(e.message));
    await runTool('image.convert', { input: [dusk], format: 'webp' }).catch((e) => console.warn(e.message));
    console.log('seeded history');
  }
}

// ---------------------------------------------------------------- browser
const browser = await chromium.launch();
async function ctx(theme, viewport = { width: 1440, height: 900 }, scale = 2) {
  const c = await browser.newContext({ viewport, deviceScaleFactor: scale, colorScheme: theme, reducedMotion: 'reduce' });
  await c.addInitScript((th) => {
    try {
      if (!sessionStorage.getItem('trailer-init')) {
        const p = JSON.parse(localStorage.getItem('dgb.prefs') || '{}');
        localStorage.setItem('dgb.prefs', JSON.stringify({ density: 'comfortable', history: true, rememberParams: false, ...p, theme: th }));
        sessionStorage.setItem('trailer-init', '1');
      }
    } catch {}
  }, theme);
  return c;
}
const center = async (loc) => { const b = await loc.first().boundingBox(); return b ? [Math.round(b.x + b.width / 2), Math.round(b.y + b.height / 2)] : null; };
const shoot = async (page, name) => { await sleep(500); await page.screenshot({ path: join(OUT, `${name}.png`) }); console.log('captured', name); };
async function go(page, hash) {
  await page.goto(`${BASE}/${hash}`);
  await page.waitForLoadState('networkidle');
  await sleep(700);
}

const light = await ctx('light');
const page = await light.newPage();

if (want('home-light') || want('flow')) {
  await go(page, '#/');
  boxes.themeToggle = await center(page.locator('button[title="Toggle theme"]'));
  await page.mouse.move(1300, 860);
  await shoot(page, 'home-light');
}

if (want('flow')) {
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Choose files' }).first().click();
  await (await chooser).setFiles(join(ASSETS, 'dune.jpg'));
  await page.waitForURL(/#\/open/);
  await sleep(900);
  await page.mouse.move(1430, 890);
  await shoot(page, 'intent-0');
  const input = page.getByPlaceholder('Type what you want, like “smaller” or “jpg”');
  boxes.intentInput = await center(input);
  await input.click();
  const word = 'smaller';
  for (let k = 1; k <= word.length; k++) {
    await page.keyboard.type(word[k - 1]);
    await sleep(350);
    await page.mouse.move(1430, 890);
    await shoot(page, `intent-${k}`);
  }
  const first = page.locator('.intent-option').first();
  boxes.intentRow1 = await center(first);
  boxes.intentRow1Name = (await first.locator('.intent-option-name').textContent())?.trim();
  await first.click();
  await page.waitForURL(/#\/tool\//);
  await sleep(1200);
  await page.mouse.move(1430, 890);
  await shoot(page, 'compress-ready');
  const run = page.getByRole('button', { name: /^Run/ }).first();
  boxes.runButton = await center(run);
  await run.click();
  await page.locator('.stage-cap-meta').nth(1).waitFor({ timeout: 60000 });
  await sleep(1500);
  await page.mouse.move(1430, 890);
  await shoot(page, 'compress-result');
  boxes.stage = await center(page.locator('.stage, .stage-figs, .stage-fig').first());
  const metas = page.locator('.stage-cap-meta');
  boxes.sizeBadge = await center(metas.nth(1));
  boxes.sizeText = (await metas.nth(1).textContent())?.trim();
}

if (want('home-light-2') || want('flow')) {
  // the same home after the drop, for the light/dark switch
  await go(page, '#/');
  await page.mouse.move(1300, 860);
  await shoot(page, 'home-light-2');
}

const dark = await ctx('dark');
const dpage = await dark.newPage();
if (want('home-dark')) {
  await go(dpage, '#/');
  await dpage.mouse.move(1300, 860);
  await shoot(dpage, 'home-dark');
}
if (want('json-dark')) {
  await go(dpage, '#/tool/dev.json');
  const sample = JSON.stringify({ order: 4182, customer: { name: 'Ada Park', email: 'ada@example.com' }, items: [{ sku: 'TENT-2P', qty: 1, price: 249.0 }, { sku: 'LAMP-USB', qty: 2, price: 18.5 }], paid: true, notes: null });
  const box = dpage.locator('textarea').first();
  await box.click();
  await box.fill(sample);
  await sleep(1200);
  await dpage.mouse.move(1430, 890);
  await shoot(dpage, 'json-dark');
}
if (want('redact-dark')) {
  await go(dpage, '#/tool/image.redact');
  await dpage.locator('input[type=file]').first().setInputFiles(join(ASSETS, 'meadow.jpg'));
  await sleep(1500);
  const img = dpage.locator('.redact-editor img, .region-editor img, [class*=redact] img, [class*=region] img').first();
  const b = await img.boundingBox().catch(() => null);
  if (b) {
    const drag = async (x0, y0, x1, y1) => {
      await dpage.mouse.move(b.x + b.width * x0, b.y + b.height * y0);
      await dpage.mouse.down();
      await dpage.mouse.move(b.x + b.width * x1, b.y + b.height * y1, { steps: 8 });
      await dpage.mouse.up();
      await sleep(300);
    };
    await drag(0.22, 0.3, 0.42, 0.55);
    await drag(0.6, 0.62, 0.86, 0.8);
  } else console.warn('redact: no image to draw on');
  await dpage.mouse.move(1430, 890);
  await shoot(dpage, 'redact-dark');
}
if (want('palette-dark')) {
  await go(dpage, '#/');
  await dpage.keyboard.press('Control+K');
  await sleep(500);
  await dpage.keyboard.type('resize', { delay: 60 });
  await sleep(700);
  await shoot(dpage, 'palette-dark');
  await dpage.keyboard.press('Escape');
}
if (want('system-dark')) {
  await go(dpage, '#/tool/system.overview');
  const run = dpage.getByRole('button', { name: /^Run/ }).first();
  if (await run.isVisible().catch(() => false)) { await run.click(); await sleep(3500); }
  await dpage.mouse.move(1430, 890);
  await shoot(dpage, 'system-dark');
}
for (const th of ['light', 'dark']) {
  if (!want(`phone-home-${th}`) && !want('phone')) continue;
  const c = await ctx(th, { width: 390, height: 844 }, 3);
  const p = await c.newPage();
  await go(p, '#/');
  await shoot(p, `phone-home-${th}`);
  await c.close();
}

writeFileSync(boxesFile, JSON.stringify(boxes, null, 2));
writeFileSync(join(OUT, 'boxes.js'), `window.BOXES = ${JSON.stringify(boxes)};\n`);
console.log(boxes);
await browser.close();
