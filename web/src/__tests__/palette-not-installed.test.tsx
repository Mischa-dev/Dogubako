import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { AvailableTool } from '../lib/types';
import { makeDoc } from '../lib/search';
import { tool as installed } from './fixtures';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;
Element.prototype.scrollIntoView ??= () => undefined;

const avail = (over: Partial<AvailableTool> = {}): AvailableTool => ({
  id: 'text.upper', name: 'Upper case', description: 'Make text upper case', keywords: ['caps'], category: 'text',
  source: 'pack', pack: 'text', version: '1.0.0', permissions: [], requires: [], platforms: [], can_install: true, ...over,
});
const upper = installed({ id: 'text.upper_installed', name: 'Upper words' });
const calls: string[] = [];
let tools: AvailableTool[] = [];
const report = (id: string) => ({ id, name: id, version: '1', sha256: 'x', passed: true, loaded: true, trusted: true, tests: [] });
const app = {
  isAdmin: true, isServer: true, toolMap: new Map(), categories: [], categoryLabel: (c: string) => c,
  docs: [{ doc: makeDoc(upper, 'Text'), item: upper }],
  reloadTools: vi.fn(async () => { calls.push('reload'); }),
  backend: {
    availableTools: vi.fn(async () => ({ tools })),
    setPack: vi.fn(async (id: string, on: boolean) => { calls.push(`pack:${id}:${on}`); return { packs: [] }; }),
    storeInstall: vi.fn(async (id: string) => { calls.push(`store:${id}`); return report(id); }),
  },
};
vi.mock('../lib/app', () => ({ useApp: () => app, notifyToolsChanged: () => undefined }));
vi.mock('../components/toast', () => ({ toast: { success: () => undefined, error: (m: string) => calls.push(`error:${m}`) } }));
vi.mock('../lib/assistant/useAssistant', () => ({ looksLikeRequest: () => false, useAssistant: () => ({ on: false }) }));
vi.mock('../lib/router', async (orig) => ({ ...(await orig<object>()), navigate: (to: string) => calls.push(`go:${to}`) }));

const { CommandPalette } = await import('../components/CommandPalette');
const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const open = async (q: string) => {
  const onClose = vi.fn(() => calls.push('close'));
  render(<CommandPalette open onClose={onClose} initialQuery={q} />);
  await flush();
};
const labels = () => screen.getAllByRole('option').map((o) => o.textContent ?? '');

afterEach(() => { cleanup(); calls.length = 0; tools = []; });

describe('Command palette: not-installed tools', () => {
  it('lists matches after installed ones, badged Not installed', async () => {
    tools = [avail()];
    await open('upper');
    const all = labels();
    expect(all[0]).toContain('Upper words');
    expect(all[1]).toContain('Upper case');
    expect(all[1]).toContain('Not installed');
    expect(screen.getByText('Not installed', { selector: '.palette-section' })).toBeTruthy();
  });

  it('turns the pack on and opens the tool without a reload', async () => {
    tools = [avail()];
    await open('upper case');
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));
    await flush();
    expect(calls).toEqual(['pack:text:true', 'reload', 'close', expect.stringContaining('text.upper')]);
  });

  it('asks before a store install, listing permissions and requirements', async () => {
    tools = [avail({ id: 'data.weather', name: 'Weather', description: 'Rain or shine', source: 'store', pack: undefined, permissions: ['network'], requires: ['curl'] })];
    await open('weather');
    fireEvent.click(screen.getByRole('button', { name: 'Install' }));
    await flush();
    expect(screen.getByText('Install Weather?')).toBeTruthy();
    expect(screen.getByText(/network/)).toBeTruthy();
    expect(screen.getByText(/curl/)).toBeTruthy();
    expect(calls).toEqual([]);
    fireEvent.click(screen.getAllByRole('button', { name: 'Install' }).at(-1)!);
    await flush();
    expect(calls).toEqual(['store:data.weather', 'reload', 'close', expect.stringContaining('data.weather')]);
  });

  it('shows Ask your admin to users who may not install', async () => {
    tools = [avail({ can_install: false })];
    await open('upper case');
    expect(screen.getByText('Ask your admin')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Install' })).toBeNull();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Enter' });
    await flush();
    expect(calls).toEqual([]);
  });
});
