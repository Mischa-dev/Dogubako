import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const runs: Record<string, unknown>[] = [];
let hold: Promise<void> | null = null;
const backend = {
  mode: 'server',
  run: (toolId: string, params: Record<string, unknown>) => {
    runs.push(params);
    const data = toolId === 'system.processes' ? { process_count: params.limit } : { cpu: { usage_percent: 12.5 } };
    const job = { id: `j_${runs.length}`, tool_id: toolId, status: 'succeeded', created_at: 0, params, result: { text: null, data, artifacts: [], warnings: [], duration_ms: 1 } } as unknown as JobView;
    const gate = hold;
    return { done: (gate ?? Promise.resolve()).then(() => job), cancel: async () => undefined };
  },
  fileUrl: () => 'about:blank',
  artifactUrl: () => 'about:blank',
  listJobs: async () => [],
};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));

const { default: SystemDash, POLL_INTERVALS } = await import('../tools/special/SystemDash');

afterEach(() => {
  cleanup();
  runs.length = 0;
  hold = null;
});

const processes = tool({
  id: 'system.processes', name: 'Top Processes', category: 'system',
  params: [{ name: 'limit', label: 'Number of processes', type: 'integer', min: 1, max: 500, default: 25 }],
});

const limitInput = () => screen.getByRole('spinbutton', { name: /Number of processes/ }) as HTMLInputElement;
const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });

describe('system dashboard validation', () => {
  it.each([[0, 'Must be at least 1'], [501, 'Must be at most 500']])('limit %i shows the error and marks the last table stale', async (bad, msg) => {
    hold = null;
    const { container, unmount } = render(<SystemDash tool={processes} />);
    await flush();
    expect(screen.getByText(/Live · updated/)).toBeTruthy();
    const before = runs.length;
    fireEvent.change(limitInput(), { target: { value: String(bad) } });
    await flush();
    expect(screen.getByText(msg)).toBeTruthy();
    expect(screen.getByText(/Not updating: fix the highlighted settings/)).toBeTruthy();
    expect(screen.queryByText(/Loading…|Live · updated/)).toBeNull();
    expect(screen.getByText(/Out of date/)).toBeTruthy();
    expect(container.querySelector('.dash-output')?.classList.contains('is-stale')).toBe(true);
    expect(runs.length).toBe(before);
    fireEvent.change(limitInput(), { target: { value: '10' } });
    await flush();
    expect(runs.at(-1)?.limit).toBe(10);
    expect(screen.queryByText(/Out of date/)).toBeNull();
    expect(screen.getByText(/Live · updated/)).toBeTruthy();
    unmount();
  });

  it('does not sit on Loading when the limit goes bad before the first reading', async () => {
    let release!: () => void;
    hold = new Promise<void>((r) => { release = r; });
    const { unmount } = render(<SystemDash tool={processes} />);
    expect(screen.getByText(/Loading…/)).toBeTruthy();
    fireEvent.change(limitInput(), { target: { value: '0' } });
    expect(screen.getByText('Must be at least 1')).toBeTruthy();
    expect(screen.queryByText(/Loading…/)).toBeNull();
    expect(screen.getByText(/Not updating/)).toBeTruthy();
    hold = null;
    await act(async () => { release(); });
    await flush();
    expect(screen.getByText(/Not updating/)).toBeTruthy();
    expect(screen.getByText(/Out of date/)).toBeTruthy();
    unmount();
  });
});

const cpuTool = tool({ id: 'system.cpu', name: 'CPU Info', category: 'system', ui: 'system' });
const flushMicro = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });
const advance = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });
let hidden = false;

describe('System dashboard polling', () => {
  beforeEach(() => {
    hidden = false;
    vi.useFakeTimers();
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => hidden });
  });
  afterEach(() => {
    vi.useRealTimers();
    hidden = false;
  });

  it('polls no faster than every 2 seconds, 3 seconds by default', async () => {
    expect(Math.min(...POLL_INTERVALS.map(Number))).toBeGreaterThanOrEqual(2);
    render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    expect(runs.length).toBe(1);
    await advance(2900);
    expect(runs.length).toBe(1);
    await advance(200);
    expect(runs.length).toBe(2);
    await advance(3000);
    expect(runs.length).toBe(3);
  });

  it('Pause stops polling without starting another run, Resume restarts it', async () => {
    render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    expect(runs.length).toBe(1);
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    await flushMicro();
    expect(vi.getTimerCount()).toBe(0);
    await advance(60_000);
    expect(runs.length).toBe(1);
    fireEvent.click(screen.getByRole('button', { name: 'Resume' }));
    await flushMicro();
    expect(runs.length).toBe(2);
    await advance(3000);
    expect(runs.length).toBe(3);
  });

  it('clears its timer on unmount', async () => {
    const { unmount } = render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    expect(vi.getTimerCount()).toBe(1);
    unmount();
    expect(vi.getTimerCount()).toBe(0);
    await advance(30_000);
    expect(runs.length).toBe(1);
  });

  it('stops polling while the window is hidden and refreshes when shown', async () => {
    render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    hidden = true;
    act(() => { document.dispatchEvent(new Event('visibilitychange')); });
    expect(vi.getTimerCount()).toBe(0);
    await advance(30_000);
    expect(runs.length).toBe(1);
    hidden = false;
    act(() => { document.dispatchEvent(new Event('visibilitychange')); });
    await flushMicro();
    expect(runs.length).toBe(2);
    expect(vi.getTimerCount()).toBe(1);
  });

  it('hides the busy spinner during background polls once readings are shown', async () => {
    const { container } = render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    expect(screen.getByText('12.5%')).toBeTruthy();
    let release!: () => void;
    const run = backend.run;
    backend.run = (toolId, params) => {
      const h = run(toolId, params);
      return { ...h, done: new Promise<JobView>((r) => { release = () => void h.done.then(r); }) };
    };
    try {
      await advance(3000);
      expect(runs.length).toBe(2);
      expect(container.querySelector('.spinner')).toBeNull();
      release();
      await flushMicro();
    } finally {
      backend.run = run;
    }
  });

  it('changing the interval during a poll does not start a second one', async () => {
    render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    expect(runs.length).toBe(1);
    let release!: () => void;
    hold = new Promise<void>((r) => { release = r; });
    await advance(3000);
    expect(runs.length).toBe(2);
    hold = null;
    fireEvent.click(screen.getByRole('radio', { name: '10 s' }));
    await flushMicro();
    expect(runs.length).toBe(2);
    await act(async () => { release(); });
    await flushMicro();
    expect(runs.length).toBe(2);
    await advance(9900);
    expect(runs.length).toBe(2);
    await advance(200);
    expect(runs.length).toBe(3);
  });

  it('Pause during the first reading still shows it and starts no more runs', async () => {
    let release!: () => void;
    hold = new Promise<void>((r) => { release = r; });
    render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    hold = null;
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    await act(async () => { release(); });
    await flushMicro();
    expect(screen.getByText('12.5%')).toBeTruthy();
    expect(screen.getByText(/Paused · updated/)).toBeTruthy();
    expect(vi.getTimerCount()).toBe(0);
    await advance(30_000);
    expect(runs.length).toBe(1);
  });

  it('changing the interval while idle waits the new interval', async () => {
    render(<SystemDash tool={cpuTool} />);
    await flushMicro();
    fireEvent.click(screen.getByRole('radio', { name: '5 s' }));
    await flushMicro();
    expect(runs.length).toBe(1);
    await advance(4900);
    expect(runs.length).toBe(1);
    await advance(200);
    expect(runs.length).toBe(2);
  });
});
