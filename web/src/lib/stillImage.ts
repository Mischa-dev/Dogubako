/**
 * Still-image formats that browsers (Chromium, WebKitGTK) can't show in an <img>.
 * They are decoded here to RGBA and drawn as a PNG so a preview still appears.
 */

export type DecodedImage = { width: number; height: number; data: Uint8ClampedArray<ArrayBuffer> };
export type StillFormat = 'tiff' | 'tga' | 'qoi' | 'pnm';

/** Larger files are not decoded in the page; they get "preview not supported" instead. */
export const MAX_DECODE_BYTES = 64 * 1024 * 1024;
const MAX_PIXELS = 50_000_000;

const ext = (name: string) => (name.includes('.') ? name.split('.').pop()!.toLowerCase() : '');

/** The format to decode in the page, or null when an <img> can show the file as is. */
export function stillFormat(mime: string, name: string): StillFormat | null {
  const m = (mime || '').split(';')[0].trim().toLowerCase();
  const e = ext(name);
  if (m === 'image/tiff' || e === 'tif' || e === 'tiff') return 'tiff';
  if (m === 'image/x-tga' || m === 'image/tga' || m === 'image/x-targa' || e === 'tga') return 'tga';
  if (m === 'image/qoi' || e === 'qoi') return 'qoi';
  if (/^image\/x-portable-(anymap|bitmap|graymap|pixmap|arbitrarymap)$/.test(m) || /^(pnm|pbm|pgm|ppm|pam)$/.test(e)) return 'pnm';
  return null;
}

function checkSize(width: number, height: number) {
  if (!(width > 0 && height > 0) || width * height > MAX_PIXELS) throw new Error('unsupported image size');
}

export function decodeQoi(b: Uint8Array): DecodedImage {
  if (b.length < 22 || String.fromCharCode(b[0], b[1], b[2], b[3]) !== 'qoif') throw new Error('not a QOI file');
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const width = dv.getUint32(4);
  const height = dv.getUint32(8);
  checkSize(width, height);
  const out = new Uint8ClampedArray(width * height * 4);
  const index = new Uint8Array(64 * 4);
  let r = 0, g = 0, bl = 0, a = 255, run = 0;
  let p = 14;
  const end = b.length - 8;
  for (let o = 0; o < out.length; o += 4) {
    if (run > 0) run--;
    else if (p < end) {
      const b1 = b[p++];
      if (b1 === 0xfe) { r = b[p++]; g = b[p++]; bl = b[p++]; }
      else if (b1 === 0xff) { r = b[p++]; g = b[p++]; bl = b[p++]; a = b[p++]; }
      else if ((b1 & 0xc0) === 0x00) { const i = b1 * 4; r = index[i]; g = index[i + 1]; bl = index[i + 2]; a = index[i + 3]; }
      else if ((b1 & 0xc0) === 0x40) {
        r = (r + ((b1 >> 4) & 3) - 2) & 255; g = (g + ((b1 >> 2) & 3) - 2) & 255; bl = (bl + (b1 & 3) - 2) & 255;
      } else if ((b1 & 0xc0) === 0x80) {
        const b2 = b[p++];
        const vg = (b1 & 0x3f) - 32;
        r = (r + vg - 8 + ((b2 >> 4) & 0x0f)) & 255; g = (g + vg) & 255; bl = (bl + vg - 8 + (b2 & 0x0f)) & 255;
      } else run = b1 & 0x3f;
      const i = ((r * 3 + g * 5 + bl * 7 + a * 11) % 64) * 4;
      index[i] = r; index[i + 1] = g; index[i + 2] = bl; index[i + 3] = a;
    }
    out[o] = r; out[o + 1] = g; out[o + 2] = bl; out[o + 3] = a;
  }
  return { width, height, data: out };
}

