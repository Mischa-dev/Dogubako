import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ToolInfo, JobView } from '../lib/types';
import { useApp } from '../lib/app';
import {
  defaultValues, fromRunParams, fileItemFromRef, persistableValues, toRunParams, validate,
  type FileItem, type FormErrors, type FormValues,
} from '../lib/schema';
import { clearToolParams, loadToolParams, pendingInput, pendingParams, prefs, saveToolParams } from '../lib/store';
import { acceptMatches } from '../form/FileField';
import { useStillImage } from '../results/MediaPreview';
import type { Setter } from '../form/Field';
import type { InputContext } from '../results/DataViews';
import { useJob } from './useJob';
import { setToolSubject } from './subject';

export interface ToolForm {
  tool: ToolInfo;
  values: FormValues;
  setValue: Setter;
  setValues: React.Dispatch<React.SetStateAction<FormValues>>;
  errors: FormErrors;
  /** Errors to display: fields the user changed, every field after an attempted run, plus server-side errors. */
  shownErrors: FormErrors;
  valid: boolean;
  /** The form differs from a fresh one (values, a result, or errors on screen): Reset has something to clear. */
  dirty: boolean;
  /** The result on screen no longer matches the form, whose current values are invalid. */
  stale: boolean;
  /** Changes on every reset, so form chrome (Advanced) can return to its initial state. */
  resetKey: number;
  job: ReturnType<typeof useJob>;
  run: (opts?: { silent?: boolean; live?: boolean; overrides?: Record<string, unknown>; toolId?: string }) => Promise<JobView | null>;
  reset: () => void;
  input: InputContext;
}

function initialValues(tool: ToolInfo): FormValues {
  const base = defaultValues(tool);
  const pending = pendingInput.value;
  if (pending) {
    const list = Array.isArray(pending) ? pending : [pending];
    const fp = tool.params.find((p) => p.type === 'file' && list.some((f) => acceptMatches(p.accept, f.name, f.mime)));
    if (fp && fp.type === 'file') {
      const ok = list.filter((f) => acceptMatches(fp.accept, f.name, f.mime));
      base[fp.name] = (fp.multiple ? ok : ok.slice(0, 1)).map(fileItemFromRef);
      pendingInput.value = null;
    }
  }
  if (prefs.get().rememberParams) {
    const saved = loadToolParams(tool.id);
    if (saved) {
      for (const p of tool.params) {
        if (p.type !== 'file' && p.type !== 'secret' && p.name in saved) base[p.name] = saved[p.name];
      }
    }
  }
  // Natural-language mode opened this tool with some settings already chosen.
  const pre = pendingParams.value;
  if (pre && pre.toolId === tool.id) {
    for (const p of tool.params) {
      if (p.type !== 'file' && p.type !== 'secret' && p.type !== 'path' && p.name in pre.params) base[p.name] = pre.params[p.name];
    }
    pendingParams.value = null;
  }
  return base;
}

/**
 * `provided`: params the tool page supplies itself at run time (via `run` overrides), so they are never validated as form fields.
 * `check`: extra validation a tool page adds on top of the spec's (rules that depend on several values).
 */
