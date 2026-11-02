// Generates the tool catalog and the engine/model tables from the real tool
// registry, so the docs can never drift from the code.
//
//   npm run gen:tools                       # uses ../target/{release,debug}/dogubako or `dogubako` on PATH
//   DOGUBAKO_BIN=/path/to/dogubako npm run gen:tools
//
// Writes:
//   src/content/docs/tools/index.md         overview of every category
//   src/content/docs/tools/<category>.md    one page per category, every tool with its options
//   src/data/dependencies.json              external engines with install commands
//   src/data/models.json                    optional local models
//   public/tools.json                       the machine-readable catalog, served at /tools.json
//
// Machine-specific fields (availability, local paths, installed state) are
// stripped, so the output is the same on every machine.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import GithubSlugger from 'github-slugger';

const here = dirname(fileURLToPath(import.meta.url));
const site = join(here, '..');
const repo = join(site, '..');

function findBin() {
  if (process.env.DOGUBAKO_BIN) return process.env.DOGUBAKO_BIN;
  const exe = process.platform === 'win32' ? 'dogubako.exe' : 'dogubako';
  for (const profile of ['release', 'debug']) {
    const p = join(repo, 'target', profile, exe);
    if (existsSync(p)) return p;
  }
  return 'dogubako';
}

const bin = findBin();
// A fixed, empty home so extensions or models on this machine never leak in.
const env = { ...process.env, DOGUBAKO_HOME: join(site, '.astro', 'gen-home'), DOGUBAKO_LOG: 'error' };
const run = (...args) => JSON.parse(execFileSync(bin, [...args, '--json'], { env, maxBuffer: 64 << 20 }).toString());

console.log(`Reading the tool registry from ${bin}`);
const version = execFileSync(bin, ['--version'], { env }).toString().trim().replace(/^dogubako\s*/i, '');
const list = run('tools', 'list');
const deps = run('deps');
const models = run('models', 'list');

// ---------------------------------------------------------------- categories

const CATEGORIES = [
  ['image', 'Images', 'Convert, resize, compress, inspect and edit images.'],
  ['video', 'Video', 'Convert, compress, trim and inspect video.'],
  ['audio', 'Audio', 'Convert, trim, normalize and analyze audio.'],
  ['speech', 'Speech & Voice', 'Offline transcription, voice activity, text to speech and subtitles.'],
  ['pdf', 'PDF & Documents', 'Merge, split, OCR, protect and convert documents.'],
  ['files', 'Files', 'Checksums, archives, duplicates and file inspection.'],
  ['text', 'Text', 'Transform, compare and clean up text.'],
  ['dev', 'Developer', 'Formatters, encoders, hashes and generators.'],
  ['data', 'Data', 'CSV, spreadsheets and structured data.'],
  ['network', 'Network & Web', 'DNS, HTTP, TLS, IP and URL utilities.'],
  ['design', 'Design', 'Colors, palettes, gradients and icons.'],
  ['system', 'System', 'Hardware, battery, storage and OS information.'],
];

const depName = Object.fromEntries(deps.map((d) => [d.id, d.name]));
const MODEL_GROUPS = {
  whisper: 'a Whisper speech model',
  'whisper-vad': 'the Silero voice activity model',
  'face-detection': 'the SeetaFace face detector',
  piper: 'a Piper voice',
};
const PERMISSIONS = {
  network: 'network',
  host_fs_read: 'reads host folders',
  host_fs_write: 'changes host files',
  system_info: 'system info',
  process_info: 'process list',
};
const PLATFORM = { linux: 'Linux', macos: 'macOS', windows: 'Windows' };
const UI = {
  live: 'live',
  'face-redact': 'visual editor',
  crop: 'visual editor',
  regex: 'live editor',
  qr: 'live preview',
  color: 'live swatches',
  diff: 'side-by-side diff',
  system: 'dashboard',
};

const tools = list.map(({ id }) => {
  const t = run('tools', 'info', id);
  delete t.availability;
  delete t.input_schema;
  return t;
});
tools.sort((a, b) => CATEGORIES.findIndex((c) => c[0] === a.category) - CATEGORIES.findIndex((c) => c[0] === b.category) || 0);

// ---------------------------------------------------------------- helpers

