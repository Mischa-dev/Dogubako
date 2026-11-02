import { describe, expect, it, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { FileItem } from '../lib/schema';
import type { FileRef } from '../lib/backend/types';

let resolveUpload: (refs: FileRef[]) => void = () => undefined;
vi.mock('../lib/app', () => ({
  useApp: () => ({
    backend: {
      uploadFiles: () => new Promise<FileRef[]>((r) => { resolveUpload = r; }),
    },
  }),
}));

const { useFileAdder } = await import('../form/FileField');

describe('useFileAdder', () => {
  it('matches upload results to their own items when an earlier item was removed', async () => {
    let state: FileItem[] = [];
    const onChange = (_name: string, v: unknown) => {
      state = typeof v === 'function' ? (v as (p: unknown) => FileItem[])(state) : (v as FileItem[]);
    };
    const param = { name: 'input', label: 'Files', type: 'file' as const, multiple: true };
    const { result } = renderHook(() => useFileAdder(param, onChange as any));
    const a = new File(['a'], 'a.txt', { type: 'text/plain' });
    const b = new File(['b'], 'b.txt', { type: 'text/plain' });
    const pending = result.current([a, b]);
    expect(state.map((i) => i.name)).toEqual(['a.txt', 'b.txt']);
    // Remove A while the upload is in flight.
    state = state.filter((i) => i.name !== 'a.txt');
    resolveUpload([
      { ref: 'f_a', name: 'a.txt', size: 1, mime: 'text/plain' },
      { ref: 'f_b', name: 'b.txt', size: 1, mime: 'text/plain' },
    ]);
    await pending;
    expect(state).toHaveLength(1);
    expect(state[0].name).toBe('b.txt');
    expect(state[0].ref).toBe('f_b');
    expect(state[0].status).toBe('ready');
  });
});
