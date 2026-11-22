import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { StoreResult, StoreTool } from '../lib/types';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const tool = (over: Partial<StoreTool> = {}): StoreTool => ({
  id: 'data.weather', name: 'Current weather', description: 'Weather for a place', keywords: [], category: 'data',
  version: '1.1.0', sha256: 'abc', permissions: ['network'], requires: [], platforms: ['linux'], url: 'data.weather/manifest.toml',
  installed: false, update_available: false, ...over,
});
let result: StoreResult;
const calls: string[] = [];
const report = (id: string) => ({ id, name: id, version: '1.1.0', sha256: 'abc', passed: true, loaded: true, trusted: true, tests: [] });
const app = { isAdmin: true, isServer: true, backend: {
  storeSearch: vi.fn(async (q?: string) => { calls.push(`search:${q}`); return result; }),
  storeInstall: vi.fn(async (id: string) => { calls.push(`install:${id}`); return report(id); }),
  storeUpdate: vi.fn(async (id?: string) => { calls.push(`update:${id}`); return { updated: [report(id ?? '')] }; }),
} };
vi.mock('../lib/app', () => ({ useApp: () => app, notifyToolsChanged: () => undefined }));
vi.mock('../components/toast', () => ({ toast: { success: () => undefined, error: () => undefined } }));

const { ToolStore } = await import('../pages/Settings');
const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const online = { url: 'https://docs.dogubako.app/store/index.json', offline: false, fetched_at: 0 };

afterEach(() => { cleanup(); calls.length = 0; app.isAdmin = true; });

describe('Settings → Tool store', () => {
  it('installs a tool that is not installed yet', async () => {
    result = { index: online, tools: [tool()] };
    render(<ToolStore />);
    await flush();
    fireEvent.click(screen.getByRole('button', { name: /Install/ }));
    await flush();
    expect(calls).toEqual(['search:', 'install:data.weather', 'search:']);
  });

  it('shows the installed store version and an Update action', async () => {
    result = { index: online, tools: [tool({ installed: true, installed_version: '1.0.0', update_available: true })] };
    render(<ToolStore />);
    await flush();
    expect(screen.getByText('1.0.0')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Update to 1.1.0' }));
    await flush();
    expect(calls).toContain('update:data.weather');
  });

  it('searches the cached index offline and turns Install off', async () => {
    result = { index: { ...online, offline: true, error: 'no route' }, tools: [tool()] };
    render(<ToolStore />);
    await flush();
    expect(screen.getByText(/Offline: showing the store/)).toBeTruthy();
    expect((screen.getByRole('button', { name: /Install/ }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText('Search the tool store'), { target: { value: 'rain' } });
    fireEvent.click(screen.getByRole('button', { name: /Search/ }));
    await flush();
    expect(calls).toContain('search:rain');
  });

  it('is read-only for non-admins on a server', async () => {
    app.isAdmin = false;
    render(<ToolStore />);
    await flush();
    expect(screen.getByText(/Only administrators/)).toBeTruthy();
    expect(calls).toEqual([]);
  });
});