const esc = (s) =>
  String(s ?? '')
    .replace(/\\/g, '\\\\')
    .replace(/\|/g, '\\|')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/\*/g, '\\*')
    .replace(/_/g, '\\_')
    .replace(/\r?\n/g, ' ');
const code = (s) => {
  const v = String(s);
  const ticks = v.includes('`') ? '``' : '`';
  const pad = v.startsWith('`') || v.endsWith('`') ? ' ' : '';
  return `${ticks}${pad}${v.replace(/\|/g, '\\|')}${pad}${ticks}`;
};
const sentence = (s) => {
  const v = String(s ?? '').trim();
  if (!v) return '';
  return /[.!?]$/.test(v) ? v : `${v}.`;
};
const cliName = (id) => id.split('.').slice(1).join('.').replace(/_/g, '-');
const RESERVED = new Set(['json', 'quiet', 'verbose', 'out', 'force', 'help', 'version']);
const flag = (name) => {
  const k = name.replace(/_/g, '-');
  return `--${RESERVED.has(k) ? `param-${k}` : k}`;
};

function acceptSummary(accept) {
  if (!accept?.length) return '';
  const wild = accept.filter((a) => a.endsWith('/*'));
  const exts = accept.filter((a) => a.startsWith('.'));
  const mimes = accept.filter((a) => !a.startsWith('.') && !a.endsWith('/*'));
  const parts = [...wild];
  const shown = exts.slice(0, wild.length ? 6 : 10);
  if (shown.length) parts.push(shown.join(' ') + (exts.length > shown.length ? ` +${exts.length - shown.length} more` : ''));
  if (!wild.length && !exts.length && mimes.length) parts.push(mimes.slice(0, 4).join(', '));
  return parts.join(', ');
}

function typeOf(p) {
  switch (p.type) {
    case 'file':
      return p.multiple ? 'files' : 'file';
    case 'path':
      return p.kind === 'dir' ? 'folder path' : p.kind ? `${p.kind} path` : 'path';
    case 'enum':
      return p.multi ? 'list of choices' : 'choice';
    case 'integer':
    case 'number': {
      const r = p.min !== undefined && p.max !== undefined ? ` ${p.min}–${p.max}` : p.min !== undefined ? ` ≥ ${p.min}` : p.max !== undefined ? ` ≤ ${p.max}` : '';
      return `${p.type}${r}`;
    }
    case 'text':
      return p.multiline ? 'text (multi-line)' : 'text';
    case 'json':
      return 'JSON';
    default:
      return p.type;
  }
}

function defaultOf(p) {
  if (p.default === undefined || p.default === null || p.default === '') return '';
  if (Array.isArray(p.default)) return p.default.length ? code(p.default.join(',')) : '';
  if (typeof p.default === 'object') return code(JSON.stringify(p.default));
  return code(p.default);
}

function describeParam(p) {
  const parts = [];
  const d = sentence(p.description || (p.label && p.label.toLowerCase() !== p.name.replace(/_/g, ' ') ? p.label : ''));
  if (d) parts.push(esc(d));
  if (p.type === 'enum' && p.options?.length) {
    const opts = p.options.map((o) => (o.label && o.label.toLowerCase() !== String(o.value).toLowerCase() ? `${code(o.value)} (${esc(o.label)})` : code(o.value)));
    parts.push(`${p.multi ? 'Any of' : 'One of'}: ${opts.join(', ')}.`);
  }
  if (p.type === 'file' && p.accept?.length) parts.push(`Accepts ${esc(acceptSummary(p.accept))}.`);
  if (p.show_if) {
    const vals = p.show_if.values.map((v) => code(v));
    const which = vals.length === 1 ? vals[0] : `one of ${vals.join(', ')}`;
    parts.push(`Only used when ${code(p.show_if.param)} is ${which}.`);
  }
  if (p.advanced) parts.push('*Advanced.*');
  return parts.join(' ');
}

