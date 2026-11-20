import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

const runs: Record<string, unknown>[] = [];
const backend = {
  mode: 'server',
  run: (toolId: string, params: Record<string, unknown>) => {
    runs.push(params);
    const text = String(params.text);
    const matches = [...text.matchAll(/\d+/g)].map((m) => ({ text: m[0], start: m.index!, end: m.index! + m[0].length, groups: [] }));
    const job = { id: `j_${runs.length}`, tool_id: toolId, status: 'succeeded', created_at: 0, params, result: { text: null, data: { matches, group_names: [] }, artifacts: [], warnings: [], duration_ms: 1 } } as unknown as JobView;
    return { done: Promise.resolve(job), cancel: async () => undefined };
  },
  fileUrl: () => 'about:blank',
  artifactUrl: () => 'about:blank',
  listJobs: async () => [],
};
vi.mock('../lib/app', () => ({ useApp: () => ({ backend }) }));

const { default: Regex } = await import('../tools/special/Regex');

const regexTool = tool({
  id: 'dev.regex', name: 'Regex Tester', category: 'dev',
  params: [
    { name: 'pattern', label: 'Pattern', type: 'text', required: true, placeholder: '\\d+' },
    { name: 'text', label: 'Test text', type: 'text', multiline: true, required: true },
    { name: 'flags', label: 'Flags', type: 'enum', multi: true, default: [], options: [{ value: 'x', label: 'x – verbose' }] },
  ],
});

const pattern = () => screen.getByLabelText(/Pattern/) as HTMLTextAreaElement;
const testText = () => screen.getByLabelText('Test text') as HTMLTextAreaElement;

describe('Regex Tester', () => {
  it('keeps a multi-line verbose pattern intact and runs it', async () => {
    render(<Regex tool={regexTool} />);
    expect(pattern().tagName).toBe('TEXTAREA');
    const verbose = '(?P<year>\\d{4})  # year\n-\n(?P<month>\\d{2})  # month';
    fireEvent.change(pattern(), { target: { value: verbose } });
    expect(pattern().value).toBe(verbose);
    expect(pattern().rows).toBe(3);
    expect(screen.getByText(/Line breaks in the pattern match literally/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText('x – verbose'));
    expect(screen.queryByText(/Line breaks in the pattern match literally/)).toBeNull();
    fireEvent.change(testText(), { target: { value: 'on 2024-05' } });
    await waitFor(() => expect(runs.at(-1)?.pattern).toBe(verbose));
    expect(runs.at(-1)?.flags).toEqual(['x']);
  });

  it('clears old matches and shows an error when the test text is emptied', async () => {
    render(<Regex tool={regexTool} />);
    fireEvent.change(pattern(), { target: { value: '\\d+' } });
    fireEvent.change(testText(), { target: { value: 'a 12 b 34' } });
    await waitFor(() => expect(screen.getByText('2 matches')).toBeTruthy());
    expect(screen.getByText('Matches & groups')).toBeTruthy();

    fireEvent.change(testText(), { target: { value: '' } });
    expect(screen.getByText('Enter some test text to match against')).toBeTruthy();
    expect(testText().getAttribute('aria-invalid')).toBe('true');
    expect(screen.queryByText('2 matches')).toBeNull();
    expect(screen.queryByText('Matches & groups')).toBeNull();
    expect(document.querySelector('mark')).toBeNull();
  });

  it('shows nothing stale when the pattern is emptied', async () => {
    render(<Regex tool={regexTool} />);
    fireEvent.change(pattern(), { target: { value: '\\d+' } });
    fireEvent.change(testText(), { target: { value: 'a 12' } });
    await waitFor(() => expect(screen.getByText('1 match')).toBeTruthy());
    fireEvent.change(pattern(), { target: { value: '' } });
    expect(screen.getByText('Enter a pattern')).toBeTruthy();
    expect(screen.queryByText('Matches & groups')).toBeNull();
  });
});
