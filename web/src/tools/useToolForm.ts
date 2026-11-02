import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ToolInfo, JobView } from '../lib/types';
import { useApp } from '../lib/app';
import {
  defaultValues, fromRunParams, fileItemFromRef, persistableValues, toRunParams, validate,
  type FileItem, type FormErrors, type FormValues,
} from '../lib/schema';
import { loadToolParams, pendingInput, prefs, saveToolParams } from '../lib/store';
import { acceptMatches } from '../form/FileField';
import type { Setter } from '../form/Field';
import type { InputContext } from '../results/DataViews';
import { useJob } from './useJob';

export interface ToolForm {
  tool: ToolInfo;
  values: FormValues;
  setValue: Setter;
  setValues: React.Dispatch<React.SetStateAction<FormValues>>;
  errors: FormErrors;
  /** Errors to display (after an attempted run, plus server-side errors). */
  shownErrors: FormErrors;
  valid: boolean;
  job: ReturnType<typeof useJob>;
  run: (opts?: { silent?: boolean; live?: boolean; overrides?: Record<string, unknown>; toolId?: string }) => Promise<JobView | null>;
  reset: () => void;
  input: InputContext;
}

function initialValues(tool: ToolInfo): FormValues {
  const base = defaultValues(tool);
  const pending = pendingInput.value;
  if (pending) {
    const fp = tool.params.find((p) => p.type === 'file' && acceptMatches(p.accept, pending.name, pending.mime));
    if (fp) { base[fp.name] = [fileItemFromRef(pending)]; pendingInput.value = null; }
  }
  if (prefs.get().rememberParams) {
    const saved = loadToolParams(tool.id);
    if (saved) {
      for (const p of tool.params) {
        if (p.type !== 'file' && p.type !== 'secret' && p.name in saved) base[p.name] = saved[p.name];
      }
    }
  }
  return base;
}

export function useToolForm(tool: ToolInfo, opts: { jobId?: string | null } = {}): ToolForm {
  const { backend } = useApp();
  const [values, setValues] = useState<FormValues>(() => initialValues(tool));
  const [attempted, setAttempted] = useState(false);
  const [serverErrors, setServerErrors] = useState<FormErrors>({});
  const job = useJob(backend);
  const { setJob, run: runJob, reset: resetJob } = job;

  const setValue: Setter = useCallback((name, v) => {
    setValues((prev) => ({ ...prev, [name]: typeof v === 'function' ? (v as (p: unknown) => unknown)(prev[name]) : v }));
    setServerErrors((e) => (e[name] ? Object.fromEntries(Object.entries(e).filter(([k]) => k !== name)) : e));
  }, []);

  const errors = useMemo(() => validate(tool, values), [tool, values]);
  const valid = Object.keys(errors).length === 0;
  const shownErrors = useMemo(() => ({ ...(attempted ? errors : {}), ...serverErrors }), [attempted, errors, serverErrors]);

  // Re-open a past job with the same parameters.
  const loadedJob = useRef<string | null>(null);
  useEffect(() => {
    const id = opts.jobId;
    if (!id || loadedJob.current === id) return;
    loadedJob.current = id;
    let alive = true;
    let finished = false;
    (async () => {
      let j: JobView | undefined;
      if (backend.mode === 'server') {
        try { j = await (backend as any).get(`/jobs/${encodeURIComponent(id)}`); } catch { j = undefined; }
      } else {
        j = (await backend.listJobs()).find((x) => x.id === id);
      }
      if (!alive || !j) return;
      const { values: v, fileRefs } = fromRunParams(tool, (j.params as Record<string, unknown>) ?? {});
      for (const [name, refs] of Object.entries(fileRefs)) {
        const items: FileItem[] = [];
        for (const r of refs) {
          try { items.push(fileItemFromRef(await backend.describeFile(r))); }
          catch { items.push({ key: r, name: r, size: 0, mime: '', status: 'error', error: 'File is no longer available' }); }
        }
        v[name] = items;
      }
      if (!alive) return;
      finished = true;
      setValues(v);
      setJob(j);
    })();
    return () => {
      alive = false;
      // Interrupted before it finished (e.g. StrictMode's mount/unmount/
      // mount in development): let the next run of this effect retry.
      if (!finished && loadedJob.current === id) loadedJob.current = null;
    };
  }, [opts.jobId, backend, tool, setJob]);

  const valuesRef = useRef(values);
  valuesRef.current = values;

  const run = useCallback<ToolForm['run']>(async (o = {}) => {
    const vals = valuesRef.current;
    const errs = validate(tool, vals);
    for (const k of Object.keys(o.overrides ?? {})) delete errs[k];
    if (Object.keys(errs).length) {
      if (!o.silent) {
        setAttempted(true);
        requestAnimationFrame(() => {
          const el = document.querySelector<HTMLElement>(`[data-param="${Object.keys(errs)[0]}"] :is(input,textarea,select,[role=button],button)`);
          el?.focus();
        });
      }
      return null;
    }
    setServerErrors({});
    const params = { ...toRunParams(tool, vals), ...(o.overrides ?? {}) };
    if (prefs.get().rememberParams) saveToolParams(tool.id, persistableValues(tool, vals));
    const result = await runJob(o.toolId ?? tool.id, params, o.live ? { wait: 15, supersede: true } : {});
    const e = result?.error;
    if (e?.code === 'invalid_input' && e.param && tool.params.some((p) => p.name === e.param)) {
      setServerErrors({ [e.param]: e.message ?? 'Invalid value' });
      setAttempted(true);
    }
    return result;
  }, [tool, runJob]);

  const reset = useCallback(() => {
    setValues(defaultValues(tool));
    setAttempted(false);
    setServerErrors({});
    resetJob();
  }, [tool, resetJob]);

  const input = useMemo<InputContext>(() => {
    let imageUrl: string | undefined;
    for (const p of tool.params) {
      if (p.type !== 'file') continue;
      const it = ((values[p.name] as FileItem[]) ?? []).find((i) => i.status === 'ready' && i.mime.startsWith('image/') && i.mime !== 'image/svg+xml');
      if (it) {
        imageUrl = it.localUrl ?? backend.fileUrl({ ref: it.ref!, name: it.name, size: it.size, mime: it.mime });
        break;
      }
    }
    const textParam = tool.params.find((p) => p.name === 'text' && p.type === 'text') ?? tool.params.find((p) => p.type === 'text' && p.multiline);
    const text = textParam ? String(values[textParam.name] ?? '') : undefined;
    return { imageUrl, text };
  }, [tool, values, backend]);

  return { tool, values, setValue, setValues, errors, shownErrors, valid, job, run, reset, input };
}
