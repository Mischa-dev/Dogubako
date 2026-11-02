import { useEffect, useMemo, useRef } from 'react';
import { Stage } from './Stage';
import { toolLayout, useStore } from '../lib/store';
import { Zap } from 'lucide-react';
import type { ToolInfo } from '../lib/types';
import { ParamForm } from '../form/ParamForm';
import { ResultView } from '../results/ResultView';
import { useToolForm, type ToolForm } from './useToolForm';
import { RunBar } from './RunBar';
import { Spinner, cx } from '../components/ui';
import { useFileAdder } from '../form/FileField';
import type { FileItem } from '../lib/schema';

export const MONO_CATEGORIES = new Set(['dev', 'data', 'network']);

/** Ctrl/Cmd+Enter runs; paste of files anywhere on the page goes to the first file param. */
export function useToolShortcuts(form: ToolForm, enabled = true) {
  const fileParam = form.tool.params.find((p) => p.type === 'file') as (ToolInfo['params'][number] & { type: 'file' }) | undefined;
  const add = useFileAdder((fileParam ?? { name: '', label: '', type: 'file' }) as any, form.setValue);
  const formRef = useRef(form);
  formRef.current = form;
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
        e.preventDefault();
        if (!formRef.current.job.running && formRef.current.tool.availability.available) void formRef.current.run();
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      if (!fileParam) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || /^(INPUT|TEXTAREA)$/.test(t.tagName))) return;
      if (t?.closest('.file-field')) return; // handled locally
      const files = Array.from(e.clipboardData?.files ?? []);
      if (files.length) { e.preventDefault(); void add(files); }
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('paste', onPaste);
    return () => { window.removeEventListener('keydown', onKey); window.removeEventListener('paste', onPaste); };
  }, [enabled, fileParam, add]);
}

/** Debounced auto-run for live tools. */
/** Whether the user has provided some input (text, file or JSON) — avoids auto-running on an empty form. */
export function hasUserInput(form: ToolForm): boolean {
  const inputs = form.tool.params.filter((p) => p.type === 'text' || p.type === 'file' || p.type === 'json' || p.type === 'secret');
  if (!inputs.length) return true;
  return inputs.some((p) => {
    const v = form.values[p.name];
    if (p.type === 'file') return Array.isArray(v) && v.some((i: FileItem) => i.status === 'ready');
    return typeof v === 'string' ? v.trim() !== '' && v !== (p.default ?? '') : v != null;
  });
}

export function useLiveRun(form: ToolForm, enabled: boolean, delay = 250) {
  const first = useRef(true);
  useEffect(() => {
    if (!enabled) return;
    if (!form.valid || !hasUserInput(form)) return;
    const t = window.setTimeout(() => { void form.run({ silent: true, live: true }); }, first.current ? 50 : delay);
    first.current = false;
    return () => window.clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [form.values, enabled, form.valid]);
}

export function GenericTool({ tool, jobId, head, meta }: { tool: ToolInfo; jobId?: string | null; head?: React.ReactNode; meta?: React.ReactNode }) {
  const form = useToolForm(tool, { jobId });
  const live = tool.ui === 'live';
  const available = tool.availability.available;
  useToolShortcuts(form);
  useLiveRun(form, live && available);
  const mono = MONO_CATEGORIES.has(tool.category);
  const resultsRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const stageShown = useRef(false);

  // After a manual (non-live) run finishes, bring results into view.
  const lastStatus = useRef<string | undefined>();
  useEffect(() => {
    const s = form.job.job?.status;
    if (!live && s && s !== lastStatus.current && (s === 'succeeded' || s === 'failed')) {
      // With a stage, the result shows next to the original: bring that into view rather than the detail below.
      const target = stageShown.current ? stageRef.current : resultsRef.current;
      target?.scrollIntoView({ behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth', block: stageShown.current ? 'start' : 'nearest' });
    }
    lastStatus.current = s;
  }, [form.job.job?.status, live]);

  const noParams = tool.params.length === 0;
  const result = form.job.job;
  const layout = useStore(toolLayout);

  // The files this tool is working on, for the stage and the tool row.
  const fileParam = tool.params.find((p) => p.type === 'file');
  const fileValue = fileParam ? form.values[fileParam.name] : undefined;
  const files = useMemo(() => (Array.isArray(fileValue) ? (fileValue as FileItem[]).filter((f) => f.status === 'ready') : []), [fileValue]);
  const hasStage = files.length > 0;
  stageShown.current = hasStage;

  const resultNode = useMemo(() => result ? <ResultView job={result} tool={tool} input={form.input} live={live} collapseArtifacts={hasStage} /> : null, [result, tool, form.input, live, hasStage]);

  return (
    <div className={cx('tool-body tool-work', `place-${layout.controls}`, hasStage && 'has-stage', live && 'tool-live')}>
      {hasStage && (
        <div className="tool-stage" ref={stageRef}>
          <Stage files={files} job={result} running={form.job.running} live={live} mode={layout.preview} />
        </div>
      )}
      <section className="tool-input tool-controls" aria-label="Settings">
        {head}
        {noParams ? <p className="muted">This tool has no options.</p> : (
          <ParamForm params={tool.params} values={form.values} errors={form.shownErrors} onChange={form.setValue} mono={mono} />
        )}
        <RunBar
          form={form}
          label={live ? 'Run now' : 'Run'}
          disabled={!available}
          disabledReason={!available ? 'Unavailable until the missing dependency is installed' : undefined}
        />
        {meta}
      </section>
      <div className="tool-output" ref={resultsRef} aria-label="Result">
        {live && !result && (
          <div className="live-placeholder">
            {form.job.running ? <Spinner /> : <Zap size={16} aria-hidden />}
            <span>{form.job.running ? 'Working…' : 'Results appear here as you type.'}</span>
          </div>
        )}
        {live && form.job.running && result && <div className="live-busy" aria-hidden><Spinner size={12} /></div>}
        {resultNode}
      </div>
    </div>
  );
}
