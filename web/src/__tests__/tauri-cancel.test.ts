import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
let releaseListen: (() => void) | null = null;

vi.mock('@tauri-apps/api/core', () => ({ invoke: (cmd: string, args?: unknown) => invoke(cmd, args) }));
vi.mock('@tauri-apps/api/event', () => ({
  // Registration stays pending until the test releases it.
  listen: () => new Promise<() => void>((resolve) => { releaseListen = () => resolve(() => {}); }),
}));

import { TauriBackend } from '../lib/backend/tauri';

describe('desktop run cancellation', () => {
  beforeEach(() => {
    releaseListen = null;
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'run_tool') return { id: 'x', tool_id: 't', status: 'succeeded', created_at: 0 };
      return undefined;
    });
  });

  it('does not start a run cancelled while listening is being set up', async () => {
    const h = new TauriBackend().run('t', {}, { ephemeral: true });
    await vi.waitFor(() => expect(releaseListen).toBeTypeOf('function'));
    await h.cancel();
    releaseListen!();
    const job = await h.done;
    expect(job.status).toBe('cancelled');
    expect(invoke.mock.calls.map((c) => c[0])).not.toContain('run_tool');
    expect(invoke.mock.calls.map((c) => c[0])).not.toContain('cancel_run');
  });

  it('forwards a cancel once the run has started', async () => {
    let finish: (v: unknown) => void = () => {};
    invoke.mockImplementation((cmd: string) =>
      cmd === 'run_tool' ? new Promise((r) => { finish = r; }) : Promise.resolve(undefined));
    const h = new TauriBackend().run('t', {}, { ephemeral: true });
    await vi.waitFor(() => expect(releaseListen).toBeTypeOf('function'));
    releaseListen!();
    await vi.waitFor(() => expect(invoke.mock.calls.map((c) => c[0])).toContain('run_tool'));
    await h.cancel();
    expect(invoke.mock.calls.map((c) => c[0])).toContain('cancel_run');
    finish({ id: 'x', tool_id: 't', status: 'cancelled', created_at: 0 });
    expect((await h.done).status).toBe('cancelled');
  });
});
