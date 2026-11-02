import { describe, expect, it } from 'vitest';
import { fuzzyMatch, makeDoc, search, scoreDoc } from '../lib/search';
import { tool } from './fixtures';

const tools = [
  tool({ id: 'pdf.compress', name: 'Compress PDF', category: 'pdf', keywords: ['shrink', 'smaller'], description: 'Reduce PDF file size' }),
  tool({ id: 'image.compress', name: 'Compress Images', category: 'image', keywords: ['optimize', 'shrink'], description: 'Make images smaller' }),
  tool({ id: 'dev.json_format', name: 'JSON Formatter', category: 'dev', keywords: ['pretty print', 'beautify'], description: 'Format and validate JSON' }),
  tool({ id: 'text.case', name: 'Change Case', category: 'text', keywords: ['uppercase', 'lowercase'], description: 'Convert text to upper or lower case' }),
  tool({ id: 'image.convert', name: 'Convert Images', category: 'image', keywords: ['png to jpg', 'webp'], description: 'Convert images between formats' }),
  tool({ id: 'video.convert', name: 'Convert Video', category: 'video', keywords: ['mp4'], description: 'Re-encode video to json-free formats' }),
];
const labels: Record<string, string> = { pdf: 'PDF & Documents', image: 'Images', dev: 'Developer', text: 'Text', video: 'Video' };
const docs = tools.map((t) => ({ doc: makeDoc(t, labels[t.category]), item: t }));
const ids = (q: string) => search(docs, q).map((h) => h.item.id);

describe('search', () => {
  it('ranks name prefix matches above keyword and description matches', () => {
    expect(ids('json')[0]).toBe('dev.json_format');
    // "json" also appears in a description, which must rank lower
    expect(ids('json')).toEqual(['dev.json_format', 'video.convert']);
  });

  it('requires every token to match (AND) and boosts phrase matches', () => {
    expect(ids('compress pdf')[0]).toBe('pdf.compress');
    expect(ids('compress pdf')).not.toContain('image.convert');
  });

  it('matches keywords and category labels', () => {
    expect(ids('uppercase')).toEqual(['text.case']);
    expect(ids('shrink').sort()).toEqual(['image.compress', 'pdf.compress']);
    expect(ids('images')).toContain('image.convert');
  });

  it('supports fuzzy subsequence matches on names with highlight ranges', () => {
    const hit = search(docs, 'jsfmt')[0];
    expect(hit.item.id).toBe('dev.json_format');
    expect(hit.ranges.length).toBeGreaterThan(0);
  });

  it('returns highlight ranges for substring matches', () => {
    const r = scoreDoc(docs[1].doc, 'imag')!;
    expect(r.ranges).toEqual([[9, 13]]);
  });

  it('applies boosts (recents/favorites) as tie breakers', () => {
    const plain = ids('convert');
    const boosted = search(docs, 'convert', { boost: (id) => (id === 'video.convert' ? 50 : 0) }).map((h) => h.item.id);
    expect(plain[0]).toBe('image.convert');
    expect(boosted[0]).toBe('video.convert');
  });

  it('returns no results for nonsense', () => {
    expect(ids('zzqqxx')).toEqual([]);
  });

  it('fuzzyMatch rejects out-of-order characters', () => {
    expect(fuzzyMatch('abc', 'ca')).toBeNull();
    expect(fuzzyMatch('json formatter', 'jf')?.positions).toEqual([0, 5]);
  });
});
