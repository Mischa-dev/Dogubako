import { useCallback, useEffect, useRef, useState } from 'react';
import type { Backend, RunHandle } from '../lib/backend';
import { ApiError } from '../lib/backend';
import type { JobView } from '../lib/types';

export interface JobState {
  job: JobView | null;
  running: boolean;
  startedAt: number | null;
}

/** Runs tools and tracks the latest job. Stale runs (superseded) are ignored. */
export function useJob(backend: Backend) {
  const [state, setState] = useState<JobState>({ job: null, running: false, startedAt: null });
  const handle = useRef<RunHandle | null>(null);
  const seq = useRef(0);

  useEffect(() => () => { seq.current++; void handle.current?.cancel(); }, []);

  const run = useCallback(async (toolId: string, params: Record<string, unknown>, opts: { wait?: number; supersede?: boolean } = {}): Promise<JobView | null> => {
    const my = ++seq.current;
    if (opts.supersede && handle.current) void handle.current.cancel();
    const startedAt = Date.now();
    setState((s) => ({ job: opts.supersede ? s.job : null, running: true, startedAt }));
    const h = backend.run(toolId, params, {
      wait: opts.wait,
      onUpdate: (j) => {
        if (my !== seq.current) return;
        if (opts.supersede && j.status !== 'succeeded' && j.status !== 'failed') return; // keep last result visible while live-updating
        setState({ job: j, running: !['succeeded', 'failed', 'cancelled'].includes(j.status), startedAt });
      },
    });
    handle.current = h;
    let final: JobView;
    try {
      final = await h.done;
    } catch (e) {
      const err = e instanceof ApiError ? e.toToolError() : { code: 'failed', message: e instanceof Error ? e.message : String(e) };
      final = { id: '', tool_id: toolId, status: 'failed', error: err, created_at: Math.floor(startedAt / 1000), params };
    }
    if (my !== seq.current) return null;
    handle.current = null;
    setState({ job: final, running: false, startedAt });
    return final;
  }, [backend]);

  const cancel = useCallback(async () => {
    await handle.current?.cancel();
  }, []);

  const reset = useCallback(() => {
    seq.current++;
    setState({ job: null, running: false, startedAt: null });
  }, []);

  const setJob = useCallback((job: JobView | null) => setState({ job, running: false, startedAt: null }), []);

  return { ...state, run, cancel, reset, setJob };
}