export function useToolForm(
  tool: ToolInfo,
  opts: { jobId?: string | null; provided?: readonly string[]; check?: (values: FormValues) => FormErrors } = {},
): ToolForm {
  const { backend } = useApp();
  const [values, setValues] = useState<FormValues>(() => initialValues(tool));
  const [attempted, setAttempted] = useState(false);
  const [touched, setTouched] = useState<ReadonlySet<string>>(() => new Set());
  const [resetKey, setResetKey] = useState(0);
  const [serverErrors, setServerErrors] = useState<FormErrors>({});
  const job = useJob(backend);
  const { setJob, run: runJob, reset: resetJob } = job;

  const setValue: Setter = useCallback((name, v) => {
    setValues((prev) => ({ ...prev, [name]: typeof v === 'function' ? (v as (p: unknown) => unknown)(prev[name]) : v }));
    setServerErrors((e) => (e[name] ? Object.fromEntries(Object.entries(e).filter(([k]) => k !== name)) : e));
    setTouched((t) => (t.has(name) ? t : new Set(t).add(name)));
  }, []);

  const providedKey = (opts.provided ?? []).join('\n');
  const checkRef = useRef(opts.check);
  checkRef.current = opts.check;
  const validateAll = useCallback((vals: FormValues) => ({ ...(checkRef.current?.(vals) ?? {}), ...validate(tool, vals) }), [tool]);
  const clientErrors = useMemo(() => {
    const errs = validateAll(values);
    for (const k of providedKey ? providedKey.split('\n') : []) delete errs[k];
    return errs;
  }, [validateAll, values, providedKey]);
  // Tools with `prechecks` look at the chosen inputs (e.g. a video's codecs) before Run, for problems the spec cannot express.
  const clientValid = Object.keys(clientErrors).length === 0;
  const precheckKey = useMemo(
    () => (tool.prechecks && backend.check && clientValid && !providedKey ? JSON.stringify(toRunParams(tool, values)) : null),
    [tool, backend, clientValid, providedKey, values],
  );
  const [precheck, setPrecheck] = useState<{ key: string; errors: FormErrors } | null>(null);
  useEffect(() => {
    if (!precheckKey || !backend.check) return;
    let alive = true;
    const t = setTimeout(() => {
      backend.check!(tool.id, JSON.parse(precheckKey)).then((e) => {
        if (!alive) return;
        const errs: FormErrors = e?.code === 'invalid_input' && e.param && tool.params.some((p) => p.name === e.param) ? { [e.param]: e.message ?? 'Invalid value' } : {};
        setPrecheck({ key: precheckKey, errors: errs });
      }, () => undefined);
    }, 300);
    return () => { alive = false; clearTimeout(t); };
  }, [precheckKey, backend, tool]);
  const precheckErrors = useMemo(() => (precheck && precheck.key === precheckKey ? precheck.errors : {}), [precheck, precheckKey]);
  const precheckRef = useRef(precheckErrors);
  precheckRef.current = precheckErrors;
  const errors = useMemo(() => ({ ...precheckErrors, ...clientErrors }), [precheckErrors, clientErrors]);
  // Tool checks span several fields, so their errors show even when only another field changed.
  const checkKeys = useMemo(() => new Set(Object.keys(checkRef.current?.(values) ?? {})), [values]);
  const valid = Object.keys(errors).length === 0;
  const shownErrors = useMemo(() => {
    const client = attempted ? errors : Object.fromEntries(Object.entries(errors).filter(([k]) => touched.has(k) || checkKeys.has(k) || k in precheckErrors));
    return { ...client, ...serverErrors };
  }, [attempted, touched, errors, serverErrors, checkKeys, precheckErrors]);
  const defaults = useMemo(() => defaultValues(tool), [tool]);
  const dirty = !!job.job || Object.keys(shownErrors).length > 0
    || tool.params.some((p) => JSON.stringify(values[p.name] ?? null) !== JSON.stringify(defaults[p.name] ?? null));
  const stale = !!job.job && !job.running && !valid;

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
    const errs = { ...precheckRef.current, ...validateAll(vals) };
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
  }, [tool, runJob, validateAll]);

  const reset = useCallback(() => {
    setValues(defaultValues(tool));
    setAttempted(false);
    setTouched(new Set());
    setServerErrors({});
    setResetKey((k) => k + 1);
    clearToolParams(tool.id);
    resetJob();
  }, [tool, resetJob]);

  const inputFile = useMemo(() => {
    for (const p of tool.params) {
      if (p.type !== 'file') continue;
      const it = ((values[p.name] as FileItem[]) ?? []).find((i) => i.status === 'ready' && i.mime.startsWith('image/') && i.mime !== 'image/svg+xml');
      if (it) return { url: it.localUrl ?? backend.fileUrl({ ref: it.ref!, name: it.name, size: it.size, mime: it.mime }), mime: it.mime, name: it.name };
    }
    return undefined;
  }, [tool, values, backend]);
  // TIFF/TGA/QOI/Netpbm inputs are redrawn as PNG so compare, crop and overlays can use them.
  const { src: imageUrl } = useStillImage(inputFile?.url, inputFile?.mime ?? '', inputFile?.name ?? '');
  const input = useMemo<InputContext>(() => {
    const textParam = tool.params.find((p) => p.name === 'text' && p.type === 'text') ?? tool.params.find((p) => p.type === 'text' && p.multiline);
    const text = textParam ? String(values[textParam.name] ?? '') : undefined;
    return { imageUrl, text };
  }, [tool, values, imageUrl]);

  // Tell the tool row what this tool is working on (its files, and what came out).
  const firstFile = tool.params.find((p) => p.type === 'file');
  const fileValue = firstFile ? values[firstFile.name] : undefined;
  const result = job.job;
  useEffect(() => {
    const items = Array.isArray(fileValue) ? (fileValue as FileItem[]).filter((f) => f.status === 'ready' && f.ref) : [];
    const arts = result?.status === 'succeeded' ? result.result?.artifacts ?? [] : [];
    setToolSubject({
      toolId: tool.id,
      files: items.map((f) => ({ ref: f.ref!, name: f.name, size: f.size, mime: f.mime })),
      results: arts.flatMap((a) => (a.id ?? a.path ? [{ ref: (a.id ?? a.path)!, name: a.name, size: a.size, mime: a.mime }] : [])),
    });
  }, [tool.id, fileValue, result]);
  useEffect(() => () => setToolSubject(null), []);

  return { tool, values, setValue, setValues, errors, shownErrors, valid, dirty, stale, resetKey, job, run, reset, input };
}
