import { describe, expect, it } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import { decodePnm, decodeQoi, decodeTga, decodeTiff, stillFormat } from '../lib/stillImage';
import { previewKind } from '../results/Artifacts';
import { PlayableMedia } from '../results/MediaPreview';

const px = (d: { data: Uint8ClampedArray }, i: number) => Array.from(d.data.subarray(i * 4, i * 4 + 4));
const bytes = (...parts: (string | number[])[]) =>
  new Uint8Array(parts.flatMap((p) => (typeof p === 'string' ? [...p].map((c) => c.charCodeAt(0)) : p)));

describe('stillFormat', () => {
  it('picks a decoder for formats browsers cannot show', () => {
    expect(stillFormat('image/tiff', 'a.tif')).toBe('tiff');
    expect(stillFormat('image/x-tga', 'a.tga')).toBe('tga');
    expect(stillFormat('image/qoi', 'a.qoi')).toBe('qoi');
    expect(stillFormat('application/octet-stream', 'a.qoi')).toBe('qoi');
    expect(stillFormat('image/x-portable-pixmap', 'a.ppm')).toBe('pnm');
    expect(stillFormat('image/x-portable-graymap', 'a.pgm')).toBe('pnm');
  });
  it('leaves browser formats alone', () => {
    for (const [m, n] of [['image/png', 'a.png'], ['image/jpeg', 'a.jpg'], ['image/webp', 'a.webp'], ['image/svg+xml', 'a.svg']]) {
      expect(stillFormat(m, n)).toBeNull();
    }
  });
});

describe('previewKind', () => {
  const a = (mime: string, name: string) => ({ name, mime, size: 100 });
  it('previews these image types and ASS subtitles as text', () => {
    expect(previewKind(a('image/tiff', 'a.tiff'))).toBe('image');
    expect(previewKind(a('image/x-tga', 'a.tga'))).toBe('image');
    expect(previewKind(a('image/qoi', 'a.qoi'))).toBe('image');
    expect(previewKind(a('image/x-portable-pixmap', 'a.ppm'))).toBe('image');
    expect(previewKind(a('text/x-ssa', 'a.ass'))).toBe('text');
    expect(previewKind(a('audio/mp4', 'a.m4a'))).toBe('audio');
  });
});

describe('decoders', () => {
  it('decodes binary and ASCII PPM/PGM', () => {
    const p6 = decodePnm(bytes('P6\n# c\n2 1\n255\n', [255, 0, 0, 0, 0, 255]));
    expect([p6.width, p6.height]).toEqual([2, 1]);
    expect(px(p6, 0)).toEqual([255, 0, 0, 255]);
    expect(px(p6, 1)).toEqual([0, 0, 255, 255]);
    const p2 = decodePnm(bytes('P2 1 2 15\n0 15\n'));
    expect(px(p2, 1)).toEqual([255, 255, 255, 255]);
    const p4 = decodePnm(bytes('P4 2 1\n', [0b10000000]));
    expect(px(p4, 0)).toEqual([0, 0, 0, 255]);
    expect(px(p4, 1)).toEqual([255, 255, 255, 255]);
  });

  it('decodes QOI', () => {
    // 2x1: QOI_OP_RGB red, then QOI_OP_RUN of 1.
    const q = decodeQoi(bytes('qoif', [0, 0, 0, 2, 0, 0, 0, 1, 3, 0, 0xfe, 255, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0, 1]));
    expect([q.width, q.height]).toEqual([2, 1]);
    expect(px(q, 0)).toEqual([255, 0, 0, 255]);
    expect(px(q, 1)).toEqual([255, 0, 0, 255]);
    expect(() => decodeQoi(bytes('nope'))).toThrow();
  });

  it('decodes raw and RLE TGA with bottom-up rows', () => {
    const head = (type: number, desc: number) => [0, 0, type, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 24, desc];
    // 1x2, bottom-up: first stored pixel is the bottom row (blue), then top (red); BGR order.
    const raw = decodeTga(bytes(head(2, 0), [255, 0, 0, 0, 0, 255]));
    expect(px(raw, 0)).toEqual([255, 0, 0, 255]);
    expect(px(raw, 1)).toEqual([0, 0, 255, 255]);
    const rle = decodeTga(bytes(head(10, 0x20), [0x81, 0, 255, 0]));
    expect(px(rle, 0)).toEqual([0, 255, 0, 255]);
    expect(px(rle, 1)).toEqual([0, 255, 0, 255]);
  });

  it('decodes an uncompressed TIFF', async () => {
    // Little-endian, 2x1 RGB, 8 bits per sample, one strip at offset 8.
    const pixels = [255, 0, 0, 0, 255, 0];
    const entries: [number, number, number, number][] = [
      [256, 3, 1, 2], [257, 3, 1, 1], [258, 3, 3, 0], [259, 3, 1, 1], [262, 3, 1, 2],
      [273, 4, 1, 8], [277, 3, 1, 3], [278, 3, 1, 1], [279, 4, 1, 6],
    ];
    const ifdAt = 8 + pixels.length;
    const bpsAt = ifdAt + 2 + entries.length * 12 + 4;
    entries[2][3] = bpsAt;
    const buf = new DataView(new ArrayBuffer(bpsAt + 6));
    const u8 = new Uint8Array(buf.buffer);
    u8.set([0x49, 0x49, 42, 0]);
    buf.setUint32(4, ifdAt, true);
    u8.set(pixels, 8);
    buf.setUint16(ifdAt, entries.length, true);
    entries.forEach(([tag, type, count, value], i) => {
      const o = ifdAt + 2 + i * 12;
      buf.setUint16(o, tag, true); buf.setUint16(o + 2, type, true); buf.setUint32(o + 4, count, true);
      if (type === 3 && count === 1) buf.setUint16(o + 8, value, true); else buf.setUint32(o + 8, value, true);
    });
    [8, 8, 8].forEach((v, i) => buf.setUint16(bpsAt + i * 2, v, true));
    const t = await decodeTiff(u8);
    expect([t.width, t.height]).toEqual([2, 1]);
    expect(px(t, 0)).toEqual([255, 0, 0, 255]);
    expect(px(t, 1)).toEqual([0, 255, 0, 255]);
  });
});

describe('PlayableMedia', () => {
  it('explains instead of showing a bare error when playback fails', () => {
    const { container, getByRole } = render(<PlayableMedia kind="audio" controls url="blob:a" name="a.m4a" />);
    const audio = container.querySelector('audio')!;
    expect(audio).toBeTruthy();
    fireEvent.error(audio);
    expect(container.querySelector('audio')).toBeNull();
    expect(getByRole('status').textContent).toMatch(/no audio output device/);
    expect(getByRole('status').textContent).not.toMatch(/^Error/);
  });
});