function usage(t) {
  const prim = t.params.filter((p) => p.primary);
  const args = prim
    .map((p) => {
      const n = p.type === 'file' ? (p.multiple ? '<files…>' : '<file>') : p.type === 'path' ? '<path>' : `<${p.name.replace(/_/g, '-')}>`;
      return p.required ? n : `[${n.replace(/[<>]/g, '')}]`;
    })
    .join(' ');
  const req = t.params.filter((p) => p.required && !p.primary).map((p) => `${flag(p.name)} <${p.name.replace(/_/g, '-')}>`);
  const hasOpts = t.params.some((p) => !p.primary && !p.required);
  return ['dogubako', t.category, cliName(t.id), args, ...req, hasOpts ? '[options]' : ''].filter(Boolean).join(' ');
}

function engineChip(t) {
  return { native: 'native', external: 'external engine', model: 'local model', network: 'network' }[t.execution] ?? t.execution;
}

function outputOf(t) {
  const o = t.output ?? {};
  if (o.kind === 'files') return o.text_mime ? `files, plus ${code(o.text_mime)} text` : 'files';
  if (o.kind === 'text') return o.text_mime ? `text (${code(o.text_mime)})` : 'text';
  if (o.kind === 'data') return 'structured data';
  return o.kind ?? '';
}

// ---------------------------------------------------------------- pages

const outDir = join(site, 'src', 'content', 'docs', 'tools');
mkdirSync(outDir, { recursive: true });
for (const f of readdirSync(outDir)) if (f.endsWith('.md')) rmSync(join(outDir, f));

const banner = `<!-- Generated by scripts/gen-tools.mjs from Dogubako ${version}. Do not edit by hand; run \`npm run gen:tools\`. -->`;
const anchors = {};

CATEGORIES.forEach(([cat, label, blurb], i) => {
  const items = tools.filter((t) => t.category === cat);
  const slugger = new GithubSlugger();
  const lines = [
    '---',
    `title: ${JSON.stringify(label)}`,
    `description: ${JSON.stringify(`Every one of Dogubako's ${items.length} ${label} tools, with all of their options. ${blurb}`)}`,
    'sidebar:',
    `  order: ${i + 1}`,
    `  badge: { text: ${JSON.stringify(String(items.length))}, variant: default }`,
    '---',
    '',
    banner,
    '',
    `${blurb} ${items.length} tools. Run any of them from the app, as \`dogubako ${cat} <tool>\`, over the [HTTP API](/reference/http-api/) or as an [MCP tool](/agents/overview/).`,
    '',
    '| Tool | Id | Needs |',
    '|---|---|---|',
  ];
  const needs = (t) => {
    const n = [...t.requires.map((r) => depName[r] ?? r)];
    if (t.requires_model) n.push(MODEL_GROUPS[t.requires_model] ?? `a ${t.requires_model} model`);
    if (t.permissions.length) n.push(...t.permissions.map((p) => PERMISSIONS[p] ?? p));
    return n.length ? esc(n.join(', ')) : '—';
  };
  const slugs = new Map();
  for (const t of items) slugs.set(t.id, slugger.slug(t.name));
  for (const t of items) lines.push(`| [${esc(t.name)}](#${slugs.get(t.id)}) | ${code(t.id)} | ${needs(t)} |`);
  lines.push('');

  for (const t of items) {
    anchors[t.id] = `/tools/${cat}/#${slugs.get(t.id)}`;
    lines.push(`## ${t.name}`, '');
    const chips = [`<span class="engine-${t.execution}">${engineChip(t)}</span>`];
    for (const r of t.requires) chips.push(`<span>${depName[r] ?? r}</span>`);
    if (t.requires_model) chips.push(`<span>model: ${t.requires_model}</span>`);
    for (const p of t.permissions) chips.push(`<span>${PERMISSIONS[p] ?? p}</span>`);
    if (t.platforms.length < 3) chips.push(`<span>${t.platforms.map((p) => PLATFORM[p] ?? p).join(', ')} only</span>`);
    if (t.ui && UI[t.ui]) chips.push(`<span>${UI[t.ui]}</span>`);
    if (t.reports_progress) chips.push('<span>progress</span>');
    if (t.destructive) chips.push('<span>changes files (preview first)</span>');
    if (t.source !== 'builtin') chips.push(`<span>extension: ${t.source}</span>`);
    lines.push(`<div class="tool-meta">${chips.join('')}</div>`, '');
    lines.push(esc(sentence(t.description)), '');
    lines.push('```sh', usage(t), '```', '');
    lines.push(`Id ${code(t.id)} · MCP ${code(t.id.replace(/\./g, '_'))} · Output: ${outputOf(t)} · Timeout ${Math.round(t.timeout_secs / 60) >= 1 ? `${Math.round(t.timeout_secs / 60)} min` : `${t.timeout_secs} s`}`, '');
    if (t.keywords?.length) lines.push(`Also found by: ${t.keywords.map((k) => esc(k)).join(', ')}`, '');
    if (t.params.length) {
      lines.push('| Option | CLI | Type | Default | Description |', '|---|---|---|---|---|');
      for (const p of t.params) {
        const cli = p.primary ? 'positional' : code(flag(p.name));
        const name = `${code(p.name)}${p.required ? ' *(required)*' : ''}`;
        lines.push(`| ${name} | ${cli} | ${esc(typeOf(p))} | ${defaultOf(p)} | ${describeParam(p)} |`);
      }
      lines.push('');
    } else {
      lines.push('This tool has no options.', '');
    }
  }
  writeFileSync(join(outDir, `${cat}.md`), lines.join('\n'));
});

