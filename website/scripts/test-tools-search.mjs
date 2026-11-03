// Run with Playwright and Chromium installed. Starts its own local static server.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const { chromium } = createRequire(import.meta.url)('playwright');
const root = new URL('../', import.meta.url);
const mime = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.png': 'image/png' };

test('catalog search agrees with rendered rows and keyboard navigation', async (t) => {
  const server = createServer(async (req, res) => {
    const path = new URL(req.url, 'http://localhost').pathname;
    const file = new URL(`.${path}${path.endsWith('/') ? 'index.html' : ''}`, root);
    if (!file.href.startsWith(root.href)) { res.writeHead(403).end(); return; }
    try {
      const body = await readFile(file);
      res.writeHead(200, { 'Content-Type': mime[fileURLToPath(file).match(/\.[^.]+$/)?.[0]] || 'application/octet-stream' }).end(body);
    } catch { res.writeHead(404).end(); }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const browser = await chromium.launch({
    // Optional path for environments with a system Chromium installation.
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  t.after(() => browser.close());
  const base = `http://127.0.0.1:${server.address().port}`;

  for (const width of [1440, 375]) {
    for (const reducedMotion of ['no-preference', 'reduce']) {
      await t.test(`${width}px, motion ${reducedMotion}`, async () => {
        const page = await browser.newPage({ viewport: { width, height: 900 }, reducedMotion });
        const errors = [];
        page.on('pageerror', (error) => errors.push(error.message));
        try {
          await page.goto(`${base}/tools/`);
          await page.waitForSelector('button.cat-btn');
          const catalog = await page.locator('.tl-cat').evaluateAll((sections) => sections.map((section) => ({
            c: section.dataset.c,
            id: section.id,
            items: [...section.querySelectorAll('.tl-i')].map((row) => ({
              id: row.id,
              text: [row.id, row.dataset.k, row.querySelector('.tl-name').textContent, row.querySelector('.tl-desc').textContent].join(' ').toLowerCase(),
            })),
          })));
          assert.equal(catalog.flatMap((s) => s.items).length, 249);
          const input = page.locator('#tq');

          async function check(query = '', category = '-1') {
            const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
            const matches = catalog.map((s) => ({ ...s, items: s.items.filter((row) => words.every((w) => row.text.includes(w))) }));
            const expected = matches.filter((s) => category === '-1' || s.c === category).flatMap((s) => s.items.map((row) => row.id));
            const actual = await page.evaluate(() => {
              const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden';
              return {
                rows: [...document.querySelectorAll('.tl-i')].filter(shown).map((row) => row.id),
                sections: [...document.querySelectorAll('.tl-cat')].filter(shown).map((s) => s.id),
                leaked: [...document.querySelectorAll('#tList [hidden]')].filter(shown).map((el) => el.id),
                count: document.querySelector('#tCount').textContent,
                empty: shown(document.querySelector('#tEmpty')),
                rail: [...document.querySelectorAll('button.cat-btn')].map((b) => ({ c: b.dataset.c, count: Number(b.querySelector('.cat-btn-n').textContent), pressed: b.getAttribute('aria-pressed') })),
              };
            });
            assert.deepEqual([...actual.rows].sort(), [...expected].sort(), `visible rows for ${query}, category ${category}`);
            assert.deepEqual(actual.leaked, [], 'hidden elements must not render');
            assert.deepEqual([...actual.sections].sort(), matches.filter((s) => s.items.length && (category === '-1' || s.c === category)).map((s) => s.id).sort());
            assert.equal(actual.count.split(' in ')[0], expected.length === 0 ? 'No tools' : `${expected.length} tool${expected.length === 1 ? '' : 's'}`);
            assert.equal(actual.empty, expected.length === 0);
            for (const button of actual.rail) {
              assert.equal(button.count, button.c === '-1' ? matches.reduce((n, s) => n + s.items.length, 0) : matches.find((s) => s.c === button.c).items.length);
              assert.equal(button.pressed, String(button.c === category));
            }
            assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'no horizontal page overflow');
            return actual.rows;
          }

          async function walkResults(ids) {
            await input.focus();
            if (!ids.length) {
              await page.keyboard.press('ArrowDown');
              assert.equal(await input.evaluate((el) => document.activeElement === el), true);
              return;
            }
            for (const id of ids) {
              await page.keyboard.press('ArrowDown');
              assert.equal(await page.evaluate(() => document.activeElement.closest('.tl-i')?.id), id);
            }
            await page.keyboard.press('ArrowDown');
            assert.equal(await page.evaluate(() => document.activeElement.closest('.tl-i')?.id), ids.at(-1));
            for (const id of ids.slice(0, -1).reverse()) {
              await page.keyboard.press('ArrowUp');
              assert.equal(await page.evaluate(() => document.activeElement.closest('.tl-i')?.id), id);
            }
            await page.keyboard.press('ArrowUp');
            assert.equal(await input.evaluate((el) => document.activeElement === el), true);
          }

          await check();
          await input.fill('compress');
          const compress = await check('compress');
          assert.equal(compress.length, 11);
          assert.ok(new Set(compress.map((id) => id.split('.')[0])).size > 1);
          await walkResults(compress);
          for (const section of catalog) {
            await page.locator(`button.cat-btn[data-c="${section.c}"]`).click();
            await walkResults(await check('compress', section.c));
          }
          await page.locator('button.cat-btn[data-c="-1"]').click();
          for (const query of ['  CoMpReSs  ', 'convert', 'image compress', 'image.convert', 'no-such-tool-zzzz']) {
            await input.fill(query);
            await walkResults(await check(query));
          }
          await input.press('Escape');
          assert.equal(await input.inputValue(), '');
          await check();
          // Shareable search/category URLs must initialize the same visible state.
          await page.goto(`${base}/tools/?q=compress&c=pdf`);
          await page.waitForSelector('button.cat-btn');
          await walkResults(await check('compress', catalog.find((s) => s.id === 'cat-pdf').c));
          await input.press('Escape');
          await check('', catalog.find((s) => s.id === 'cat-pdf').c);
          await page.locator('button.cat-btn[data-c="-1"]').click();
          await check();
          assert.deepEqual(errors, []);
        } finally { await page.close(); }
      });
    }
  }
});
