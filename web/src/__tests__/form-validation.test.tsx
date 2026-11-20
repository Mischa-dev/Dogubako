import { describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, renderHook, screen } from '@testing-library/react';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;
Element.prototype.scrollIntoView ??= () => undefined;

const runs: Record<string, unknown>[] = [];
const backend = {
  mode: 'server',
  run: (toolId: string, params: Record<string, unknown>) => {
    runs.push(params);
    const job = { id: `j_${runs.length}`, tool_id: toolId, status: 'succeeded', created_at: 0, params, result: { text: `result ${params.count}`, data: null, artifacts: [], warnings: [], duration_ms: 1 } } as unknown as JobView;
    return { done: Promise.resolve(job), cancel: async () => undefined };
  },
  fileUrl: () => 'about:blank',
  artifactUrl: () => 'about:blank',
  listJobs: async () => [],
};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));
vi.mock('../results/ResultView', () => ({
  ResultView: ({ job }: { job: JobView }) => <div className="result" data-testid="result">{job.result?.text}</div>,
}));

const { GenericTool } = await import('../tools/GenericTool');
const { useToolForm } = await import('../tools/useToolForm');

const counter = tool({
  id: 'text.repeat', name: 'Repeat',
  params: [
    { name: 'count', label: 'Count', type: 'integer', min: 1, max: 10, default: 2 },
    { name: 'note', label: 'Note', type: 'text', required: true },
    { name: 'sep', label: 'Separator', type: 'text', advanced: true, default: ',' },
  ],
});

const runButton = () => screen.getByRole('button', { name: 'Run' }) as HTMLButtonElement;
const countInput = () => screen.getByLabelText('Count') as HTMLInputElement;

async function succeed() {
  fireEvent.change(screen.getByLabelText(/Note/), { target: { value: 'hi' } });
  await act(async () => { fireEvent.click(runButton()); });
  expect(screen.getByTestId('result').textContent).toBe('result 2');
}

describe('tool form validation', () => {
  it('flags an edited field as soon as it is invalid and blocks Run', () => {
    render(<GenericTool tool={counter} />);
    expect(screen.queryByText('Must be at least 1')).toBeNull();
    fireEvent.change(countInput(), { target: { value: '0' } });
    expect(screen.getByText('Must be at least 1')).toBeTruthy();
    expect(runButton().disabled).toBe(true);
    expect(screen.getByText('Fix Count to run')).toBeTruthy();
    fireEvent.change(countInput(), { target: { value: '3' } });
    expect(runButton().disabled).toBe(false);
  });

  it('shows untouched required fields after Run and blocks further runs until fixed', async () => {
    render(<GenericTool tool={counter} />);
    const before = runs.length;
    await act(async () => { fireEvent.click(runButton()); });
    expect(runs.length).toBe(before);
    expect(screen.getByText('Required')).toBeTruthy();
    expect(runButton().disabled).toBe(true);
  });

  it('marks the previous result stale after a validation error and keeps Reset', async () => {
    render(<GenericTool tool={counter} />);
    await succeed();
    expect(screen.queryByText(/Out of date/)).toBeNull();
    fireEvent.change(countInput(), { target: { value: '0' } });
    expect(screen.getByText(/Out of date/)).toBeTruthy();
    expect(screen.getByTestId('result').closest('.tool-output')?.classList.contains('is-stale')).toBe(true);
    expect(screen.getByRole('button', { name: 'Reset' })).toBeTruthy();
    fireEvent.change(countInput(), { target: { value: '4' } });
    expect(screen.queryByText(/Out of date/)).toBeNull();
  });

  it('offers Reset before any run, and Reset clears values, errors, result and Advanced', async () => {
    render(<GenericTool tool={counter} />);
    expect(screen.queryByRole('button', { name: 'Reset' })).toBeNull();
    await succeed();
    fireEvent.click(screen.getByRole('button', { name: /Advanced options/ }));
    expect(screen.getByLabelText('Separator')).toBeTruthy();
    fireEvent.change(countInput(), { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: 'Reset' }));
    expect(countInput().value).toBe('2');
    expect(screen.queryByText('Must be at least 1')).toBeNull();
    expect(screen.queryByTestId('result')).toBeNull();
    expect(screen.queryByLabelText('Separator')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reset' })).toBeNull();
    fireEvent.change(countInput(), { target: { value: '5' } });
    expect(screen.getByRole('button', { name: 'Reset' })).toBeTruthy();
  });

  it('does not validate params the page supplies at run time', async () => {
    const redact = tool({ id: 'image.redact', name: 'Redact', params: [{ name: 'regions', label: 'Regions', type: 'json', required: true }] });
    const { result } = renderHook(() => useToolForm(redact, { provided: ['regions'] }));
    expect(result.current.valid).toBe(true);
    await act(async () => { await result.current.run({ overrides: { regions: [[0, 0, 1, 1]] } }); });
    expect(result.current.stale).toBe(false);
    expect(Object.keys(result.current.shownErrors)).toEqual([]);
  });
  it('keeps over-long text, shows it is too long and blocks Run instead of truncating', () => {
    const rename = tool({ id: 'file.rename', name: 'Rename', params: [
      { name: 'pattern', label: 'Pattern', type: 'text', max_len: 10 },
      { name: 'body', label: 'Body', type: 'text', multiline: true, max_len: 5 },
    ] });
    render(<GenericTool tool={rename} />);
    const pattern = screen.getByLabelText('Pattern') as HTMLInputElement;
    expect(pattern.maxLength).toBe(-1);
    expect(screen.queryByText(/\/ 10/)).toBeNull();
    fireEvent.change(pattern, { target: { value: 'abcdefghijkl' } });
    expect(pattern.value).toBe('abcdefghijkl');
    expect(screen.getByText('Too long: 12 of 10 characters allowed. Remove 2 to run.')).toBeTruthy();
    expect(screen.getByText('12 / 10').classList.contains('is-over')).toBe(true);
    expect(runButton().disabled).toBe(true);
    fireEvent.change(pattern, { target: { value: 'abcdefghij' } });
    expect(screen.queryByText(/Too long/)).toBeNull();
    expect(screen.getByText('10 / 10').classList.contains('is-over')).toBe(false);
    expect(runButton().disabled).toBe(false);

    const body = screen.getByLabelText('Body') as HTMLTextAreaElement;
    expect(body.maxLength).toBe(-1);
    fireEvent.change(body, { target: { value: 'héllo' } });
    expect(screen.getByText('Too long: 6 of 5 bytes allowed. Remove 1 to run.')).toBeTruthy();
    expect(runButton().disabled).toBe(true);
  });
});
