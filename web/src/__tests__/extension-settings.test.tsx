import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { ExtensionSettings } from '../lib/types';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const ext = (over: Partial<Record<string, unknown>> = {}): ExtensionSettings => ({
  id: 'print.ext_lp', name: 'Print', settings: [
    { name: 'queue', label: 'Queue', type: 'text', required: true, secret: false, set: false, ...over },
    { name: 'token', label: 'Token', type: 'secret', secret: true, set: true },
  ],
});
const saved: { id: string; values: Record<string, unknown> }[] = [];
const app = { isAdmin: true, isServer: true, backend: {
  extensionSettings: vi.fn(async () => [ext()]),
  configureExtension: vi.fn(async (id: string, values: Record<string, unknown>) => { saved.push({ id, values }); return ext({ set: true, value: values.queue }); }),
} };
vi.mock('../lib/app', () => ({ useApp: () => app, notifyToolsChanged: () => undefined }));

const { Extensions } = await import('../pages/Settings');
const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });

afterEach(() => { cleanup(); saved.length = 0; app.isAdmin = true; });

describe('Settings → Extensions', () => {
  it('sends only changed values and never shows a stored secret', async () => {
    render(<Extensions />);
    await flush();
    expect(screen.getByText('Needs setup')).toBeTruthy();
    const token = screen.getByLabelText('Token') as HTMLInputElement;
    expect(token.type).toBe('password');
    expect(token.value).toBe('');
    expect(token.placeholder).toMatch(/Saved/);
    fireEvent.change(screen.getByLabelText('Queue *'), { target: { value: 'office' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await flush();
    expect(saved).toEqual([{ id: 'print.ext_lp', values: { queue: 'office' } }]);
    expect(screen.getByText('Ready')).toBeTruthy();
  });

  it('is read-only for non-admins on a server', async () => {
    app.isAdmin = false;
    render(<Extensions />);
    await flush();
    expect(screen.getByText(/Only administrators/)).toBeTruthy();
    expect(screen.queryByLabelText('Queue *')).toBeNull();
  });
});