/** PAM (P7): a text header of KEY value lines, then binary tuples. */
function decodePam(b: Uint8Array): DecodedImage {
  let p = 2;
  const head: Record<string, string> = {};
  for (;;) {
    const nl = b.indexOf(0x0a, p);
    if (nl < 0) throw new Error('bad PAM header');
    const line = String.fromCharCode(...b.subarray(p, nl)).trim();
    p = nl + 1;
    if (line === 'ENDHDR') break;
    if (!line || line.startsWith('#')) continue;
    const [k, ...v] = line.split(/\s+/);
    head[k.toUpperCase()] = v.join(' ');
  }
  const width = Number(head.WIDTH), height = Number(head.HEIGHT), depth = Number(head.DEPTH), maxval = Number(head.MAXVAL);
  checkSize(width, height);
  if (!(depth >= 1 && depth <= 4) || !(maxval >= 1 && maxval <= 65535)) throw new Error('unsupported PAM');
  const wide = maxval > 255;
  const n = width * height;
  if (p + n * depth * (wide ? 2 : 1) > b.length) throw new Error('truncated PAM data');
  const out = new Uint8ClampedArray(n * 4);
  const v = [0, 0, 0, 0];
  for (let i = 0; i < n; i++) {
    for (let c = 0; c < depth; c++) {
      const raw = wide ? (b[p] << 8) | b[p + 1] : b[p];
      p += wide ? 2 : 1;
      v[c] = maxval === 1 && depth <= 2 && c === 0 ? raw * 255 : Math.round((raw * 255) / maxval);
    }
    const o = i * 4;
    if (depth >= 3) { out[o] = v[0]; out[o + 1] = v[1]; out[o + 2] = v[2]; }
    else { out[o] = out[o + 1] = out[o + 2] = v[0]; }
    out[o + 3] = depth === 2 ? v[1] : depth === 4 ? v[3] : 255;
  }
  return { width, height, data: out };
}

/** Netpbm: P1-P6 (PBM, PGM, PPM, ASCII or binary, 8 or 16 bit) and P7 (PAM). */
export function decodePnm(b: Uint8Array): DecodedImage {
  if (b.length >= 3 && b[0] === 0x50 && b[1] === 0x37) return decodePam(b);
  if (b.length < 3 || b[0] !== 0x50 || b[1] < 0x31 || b[1] > 0x36) throw new Error('not a Netpbm file');
  const kind = b[1] - 0x30;
  let p = 2;
  const isSpace = (c: number) => c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d || c === 0x0b || c === 0x0c;
  const token = (): number => {
    for (;;) {
      while (p < b.length && isSpace(b[p])) p++;
      if (b[p] === 0x23) { while (p < b.length && b[p] !== 0x0a && b[p] !== 0x0d) p++; continue; }
      break;
    }
    const start = p;
    while (p < b.length && b[p] >= 0x30 && b[p] <= 0x39) p++;
    if (p === start) throw new Error('bad Netpbm header');
    return Number(String.fromCharCode(...b.subarray(start, p)));
  };
  const width = token();
  const height = token();
  checkSize(width, height);
  const bitmap = kind === 1 || kind === 4;
  const maxval = bitmap ? 1 : token();
  if (maxval < 1 || maxval > 65535) throw new Error('bad Netpbm maxval');
  const channels = kind === 3 || kind === 6 ? 3 : 1;
  const out = new Uint8ClampedArray(width * height * 4);
  const n = width * height;
  const put = (i: number, v: number[]) => {
    const o = i * 4;
    if (channels === 3) { out[o] = v[0]; out[o + 1] = v[1]; out[o + 2] = v[2]; }
    else { out[o] = out[o + 1] = out[o + 2] = v[0]; }
    out[o + 3] = 255;
  };
  const scale = (v: number) => Math.round((v * 255) / maxval);
  if (kind <= 3) {
    for (let i = 0; i < n; i++) {
      if (kind === 1) {
        while (p < b.length && (isSpace(b[p]) || b[p] === 0x23)) { if (b[p] === 0x23) while (p < b.length && b[p] !== 0x0a) p++; else p++; }
        put(i, [b[p++] === 0x31 ? 0 : 255]);
      } else {
        const v = [];
        for (let c = 0; c < channels; c++) v.push(scale(token()));
        put(i, v);
      }
    }
  } else {
    p++; // single whitespace after the header
    if (kind === 4) {
      const rowBytes = Math.ceil(width / 8);
      if (p + rowBytes * height > b.length) throw new Error('truncated Netpbm data');
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const bit = (b[p + y * rowBytes + (x >> 3)] >> (7 - (x & 7))) & 1;
          put(y * width + x, [bit ? 0 : 255]);
        }
      }
    } else {
      const wide = maxval > 255;
      if (p + n * channels * (wide ? 2 : 1) > b.length) throw new Error('truncated Netpbm data');
      for (let i = 0; i < n; i++) {
        const v = [];
        for (let c = 0; c < channels; c++) {
          v.push(scale(wide ? (b[p] << 8) | b[p + 1] : b[p]));
          p += wide ? 2 : 1;
        }
        put(i, v);
      }
    }
  }
  return { width, height, data: out };
}

