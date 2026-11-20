import { describe, expect, it, vi } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';
import type { JobView, ToolErrorView } from '../lib/types';
import { tool } from './fixtures';

const runs: Record<string, unknown>[] = [];
const checks: Record<string, unknown>[] = [];
const webmError: ToolErrorView = {
  code: 'invalid_input', param: 'format',
  message: 'WebM cannot hold h264 video (it needs VP8, VP9 or AV1) without re-encoding, and Add Subtitles does not re-encode; choose MKV or MP4, or convert the video to WebM first',
};
const backend = {
  mode: 'server',
  check: async (_toolId: string, params: Record<string, unknown>) => {
    checks.push(params);
    return params.format === 'webm' ? webmError : null;
  },
  run: (toolId: string, params: Record<string, unknown>) => {
    runs.push(params);
    const job = { id: `j_${runs.length}`, tool_id: toolId, status: 'succeeded', created_at: 0, params, result: { text: 'ok', data: null, artifacts: [], warnings: [], duration_ms: 1 } } as unknown as JobView;
    return { done: Promise.resolve(job), cancel: async () => undefined };
  },
  fileUrl: () => 'about:blank',
  artifactUrl: () => 'about:blank',
  listJobs: async () => [],
};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));

const { useToolForm } = await import('../tools/useToolForm');

const subtitles = tool({
  id: 'video.subtitles_add', name: 'Add Subtitles', prechecks: true,
  params: [
    { name: 'languages', label: 'Languages', type: 'text', default: 'eng' },
    { name: 'format', label: 'Format', type: 'enum', default: 'same', options: [{ value: 'same', label: 'Same as input' }, { value: 'mkv', label: 'MKV' }, { value: 'webm', label: 'WebM' }] },
  ],
});

describe('tool prechecks', () => {
  it('warns about WebM for an h264 source before Run and blocks the run', async () => {
    const { result } = renderHook(() => useToolForm(subtitles));
    await waitFor(() => expect(checks.length).toBe(1));
    expect(result.current.valid).toBe(true);

    act(() => result.current.setValue('format', 'webm'));
    await waitFor(() => expect(result.current.shownErrors.format).toContain('WebM cannot hold h264 video'));
    expect(result.current.valid).toBe(false);
    await act(async () => { await result.current.run(); });
    expect(runs).toHaveLength(0);

    act(() => result.current.setValue('format', 'mkv'));
    expect(result.current.errors.format).toBeUndefined();
    await waitFor(() => expect(checks.at(-1)?.format).toBe('mkv'));
    expect(result.current.valid).toBe(true);
    await act(async () => { await result.current.run(); });
    expect(runs).toHaveLength(1);
  });

  it('skips the check for tools without prechecks', async () => {
    const n = checks.length;
    const { result } = renderHook(() => useToolForm({ ...subtitles, prechecks: false }));
    await new Promise((r) => setTimeout(r, 400));
    expect(checks.length).toBe(n);
    expect(result.current.valid).toBe(true);
  });
});
