import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import type { Availability, ToolInfo } from '../lib/types';
import { GetMissing } from '../components/GetMissing';
import { ErrorView } from '../results/ErrorView';

const app = vi.hoisted(() => ({ state: {} as Record<string, unknown>, changed: vi.fn() }));
vi.mock('../lib/app', () => ({ useApp: () => app.state, notifyToolsChanged: app.changed }));

afterEach(() => { cleanup(); app.changed.mockReset(); });

const engineMissing: Availability = {
  available: false, platform_supported: true,
  missing: [{ id: 'ghostscript', name: 'Ghostscript', hint: 'apt install ghostscript', downloadable: 'ghostscript', download_bytes: 20 * 1024 * 1024 }],
};
const systemMissing: Availability = {
  available: false, platform_supported: true,
  missing: [{ id: 'tesseract', name: 'Tesseract', hint: 'apt install tesseract-ocr' }],
};
const modelMissing: Availability = {
  available: false, platform_supported: true,
  missing_model: "No 'whisper' model installed.", model_group: 'whisper',
  recommended_model: { id: 'whisper-base', name: 'Whisper base', size_bytes: 147_951_465 },
};

function desktop(extra: Record<string, unknown> = {}) {
  const backend = {
    mode: 'desktop',
    engineInstall: vi.fn(async () => {}),
    onEngineProgress: vi.fn(async () => () => {}),
    modelDownload: vi.fn(async () => {}),
    onModelProgress: vi.fn(async () => () => {}),
    ...extra,
  };
  app.state = { backend, isServer: false, isAdmin: false, me: null };
  return backend;
}

