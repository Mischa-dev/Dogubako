import { describe, expect, it } from 'vitest';
import { benchItems, benchKindClass, kindOf, suggestion } from '../lib/bench';
import type { JobView } from '../lib/types';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { FileThing } from '../components/BenchObject';
import { tool } from './fixtures';

const job = (id: string, at: number, artifacts: { name: string; mime: string; size?: number }[], status: JobView['status'] = 'succeeded'): JobView => ({
  id, tool_id: 'image.compress', status, created_at: at, finished_at: at,
  result: { data: null, duration_ms: 10, artifacts: artifacts.map((a) => ({ size: 1000, ...a })) },
});

describe('kindOf', () => {
  it('classifies by mime and extension', () => {
    expect(kindOf('image/avif', 'a.avif')).toBe('image');
    expect(kindOf('application/octet-stream', 'thesis.pdf')).toBe('pdf');
    expect(kindOf('video/mp4', 'x.mp4')).toBe('video');
    expect(kindOf('audio/mpeg', 'x.mp3')).toBe('audio');
    expect(kindOf('text/csv', 'orders.csv')).toBe('sheet');
    expect(kindOf('application/zip', 'bundle.zip')).toBe('archive');
    expect(kindOf('application/json', 'config.json')).toBe('text');
    expect(kindOf('application/x-iso9660-image', 'ubuntu.iso')).toBe('file');
  });
});

describe('benchItems', () => {
  it('lists newest outputs first, skips failed runs and duplicate names, and caps the count', () => {
    const jobs = [
      job('old', 100, [{ name: 'a.png', mime: 'image/png' }]),
      job('new', 300, [{ name: 'b.pdf', mime: 'application/pdf' }, { name: 'a.png', mime: 'image/png' }]),
      job('bad', 400, [{ name: 'c.png', mime: 'image/png' }], 'failed'),
      job('mid', 200, [{ name: 'd.mp3', mime: 'audio/mpeg' }]),
    ];
    const items = benchItems(jobs, 3);
    expect(items.map((i) => i.name)).toEqual(['b.pdf', 'a.png', 'd.mp3']);
    expect(items[1].jobId).toBe('new');
    expect(items[0].kind).toBe('pdf');
  });
});

describe('suggestion', () => {
  const tools = new Map([
    ['pdf.compress', tool({ id: 'pdf.compress', name: 'Compress PDF' })],
    ['speech.transcribe', tool({ id: 'speech.transcribe', name: 'Transcribe', availability: { available: false, platform_supported: true } })],
  ]);
  it('suggests compressing a large PDF', () => {
    const items = benchItems([job('j', 1, [{ name: 'big.pdf', mime: 'application/pdf', size: 6 * 1024 * 1024 }])]);
    expect(suggestion(items, tools)?.toolId).toBe('pdf.compress');
  });
  it('never suggests a tool that cannot run', () => {
    const items = benchItems([job('j', 1, [{ name: 'talk.mp3', mime: 'audio/mpeg' }])]);
    expect(suggestion(items, tools)).toBeNull();
  });
});

describe('benchKindClass', () => {
  it('never reuses a class the file drawings are styled with', () => {
    const kinds = ['image', 'pdf', 'video', 'audio', 'sheet', 'text', 'archive', 'file'] as const;
    const markup = kinds.map((kind) => renderToStaticMarkup(createElement(FileThing, { kind, name: `a.${kind}` }))).join('');
    const drawings = new Set(Array.from(markup.matchAll(/class="([^"]*)"/g), (m) => m[1].split(' ')).flat());
    expect(drawings.has('bo-sheet')).toBe(true);
    // A spreadsheet's wrapper used to get `.bo-sheet`, the absolutely positioned PDF page,
    // and flew to the top-left corner of the app.
    for (const k of kinds) expect(drawings.has(benchKindClass(k))).toBe(false);
  });
});
