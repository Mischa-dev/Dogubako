import { describe, expect, it } from 'vitest';
import { rowTabs } from '../tools/ToolTopRow';
import type { ToolInfo } from '../lib/types';
import { tool } from './fixtures';

const fileParam = { name: 'input', label: 'Input', type: 'file' } as ToolInfo['params'][number];
const uuid = tool({ id: 'dev.uuid', name: 'UUID', category: 'dev' });
const hash = tool({ id: 'dev.hash', name: 'Hash', category: 'dev' });
const jwt = tool({ id: 'dev.jwt', name: 'JWT', category: 'dev' });
const resize = tool({ id: 'image.resize', name: 'Resize Images', category: 'image', params: [fileParam] });
const compress = tool({ id: 'image.compress', name: 'Compress Images', category: 'image', params: [fileParam] });
const all = [uuid, hash, jwt, resize, compress];
const toolMap = new Map(all.map((t) => [t.id, t]));
const base = { files: [], visibleTools: all, toolMap, recent: [] };

describe('rowTabs', () => {
  it('shows related tools, not unrelated pinned ones, for a tool that takes no file', () => {
    const ids = rowTabs({ ...base, tool: hash, row: 'file', pinned: ['image.resize', 'dev.jwt'] }).map((t) => t.id);
    expect(ids).toEqual(['dev.jwt', 'dev.uuid', 'dev.hash']);
  });

  it('keeps pinned tools when the row is set to pinned', () => {
    const ids = rowTabs({ ...base, tool: hash, row: 'pinned', pinned: ['image.resize'] }).map((t) => t.id);
    expect(ids).toEqual(['image.resize', 'dev.hash']);
  });

  it('shows pinned tools for a file tool still waiting for its file', () => {
    const ids = rowTabs({ ...base, tool: compress, row: 'file', pinned: ['dev.uuid'] }).map((t) => t.id);
    expect(ids).toEqual(['dev.uuid', 'image.compress']);
  });
});