/** Truevision TGA: colour-mapped, true-colour and greyscale, raw or RLE. */
export function decodeTga(b: Uint8Array): DecodedImage {
  if (b.length < 18) throw new Error('not a TGA file');
  const idLen = b[0];
  const cmapType = b[1];
  const type = b[2];
  const cmapFirst = b[3] | (b[4] << 8);
  const cmapLen = b[5] | (b[6] << 8);
  const cmapBits = b[7];
  const width = b[12] | (b[13] << 8);
  const height = b[14] | (b[15] << 8);
  const bpp = b[16];
  const desc = b[17];
  const base = type & 7;
  if (![1, 2, 3].includes(base) || (type & ~8) !== base) throw new Error('unsupported TGA type');
  checkSize(width, height);
  const alphaBits = desc & 0x0f;
  let p = 18 + idLen;
  const readColor = (q: number, bits: number, out: number[]) => {
    if (bits === 8) { out[0] = out[1] = out[2] = b[q]; out[3] = 255; }
    else if (bits === 15 || bits === 16) {
      const v = b[q] | (b[q + 1] << 8);
      out[0] = (((v >> 10) & 31) * 255) / 31; out[1] = (((v >> 5) & 31) * 255) / 31; out[2] = ((v & 31) * 255) / 31;
      out[3] = bits === 16 && alphaBits ? (v & 0x8000 ? 255 : 0) : 255;
    } else if (bits === 24) { out[0] = b[q + 2]; out[1] = b[q + 1]; out[2] = b[q]; out[3] = 255; }
    else if (bits === 32) { out[0] = b[q + 2]; out[1] = b[q + 1]; out[2] = b[q]; out[3] = alphaBits ? b[q + 3] : 255; }
    else throw new Error('unsupported TGA depth');
  };
  let palette: number[][] = [];
  if (cmapType === 1) {
    const step = Math.ceil(cmapBits / 8);
    for (let i = 0; i < cmapLen; i++) { const c = [0, 0, 0, 255]; readColor(p + i * step, cmapBits, c); palette.push(c); }
    p += cmapLen * step;
  } else if (base === 1) throw new Error('TGA colour map missing');
  const pixBytes = Math.ceil(bpp / 8);
  const pixel = (q: number, out: number[]) => {
    if (base === 1) {
      const idx = (pixBytes === 2 ? b[q] | (b[q + 1] << 8) : b[q]) - cmapFirst;
      const c = palette[idx] ?? [0, 0, 0, 255];
      out[0] = c[0]; out[1] = c[1]; out[2] = c[2]; out[3] = c[3];
    } else if (base === 3) {
      out[0] = out[1] = out[2] = b[q];
      out[3] = bpp === 16 && alphaBits ? b[q + 1] : 255;
    } else readColor(q, bpp, out);
  };
  const n = width * height;
  const linear = new Uint8ClampedArray(n * 4);
  const c = [0, 0, 0, 255];
  const need = () => { if (p + pixBytes > b.length) throw new Error('truncated TGA data'); };
  if (type & 8) {
    let i = 0;
    while (i < n) {
      if (p >= b.length) throw new Error('truncated TGA data');
      const h = b[p++];
      const count = (h & 0x7f) + 1;
      if (h & 0x80) {
        need(); pixel(p, c); p += pixBytes;
        for (let k = 0; k < count && i < n; k++, i++) linear.set(c, i * 4);
      } else {
        for (let k = 0; k < count && i < n; k++, i++) { need(); pixel(p, c); p += pixBytes; linear.set(c, i * 4); }
      }
    }
  } else {
    for (let i = 0; i < n; i++) { need(); pixel(p, c); p += pixBytes; linear.set(c, i * 4); }
  }
  const topDown = (desc & 0x20) !== 0;
  const rightToLeft = (desc & 0x10) !== 0;
  if (!topDown || rightToLeft) {
    const out = new Uint8ClampedArray(n * 4);
    for (let y = 0; y < height; y++) {
      const sy = topDown ? y : height - 1 - y;
      for (let x = 0; x < width; x++) {
        const sx = rightToLeft ? width - 1 - x : x;
        out.set(linear.subarray((sy * width + sx) * 4, (sy * width + sx) * 4 + 4), (y * width + x) * 4);
      }
    }
    return { width, height, data: out };
  }
  return { width, height, data: linear };
}

