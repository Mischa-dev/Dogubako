import { describe, expect, it } from 'vitest';
import { matchIntent, rankTools, shortHint, toolsForFiles } from '../lib/intent';
import { makeDoc } from '../lib/search';
import type { ToolInfo } from '../lib/types';
import { tool } from './fixtures';

const fileParam = (accept?: string[], multiple = false) => [{ name: 'input', type: 'file', accept, multiple, required: true } as unknown as ToolInfo['params'][number]];
const tools: ToolInfo[] = [
  tool({ id: 'image.compress', name: 'Compress Images', category: 'image', params: fileParam(['image/*'], true), description: 'Reduce image file size: lossy re-encoding' }),
  tool({ id: 'image.convert', name: 'Convert Images', category: 'image', params: fileParam(['image/*'], true) }),
  tool({ id: 'image.resize', name: 'Resize Images', category: 'image', params: fileParam(['image/*'], true) }),
  tool({ id: 'image.remove_background', name: 'Remove Background', category: 'image', params: fileParam(['image/*']), availability: { available: false, platform_supported: true } }),
  tool({ id: 'pdf.merge', name: 'Merge PDFs', category: 'pdf', params: fileParam(['application/pdf', '.pdf'], true) }),
  tool({ id: 'files.hash', name: 'Hash Files', category: 'files', params: fileParam(undefined, true) }),
  tool({ id: 'text.case', name: 'Change Case', category: 'text', params: [] }),
];
const photo = { ref: 'f_1', name: 'IMG_2041.heic', size: 4_800_000, mime: 'image/heic' };

describe('toolsForFiles', () => {
  it('keeps tools whose file input accepts the file, including any-file tools', () => {
    expect(toolsForFiles(tools, [photo]).map((t) => t.id)).toEqual(['image.compress', 'image.convert', 'image.resize', 'image.remove_background', 'files.hash']);
  });
  it('needs a multi-file input for several files', () => {
    const ids = toolsForFiles(tools, [photo, { ...photo, ref: 'f_2' }]).map((t) => t.id);
    expect(ids).not.toContain('image.remove_background');
    expect(ids).toContain('image.compress');
  });
});

describe('rankTools', () => {
  it('puts the usual actions first, nudges pinned tools up, and puts tools that cannot run last', () => {
    const ranked = rankTools(toolsForFiles(tools, [photo]), [photo], { pinned: ['image.resize'], recent: [] }).map((t) => t.id);
    expect(ranked.slice(0, 3)).toEqual(['image.resize', 'image.compress', 'image.convert']);
    expect(ranked[ranked.length - 1]).toBe('image.remove_background');
  });
});

describe('rankTools with a pinned tool outside the usual actions', () => {
  it('does not let a pin outrank the usual actions', () => {
    const ranked = rankTools(toolsForFiles(tools, [photo]), [photo], { pinned: ['files.hash'], recent: [] }).map((t) => t.id);
    expect(ranked.indexOf('files.hash')).toBeGreaterThan(ranked.indexOf('image.resize'));
  });
});

describe('matchIntent', () => {
  const cands = toolsForFiles(tools, [photo]);
  const docs = tools.map((t) => ({ doc: makeDoc(t), item: t }));
  it('understands plain words', () => {
    expect(matchIntent(cands, docs, 'smaller')[0].id).toBe('image.compress');
    expect(matchIntent(cands, docs, 'jpg').map((t) => t.id)).toContain('image.convert');
  });
  it('puts what a word most likely means first, whatever the candidate order', () => {
    const all = tools.filter((t) => t.id === 'image.convert' || t.id === 'image.compress').reverse();
    expect(matchIntent(all, docs, 'make this smaller')[0].id).toBe('image.compress');
  });
  it('still matches tool names', () => {
    expect(matchIntent(cands, docs, 'resize')[0].id).toBe('image.resize');
  });
});

describe('shortHint', () => {
  it('uses the first clause of the description', () => {
    expect(shortHint(tools[0])).toBe('Reduce image file size');
  });
});
