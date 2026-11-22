import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import type { Backend, PackInfo, PacksState } from '../lib/backend/types';
import { InstallChoice, ToolsSettings } from '../components/Packs';

window.matchMedia ??= ((q: string) => ({ matches: false, media: q, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;

const pack = (id: string, name: string, over: Partial<PackInfo> = {}): PackInfo => ({
  id, name, description: `${name} tools`, enabled: true, tools: 3, engines: [], models: [], download_bytes: 0, disk_bytes: 0, ...over,
});
const state = (packs: PackInfo[], unused: PacksState['unused'] = []): PacksState => ({ packs, unused, unused_bytes: unused.reduce((a, u) => a + u.bytes, 0) });
const flush = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
afterEach(cleanup);

describe('Settings → Tools', () => {
  it('turns a pack off and offers to free what only it used', async () => {
    const whisper = { kind: 'model' as const, id: 'whisper-base', name: 'Whisper base', bytes: 150_000_000 };
    const backend = {
      packs: vi.fn(async () => state([pack('audio', 'Audio', { models: ['whisper'], download_bytes: 150_000_000, disk_bytes: 150_000_000 }), pack('text', 'Text')])),
      setPack: vi.fn(async (_id: string, on: boolean) => state([pack('audio', 'Audio', { enabled: on }), pack('text', 'Text')], on ? [] : [whisper])),
      packsCleanup: vi.fn(async () => ({ ...state([pack('audio', 'Audio', { enabled: false }), pack('text', 'Text')]), removed: [whisper], freed_bytes: whisper.bytes })),
    } as unknown as Backend;
    render(<ToolsSettings backend={backend} canManage />);
    await flush();
    expect(screen.getByText(/3 tools · up to 143 MB download/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText('Audio pack'));
    await flush();
    expect(backend.setPack).toHaveBeenCalledWith('audio', false);
    expect(screen.getAllByText(/Whisper base/).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: /Delete and free/ }));
    await flush();
    expect(backend.packsCleanup).toHaveBeenCalledWith(['whisper-base']);
  });

  it('is read-only for non-admins', async () => {
    const backend = { packs: vi.fn(), setPack: vi.fn() } as unknown as Backend;
    render(<ToolsSettings backend={backend} canManage={false} />);
    await flush();
    expect(screen.getByText(/Only administrators/)).toBeTruthy();
    expect(backend.packs).not.toHaveBeenCalled();
  });
});

describe('First-run install choice', () => {
  const initial = state([pack('audio', 'Audio'), pack('text', 'Text'), pack('pdf', 'PDF')]);
  it('defaults to Everything and skipping keeps every pack', async () => {
    const choosePacks = vi.fn(async () => initial);
    const onDone = vi.fn();
    render(<InstallChoice backend={{ choosePacks } as unknown as Backend} initial={initial} onDone={onDone} />);
    expect(screen.getByRole('radio', { name: /Everything/ }).getAttribute('aria-checked')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }));
    await flush();
    expect(choosePacks).toHaveBeenCalledWith(null);
    expect(onDone).toHaveBeenCalled();
  });

  it('Custom saves only the chosen packs', async () => {
    const choosePacks = vi.fn(async () => initial);
    render(<InstallChoice backend={{ choosePacks } as unknown as Backend} initial={initial} onDone={() => {}} />);
    fireEvent.click(screen.getByRole('radio', { name: /Custom/ }));
    fireEvent.click(screen.getByLabelText('Audio pack'));
    expect(screen.getByText(/2 of 3 packs · 6 tools/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }));
    await flush();
    expect(choosePacks).toHaveBeenCalledWith(['text', 'pdf']);
  });
});