export async function decodeTiff(b: Uint8Array): Promise<DecodedImage> {
  const { default: UTIF } = await import('utif');
  const buf = b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
  const ifds = UTIF.decode(buf);
  const first = ifds.find((d) => d.t256 && d.t257);
  if (!first) throw new Error('no image in TIFF');
  checkSize(first.t256[0], first.t257[0]);
  UTIF.decodeImage(buf, first, ifds);
  // UTIF returns all-transparent pixels for colour spaces it can't convert (CIELab and friends).
  const photometric = first.t262?.[0];
  if (photometric === 8 || photometric === 9 || photometric === 10) throw new Error('unsupported TIFF colour space');
  const rgba = new Uint8ClampedArray(UTIF.toRGBA8(first));
  let visible = false;
  for (let i = 3; i < rgba.length; i += 4) if (rgba[i]) { visible = true; break; }
  if (!visible) throw new Error('TIFF decoded to nothing');
  return orient({ width: first.width, height: first.height, data: rgba }, first.t274?.[0] ?? 1);
}

/** Apply a TIFF/EXIF Orientation value (1-8) to decoded pixels. */
export function orient(img: DecodedImage, o: number): DecodedImage {
  if (!(o >= 2 && o <= 8)) return img;
  const { width: w, height: h, data } = img;
  const swap = o >= 5;
  const W = swap ? h : w, H = swap ? w : h;
  const out = new Uint8ClampedArray(data.length);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let X: number, Y: number;
      switch (o) {
        case 2: X = w - 1 - x; Y = y; break;
        case 3: X = w - 1 - x; Y = h - 1 - y; break;
        case 4: X = x; Y = h - 1 - y; break;
        case 5: X = y; Y = x; break;
        case 6: X = h - 1 - y; Y = x; break;
        case 7: X = h - 1 - y; Y = w - 1 - x; break;
        default: X = y; Y = w - 1 - x; break;
      }
      out.set(data.subarray((y * w + x) * 4, (y * w + x) * 4 + 4), (Y * W + X) * 4);
    }
  }
  return { width: W, height: H, data: out };
}

export function decodeStill(format: StillFormat, bytes: Uint8Array): Promise<DecodedImage> {
  switch (format) {
    case 'tiff': return decodeTiff(bytes);
    case 'tga': return Promise.resolve().then(() => decodeTga(bytes));
    case 'qoi': return Promise.resolve().then(() => decodeQoi(bytes));
    case 'pnm': return Promise.resolve().then(() => decodePnm(bytes));
  }
}

/** Fetch, decode and redraw as a PNG object URL (caller revokes it). */
export async function stillToObjectUrl(url: string, format: StillFormat): Promise<string> {
  const res = await fetch(url, { credentials: 'same-origin' });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const len = Number(res.headers.get('content-length') || 0);
  if (len > MAX_DECODE_BYTES) throw new Error('too large to preview');
  const bytes = new Uint8Array(await res.arrayBuffer());
  if (bytes.length > MAX_DECODE_BYTES) throw new Error('too large to preview');
  const img = await decodeStill(format, bytes);
  const canvas = document.createElement('canvas');
  canvas.width = img.width;
  canvas.height = img.height;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('no canvas');
  ctx.putImageData(new ImageData(img.data, img.width, img.height), 0, 0);
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
  if (!blob) throw new Error('could not encode preview');
  return URL.createObjectURL(blob);
}
