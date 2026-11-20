import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

let runs = 0;
const backend = {
  mode: 'server',
  run: (toolId: string, params: Record<string, unknown>) => {
    runs++;
    const job = { id: `j_${runs}`, tool_id: toolId, status: 'succeeded', created_at: 0, params, result: { text: null, data: { cpu: { usage_percent: 12.5 } }, artifacts: [], warnings: [], duration_ms: 1 } } as unknown as JobView;
    return { done: Promise.resolve(job), cancel: async () => undefined };
  },
  fileUrl: () => 'about:blank',
  artifactUrl: () => 'about:blank',
  listJobs: async () => [],
};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));

const { default: SystemDash, POLL_INTERVALS } = await import('../tools/special/SystemDash');

const cpuTool = tool({ id: 'system.cpu', name: 'CPU Info', category: 'system', ui: 'system' });
const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });
const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
let hidden = false;

beforeEach(() => {
  runs = 0;
  hidden = false;
  vi.useFakeTimers();
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('System dashboard polling', () => {
  it('polls no faster than every 2 seconds, 3 seconds by default', async () => {
    expect(Math.min(...POLL_INTERVALS.map(Number))).toBeGreaterThanOrEqual(2);
    render(<SystemDash tool={cpuTool} />);
    await flush();
    expect(runs).toBe(1);
    await advance(2900);
    expect(runs).toBe(1);
    await advance(200);
    expect(runs).toBe(2);
    await advance(3000);
    expect(runs).toBe(3);
  });

  it('Pause stops polling without starting another run, Resume restarts it', async () => {
    render(<SystemDash tool={cpuTool} />);
    await flush();
    expect(runs).toBe(1);
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    await flush();
    expect(vi.getTimerCount()).toBe(0);
    await advance(60_000);
    expect(runs).toBe(1);
    fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
    await flush();
    expect(runs).toBe(2);
    await advance(3000);
    expect(runs).toBe(3);
  });

  it('clears its timer on unmount', async () => {
    const { unmount } = render(<SystemDash tool={cpuTool} />);
    await flush();
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
    await advance(30_000);
    expect(runs).toBe(1);
  });

  it('stops polling while the window is hidden and refreshes when shown', async () => {
    render(<SystemDash tool={cpuTool} />);
    await flush();
    hidden = true;
    act(() => { document.dispatchEvent(new Event('visibilitychange')); });
    expect(vi.getTimerCount()).toBe(0);
    await advance(30_000);
    expect(runs).toBe(1);
    hidden = false;
    act(() => { document.dispatchEvent(new Event('visibilitychange')); });
    await flush();
    expect(runs).toBe(2);
    expect(vi.getTimerCount()).toBe(1);
  });

  it('hides the busy spinner during background polls once readings are shown', async () => {
    const { container } = render(<SystemDash tool={cpuTool} />);
    await flush();
    expect(screen.getByText('12.5%')).toBeTruthy();
    let release!: () => void;
    const run = backend.run;
    backend.run = (toolId, params) => {
      const h = run(toolId, params);
      return { ...h, done: new Promise<JobView>((r) => { release = () => void h.done.then(r); }) };
    };
    try {
      await advance(3000);
      expect(runs).toBe(2);
      expect(container.querySelector('.spinner')).toBeNull();
      release();
      await flush();
    } finally {
      backend.run = run;
    }
  });
});
