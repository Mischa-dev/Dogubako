// Render the trailer: step scenes/index.html frame by frame in headless
// Chromium, pipe the frames to FFmpeg, and lay the soundtrack under them.
//
//   node trailer/tools/render.mjs                 # full film, 60 fps, 1080p
//   node trailer/tools/render.mjs --stills 3,12.4 # PNG stills into build/stills
//   node trailer/tools/render.mjs --fps 30 --from 10 --to 20 --out build/part.mp4
//
//   --workers 3 browsers render in parallel; frames go to build/frames first.
// FFmpeg comes from $FFMPEG, or `ffmpeg` on PATH, or the imageio-ffmpeg wheel.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (name, d) => { const i = args.indexOf(`--${name}`); return i >= 0 ? args[i + 1] : d; };
const fps = Number(opt('fps', 60));
const from = Number(opt('from', 0));
const stills = opt('stills', null);
const out = resolve(opt('out', join(root, 'build', 'dogubako-alpha-trailer.mp4')));
const music = join(root, 'build', 'music.wav');

function ffmpegPath() {
  if (process.env.FFMPEG) return process.env.FFMPEG;
  try { execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' }); return 'ffmpeg'; } catch {}
  return execFileSync('python3', ['-I', '-c', 'import imageio_ffmpeg;print(imageio_ffmpeg.get_ffmpeg_exe())']).toString().trim();
}

const workers = Number(opt('workers', 3));
const launch = () => chromium.launch({ args: ['--force-color-profile=srgb', '--font-render-hinting=none'] });

async function openPage(browser) {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
  page.on('pageerror', (e) => console.error('page error:', e.message));
  await page.goto(pathToFileURL(join(root, 'scenes', 'index.html')).href + '?t=0');
  await page.evaluate(() => window.trailerReady);
  const cdp = await page.context().newCDPSession(page);
  return {
    page,
    async frame(t, type = 'jpeg') {
      await page.evaluate((tt) => window.render(tt), t);
      const { data } = await cdp.send('Page.captureScreenshot', type === 'png' ? { format: 'png' } : { format: 'jpeg', quality: 96 });
      return Buffer.from(data, 'base64');
    },
  };
}

if (stills) {
  const browser = await launch();
  const { frame } = await openPage(browser);
  const dir = join(root, 'build', 'stills');
  mkdirSync(dir, { recursive: true });
  for (const s of stills.split(',')) {
    writeFileSync(join(dir, `t${Number(s).toFixed(2).padStart(6, '0')}.png`), await frame(Number(s), 'png'));
  }
  console.log(`stills in ${dir}`);
  await browser.close();
  process.exit(0);
}

// 1. frames: a few browsers in parallel, each taking every Nth frame
const probe = await launch();
const duration = Number(opt('to', await (await openPage(probe)).page.evaluate(() => window.DURATION)));
await probe.close();
const frames = join(root, 'build', 'frames');
rmSync(frames, { recursive: true, force: true });
mkdirSync(frames, { recursive: true });
const total = Math.round((duration - from) * fps);
const started = Date.now();
let done = 0;
await Promise.all(Array.from({ length: workers }, async (_, w) => {
  const browser = await launch();
  const { frame } = await openPage(browser);
  for (let i = w; i < total; i += workers) {
    writeFileSync(join(frames, `${String(i).padStart(5, '0')}.jpg`), await frame(from + i / fps));
    if (++done % (fps * 4) === 0) console.log(`frame ${done}/${total}  ${(done / ((Date.now() - started) / 1000)).toFixed(1)} fps`);
  }
  await browser.close();
}));

// 2. encode with the soundtrack
mkdirSync(dirname(out), { recursive: true });
const withAudio = existsSync(music);
execFileSync(ffmpegPath(), [
  '-y', '-loglevel', 'error',
  '-framerate', String(fps), '-i', join(frames, '%05d.jpg'),
  ...(withAudio ? ['-ss', String(from), '-t', String(duration - from), '-i', music] : []),
  '-c:v', 'libx264', '-preset', 'slow', '-crf', String(opt('crf', 18)), '-pix_fmt', 'yuv420p',
  '-vf', 'scale=in_range=full:out_range=tv,format=yuv420p',
  '-x264-params', `keyint=${fps * 2}:min-keyint=${fps}`,
  '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'bt709',
  ...(withAudio ? ['-c:a', 'aac', '-b:a', '192k', '-shortest'] : []),
  '-movflags', '+faststart', out,
], { stdio: 'inherit' });
console.log(`wrote ${out} in ${((Date.now() - started) / 1000).toFixed(0)} s`);
