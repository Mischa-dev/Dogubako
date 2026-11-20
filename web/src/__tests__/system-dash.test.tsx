import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen } from '@testing-library/react';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const runs: Record<string, unknown>[] = [];
let hold: Promise<void> | null = null;
const backend = {
  mode: 'server',
  run: (toolId: string, params: Record<string, unknown>) => {
    runs.push(params);
    const job = { id: `j_${runs.length}`, tool_id: toolId, status: 'succeeded', created_at: 0, params, result: { text: '', data: { process_count: params.limit }, artifacts: [], warnings: [], duration_ms: 1 } } as unknown as JobView;
    const gate = hold;
    return { done: (gate ?? Promise.resolve()).then(() => job), cancel: async () => undefined };
  },
  fileUrl: () => 'about:blank',
  artifactUrl: () => 'about:blank',
  listJobs: async () => [],
};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));

const { default: SystemDash } = await import('../tools/special/SystemDash');

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
