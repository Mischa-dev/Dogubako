// The bench, in 3D. One fixed WebGL canvas behind the hero and the drawer tour:
// a lacquered toolbox (the logo, as an object), file objects that tumble onto a
// hinoki bench, and a scroll-driven camera that walks past all twelve drawers.
import * as THREE from 'three';
import * as TX from './textures.js';
import { CATS } from './data.js';

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const lerp = (a, b, t) => a + (b - a) * t;
const smooth = (t) => { t = clamp(t); return t * t * (3 - 2 * t); };
const smoother = (t) => { t = clamp(t); return t * t * t * (t * (t * 6 - 15) + 10); };
const easeInOut = (t) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);
const backOut = (t) => { const c1 = 1.5, c3 = c1 + 1; return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2); };
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

export function fmtBytes(n) {
  if (n < 1000) return `${n} bytes`;
  if (n < 1000 * 1024) return `${Math.max(1, Math.round(n / 1024))} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

// Likeliest tools per kind of file, in the order the app's drop screen ranks them (real names and ids).
const SUGGEST = {
  image: [['image.compress', 'Compress Images'], ['image.convert', 'Convert Images'], ['image.strip_metadata', 'Remove Image Metadata']],
  pdf: [['pdf.compress', 'Compress PDF'], ['pdf.merge', 'Merge PDFs'], ['pdf.ocr', 'OCR PDF']],
  pdfs: [['pdf.merge', 'Merge PDFs'], ['pdf.compress', 'Compress PDF'], ['pdf.ocr', 'OCR PDF']],
  video: [['video.trim', 'Trim Video'], ['video.compress', 'Compress Video'], ['video.to_gif', 'Video to GIF']],
  audio: [['speech.transcribe', 'Transcribe Audio / Video'], ['audio.normalize', 'Normalize Audio'], ['audio.remove_silence', 'Remove Silence']],
  sheet: [['data.clean', 'Clean CSV Data'], ['data.stats', 'Column Statistics'], ['data.csv_filter_sort', 'Filter & Sort CSV']],
  sheetOwn: [['data.stats', 'Column Statistics'], ['data.clean', 'Clean CSV Data'], ['data.csv_filter_sort', 'Filter & Sort CSV']],
  json: [['dev.json', 'JSON Formatter & Validator'], ['data.json_to_csv', 'JSON to CSV'], ['dev.jq', 'jq Filter']],
  text: [['text.count', 'Word & Character Count'], ['text.sort_lines', 'Sort Lines'], ['text.diff', 'Compare Text (Diff)']],
  archive: [['files.archive_list', 'List Archive Contents'], ['files.archive_extract', 'Extract Archive'], ['files.hash', 'File Hash']],
  file: [['files.type_detect', 'Detect File Type'], ['files.hash', 'File Hash'], ['files.hexdump', 'Hex Viewer']],
};
const KIND_WORD = { image: 'Photo', pdf: 'PDF', video: 'Video', audio: 'Audio', sheet: 'Spreadsheet', json: 'JSON', text: 'Text', archive: 'Archive', file: 'File' };

export function kindOf(mime, name) {
  const m = (mime || '').toLowerCase();
  const e = (name.includes('.') ? name.split('.').pop() : '').toLowerCase();
  if (m.startsWith('image/') || /^(jpe?g|png|webp|gif|avif|bmp|heic|heif|tiff?|svg)$/.test(e)) return 'image';
  if (m === 'application/pdf' || e === 'pdf') return 'pdf';
  if (m.startsWith('video/') || /^(mp4|mov|mkv|webm|avi|m4v)$/.test(e)) return 'video';
  if (m.startsWith('audio/') || /^(mp3|wav|flac|m4a|ogg|opus|aac)$/.test(e)) return 'audio';
  if (m === 'text/csv' || /^(csv|tsv|xlsx|xls|ods)$/.test(e) || m.includes('spreadsheet')) return 'sheet';
  if (/^(zip|tar|gz|tgz|7z|xz|zst|bz2|rar)$/.test(e) || m.includes('zip') || m.includes('compressed')) return 'archive';
  if (e === 'json' || m === 'application/json') return 'json';
  if (m.startsWith('text/') || /xml|yaml|toml|javascript/.test(m) || /^(txt|md|yaml|yml|toml|xml|srt|vtt|log|js|mjs|ts|css|html|py|rs|go|ini|conf)$/.test(e)) return 'text';
  return 'file';
}

const JSON_MIN = '{"name":"dogubako","tools":249,"categories":12,"local":true,"telemetry":false,"license":"AGPL-3.0-or-later"}';

// The sample files. Results are illustrative of what each tool does; they are labelled as such on the page.
const SAMPLES = [
  { id: 'photo', kind: 'image', name: 'dune.jpg', meta: 'Photo · 4.2 MB', suggest: SUGGEST.image, result: '4.2 MB → 1.1 MB', phone: true },
  { id: 'pdf', kind: 'pdfs', name: '3 PDFs', meta: 'lease.pdf, addendum.pdf, terms.pdf', suggest: SUGGEST.pdfs, result: '3 files → merged.pdf, 9 pages', phone: true },
  { id: 'film', kind: 'video', name: 'talk.mov', meta: 'Video · 12:40 · 418 MB', suggest: SUGGEST.video, result: '12:40 → 2:05', phone: true },
  { id: 'wave', kind: 'audio', name: 'interview.m4a', meta: 'Audio · 34 MB', suggest: SUGGEST.audio, result: 'interview.srt, offline' },
  { id: 'sheet', kind: 'sheet', name: 'orders.csv', meta: 'CSV · 11 rows', suggest: SUGGEST.sheet, result: '2 duplicate rows removed' },
  { id: 'json', kind: 'json', name: 'config.json', meta: `JSON · ${JSON_MIN.length} bytes`, suggest: SUGGEST.json, result: 'Valid JSON · formatted', phone: true },
];

// Where things sit, per layout. Box: [x, z, yaw]. Things: [x, z, yaw].
const LAYOUTS = {
  desk: {
    fov: 28,
    box: [2.35, -1.55, -0.2],
    things: { photo: [-0.7, 0.55, -0.12], pdf: [-3.1, 0.98, 0.18], film: [1.25, 1.98, 0.06], wave: [5.55, 0.95, -0.08], sheet: [2.35, 0.75, 0.13], json: [5.2, 2.55, -0.2] },
    spots: [[-0.9, -2.3], [7.0, -1.85]],
    hero: { pos: [-1.42, 16.7, 13.36], target: [1.0, 0.0, 0.6], shift: [0.25, -0.42] },
    drawerShift: [0.3, -0.02],
    focusR: 0.42, blur: 5.5,
  },
  phone: {
    fov: 40,
    box: [0.0, -1.7, 0],
    things: { photo: [-2.7, 0.35, -0.12], pdf: [2.6, 0.45, 0.14], film: [0, 2.05, 0.05], json: [-3.05, -1.35, -0.18] },
    spots: [[2.0, 1.75], [-2.0, 1.9], [3.1, -1.3]],
    hero: { pos: [0, 18.6, 16.0], target: [0, 0.15, 0.6], shift: [0, -0.5] },
    drawerShift: [0, 0.36],
    focusR: 0.46, blur: 3.5,
  },
};

const BOX = { W: 4.0, D: 2.4, Hb: 1.72, Hl: 0.8, wall: 0.1, r: 0.16, tray: 0.3, cap: 0.12 };
const DR = { cols: 4, rows: 3, mx: 0.12, gap: 0.05, band: 0.3, bottom: 0.12, len: 1.95, pull: 1.25 };
const LID_OPEN = -2.12;

// ---------------------------------------------------------------------- geometry helpers
function rrShape(w, h, r, target = new THREE.Shape()) {
  const x = -w / 2, y = -h / 2;
  r = Math.min(r, w / 2, h / 2);
  target.moveTo(x + r, y);
  target.lineTo(x + w - r, y); target.absarc(x + w - r, y + r, r, -Math.PI / 2, 0, false);
  target.lineTo(x + w, y + h - r); target.absarc(x + w - r, y + h - r, r, 0, Math.PI / 2, false);
  target.lineTo(x + r, y + h); target.absarc(x + r, y + h - r, r, Math.PI / 2, Math.PI, false);
  target.lineTo(x, y + r); target.absarc(x + r, y + r, r, Math.PI, Math.PI * 1.5, false);
  return target;
}
/** Rounded slab lying flat: footprint w x d, from y=0 to y=h, soft bevelled edges. */
function slab(w, d, h, r, b = 0.03) {
  const g = new THREE.ExtrudeGeometry(rrShape(w - 2 * b, d - 2 * b, Math.max(0.005, r - b)), {
    depth: Math.max(0.001, h - 2 * b), bevelEnabled: true, bevelThickness: b, bevelSize: b, bevelSegments: 4, curveSegments: 10,
  });
  g.rotateX(-Math.PI / 2);
  g.translate(0, b, 0);
  return g;
}
/** Rounded rim (a ring of walls), from y=0 to y=h. */
function rim(w, d, h, r, t, b = 0.014) {
  const s = rrShape(w - 2 * b, d - 2 * b, r - b);
  s.holes.push(rrShape(w - 2 * t + 2 * b, d - 2 * t + 2 * b, Math.max(0.01, r - t + b), new THREE.Path()));
  const g = new THREE.ExtrudeGeometry(s, { depth: Math.max(0.001, h - 2 * b), bevelEnabled: true, bevelThickness: b, bevelSize: b, bevelSegments: 3, curveSegments: 10 });
  g.rotateX(-Math.PI / 2);
  g.translate(0, b, 0);
  return g;
}
/** Rounded plate facing +z (drawer fronts, the latch): w x h, from z=0 to z=t. */
function plate(w, h, t, r, b = 0.012) {
  const g = new THREE.ExtrudeGeometry(rrShape(w - 2 * b, h - 2 * b, Math.max(0.004, r - b)), {
    depth: Math.max(0.001, t - 2 * b), bevelEnabled: true, bevelThickness: b, bevelSize: b, bevelSegments: 3, curveSegments: 8,
  });
  g.translate(0, 0, b);
  return g;
}

// Neutral tone mapping, so the fog and clear colour can be pre-compensated to land exactly on the page's paper.
function neutral([r, g, b]) {
  const start = 0.76, desat = 0.15;
  const x = Math.min(r, g, b);
  const off = x < 0.08 ? x - 6.25 * x * x : 0.04;
  r -= off; g -= off; b -= off;
  const peak = Math.max(r, g, b);
  if (peak < start) return [r, g, b];
  const d = 1 - start, np = 1 - (d * d) / (peak + d - start), s = np / peak;
  r *= s; g *= s; b *= s;
  const k = 1 - 1 / (desat * (peak - np) + 1);
  return [r + (np - r) * k, g + (np - g) * k, b + (np - b) * k];
}
function untone(hex) {
  const c = new THREE.Color(hex);
  const target = [c.r, c.g, c.b];
  let v = target.slice();
  for (let i = 0; i < 80; i++) {
    const m = neutral(v);
    v = v.map((x, k) => Math.max(0, x + (target[k] - m[k]) * 1.1));
  }
  return new THREE.Color(v[0], v[1], v[2]);
}

// ---------------------------------------------------------------------- post: soft focus, vignette, dither
const POST_VS = 'varying vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }';
const POST_FS = `
uniform sampler2D tColor; uniform vec2 uRes; uniform vec2 uFocus; uniform float uRadius; uniform float uBlur; uniform float uVig; uniform float uAspect;
varying vec2 vUv;
float hash(vec2 p){ return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453); }
void main(){
  vec2 d = (vUv - uFocus) * vec2(uAspect * 0.62, 1.0);
  float amt = smoothstep(uRadius, uRadius + 0.5, length(d)) * uBlur;
  vec4 c = texture2D(tColor, vUv);
  if (amt > 0.4) {
    vec4 acc = c; float rot = hash(gl_FragCoord.xy) * 6.2831;
    for (int i = 0; i < 12; i++) {
      float fi = float(i);
      float a = fi * 2.39996 + rot;
      float r = sqrt((fi + 0.5) / 12.0) * amt;
      acc += texture2D(tColor, vUv + vec2(cos(a), sin(a)) * r / uRes);
    }
    c = acc / 13.0;
  }
  gl_FragColor = vec4(c.rgb, 1.0);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
  float v = smoothstep(0.55, 1.25, length((vUv - 0.5) * vec2(uAspect, 1.0) * 1.05));
  gl_FragColor.rgb *= 1.0 - v * uVig;
  gl_FragColor.rgb += (hash(gl_FragCoord.xy + 3.7) - 0.5) / 255.0;
}`;

// ======================================================================= start
export async function start() {
  const canvas = document.getElementById('stage');
  const html = document.documentElement;
  const params = new URLSearchParams(location.search);
  const STILL = params.has('still');
  const MANUAL = params.has('manual') || STILL;
  // The drawer still is a portrait of the box; loose prints at the frame edge only distract.
  const BARE = STILL && parseFloat(params.get('t') || '0') >= 1;
  const touch = matchMedia('(hover: none)').matches;

  // Fonts first: labels and cards are painted with them.
  try {
    await Promise.race([
      Promise.all([`600 40px ${TX.SANS}`, `500 40px ${TX.SANS}`, `400 30px ${TX.MONO}`, `500 30px ${TX.MONO}`].map((f) => document.fonts.load(f))),
      wait(2500),
    ]);
  } catch { /* draw with fallbacks */ }

  const renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: 'high-performance', preserveDrawingBuffer: STILL });
  renderer.toneMapping = THREE.NeutralToneMapping;
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.VSMShadowMap;
  renderer.shadowMap.autoUpdate = true;
  const maxAniso = Math.min(8, renderer.capabilities.getMaxAnisotropy());

  let layoutName = pickLayout();
  let L = LAYOUTS[layoutName];
  let dprCap = STILL ? 2 : layoutName === 'phone' ? 1.6 : 1.75;
  let dpr = Math.min(window.devicePixelRatio || 1, dprCap);

  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(L.fov, 1, 0.5, 80);
  scene.fog = new THREE.Fog(0xffffff, 13, 30);

  // ------------------------------------------------------------------ environment (a soft studio, for reflections)
  const pmrem = new THREE.PMREMGenerator(renderer);
  function studio() {
    const room = new THREE.Scene();
    const shell = new THREE.Mesh(new THREE.BoxGeometry(40, 20, 40), new THREE.MeshBasicMaterial({ color: new THREE.Color(0xaeaaa4).multiplyScalar(0.42), side: THREE.BackSide }));
    shell.position.y = 6;
    room.add(shell);
    const panel = (w, h, hex, k, x, y, z) => {
      const m = new THREE.Mesh(new THREE.PlaneGeometry(w, h), new THREE.MeshBasicMaterial({ color: new THREE.Color(hex).multiplyScalar(k), side: THREE.DoubleSide }));
      m.position.set(x, y, z); m.lookAt(0, 0, 0); room.add(m);
    };
    panel(12, 7, 0xfff1de, 5.0, -9, 11, 7);
    panel(9, 3, 0xe9eeff, 1.6, 12, 5, 4);
    panel(18, 2.2, 0xffffff, 2.6, 0, 8, -13);
    panel(5, 5, 0xffffff, 1.1, 3, 14, 3);
    const floor = new THREE.Mesh(new THREE.PlaneGeometry(40, 40), new THREE.MeshBasicMaterial({ color: new THREE.Color(0xddd6ca).multiplyScalar(0.7) }));
    floor.rotation.x = -Math.PI / 2; floor.position.y = -0.5;
    room.add(floor);
    const env = pmrem.fromScene(room, 0.035).texture;
    room.traverse((o) => { if (o.isMesh) { o.geometry.dispose(); o.material.dispose(); } });
    return env;
  }
  scene.environment = studio();

  // ------------------------------------------------------------------ lights
  const hemi = new THREE.HemisphereLight(0xffffff, 0xd8cfc0, 0.35);
  scene.add(hemi);
  const key = new THREE.DirectionalLight(0xfff5ea, 2.6);
  key.position.set(-6.5, 11, 6);
  key.castShadow = true;
  key.shadow.mapSize.set(layoutName === 'phone' ? 1024 : 2048, layoutName === 'phone' ? 1024 : 2048);
  Object.assign(key.shadow.camera, { left: -8, right: 8, top: 8, bottom: -8, near: 2, far: 32 });
  key.shadow.radius = 9;
  key.shadow.blurSamples = 14;
  key.shadow.bias = -0.0004;
  scene.add(key, key.target);
  const fill = new THREE.DirectionalLight(0xe4ebff, 0.45);
  fill.position.set(8, 4, 6);
  scene.add(fill);
  const rimL = new THREE.DirectionalLight(0xffffff, 0.9);
  rimL.position.set(2, 6, -10);
  scene.add(rimL);

  // ------------------------------------------------------------------ textures & materials
  const tex = (c, { repeat = 0, srgb = true } = {}) => {
    const t = new THREE.CanvasTexture(c);
    if (srgb) t.colorSpace = THREE.SRGBColorSpace;
    t.anisotropy = maxAniso;
    if (repeat) { t.wrapS = t.wrapT = THREE.RepeatWrapping; t.repeat.set(repeat, repeat); }
    return t;
  };
  const grain = tex(TX.paperGrain(256), { repeat: 3, srgb: false });
  const woodLight = tex(TX.benchWood(false), { repeat: 15 });
  let woodDark = null;
  const blobTex = tex(TX.blob(128), { srgb: false });

  const M = {
    lacquer: new THREE.MeshPhysicalMaterial({ color: 0x141414, roughness: 0.34, metalness: 0, clearcoat: 1, clearcoatRoughness: 0.14, envMapIntensity: 1.1 }),
    front: new THREE.MeshPhysicalMaterial({ color: 0x1b1a17, roughness: 0.38, clearcoat: 0.9, clearcoatRoughness: 0.18, envMapIntensity: 1.0 }),
    latch: new THREE.MeshPhysicalMaterial({ color: 0xe0533d, roughness: 0.36, clearcoat: 0.9, clearcoatRoughness: 0.16, envMapIntensity: 0.9 }),
    pull: new THREE.MeshStandardMaterial({ color: 0x3a352e, metalness: 0.75, roughness: 0.32, envMapIntensity: 1.2 }),
    lining: new THREE.MeshStandardMaterial({ color: 0xffffff, map: tex(TX.trayLining()), roughness: 0.95, bumpMap: grain, bumpScale: 0.5 }),
    lidLining: new THREE.MeshStandardMaterial({ color: 0xffffff, map: tex(TX.lidLining()), roughness: 0.95, bumpMap: grain, bumpScale: 0.5 }),
    trayWood: new THREE.MeshStandardMaterial({ color: 0xe4dccd, roughness: 0.85 }),
    cavity: new THREE.MeshBasicMaterial({ color: 0x0a0908 }),
    edge: new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.9, bumpMap: grain, bumpScale: 0.3 }),
    bench: new THREE.MeshStandardMaterial({ color: 0xffffff, map: woodLight, roughness: 0.82, envMapIntensity: 0.6 }),
    blob: new THREE.MeshBasicMaterial({ color: 0x2a1c0e, alphaMap: blobTex, transparent: true, opacity: 0.42, depthWrite: false }),
  };
  const paperMats = [M.edge, M.lining, M.lidLining, M.trayWood];
  const paperMat = (map, rough = 0.86) => {
    const m = new THREE.MeshStandardMaterial({ color: 0xffffff, map, roughness: rough, bumpMap: grain, bumpScale: 0.3 });
    paperMats.push(m);
    return m;
  };

  // ------------------------------------------------------------------ bench
  const bench = new THREE.Mesh(new THREE.PlaneGeometry(80, 80), M.bench);
  bench.rotation.x = -Math.PI / 2;
  bench.receiveShadow = true;
  scene.add(bench);

  // ------------------------------------------------------------------ the toolbox
  const box = new THREE.Group();
  scene.add(box);
  const { W, D, Hb, Hl, wall, r: R, tray, cap } = BOX;
  const bodyH = Hb - tray;
  const add = (parent, geo, mat, { cast = true, receive = true } = {}) => {
    const m = new THREE.Mesh(geo, mat);
    m.castShadow = cast; m.receiveShadow = receive;
    parent.add(m);
    return m;
  };
  add(box, slab(W, D, bodyH, R, 0.035), M.lacquer);
  add(box, rim(W, D, tray, R, wall), M.lacquer).position.y = bodyH - 0.02;
  const trayFloor = add(box, new THREE.PlaneGeometry(W - 2 * wall - 0.02, D - 2 * wall - 0.02), M.lining, { cast: false });
  trayFloor.rotation.x = -Math.PI / 2;
  trayFloor.position.y = bodyH + 0.004;

  // dividers in the top tray, like a real tool tray
  {
    const inW = W - 2 * wall, inD = D - 2 * wall, dh2 = tray - 0.05;
    const long = add(box, new THREE.BoxGeometry(inW - 0.02, dh2, 0.045), M.lacquer);
    long.position.set(0, bodyH + dh2 / 2, inD / 2 - 0.62);
    for (const x of [-0.55, 0.75]) {
      const short = add(box, new THREE.BoxGeometry(0.045, dh2, inD - 0.64), M.lacquer);
      short.position.set(x, bodyH + dh2 / 2, -0.3);
    }
  }
  const lidPivot = new THREE.Group();
  lidPivot.position.set(0, Hb, -D / 2);
  box.add(lidPivot);
  const lid = new THREE.Group();
  lid.position.z = D / 2;
  lidPivot.add(lid);
  add(lid, rim(W, D, Hl - cap + 0.02, R, wall), M.lacquer);
  add(lid, slab(W, D, cap, R, 0.035), M.lacquer).position.y = Hl - cap;
  const lidIn = add(lid, new THREE.PlaneGeometry(W - 2 * wall - 0.02, D - 2 * wall - 0.02), M.lidLining, { cast: false });
  lidIn.rotation.x = Math.PI / 2;
  lidIn.position.y = Hl - cap - 0.004;
  // The latch: the logo's vermilion mark, straddling the lid line.
  const latch = add(lid, plate(0.66, 0.46, 0.075, 0.06, 0.018), M.latch);
  latch.position.set(0, -0.06, D / 2 - 0.004);
  const latchPin = add(lid, plate(0.2, 0.08, 0.03, 0.03, 0.008), M.pull);
  latchPin.position.set(0, -0.06, D / 2 + 0.072);

  // Drawers: 4 x 3, in category order, each with its paper label, real count and tools.
  const drawers = [];
  const dw = (W - 2 * DR.mx - (DR.cols - 1) * DR.gap) / DR.cols;
  const dh = (Hb - DR.band - DR.bottom - (DR.rows - 1) * DR.gap) / DR.rows;
  const frontGeo = plate(dw - 0.008, dh - 0.008, 0.045, 0.03, 0.01);
  const labelGeo = new THREE.PlaneGeometry(0.5, 0.156);
  const pullGeo = new THREE.BoxGeometry(0.24, 0.032, 0.036);
  const sideGeo = new THREE.BoxGeometry(0.02, dh - 0.08, DR.len);
  const backGeo = new THREE.BoxGeometry(dw - 0.1, dh - 0.08, 0.02);
  const floorGeo = new THREE.PlaneGeometry(dw - 0.1, DR.len);
  const cavGeo = new THREE.PlaneGeometry(dw - 0.03, dh - 0.03);
  CATS.forEach((cat, i) => {
    const c = i % DR.cols, row = Math.floor(i / DR.cols);
    const x = -W / 2 + DR.mx + dw / 2 + c * (dw + DR.gap);
    const y = Hb - DR.band - dh / 2 - row * (dh + DR.gap);
    const cav = add(box, cavGeo, M.cavity, { cast: false, receive: false });
    cav.position.set(x, y, D / 2 + 0.002);
    const g = new THREE.Group();
    g.position.set(x, y, D / 2);
    box.add(g);
    add(g, frontGeo, M.front);
    const lm = paperMat(tex(TX.drawerLabel(cat.name, cat.count)), 0.7);
    const label = add(g, labelGeo, lm, { cast: false });
    label.position.set(0, 0.052, 0.047);
    add(g, pullGeo, M.pull).position.set(0, -0.112, 0.064);
    const inner = new THREE.Group();
    inner.visible = false; // only drawn when the drawer is out
    g.add(inner);
    add(inner, sideGeo, M.trayWood).position.set(-(dw / 2 - 0.06), 0, -DR.len / 2);
    add(inner, sideGeo, M.trayWood).position.set(dw / 2 - 0.06, 0, -DR.len / 2);
    add(inner, backGeo, M.trayWood).position.set(0, 0, -DR.len + 0.01);
    const floorMat = paperMat(null, 0.92);
    const floor = add(inner, floorGeo, floorMat, { cast: false });
    floor.rotation.x = -Math.PI / 2;
    floor.position.set(0, -dh / 2 + 0.05, -DR.len / 2);
    drawers.push({ cat, g, inner, floorMat, x, y, open: 0, painted: false });
  });
  const paintFloor = (d) => {
    if (d.painted) return;
    d.painted = true;
    d.floorMat.map = tex(TX.drawerFloor(d.cat.tools));
    d.floorMat.needsUpdate = true;
  };
  const idle = window.requestIdleCallback || ((f) => setTimeout(f, 60));
  const paintQueue = drawers.slice();
  const paintNext = () => { const d = paintQueue.shift(); if (!d) return; paintFloor(d); idle(paintNext); };
  setTimeout(() => idle(paintNext), 2500);

  // A soft contact shadow under the box: the ambient-occlusion feel.
  const boxBlob = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), M.blob.clone());
  boxBlob.material.opacity = 0.62;
  boxBlob.rotation.order = 'YXZ';
  boxBlob.renderOrder = 1;
  scene.add(boxBlob);

  // ------------------------------------------------------------------ bench things
  const things = [];
  const hitMat = new THREE.MeshBasicMaterial();
  const unit = new THREE.PlaneGeometry(1, 1);

  function canvasTex(c) { return tex(c); }

  function buildPhoto(c, maxW = 1.62) {
    const ar = c.width / c.height;
    const w = ar >= 1 ? maxW : maxW * 0.78 * ar, d = w / ar;
    const top = new THREE.MeshStandardMaterial({ map: canvasTex(c), roughness: 0.48, envMapIntensity: 0.7 });
    const m = new THREE.Mesh(new THREE.BoxGeometry(w, 0.012, d), [M.edge, M.edge, top, M.edge, M.edge, M.edge]);
    m.castShadow = m.receiveShadow = true;
    const object = new THREE.Group();
    object.add(m);
    return { object, w, d, hy: 0.006, apply(p) { const s = 1 - 0.24 * backOut(clamp(p)); m.scale.set(s, 1, s); return s; } };
  }

  function buildPdfs() {
    const pw = 1.0, pd = 1.414, t = 0.0045;
    const object = new THREE.Group();
    const under = paperMat(canvasTex(TX.pdfPage('page', 2)));
    const pageGeo = new THREE.BoxGeometry(pw, t, pd);
    const docs = [
      { seed: 'terms', x: 0.38, z: 0.12, r: -0.22 },
      { seed: 'addendum', x: -0.02, z: -0.08, r: -0.04 },
      { seed: 'lease', x: -0.4, z: 0.1, r: 0.17 },
    ].map((dd, k) => {
      const g = new THREE.Group();
      const top = paperMat(canvasTex(TX.pdfPage(dd.seed, 1)));
      for (let j = 0; j < 3; j++) {
        const mat = j === 2 ? top : under;
        const m = new THREE.Mesh(pageGeo, [M.edge, M.edge, mat, M.edge, M.edge, M.edge]);
        m.position.y = j * t + t / 2;
        m.rotation.y = (j - 1) * 0.018;
        m.castShadow = m.receiveShadow = true;
        g.add(m);
      }
      g.position.set(dd.x, -0.02 + k * 3 * t, dd.z);
      g.rotation.y = dd.r;
      object.add(g);
      return { g, ...dd, k };
    });
    return {
      object, w: 1.95, d: 1.65, hy: 0.02,
      apply(p) {
        const e = smoother(p);
        for (const dd of docs) {
          dd.g.position.x = lerp(dd.x, 0, e);
          dd.g.position.z = lerp(dd.z, 0, e);
          dd.g.position.y = -0.02 + dd.k * 3 * t + Math.sin(Math.PI * clamp(p * 1.15)) * (0.1 + dd.k * 0.06);
          dd.g.rotation.y = lerp(dd.r, (dd.k - 1) * 0.012, e);
        }
        return lerp(1, 0.66, e);
      },
    };
  }

  function buildFilm(c, len = 2.7) {
    const h = len / (c.width / c.height);
    const t = canvasTex(c);
    t.wrapS = THREE.ClampToEdgeWrapping;
    const mat = new THREE.MeshStandardMaterial({ map: t, alphaTest: 0.5, roughness: 0.38, side: THREE.DoubleSide, envMapIntensity: 0.8 });
    const m = new THREE.Mesh(new THREE.PlaneGeometry(len, h), mat);
    m.rotation.x = -Math.PI / 2;
    m.castShadow = m.receiveShadow = true;
    const object = new THREE.Group();
    object.add(m);
    return {
      object, w: len, d: h, hy: 0.003,
      apply(p) {
        const k = 1 - 0.54 * smoother(p);
        m.scale.x = k; t.repeat.x = k; t.offset.x = (1 - k) / 2;
        return k;
      },
    };
  }

  function buildCard(c, w, d, thick, redraw, rough = 0.86) {
    const t = canvasTex(c);
    const top = paperMat(t, rough);
    const m = new THREE.Mesh(new THREE.BoxGeometry(w, thick, d), [M.edge, M.edge, top, M.edge, M.edge, M.edge]);
    m.castShadow = m.receiveShadow = true;
    const object = new THREE.Group();
    object.add(m);
    return {
      object, w, d, hy: thick / 2, mesh: m,
      apply(p) { if (redraw) { redraw(c, p); t.needsUpdate = true; } else { const s = 1 + 0.05 * Math.sin(Math.PI * clamp(p)); m.scale.set(s, 1, s); } return 1; },
    };
  }

  function makeThing(spec, parts) {
    const rootG = new THREE.Group();
    rootG.rotation.order = 'YXZ';
    const lift = new THREE.Group();
    rootG.add(lift);
    lift.add(parts.object);
    const hit = new THREE.Mesh(new THREE.BoxGeometry(parts.w * 1.04, 0.3, parts.d * 1.04), hitMat);
    hit.visible = false;
    hit.position.y = 0.12;
    lift.add(hit);
    scene.add(rootG);
    const blobM = new THREE.Mesh(unit, M.blob.clone());
    blobM.rotation.order = 'YXZ';
    blobM.renderOrder = 1;
    scene.add(blobM);
    const th = {
      ...spec, root: rootG, lift, hit, blob: blobM, w: parts.w, d: parts.d, hy: parts.hy, apply: parts.apply,
      sim: { x: 0, y: -10, z: 0, vx: 0, vy: 0, vz: 0, rx: 0, ry: 0, rz: 0, wx: 0, wy: 0, wz: 0, awake: false, still: 0, contact: false },
      rest: { x: 0, z: 0, yaw: 0 }, hover: 0, hoverT: 0, p: 0, foot: 1, done: false, busy: false, spawned: false, scaleIn: 1,
    };
    hit.userData.thing = th;
    rootG.visible = false;
    blobM.visible = false;
    things.push(th);
    return th;
  }

  const samples = {};
  {
    const photo = buildPhoto(TX.dunePhoto(1024));
    samples.photo = makeThing({ ...SAMPLES[0], sample: true }, photo);
    samples.pdf = makeThing({ ...SAMPLES[1], sample: true }, buildPdfs());
    samples.film = makeThing({ ...SAMPLES[2], sample: true }, buildFilm(TX.filmStrip(7)));
    const wc = TX.waveCard(null, 'interview.m4a', 0);
    samples.wave = makeThing({ ...SAMPLES[3], sample: true }, buildCard(wc, 1.75, 1.75 * wc.height / wc.width, 0.018, (c, p) => TX.waveCard(c, 'interview.m4a', p)));
    const sc = TX.sheet(null, 0);
    samples.sheet = makeThing({ ...SAMPLES[4], sample: true }, buildCard(sc, 1.38, 1.38 * sc.height / sc.width, 0.006, (c, p) => TX.sheet(c, p)));
    const jc = TX.jsonCard(null, 0);
    samples.json = makeThing({ ...SAMPLES[5], sample: true }, buildCard(jc, 0.95, 0.95 * jc.height / jc.width, 0.009, (c, p) => TX.jsonCard(c, p)));
  }

  // ------------------------------------------------------------------ physics: a small settle simulation
  const G = -24;
  function spawn(th, delay = 0, instant = false) {
    const s = th.sim;
    th.spawned = true;
    th.root.visible = !BARE;
    th.blob.visible = !BARE;
    if (instant) {
      Object.assign(s, { x: th.rest.x, y: th.hy, z: th.rest.z, vx: 0, vy: 0, vz: 0, rx: 0, rz: 0, ry: th.rest.yaw, wx: 0, wy: 0, wz: 0, awake: false, contact: true });
      return;
    }
    const r = TX.rng(th.id + performance.now());
    Object.assign(s, {
      x: th.rest.x + (r() - 0.5) * 1.6, z: th.rest.z - 1.2 - r() * 1.2, y: 5.5 + r() * 2,
      vx: 0, vy: -2 - r() * 2, vz: 0,
      rx: (r() - 0.5) * 1.3, rz: (r() - 0.5) * 1.3, ry: th.rest.yaw + (r() - 0.5) * 1.6,
      wx: (r() - 0.5) * 4, wy: (r() - 0.5) * 3, wz: (r() - 0.5) * 4,
      awake: false, still: 0, contact: false, delay,
    });
    th.waiting = delay;
    th.root.visible = delay <= 0;
    th.blob.visible = delay <= 0;
    s.awake = true;
  }
  function stepSim(th, dt) {
    const s = th.sim;
    if (th.waiting > 0) { th.waiting -= dt; if (th.waiting <= 0) { th.root.visible = th.blob.visible = true; } return true; }
    if (!s.awake) return false;
    const sub = Math.max(1, Math.ceil(dt / (1 / 120)));
    const h = dt / sub;
    for (let i = 0; i < sub; i++) {
      s.vx += (7 * (th.rest.x - s.x) - 4.2 * s.vx) * h;
      s.vz += (7 * (th.rest.z - s.z) - 4.2 * s.vz) * h;
      s.vy += G * h;
      s.x += s.vx * h; s.y += s.vy * h; s.z += s.vz * h;
      s.rx += s.wx * h; s.rz += s.wz * h; s.ry += s.wy * h;
      s.wy += (6 * (th.rest.yaw - s.ry) - 3.5 * s.wy) * h;
      const hx = th.w / 2, hz = th.d / 2;
      const ext = th.hy * Math.cos(s.rx) * Math.cos(s.rz) + hz * Math.abs(Math.sin(s.rx)) + hx * Math.abs(Math.sin(s.rz));
      if (s.y - ext <= 0) {
        s.y = ext;
        if (s.vy < 0) s.vy = s.vy < -2.2 ? -s.vy * 0.26 : 0;
        s.contact = true;
        s.wx += (-42 * s.rx - 9 * s.wx) * h;
        s.wz += (-42 * s.rz - 9 * s.wz) * h;
      } else {
        s.wx *= Math.exp(-0.6 * h); s.wz *= Math.exp(-0.6 * h);
      }
    }
    const calm = s.contact && Math.abs(s.vy) < 0.05 && Math.abs(s.rx) + Math.abs(s.rz) < 0.004 && Math.hypot(s.vx, s.vz) < 0.02 && Math.abs(th.rest.yaw - s.ry) < 0.004;
    s.still = calm ? s.still + dt : 0;
    if (s.still > 0.25) { Object.assign(s, { x: th.rest.x, z: th.rest.z, y: th.hy, rx: 0, rz: 0, ry: th.rest.yaw, vx: 0, vy: 0, vz: 0, wx: 0, wy: 0, wz: 0, awake: false }); }
    return true;
  }
  function placeThing(th) {
    const s = th.sim;
    th.root.position.set(s.x, s.y, s.z);
    th.root.rotation.set(s.rx, s.ry, s.rz);
    th.root.scale.setScalar(th.scaleIn);
    const e = smooth(th.hover);
    th.lift.position.y = e * 0.3;
    th.lift.rotation.x = -e * 0.09;
    const height = Math.max(0, s.y - th.hy) + e * 0.3;
    const spread = 1 + height * 0.35;
    th.blob.position.set(s.x, 0.003, s.z);
    th.blob.rotation.set(-Math.PI / 2, s.ry, 0);
    th.blob.scale.set((th.w * 1.62 + 0.18) * spread * th.foot * th.scaleIn, (th.d * 1.62 + 0.18) * spread * th.foot * th.scaleIn, 1);
    th.blob.material.opacity = blobBase * clamp(1 - height * 0.32) * th.scaleIn;
  }

  // ------------------------------------------------------------------ layout: box, things, keyframes
  const keyframes = [];
  let curvePos, curveTgt;
  function boxWorld(v) { return box.localToWorld(v.clone()); }
  function applyLayout() {
    L = LAYOUTS[layoutName];
    camera.fov = L.fov;
    box.position.set(L.box[0], 0, L.box[1]);
    box.rotation.y = L.box[2];
    box.updateMatrixWorld(true);
    boxBlob.position.set(L.box[0], 0.002, L.box[1]);
    boxBlob.rotation.set(-Math.PI / 2, L.box[2], 0);
    boxBlob.scale.set(W * 1.55, D * 1.75, 1);
    for (const th of things) {
      const spot = th.sample ? L.things[th.id] : th.spot;
      th.enabled = !!spot;
      if (!spot) { th.root.visible = th.blob.visible = false; continue; }
      th.rest = { x: spot[0], z: spot[1], yaw: spot[2] || 0 };
      if (th.spawned && !th.sim.awake) { spawn(th, 0, true); placeThing(th); }
    }
    // keyframes: hero, box front, 12 drawers, out
    keyframes.length = 0;
    const fwd = new THREE.Vector3(Math.sin(L.box[2]), 0, Math.cos(L.box[2]));
    const right = new THREE.Vector3(fwd.z, 0, -fwd.x);
    const up = new THREE.Vector3(0, 1, 0);
    const P = (a) => new THREE.Vector3(...a);
    const phone = layoutName === 'phone';
    // keep the whole bench in frame on narrower screens: back the hero camera off as the aspect drops
    const fitK = phone ? clamp(0.5 / camera.aspect, 0.85, 1.25) : clamp(1.6 / camera.aspect, 0.92, 1.45);
    const heroTgt = P(L.hero.target);
    const heroPos = heroTgt.clone().add(P(L.hero.pos).sub(heroTgt).multiplyScalar(fitK));
    keyframes.push({ pos: heroPos, tgt: heroTgt, shift: L.hero.shift, focus: L.focusR, blur: L.blur });
    const front = boxWorld(new THREE.Vector3(0, Hb * 0.62, D / 2));
    keyframes.push({
      pos: front.clone().addScaledVector(fwd, (phone ? 10.5 : 11.2) * fitK).addScaledVector(up, phone ? 3.6 : 3.4).addScaledVector(right, phone ? 0 : -1.2),
      tgt: front.clone(), shift: phone ? [0, 0.22] : [0.3, 0.02], focus: 0.4, blur: L.blur * 0.8,
    });
    drawers.forEach((d) => {
      const tgt = boxWorld(new THREE.Vector3(d.x, d.y - dh * 0.25, D / 2 + DR.pull - 0.62));
      const pos = tgt.clone().addScaledVector(fwd, phone ? 4.0 : 2.9).addScaledVector(up, phone ? 3.6 : 2.15).addScaledVector(right, (phone ? 0 : 0.35) - d.x * 0.08);
      keyframes.push({ pos, tgt, shift: L.drawerShift, focus: phone ? 0.3 : 0.26, blur: L.blur * 1.25 });
    });
    const top = boxWorld(new THREE.Vector3(0, Hb, 0.6));
    keyframes.push({ pos: top.clone().addScaledVector(fwd, phone ? 9 : 7.5).addScaledVector(up, phone ? 14 : 11), tgt: top, shift: [0, phone ? 0.26 : 0.24], focus: 0.6, blur: L.blur * 0.5 });
    curvePos = new THREE.CatmullRomCurve3(keyframes.map((k) => k.pos), false, 'centripetal');
    curveTgt = new THREE.CatmullRomCurve3(keyframes.map((k) => k.tgt), false, 'centripetal');
    dirty = true;
  }

  // ------------------------------------------------------------------ post pipeline
  const rt = new THREE.WebGLRenderTarget(4, 4, { type: THREE.HalfFloatType, samples: 4 });
  const postMat = new THREE.ShaderMaterial({
    vertexShader: POST_VS, fragmentShader: POST_FS,
    uniforms: { tColor: { value: rt.texture }, uRes: { value: new THREE.Vector2(1, 1) }, uFocus: { value: new THREE.Vector2(0.5, 0.5) }, uRadius: { value: 0.4 }, uBlur: { value: 4 }, uVig: { value: 0.1 }, uAspect: { value: 1 } },
    depthTest: false, depthWrite: false,
  });
  const postScene = new THREE.Scene();
  const postCam = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), postMat);
  quad.frustumCulled = false;
  postScene.add(quad);

  // ------------------------------------------------------------------ theme
  let dark = false, blobBase = 0.42;
  const BG = { light: '#f7f6f3', dark: '#151412' };
  function isDark() {
    const t = html.getAttribute('data-theme');
    return t ? t === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
  }
  function applyTheme() {
    dark = isDark();
    const bg = untone(dark ? BG.dark : BG.light);
    scene.fog.color.copy(bg);
    renderer.setClearColor(bg, 1);
    if (dark && !woodDark) woodDark = tex(TX.benchWood(true), { repeat: 15 });
    M.bench.map = dark ? woodDark : woodLight;
    M.bench.needsUpdate = true;
    const paper = new THREE.Color(dark ? 0xc9c4ba : 0xffffff);
    for (const m of paperMats) m.color.copy(paper);
    M.trayWood.color.set(dark ? 0xa59d90 : 0xe4dccd);
    M.lacquer.color.set(dark ? 0x0f0e0d : 0x161513);
    M.front.color.set(dark ? 0x141311 : 0x1b1a17);
    key.intensity = dark ? 1.9 : 2.6;
    key.color.set(dark ? 0xffecd8 : 0xfff5ea);
    fill.intensity = dark ? 0.25 : 0.45;
    rimL.intensity = dark ? 1.5 : 0.9;
    hemi.intensity = dark ? 0.16 : 0.35;
    hemi.color.set(dark ? 0xd8d0c4 : 0xffffff);
    hemi.groundColor.set(dark ? 0x2a2622 : 0xd8cfc0);
    scene.environmentIntensity = dark ? 0.32 : 0.62;
    blobBase = dark ? 0.85 : 0.6;
    boxBlob.material.opacity = dark ? 0.8 : 0.62;
    for (const th of things) placeThing(th);
    postMat.uniforms.uVig.value = dark ? 0.16 : 0.08;
    dirty = true;
    wake();
  }

  // ------------------------------------------------------------------ DOM: overlays
  const layer = document.getElementById('float-layer');
  const flabel = document.createElement('div');
  flabel.className = 'flabel';
  layer.append(flabel);
  const live = document.getElementById('bench-live');
  const chipsEl = document.getElementById('bench-chips');
  const hero = document.getElementById('top');
  const inside = document.getElementById('inside');
  const panels = [...document.querySelectorAll('.dpanel')];
  const railBtns = [...document.querySelectorAll('.drawer-rail button')];
  const intro = document.querySelector('.inside-intro');
  const outro = document.querySelector('.inside-outro');
  const rail = document.querySelector('.drawer-rail');

  function chipFor(th) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'chip';
    b.innerHTML = `<span class="chip-k" aria-hidden="true">${esc(KIND_WORD[th.kind === 'pdfs' ? 'pdf' : th.kind] || 'File')}</span>${esc(th.name)}`;
    b.setAttribute('aria-label', `${th.name}, ${th.meta}. Suggested: ${th.suggest.map((s) => s[1]).join(', ')}. Press to run ${runName(th)}.`);
    b.addEventListener('focus', () => select(th, 'focus'));
    b.addEventListener('blur', () => { if (selected === th && selectedBy === 'focus') select(null); });
    b.addEventListener('mouseenter', () => select(th, 'chip'));
    b.addEventListener('mouseleave', () => { if (selectedBy === 'chip') select(null); });
    b.addEventListener('click', () => run(th));
    th.chip = b;
    chipsEl.append(b);
    if (!th.sample) chipsEl.scrollTo({ left: chipsEl.scrollWidth, behavior: calmScroll });
    fadeChips();
  }
  // a soft fade on the right edge while more chips sit out of view
  const calmScroll = matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth';
  function fadeChips() {
    chipsEl.classList.toggle('more-r', chipsEl.scrollWidth - chipsEl.clientWidth - chipsEl.scrollLeft > 4);
    chipsEl.classList.toggle('more-l', chipsEl.scrollLeft > 4);
  }
  chipsEl.addEventListener('scroll', fadeChips, { passive: true });
  const runName = (th) => th.action ? th.action.name : th.suggest[0][1];

  let selected = null, selectedBy = null, suppress = null;
  function select(th, by = 'pointer') {
    if (th && !th.enabled) th = null;
    if (selected === th) { if (th) selectedBy = by; return; }
    if (selected) selected.hoverT = 0;
    selected = th;
    selectedBy = th ? by : null;
    if (th) {
      th.hoverT = 1;
      renderLabel(th);
      flabel.classList.add('is-on');
    } else flabel.classList.remove('is-on');
    for (const t of things) t.chip && t.chip.classList.toggle('is-on', t === th);
    hero.classList.toggle('is-pointing', !!th && by === 'pointer');
    wake();
  }
  function renderLabel(th) {
    const verb = touch ? 'Tap again' : 'Click';
    let foot;
    if (th.busy) foot = `Running ${runName(th)}…`;
    else if (th.done) foot = `${verb} to put it back`;
    else if (th.sample) foot = `${verb} to run ${runName(th)}`;
    else foot = `${verb} to run ${runName(th)} here. Nothing is uploaded.`;
    const res = th.done && !th.busy && th.lastResult ? `<p class="fl-res"><span class="mono">${esc(th.lastResult.tool)}</span>${esc(th.lastResult.text)}</p>` : '';
    flabel.innerHTML = `<p class="fl-name">${esc(th.name)}</p><p class="fl-meta">${esc(th.meta)}</p>`
      + `<p class="fl-q">What should happen to it?</p><ol>${th.suggest.map((s, i) => `<li${i === 0 ? ' class="is-first"' : ''}><span class="n">${i + 1}</span><span class="t">${esc(s[1])}</span></li>`).join('')}</ol>`
      + res + `<p class="fl-foot">${esc(foot)}</p>`;
  }
  function showTag(th, text, tool) {
    if (th.tag) th.tag.remove();
    const el = document.createElement('div');
    el.className = 'ftag';
    el.innerHTML = `<div class="ftag-in"><span class="ftag-tool">${esc(tool)}</span><span class="ftag-v">${esc(text)}</span></div>`;
    layer.append(el);
    th.tag = el;
    placeTag(el, project(th, pt));
    requestAnimationFrame(() => requestAnimationFrame(() => el.classList.add('is-on')));
    dirty = true;
    wake();
    live.textContent = `${tool}: ${text}`;
    clearTimeout(th.tagTimer);
    th.tagTimer = setTimeout(() => hideTag(th), 6500);
  }
  function hideTag(th) {
    const el = th.tag;
    if (!el) return;
    th.tag = null;
    el.classList.remove('is-on');
    el.classList.add('is-off');
    setTimeout(() => el.remove(), 500);
  }

  // ------------------------------------------------------------------ tweens
  const tweens = [];
  function tween(dur, fn, done) {
    const t = { t: 0, dur, fn, done };
    tweens.push(t);
    wake();
    return t;
  }

  // ------------------------------------------------------------------ running tools
  async function run(th, fromDemo = false) {
    if (!th || !th.enabled || th.busy) return;
    if (!fromDemo) cancelDemo(false);
    if (th.done) {
      th.busy = true;
      hideTag(th);
      tween(0.7, (u) => { th.p = 1 - easeInOut(u); th.foot = th.apply(th.p) || 1; }, () => { th.busy = false; th.done = false; if (selected === th) renderLabel(th); });
      live.textContent = `${th.name} is back as it was.`;
      return;
    }
    th.busy = true;
    if (selected === th) renderLabel(th);
    let result = th.result, tool = runName(th);
    const anim = new Promise((res) => tween(th.sample ? 1.25 : 0.9, (u) => { th.p = u; th.foot = th.apply(u) || 1; }, res));
    if (!th.sample && th.action) {
      try { result = await th.action.run(); } catch (e) { result = 'Could not run it in this tab'; }
    }
    await anim;
    th.busy = false;
    th.done = true;
    const toolNote = th.sample ? `${tool} · example` : `${tool} · ran in this tab`;
    th.lastResult = { tool: toolNote, text: result };
    showTag(th, result, toolNote);
    // step the label aside so the result can float up; it comes back on the next hover
    if (selected === th && selectedBy !== 'focus') { suppress = th; select(null); } else if (selected === th) renderLabel(th);
    // while the label stays up (keyboard, chips) it carries the result itself
    if (th.tag) th.tag.classList.toggle('is-hidden', selected === th);
  }

  // A short demo once things settle: the photo lifts, compresses, and puts itself back.
  let demoTimers = [], demoRan = false;
  function cancelDemo(revert = true) {
    demoTimers.forEach(clearTimeout);
    demoTimers = [];
    const th = samples.photo;
    if (revert && demoRan && th.done && !th.busy) run(th, true);
    demoRan = false;
  }
  function demo() {
    const th = samples.photo;
    if (!th.enabled || STILL) return;
    const at = (ms, f) => demoTimers.push(setTimeout(f, ms));
    at(0, () => { if (!selected && tCur < 0.15) select(th, 'demo'); });
    at(1400, () => { if (selected === th && selectedBy === 'demo') { demoRan = true; run(th, true); } });
    at(6200, () => { if (selected === th && selectedBy === 'demo') select(null); });
    at(7000, () => { if (demoRan && th.done && !th.busy) run(th, true); demoRan = false; });
  }

  // ------------------------------------------------------------------ pointer
  const ray = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  let px = 0, py = 0, pxS = 0, pyS = 0;
  const interactive = (el) => el && el.closest && el.closest('a, button, input, label, .hero-copy, .bench-bar, .site-header');
  function pick(cx, cy) {
    if (tCur > 0.3) return null;
    const r = canvas.getBoundingClientRect();
    ndc.set(((cx - r.left) / r.width) * 2 - 1, -((cy - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ndc, camera);
    const hits = ray.intersectObjects(things.filter((t) => t.enabled && t.root.visible).map((t) => t.hit), false);
    return hits.length ? hits[0].object.userData.thing : null;
  }
  window.addEventListener('pointermove', (e) => {
    if (e.pointerType === 'touch') return;
    px = (e.clientX / innerWidth) * 2 - 1;
    py = (e.clientY / innerHeight) * 2 - 1;
    if (tCur < 0.3) {
      let over = interactive(e.target) ? null : pick(e.clientX, e.clientY);
      if (suppress) { if (over === suppress) over = null; else suppress = null; }
      if (over) { if (over !== selected || selectedBy !== 'pointer') { if (selectedBy === 'demo' && over !== selected) cancelDemo(); select(over, 'pointer'); } } else if (selectedBy === 'pointer') select(null);
    }
    wake();
  }, { passive: true });
  hero.addEventListener('click', (e) => {
    if (interactive(e.target)) return;
    const th = pick(e.clientX, e.clientY);
    if (!th) { if (touch) select(null); return; }
    if (touch && selected !== th) { cancelDemo(); select(th, 'tap'); return; }
    run(th);
  });

  // ------------------------------------------------------------------ the visitor's own files
  const own = [];
  async function addFiles(list) {
    const files = [...list].slice(0, 3);
    if (!files.length) return;
    if (tCur > 0.3) { window.scrollTo({ top: 0, behavior: 'smooth' }); await wait(700); }
    for (const f of files) await addFile(f);
  }
  async function addFile(file) {
    const kind = kindOf(file.type, file.name);
    let parts = null, action = null, suggest = SUGGEST[kind] || SUGGEST.file;
    const url = URL.createObjectURL(file);
    const small = file.size < 40 * 1024 * 1024;
    try {
      if (kind === 'image' && small) {
        const img = await loadImage(url);
        parts = buildPhoto(TX.photoFrom(img), 1.5);
        action = { name: 'Compress Images', run: () => compressImage(img, file) };
      } else if (kind === 'video') {
        const still = await videoStill(url).catch(() => null);
        parts = buildFilm(TX.filmStrip(5, still), 2.1);
      } else if (kind === 'audio') {
        const b = small ? await audioBars(file).catch(() => null) : null;
        const c = TX.waveCard(null, file.name, 0, null, b);
        parts = buildCard(c, 1.6, 1.6 * c.height / c.width, 0.018, null);
      } else if (kind === 'pdf') {
        const c = TX.pdfNamed(file.name);
        parts = buildCard(c, 1.0, 1.414, 0.012, null);
      } else if (kind === 'sheet' && /\.(csv|tsv)$/i.test(file.name)) {
        const text = await file.slice(0, 64e3).text();
        const { head, rows } = parseCsv(text, /\.tsv$/i.test(file.name) ? '\t' : null);
        const c = TX.sheet(null, 0, rows.slice(0, 10), head.slice(0, 5), new Set());
        parts = buildCard(c, 1.38, 1.38 * c.height / c.width, 0.006, null);
        suggest = SUGGEST.sheetOwn;
        action = { name: 'Column Statistics', run: () => csvStats(file) };
      } else if (kind === 'json' || kind === 'text') {
        const text = await file.slice(0, 16e3).text();
        const c = TX.textCard(file.name, text);
        const redraw = (cv, p) => { if (p > 0.5 && th.formatted) { const n = TX.textCard(file.name, th.formatted); cv.getContext('2d').drawImage(n, 0, 0); } else if (p <= 0.5) { cv.getContext('2d').drawImage(TX.textCard(file.name, text), 0, 0); } };
        parts = buildCard(c, 0.95, 1.25, 0.009, kind === 'json' ? redraw : null);
        action = kind === 'json' ? { name: 'JSON Formatter & Validator', run: () => formatJson(file, th) } : { name: 'Word & Character Count', run: () => countText(file) };
      }
    } catch { parts = null; }
    if (!parts) {
      parts = buildCard(TX.extCard(file.name), 0.86, 1.075, 0.01, null);
      if (kind === 'image') suggest = SUGGEST.image;
    }
    if (!action) action = { name: 'File Hash', run: () => hashFile(file) };
    const spots = L.spots;
    const used = new Set(own.map((t) => t.spotIndex));
    let si = spots.map((_, i) => i).find((i) => !used.has(i));
    if (si === undefined) { const old = own.shift(); si = old.spotIndex; removeThing(old); }
    const th = makeThing({
      id: `own-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`, kind, name: file.name,
      meta: `${KIND_WORD[kind] || 'File'} · ${fmtBytes(file.size)} · yours, in this tab`, suggest, action, sample: false, url,
    }, parts);
    th.spotIndex = si;
    th.spot = [spots[si][0], spots[si][1], (Math.random() - 0.5) * 0.4];
    th.enabled = true;
    th.rest = { x: th.spot[0], z: th.spot[1], yaw: th.spot[2] };
    own.push(th);
    spawn(th, 0);
    chipFor(th);
    live.textContent = `${file.name} is on the bench. Suggested: ${suggest.map((s) => s[1]).join(', ')}.`;
    setTimeout(() => { if (!selected || selectedBy !== 'pointer') select(th, 'demo'); }, 900);
    wake();
  }
  function removeThing(th) {
    const i = things.indexOf(th);
    if (i >= 0) things.splice(i, 1);
    if (selected === th) select(null);
    hideTag(th);
    th.chip && th.chip.remove();
    fadeChips();
    scene.remove(th.root, th.blob);
    th.root.traverse((o) => { if (o.isMesh) { o.geometry.dispose(); const ms = Array.isArray(o.material) ? o.material : [o.material]; ms.forEach((m) => { if (m !== M.edge && m !== hitMat) { m.map && m.map.dispose(); m.dispose(); } }); } });
    URL.revokeObjectURL(th.url);
  }
  window.addEventListener('pagehide', () => own.forEach((t) => URL.revokeObjectURL(t.url)));

  const veil = document.getElementById('drop-veil');
  let dragDepth = 0;
  const hasFiles = (e) => e.dataTransfer && [...(e.dataTransfer.types || [])].includes('Files');
  window.addEventListener('dragenter', (e) => { if (!hasFiles(e)) return; e.preventDefault(); dragDepth++; veil.classList.add('is-on'); });
  window.addEventListener('dragover', (e) => { if (!hasFiles(e)) return; e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; });
  window.addEventListener('dragleave', () => { dragDepth = Math.max(0, dragDepth - 1); if (!dragDepth) veil.classList.remove('is-on'); });
  window.addEventListener('drop', (e) => { if (!hasFiles(e)) return; e.preventDefault(); dragDepth = 0; veil.classList.remove('is-on'); addFiles(e.dataTransfer.files); });
  const input = document.getElementById('own-file');
  if (input) input.addEventListener('change', () => { addFiles(input.files); input.value = ''; });

  // ------------------------------------------------------------------ scroll choreography
  let anchors = [[0, 0]], holdY = [], endY = 1, tTarget = 0, tCur = 0;
  const tForce = STILL ? parseFloat(params.get('t') || '0') : null;
  const MOVE = 0.24, HOLD = 0.3;
  function layoutScroll() {
    const vh = innerHeight;
    const heroH = hero.offsetHeight;
    const insideTop = inside.getBoundingClientRect().top + scrollY;
    anchors = [[0, 0], [heroH * 0.06, 0], [insideTop + 0.32 * vh, 1], [insideTop + 0.8 * vh, 1]];
    let y = insideTop + 0.8 * vh;
    holdY = [];
    for (let i = 0; i < 12; i++) {
      y += MOVE * vh; anchors.push([y, 2 + i]);
      holdY.push(y + HOLD * vh * 0.4);
      y += HOLD * vh; anchors.push([y, 2 + i]);
    }
    y += 0.5 * vh; anchors.push([y, 14]);
    endY = y;
    inside.style.height = `${Math.round(y - insideTop + vh)}px`;
    // the page above may have shifted: recompute once more if the section moved
    const again = inside.getBoundingClientRect().top + scrollY;
    if (Math.abs(again - insideTop) > 1) return layoutScroll();
  }
  function tFromScroll(y) {
    const A = anchors;
    if (y <= A[0][0]) return A[0][1];
    for (let i = 1; i < A.length; i++) {
      if (y <= A[i][0]) {
        const [y0, t0] = A[i - 1], [y1, t1] = A[i];
        return t0 === t1 ? t0 : lerp(t0, t1, easeInOut(clamp((y - y0) / (y1 - y0 || 1))));
      }
    }
    return A[A.length - 1][1];
  }
  railBtns.forEach((b, i) => b.addEventListener('click', () => window.scrollTo({ top: holdY[i], behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' })));
  window.addEventListener('scroll', () => { wake(); }, { passive: true });

  // ------------------------------------------------------------------ resize
  function resize() {
    const w = canvas.clientWidth || innerWidth, h = canvas.clientHeight || innerHeight;
    const next = pickLayout();
    if (next !== layoutName) { layoutName = next; dprCap = STILL ? 2 : layoutName === 'phone' ? 1.6 : 1.75; }
    dpr = Math.min(window.devicePixelRatio || 1, dprCap, qualityCap);
    renderer.setPixelRatio(dpr);
    renderer.setSize(w, h, false);
    rt.setSize(Math.round(w * dpr), Math.round(h * dpr));
    camera.aspect = w / h;
    postMat.uniforms.uRes.value.set(w * dpr, h * dpr);
    postMat.uniforms.uAspect.value = w / h;
    applyLayout();
    layoutScroll();
    dirty = true;
    wake();
  }
  function pickLayout() { return innerWidth < 760 || (innerWidth < 1000 && innerHeight > innerWidth * 1.2) ? 'phone' : 'desk'; }

  // ------------------------------------------------------------------ frame
  const camPos = new THREE.Vector3(), camTgt = new THREE.Vector3(), lastPos = new THREE.Vector3(), lastTgt = new THREE.Vector3();
  const tmpV = new THREE.Vector3(), anchorV = new THREE.Vector3();
  let raf = 0, last = 0, running = true, dirty = true, quiet = 0, qualityCap = 3, slowFrames = 0, sampleFrames = 0;
  let lidIntro = STILL ? 1 : 0, lidAngleLast = 99, introStart = 0, lidClock = 0, canvasOpacity = 1;
  const panelState = panels.map(() => -1);
  let introState = -1, outroState = -1, railActive = -2;

  function update(dt) {
    let busy = false;
    // scroll -> t
    tTarget = tForce !== null ? tForce : tFromScroll(scrollY);
    const jump = Math.abs(tTarget - tCur);
    const before = tCur;
    tCur = jump < 0.0005 ? tTarget : lerp(tCur, tTarget, 1 - Math.exp(-dt * (jump > 2 ? 4 : 7)));
    if (tCur !== before) busy = true;

    // camera pose from keyframes
    const n = keyframes.length - 1;
    const s = clamp(tCur, 0, n);
    curvePos.getPoint(s / n, camPos);
    curveTgt.getPoint(s / n, camTgt);
    const i0 = Math.min(n - 1, Math.floor(s)), f = s - i0;
    const k0 = keyframes[i0], k1 = keyframes[i0 + 1];
    const fe = smooth(f);
    let sx = lerp(k0.shift[0], k1.shift[0], fe), sy = lerp(k0.shift[1], k1.shift[1], fe);
    if (STILL && params.has('sx')) { sx = +params.get('sx'); sy = +params.get('sy') || 0; }
    // pointer parallax (desktop): a gentle sway, strongest on the hero
    const pw = layoutName === 'phone' ? 0 : (tCur < 1 ? 1 - tCur * 0.6 : 0.35);
    pxS = lerp(pxS, px, 1 - Math.exp(-dt * 3));
    pyS = lerp(pyS, py, 1 - Math.exp(-dt * 3));
    if (Math.abs(pxS - px) > 0.0005 || Math.abs(pyS - py) > 0.0005) busy = true;
    tmpV.subVectors(camPos, camTgt).normalize();
    const right = new THREE.Vector3().crossVectors(new THREE.Vector3(0, 1, 0), tmpV).normalize();
    camPos.addScaledVector(right, pxS * 0.55 * pw).y += -pyS * 0.35 * pw;
    camera.position.copy(camPos);
    camera.lookAt(camTgt);
    const camDist = camPos.distanceTo(camTgt);
    scene.fog.near = camDist + 2.5;
    scene.fog.far = camDist + 15;
    camera.updateProjectionMatrix();
    camera.projectionMatrix.elements[8] -= sx;
    camera.projectionMatrix.elements[9] -= sy;
    camera.projectionMatrixInverse.copy(camera.projectionMatrix).invert();
    if (camPos.distanceToSquared(lastPos) > 1e-9 || camTgt.distanceToSquared(lastTgt) > 1e-9) { busy = true; lastPos.copy(camPos); lastTgt.copy(camTgt); }
    postMat.uniforms.uFocus.value.set(0.5 + sx / 2, 0.5 + sy / 2);
    postMat.uniforms.uRadius.value = lerp(k0.focus, k1.focus, fe);
    postMat.uniforms.uBlur.value = lerp(k0.blur, k1.blur, fe) * dpr;

    // lid: opens on arrival, closes as we go in
    if (lidIntro < 1 && introStart) { lidClock += dt; lidIntro = clamp(lidClock / 1.3); busy = true; }
    const lidOpen = backOut(lidIntro) * (1 - smoother((tCur - 0.08) / 0.85)) + smoother((tCur - 13.2) / 0.8) * 0.0;
    const ang = LID_OPEN * lidOpen;
    if (Math.abs(ang - lidAngleLast) > 1e-5) { lidPivot.rotation.x = ang; lidAngleLast = ang; busy = true; }

    // drawers
    drawers.forEach((d, i) => {
      const o = smoother(1 - Math.abs(tCur - (2 + i)) / 0.78);
      if (o !== d.open) {
        d.open = o;
        d.g.position.z = D / 2 + o * DR.pull;
        d.inner.visible = o > 0.001;
        if (o > 0) paintFloor(d);
        busy = true;
      }
    });

    // things
    for (const th of things) {
      if (!th.enabled) continue;
      if (stepSim(th, dt)) busy = true;
      const target = th.hoverT && tCur < 0.3 ? 1 : 0;
      if (Math.abs(th.hover - target) > 0.001) { th.hover = lerp(th.hover, target, 1 - Math.exp(-dt * 10)); busy = true; } else th.hover = target;
      placeThing(th);
    }

    // tweens
    for (let i = tweens.length - 1; i >= 0; i--) {
      const tw = tweens[i];
      tw.t = Math.min(1, tw.t + dt / tw.dur);
      tw.fn(tw.t);
      if (tw.t >= 1) { tweens.splice(i, 1); tw.done && tw.done(); }
      busy = true;
    }

    // leaving the hero drops any hover label
    if (tCur > 0.3 && selected && selectedBy !== 'focus') select(null);

    // overlays
    updateOverlays();

    // canvas fades as the page continues below the drawers
    const vh = innerHeight;
    const op = 1 - smooth((scrollY - (endY - 0.15 * vh)) / (0.75 * vh));
    if (Math.abs(op - canvasOpacity) > 0.002 || (op === 0) !== (canvasOpacity === 0)) { canvasOpacity = op; canvas.style.opacity = op.toFixed(3); canvas.style.visibility = op === 0 ? 'hidden' : ''; }
    return busy;
  }

  function project(th, out) {
    anchorV.set(0, 0.28 + th.hover * 0.3, 0);
    th.root.localToWorld(anchorV);
    anchorV.project(camera);
    out.x = (anchorV.x * 0.5 + 0.5) * innerWidth;
    out.y = (-anchorV.y * 0.5 + 0.5) * canvas.clientHeight;
    return out;
  }
  const pt = { x: 0, y: 0 };
  function placeTag(el, p) {
    const hw = (el.firstElementChild ? el.firstElementChild.offsetWidth : 200) / 2 + 12;
    el.style.transform = `translate3d(${clamp(p.x, hw, innerWidth - hw)}px, ${Math.max(p.y, 96)}px, 0)`;
  }
  function updateOverlays() {
    const heroOn = tCur < 0.3;
    if (selected && heroOn) {
      project(selected, pt);
      const w = flabel.offsetWidth || 240, h = flabel.offsetHeight || 160;
      const x = clamp(pt.x, w / 2 + 12, innerWidth - w / 2 - 12);
      const y = clamp(pt.y - 18, h + 70, innerHeight);
      flabel.style.transform = `translate3d(${x - w / 2}px, ${y - h}px, 0)`;
    }
    flabel.classList.toggle('is-hidden', !heroOn);
    for (const th of things) {
      if (!th.tag) continue;
      placeTag(th.tag, project(th, pt));
      th.tag.classList.toggle('is-hidden', !heroOn || (selected === th && !th.busy));
    }
    // drawer tour text
    const io = smooth(1 - Math.abs(tCur - 1) / 0.6);
    if (Math.abs(io - introState) > 0.001) { introState = io; intro.style.opacity = io.toFixed(3); intro.style.transform = `translate3d(0, ${((1 - tCur) * 40).toFixed(1)}px, 0)`; intro.classList.toggle('is-on', io > 0.5); }
    const oo = smooth(1 - Math.abs(tCur - 14) / 0.6);
    if (Math.abs(oo - outroState) > 0.001) { outroState = oo; outro.style.opacity = oo.toFixed(3); outro.classList.toggle('is-on', oo > 0.5); }
    panels.forEach((el, i) => {
      const dd = tCur - (2 + i);
      const o = smooth(1 - Math.abs(dd) / 0.46);
      if (Math.abs(o - panelState[i]) < 0.001) return;
      panelState[i] = o;
      el.style.opacity = o.toFixed(3);
      el.style.transform = `translate3d(0, ${(-dd * 46).toFixed(1)}px, 0)`;
      el.classList.toggle('is-on', o > 0.5);
    });
    const ra = tCur > 1.5 && tCur < 13.6 ? Math.round(tCur - 2) : -1;
    if (ra !== railActive) {
      railActive = ra;
      railBtns.forEach((b, i) => b.toggleAttribute('aria-current', i === ra));
      rail.classList.toggle('is-on', tCur > 1.45 && tCur < 13.7);
    }
    const railOn = tCur > 1.45 && tCur < 13.7;
    rail.classList.toggle('is-on', railOn);
    hero.classList.toggle('is-away', tCur > 0.3);
  }

  function render() {
    renderer.setRenderTarget(rt);
    renderer.render(scene, camera);
    renderer.setRenderTarget(null);
    renderer.render(postScene, postCam);
  }

  function frame(now) {
    raf = 0;
    if (!running) return;
    const dt = Math.min(0.05, last ? (now - last) / 1000 : 1 / 60);
    last = now;
    const busy = update(dt);
    if ((busy || dirty) && canvasOpacity > 0) {
      const t0 = performance.now();
      render();
      dirty = false;
      quiet = 0;
      // adaptive resolution: if frames keep running long, step the pixel ratio down
      if (busy) {
        sampleFrames++;
        if (dt > 0.026) slowFrames++;
        if (sampleFrames >= 50) {
          if (slowFrames > 30 && dpr > 1) { qualityCap = Math.max(1, dpr - 0.25); resize(); }
          sampleFrames = slowFrames = 0;
        }
      }
      void t0;
    } else quiet++;
    if (quiet < 20) raf = requestAnimationFrame(frame);
    else last = 0;
  }
  function wake() {
    if (!running || raf || MANUAL) return;
    raf = requestAnimationFrame(frame);
  }
  document.addEventListener('visibilitychange', () => {
    running = !document.hidden;
    if (running) { last = 0; wake(); } else if (raf) { cancelAnimationFrame(raf); raf = 0; }
  });

  // ------------------------------------------------------------------ go
  new ResizeObserver(() => resize()).observe(canvas);
  window.addEventListener('resize', () => { layoutScroll(); wake(); });
  document.addEventListener('dgb:theme', applyTheme);
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  resize();
  applyTheme();
  SAMPLES.forEach((sm) => chipFor(samples[sm.id]));
  for (const th of things) if (!th.enabled && th.chip) th.chip.hidden = true;
  const syncChips = () => { things.forEach((th) => { if (th.chip) th.chip.hidden = !th.enabled; }); fadeChips(); };
  syncChips();
  new ResizeObserver(syncChips).observe(document.body);

  html.classList.add('scene-ready');
  const enabled = things.filter((t) => t.enabled);
  if (STILL) {
    enabled.forEach((th) => { spawn(th, 0, true); placeThing(th); });
    lidIntro = 1;
    tCur = tTarget = tForce;
    dirty = true;
    update(0.016);
    render();
    window.__stillReady = true;
  } else {
    if (!MANUAL) await wait(250);
    introStart = 1;
    enabled.forEach((th, i) => spawn(th, 0.45 + i * 0.17));
    wake();
    if (!MANUAL) setTimeout(demo, 3200);
  }

  window.__bench = {
    settle() { cancelDemo(); things.forEach((th) => { if (th.enabled) { spawn(th, 0, true); placeThing(th); } }); lidIntro = 1; dirty = true; wake(); },
    select(id) { select(samples[id] || null, 'focus'); },
    run(id) { return run(samples[id]); },
    get t() { return tCur; },
    renderer,
    ground(sx, sy) { ndc.set((sx / innerWidth) * 2 - 1, -(sy / canvas.clientHeight) * 2 + 1); ray.setFromCamera(ndc, camera); const t = -ray.ray.origin.y / ray.ray.direction.y; const p = ray.ray.origin.clone().addScaledVector(ray.ray.direction, t); return [+p.x.toFixed(2), +p.z.toFixed(2)]; },
    screen(x, y, z) { const v = new THREE.Vector3(x, y, z).project(camera); return [Math.round((v.x * 0.5 + 0.5) * innerWidth), Math.round((-v.y * 0.5 + 0.5) * canvas.clientHeight)]; },
    boxCorners() { const out = []; for (const [x, y, z] of [[-W / 2, 0, D / 2], [W / 2, 0, D / 2], [0, Hb + D * 0.95, -D / 2]]) { const v = box.localToWorld(new THREE.Vector3(x, y, z)); out.push(this.screen(v.x, v.y, v.z)); } return out; },
    proj() { const o = {}; for (const th of things) { if (!th.enabled) continue; project(th, pt); o[th.id] = [Math.round(pt.x), Math.round(pt.y)]; } return o; },
    // deterministic stepping for screenshots (?manual): advance simulated time, then draw once
    yFor(t) { const a = anchors.find((x) => x[1] >= t); return a ? a[0] + (t >= 2 && t < 14 ? innerHeight * HOLD * 0.4 : 0) : 0; },
    advance(sec = 1, fps = 30) { const n = Math.max(1, Math.round(sec * fps)); for (let i = 0; i < n; i++) update(1 / fps); render(); return tCur; },
    demo,
  };
}

// ---------------------------------------------------------------------- real in-tab tools for the visitor's files
function loadImage(url) {
  return new Promise((res, rej) => { const i = new Image(); i.onload = () => res(i); i.onerror = rej; i.decoding = 'async'; i.src = url; });
}
async function compressImage(img, file) {
  const c = document.createElement('canvas');
  c.width = img.naturalWidth; c.height = img.naturalHeight;
  c.getContext('2d').drawImage(img, 0, 0);
  let blob = await new Promise((r) => c.toBlob(r, 'image/webp', 0.75));
  let fmt = 'WebP';
  if (!blob || blob.type !== 'image/webp') { blob = await new Promise((r) => c.toBlob(r, 'image/jpeg', 0.8)); fmt = 'JPEG'; }
  if (!blob) return 'Your browser could not re-encode it';
  if (blob.size >= file.size) return `Already small: a ${fmt} copy would be ${fmtBytes(blob.size)}`;
  return `${fmtBytes(file.size)} → ${fmtBytes(blob.size)} as ${fmt}`;
}
async function hashFile(file) {
  if (!crypto.subtle) return 'Hashing needs a secure page';
  if (file.size > 300 * 1024 * 1024) return 'Too big to hash in a tab. The app streams it.';
  const buf = await file.arrayBuffer();
  const h = new Uint8Array(await crypto.subtle.digest('SHA-256', buf));
  const hex = [...h].map((b) => b.toString(16).padStart(2, '0')).join('');
  return `SHA-256 ${hex.slice(0, 10)}…${hex.slice(-6)}`;
}
async function formatJson(file, th) {
  if (file.size > 8 * 1024 * 1024) return 'Large file: the app formats it';
  const text = await file.text();
  try {
    const v = JSON.parse(text);
    const out = JSON.stringify(v, null, 2);
    th.formatted = out;
    return `Valid JSON · ${out.split('\n').length.toLocaleString('en')} lines, 2-space indent`;
  } catch (e) {
    return `Not valid JSON: ${String(e.message).replace(/^JSON\.parse: /, '').slice(0, 60)}`;
  }
}
async function countText(file) {
  if (file.size > 20 * 1024 * 1024) return 'Large file: the app counts it';
  const t = await file.text();
  const words = (t.match(/\S+/g) || []).length;
  return `${words.toLocaleString('en')} words · ${[...t].length.toLocaleString('en')} characters`;
}
function parseCsv(text, delim) {
  const first = text.split(/\r?\n/)[0] || '';
  const d = delim || (first.split(';').length > first.split(',').length ? ';' : ',');
  const lines = text.split(/\r?\n/).filter((l) => l.trim());
  const split = (l) => l.split(d).map((s) => s.replace(/^"|"$/g, '').trim());
  return { head: split(lines[0] || ''), rows: lines.slice(1).map(split), d };
}
async function csvStats(file) {
  if (file.size > 30 * 1024 * 1024) return 'Large file: the app reads it';
  const t = await file.text();
  const { head, rows } = parseCsv(t, /\.tsv$/i.test(file.name) ? '\t' : null);
  return `${rows.length.toLocaleString('en')} rows × ${head.length} columns`;
}
async function videoStill(url) {
  const v = document.createElement('video');
  v.muted = true; v.playsInline = true; v.preload = 'auto'; v.src = url;
  await new Promise((res, rej) => { v.onloadeddata = res; v.onerror = rej; setTimeout(rej, 3000); });
  v.currentTime = Math.min(1.5, (v.duration || 3) / 3);
  await new Promise((res, rej) => { v.onseeked = res; setTimeout(rej, 3000); });
  const c = document.createElement('canvas');
  c.width = 320; c.height = Math.round(320 * (v.videoHeight / v.videoWidth || 0.5625));
  c.getContext('2d').drawImage(v, 0, 0, c.width, c.height);
  v.removeAttribute('src'); v.load();
  return c;
}
async function audioBars(file) {
  const AC = window.AudioContext || window.webkitAudioContext;
  if (!AC) return null;
  const ac = new AC();
  try {
    const buf = await ac.decodeAudioData(await file.arrayBuffer());
    const ch = buf.getChannelData(0), n = 42, step = Math.floor(ch.length / n);
    const out = [];
    let max = 0;
    for (let i = 0; i < n; i++) { let s = 0; for (let j = 0; j < step; j += 16) s += ch[i * step + j] ** 2; const v = Math.sqrt(s / (step / 16)); out.push(v); max = Math.max(max, v); }
    return out.map((v) => Math.round(8 + 36 * (v / (max || 1))));
  } finally { ac.close && ac.close(); }
}