describe('one-click engine and model downloads', () => {
  it('downloads a missing engine in place and refreshes the tools', async () => {
    const b = desktop();
    render(<GetMissing availability={engineMissing} />);
    fireEvent.click(screen.getByRole('button', { name: 'Download Ghostscript (20 MB)' }));
    await waitFor(() => expect(app.changed).toHaveBeenCalled());
    expect(b.engineInstall).toHaveBeenCalledWith('ghostscript');
  });

  it('offers the recommended model with a link to choose another', async () => {
    const b = desktop();
    render(<GetMissing availability={modelMissing} />);
    expect(screen.getByRole('link', { name: 'Choose another model' }).getAttribute('href')).toContain('models');
    fireEvent.click(screen.getByRole('button', { name: 'Download Whisper base (141 MB)' }));
    await waitFor(() => expect(app.changed).toHaveBeenCalled());
    expect(b.modelDownload).toHaveBeenCalledWith('whisper-base');
  });

  it('shows progress while downloading and the failure when it fails', async () => {
    let fail!: (e: Error) => void;
    let report!: (p: { id: string; done: number; total: number }) => void;
    desktop({
      engineInstall: vi.fn(() => new Promise<void>((_, rej) => { fail = rej; })),
      onEngineProgress: vi.fn(async (cb: typeof report) => { report = cb; return () => {}; }),
    });
    render(<GetMissing availability={engineMissing} />);
    fireEvent.click(screen.getByRole('button', { name: /Download Ghostscript/ }));
    await waitFor(() => expect(report).toBeDefined());
    report({ id: 'ghostscript', done: 10 * 1024 * 1024, total: 20 * 1024 * 1024 });
    expect(await screen.findByText(/10 MB · 50%/)).toBeTruthy();
    fail(new Error('checksum mismatch'));
    expect((await screen.findByRole('alert')).textContent).toMatch(/Couldn't download Ghostscript: checksum mismatch/);
    expect(app.changed).not.toHaveBeenCalled();
  });

  it('keeps the install hint for system programs', () => {
    desktop();
    const { container } = render(<GetMissing availability={systemMissing} />);
    expect(container.textContent).toBe('');
  });

  it('offers engines only where the app can install them (not the web server)', () => {
    app.state = { backend: { mode: 'server', modelDownload: vi.fn() }, isServer: true, isAdmin: true, me: null };
    const { container } = render(<GetMissing availability={engineMissing} />);
    expect(container.textContent).toBe('');
  });

  it('follows a server download until it finishes for admins', async () => {
    let calls = 0;
    const backend = {
      mode: 'server',
      modelDownload: vi.fn(async () => {}),
      models: vi.fn(async () => ({ models: [], downloads: [{ model: 'whisper-base', done: 1, total: 1, status: ++calls > 1 ? 'done' : 'downloading' }] })),
    };
    app.state = { backend, isServer: true, isAdmin: true, me: null };
    const view = render(<GetMissing availability={modelMissing} />);
    fireEvent.click(screen.getByRole('button', { name: /Download Whisper base/ }));
    view.unmount(); // leaving the page does not lose the download
    await waitFor(() => expect(app.changed).toHaveBeenCalled(), { timeout: 4000 });
    expect(backend.models).toHaveBeenCalledTimes(2);
    expect(backend.modelDownload).toHaveBeenCalledWith('whisper-base');
  });

  it('tells non-admin server users who to ask', () => {
    app.state = { backend: { mode: 'server', modelDownload: vi.fn() }, isServer: true, isAdmin: false, me: { admins: ['mischa', 'ops'] } };
    render(<GetMissing availability={modelMissing} />);
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.getByText('Ask mischa or ops to download Whisper base (141 MB).')).toBeTruthy();
  });

  it('offers the same button in the missing model error view', () => {
    desktop();
    const tool = { id: 'audio.transcribe', name: 'Transcribe', availability: modelMissing } as unknown as ToolInfo;
    render(<ErrorView error={{ code: 'missing_model', message: 'no model' }} tool={tool} />);
    expect(screen.getByRole('button', { name: 'Download Whisper base (141 MB)' })).toBeTruthy();
    expect(screen.getByRole('link', { name: /Open Models settings/ })).toBeTruthy();
  });

  it('refreshes a stale tool list when a run finds the model gone', () => {
    desktop();
    const tool = { id: 'audio.tts', name: 'Speak', availability: { available: true, platform_supported: true } } as unknown as ToolInfo;
    render(<ErrorView error={{ code: 'missing_model', message: 'Model not installed' }} tool={tool} />);
    expect(app.changed).toHaveBeenCalledTimes(1);
  });

  it('joins a download already running instead of starting another', async () => {
    let finish!: () => void;
    const b = desktop({ modelDownload: vi.fn(() => new Promise<void>((r) => { finish = r; })) });
    render(<><GetMissing availability={modelMissing} /><GetMissing availability={modelMissing} /></>);
    const [a, c] = screen.getAllByRole('button', { name: /Download Whisper base/ });
    fireEvent.click(a);
    fireEvent.click(c);
    await waitFor(() => expect(b.modelDownload).toHaveBeenCalledTimes(1));
    finish();
    await waitFor(() => expect(app.changed).toHaveBeenCalledTimes(1));
  });

  it('offers the model a failed run named when an option needs its own', async () => {
    desktop({
      models: vi.fn(async () => ({ models: [{ id: 'whisper-small.en-tdrz', group: 'whisper', name: 'Whisper small.en (tinydiarize)', size_bytes: 487_614_201, installed: false }], downloads: [] })),
    });
    const tool = { id: 'audio.transcribe', name: 'Transcribe', availability: { available: true, platform_supported: true } } as unknown as ToolInfo;
    render(<ErrorView error={{ code: 'missing_model', message: 'speaker turns', model: 'whisper-small.en-tdrz' }} tool={tool} />);
    expect(await screen.findByRole('button', { name: 'Download Whisper small.en (tinydiarize) (465 MB)' })).toBeTruthy();
  });

  it('offers a downloadable engine a failed run named', async () => {
    desktop({
      deps: vi.fn(async () => ({ dependencies: [{ id: 'whisper', name: 'whisper.cpp', found: false, downloadable: 'whisper', download_bytes: 5 * 1024 * 1024 }] })),
    });
    const tool = { id: 'web.transcript', name: 'URL transcript', availability: { available: true, platform_supported: true } } as unknown as ToolInfo;
    render(<ErrorView error={{ code: 'missing_dependency', dependency: 'whisper', hint: 'needs whisper.cpp' }} tool={tool} />);
    expect(await screen.findByRole('button', { name: 'Download whisper.cpp (5.0 MB)' })).toBeTruthy();
  });
});
