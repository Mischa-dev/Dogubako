#!/usr/bin/env node
// Writes the static tool list into website/tools/index.html from assets/js/tools-data.js,
// so the page shows all tools without JavaScript. Run it again whenever tools-data.js changes:
//
//   node website/scripts/build-tools-page.mjs
//
// It only replaces what sits between the <!-- tools:…:start --> and <!-- tools:…:end -->
// markers in the page, and fills in the tool count wherever <!-- tools:count --> is followed
// by a number.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const dataFile = join(site, 'assets/js/tools-data.js');
const pageFile = join(site, 'tools/index.html');

const sandbox = { window: {} };
vm.runInNewContext(readFileSync(dataFile, 'utf8'), sandbox);
const CATS = sandbox.window.DGB_CATS;
const TOOLS = sandbox.window.DGB_TOOLS;
if (!Array.isArray(CATS) || !Array.isArray(TOOLS)) throw new Error('tools-data.js did not define DGB_CATS / DGB_TOOLS');

const DOCS = 'https://docs.dogubako.app';
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

// Heading anchors on the docs site (github-slugger rules: lower case, drop punctuation and
// symbols except "-" and "_", spaces become "-", repeats get -1, -2…).
function slugger() {
  const seen = new Map();
  return (text) => {
    const base = text.toLowerCase().replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '').replace(/ /g, '-');
    let s = base;
    let n = seen.get(base) || 0;
    while (seen.has(s)) s = `${base}-${++n}`;
    seen.set(base, n);
    seen.set(s, 0);
    return s;
  };
}

const cliOf = (id) => {
  const [cat, rest] = id.split('.');
  return `dogubako ${cat} ${rest.replace(/_/g, '-')}`;
};
const icon = (name, cls = 'i') => `<svg class="${cls}" aria-hidden="true"><use href="#i-${name}"/></svg>`;

const groups = CATS.map((c, ci) => ({ c, ci, tools: TOOLS.map((t, i) => ({ t, i })).filter((x) => x.t[0] === ci) }));

const rail = [
  `<li><a class="cat-btn" href="#catalog" data-c="-1"><span class="cat-btn-l">All tools</span><span class="cat-btn-n mono">${TOOLS.length}</span></a></li>`,
  ...groups.map(({ c, ci, tools }) =>
    `<li><a class="cat-btn" href="#cat-${c[0]}" data-c="${ci}">${icon(c[2])}<span class="cat-btn-l">${esc(c[1])}</span><span class="cat-btn-n mono">${tools.length}</span></a></li>`),
].join('\n            ');

const list = groups.map(({ c, ci, tools }) => {
  const slug = slugger();
  const rows = tools.map(({ t, i }) => {
    const [, name, id, desc, kw] = t;
    const href = `${DOCS}/tools/${c[0]}/#${slug(name)}`;
    return `<li class="tl-i" id="${esc(id)}" data-i="${i}" data-k="${esc(kw)}">` +
      `<h3 class="tl-name"><a href="${href}">${esc(name)}</a></h3>` +
      `<p class="tl-desc">${esc(desc)}</p>` +
      `<p class="tl-meta"><code class="tl-id">${esc(id)}</code><code class="tl-cli">${esc(cliOf(id))}</code></p></li>`;
  }).join('\n          ');
  return `<section class="tl-cat" id="cat-${c[0]}" data-c="${ci}" aria-labelledby="h-${c[0]}">
        <div class="tl-head">
          <h2 class="tl-h" id="h-${c[0]}">${icon(c[2], 'i tl-h-i')}${esc(c[1])} <span class="tl-h-n mono">${tools.length}</span></h2>
          <a class="tl-doc" href="${DOCS}/tools/${c[0]}/">${esc(c[1])} in the docs ${icon('arrow-up-right')}</a>
        </div>
        <ul class="tl-list">
          ${rows}
        </ul>
      </section>`;
}).join('\n      ');

let html = readFileSync(pageFile, 'utf8');
function fill(name, body, indent) {
  const re = new RegExp(`(<!-- tools:${name}:start -->)[\\s\\S]*?(\\n[ \\t]*<!-- tools:${name}:end -->)`);
  if (!re.test(html)) throw new Error(`marker tools:${name} not found in tools/index.html`);
  html = html.replace(re, (_, a, b) => `${a}\n${indent}${body}${b}`);
}
fill('rail', rail, '            ');
fill('list', list, '      ');
html = html.replace(/(<!-- tools:count -->)\d+/g, `$1${TOOLS.length}`);
writeFileSync(pageFile, html);
console.log(`tools/index.html: ${TOOLS.length} tools in ${CATS.length} categories`);