// ---------------------------------------------------------------- overview

{
  const count = (pred) => tools.filter(pred).length;
  const lines = [
    '---',
    'title: Tool catalog',
    `description: ${JSON.stringify(`Every one of Dogubako's ${tools.length} tools, by category, with the engines and permissions each one needs.`)}`,
    'sidebar:',
    '  label: All tools',
    '  order: 0',
    '---',
    '',
    banner,
    '',
    `Dogubako ${version} has **${tools.length} tools** in ${CATEGORIES.length} categories. Each category page lists every tool with its description, CLI usage and all of its options.`,
    '',
    `- **Native** tools run inside Dogubako with nothing else installed (${count((t) => t.execution === 'native')} tools).`,
    `- **External engine** tools call a program such as FFmpeg or Tesseract that you install separately (${count((t) => t.execution === 'external')} tools). See [Engines and models](/app/engines-and-models/).`,
    `- **Local model** tools need a downloaded model, such as a Whisper model for transcription (${count((t) => t.execution === 'model')} tools).`,
    `- **Network** tools make outbound requests and need the \`network\` permission (${count((t) => t.execution === 'network')} tools).`,
    '',
    'The same catalog is available as JSON at [`/tools.json`](/tools.json), from `dogubako tools list --json`, and from a running server at `GET /api/v1/tools`.',
    '',
    '| Category | Tools | Native | External | Model | Network |',
    '|---|---:|---:|---:|---:|---:|',
  ];
  for (const [cat, label] of CATEGORIES) {
    const c = (e) => count((t) => t.category === cat && t.execution === e);
    lines.push(`| [${esc(label)}](/tools/${cat}/) | ${count((t) => t.category === cat)} | ${c('native')} | ${c('external')} | ${c('model')} | ${c('network')} |`);
  }
  lines.push('');
  for (const [cat, label, blurb] of CATEGORIES) {
    lines.push(`## ${label}`, '', blurb, '');
    lines.push(
      tools
        .filter((t) => t.category === cat)
        .map((t) => `[${esc(t.name)}](${anchors[t.id]})`)
        .join(' · '),
      '',
    );
  }
  writeFileSync(join(outDir, 'index.md'), lines.join('\n'));
}

// ---------------------------------------------------------------- data files

const dataDir = join(site, 'src', 'data');
mkdirSync(dataDir, { recursive: true });
const usedBy = (id) => tools.filter((t) => t.requires.includes(id)).length;
writeFileSync(
  join(dataDir, 'dependencies.json'),
  JSON.stringify(
    deps.map(({ id, name, purpose, license, install }) => ({ id, name, purpose, license, install, tools: usedBy(id) })),
    null,
    2,
  ) + '\n',
);
writeFileSync(
  join(dataDir, 'models.json'),
  JSON.stringify(
    models.map(({ id, group, name, description, size_bytes, license, languages }) => ({ id, group, name, description, size_bytes, license, languages })),
    null,
    2,
  ) + '\n',
);
writeFileSync(join(site, 'public', 'tools.json'), JSON.stringify({ version, tools }, null, 1) + '\n');

console.log(`Wrote ${tools.length} tools in ${CATEGORIES.length} categories, ${deps.length} engines and ${models.length} models.`);
