import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ArrowLeft, ArrowRight, Ban, Trash2, Undo2, ChevronRight, CircleAlert, CircleCheck, CircleDashed, CornerDownLeft, FileText, Hand, Settings2, Sparkles, Square,
} from 'lucide-react';
import { useApp } from '../lib/app';
import { href, navigate } from '../lib/router';
import { droppedNow, pendingInput, pendingParams, prefs } from '../lib/store';
import { ApiError } from '../lib/backend';
import { toast } from '../components/toast';
import { matchIntent, shortHint } from '../lib/intent';
import { errorText } from '../lib/errors';
import { formatBytes } from '../lib/format';
import { changesFiles, outputsOf, redactForStorage, runAgent, type AgentFile, type ChatMessage, type Step } from '../lib/assistant/agent';
import { targetFor } from '../lib/assistant/settings';
import { useAssistant } from '../lib/assistant/useAssistant';
import type { Artifact, ToolInfo } from '../lib/types';
import { ArtifactList } from '../results/Artifacts';
import { ResultView } from '../results/ResultView';
import { ModelChip } from '../components/ModelPicker';
import { Button, Callout, ProgressBar, Spinner, cx } from '../components/ui';

interface Turn {
  request: string;
  steps: Step[];
  reply?: string;
  error?: string;
  done: boolean;
}

/** Short "key: value" list for a step, with file refs shown as file names. */
function paramSummary(params: Record<string, unknown>, names: Map<string, string>): string {
  const show = (v: unknown): string => {
    if (Array.isArray(v)) return v.length > 2 ? `${v.length} files` : v.map(show).join(', ');
    if (typeof v === 'string') return names.get(v) ?? (v.length > 32 ? `${v.slice(0, 31)}…` : v);
    if (v && typeof v === 'object') return '…';
    return String(v);
  };
  return Object.entries(params)
    .filter(([, v]) => v !== null && v !== undefined && v !== '')
    .slice(0, 4)
    .map(([k, v]) => `${k.replace(/_/g, ' ')}: ${show(v)}`)
    .join(' · ');
}

function fullValue(v: unknown, names: Map<string, string>): string {
  if (Array.isArray(v)) return v.map((x) => fullValue(x, names)).join(', ');
  if (typeof v === 'string') return names.has(v) ? `${names.get(v)} (${v})` : v;
  return JSON.stringify(v);
}

/** Finished requests are kept for this tab, so coming Back doesn't run them again. */
interface Saved { turns: Turn[]; history: ChatMessage[]; undone?: boolean; partial?: boolean }
const savedKey = (k: string) => `dgb.do.${k}`;
function loadSaved(k: string): Saved | null {
  if (!k) return null;
  try {
    const v = JSON.parse(sessionStorage.getItem(savedKey(k)) ?? 'null');
    return v && Array.isArray(v.turns) && Array.isArray(v.history) ? v : null;
  } catch { return null; }
}
function save(k: string, v: Saved) {
  if (!k) return;
  try { sessionStorage.setItem(savedKey(k), JSON.stringify(v)); } catch { /* storage full or off: Back reruns */ }
}

function StepRow({ step, tool, names, open, onToggle, onAnswer }: {
  step: Step; tool?: ToolInfo; names: Map<string, string>; open: boolean; onToggle: () => void; onAnswer: (ok: boolean) => void;
}) {
  const job = step.job;
  const name = tool?.name ?? step.toolId;
  const Icon = step.status === 'done' ? CircleCheck : step.status === 'failed' ? CircleAlert : step.status === 'skipped' ? Ban : step.status === 'asking' ? Hand : CircleDashed;
  const canOpen = !!job && (job.status === 'succeeded' || job.status === 'failed') && !!tool;
  return (
    <li className={cx('do-step', `is-${step.status}`)}>
      <div className="do-step-head">
        <span className="do-step-icon" aria-hidden>{step.status === 'running' ? <Spinner size={14} /> : <Icon size={15} />}</span>
        <span className="do-step-text">
          <span className="do-step-name">
            {tool ? <a href={href.tool(tool.id)}>{name}</a> : name}
            {step.status === 'skipped' && <span className="muted"> · skipped</span>}
          </span>
          <span className="do-step-params">{step.problem ?? (job?.status === 'failed' ? job.error?.message ?? 'Failed' : paramSummary(step.params, names))}</span>
        </span>
        {canOpen && (
          <button type="button" className="do-step-toggle" aria-expanded={open} onClick={onToggle}>
            {open ? 'Hide' : 'Details'} <ChevronRight size={13} aria-hidden className={cx(open && 'is-open')} />
          </button>
        )}
      </div>
      {step.status === 'running' && job?.progress != null && (
        <div className="do-step-progress"><ProgressBar value={job.progress} label={`${name} progress`} />{job.message && <span className="muted small">{job.message}</span>}</div>
      )}
      {step.status === 'asking' && (
        <div className="do-step-ask" role="group" aria-label={`Allow ${name}?`}>
          {/* Show every value in full: this is what you're agreeing to. */}
          <dl className="do-step-full">
            {Object.entries(step.params).map(([k, v]) => (
              <div key={k}><dt>{k.replace(/_/g, ' ')}</dt><dd className="mono">{fullValue(v, names)}</dd></div>
            ))}
          </dl>
          <span className="small">{step.reason}. Run it?</span>
          <Button size="sm" variant="primary" onClick={() => onAnswer(true)}>Run</Button>
          <Button size="sm" variant="ghost" onClick={() => onAnswer(false)}>Skip</Button>
        </div>
      )}
      {open && job && tool && <div className="do-step-result"><ResultView job={job} tool={tool} collapseArtifacts /></div>}
    </li>
  );
}

/** A plain-words request, carried out in place: the steps the model took and what came out. */
export default function DoPage({ request, withFiles, runKey = '' }: { request: string; withFiles: boolean; runKey?: string }) {
  const app = useApp();
  const { backend, toolMap, visibleTools, docs } = app;
  const assistant = useAssistant();
  const [files] = useState<AgentFile[]>(() => (withFiles ? droppedNow() : []));
  const [saved] = useState(() => loadSaved(runKey));
  // A request interrupted by leaving the page shows as stopped, not stuck.
  const [turns, setTurns] = useState<Turn[]>(() => (saved?.turns ?? []).map((t) => (t.done ? t : { ...t, done: true, error: 'Stopped.' })));
  const [thinking, setThinking] = useState(false);
  const [running, setRunning] = useState(false);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [more, setMore] = useState('');
  const history = useRef<ChatMessage[]>(saved?.history ?? []);
  const abort = useRef<AbortController | null>(null);
  const answers = useRef(new Map<string, (ok: boolean) => void>());
  const started = useRef(!!saved);
  // Kept for this tab only when History is on, with passwords and keys masked.
  useEffect(() => {
    if (!prefs.get().history) return;
    const turnsSafe = turns.map((t) => ({ ...t, steps: redactForStorage(t.steps, [], toolMap).steps }));
    save(runKey, { turns: turnsSafe, history: redactForStorage([], history.current, toolMap).history });
  }, [runKey, turns, toolMap]);

  const names = useMemo(() => {
    const m = new Map<string, string>(files.map((f) => [f.ref, f.name]));
    for (const t of turns) for (const a of outputsOf(t.steps)) { const r = a.id ?? a.path; if (r) m.set(r, a.name); }
    return m;
  }, [files, turns]);

  const ask = useCallback(async (text: string) => {
    const req = text.trim();
    if (!req || running) return;
    const s = assistant.settings;
    const ctrl = new AbortController();
    abort.current = ctrl;
    setRunning(true);
    setTurns((ts) => [...ts, { request: req, steps: [], done: false }]);
    const patchTurn = (f: (t: Turn) => Turn) => setTurns((ts) => ts.map((t, i) => (i === ts.length - 1 ? f(t) : t)));
    try {
      const res = await runAgent({
        backend,
        target: targetFor(s),
        model: assistant.model,
        tools: visibleTools,
        docs,
        files,
        history: history.current,
        knownRefs: names.keys(),
        request: req,
        confirm: s.confirm,
        signal: ctrl.signal,
        onThinking: setThinking,
        approve: (step) => new Promise<boolean>((resolve) => {
          answers.current.set(step.id, resolve);
          ctrl.signal.addEventListener('abort', () => resolve(false), { once: true });
        }),
        onStep: (step) => patchTurn((t) => {
          const i = t.steps.findIndex((x) => x.id === step.id);
          const steps = i < 0 ? [...t.steps, step] : t.steps.map((x, j) => (j === i ? step : x));
          return { ...t, steps };
        }),
      });
      history.current = res.history;
      if (res.open && toolMap.has(res.open.toolId)) {
        // A request that names a tool or leaves a choice open: hand over to
        // the tool page, files and certain settings filled in. Replacing the
        // history entry keeps Back pointing at where the words were typed.
        if (files.length) pendingInput.value = files;
        pendingParams.value = { toolId: res.open.toolId, params: res.open.params };
        if (res.open.note) toast(res.open.note);
        navigate(href.tool(res.open.toolId), { replace: true });
        return;
      }
      patchTurn((t) => ({ ...t, reply: res.reply, done: true }));
      // Open the result of a last step that produced text or data but no files.
      const last = [...res.steps].reverse().find((x) => x.status === 'done');
      if (last && !(last.job?.result?.artifacts?.length)) setOpen((o) => ({ ...o, [last.id]: true }));
    } catch (e) {
      const cancelled = (e as Error)?.name === 'AbortError';
      patchTurn((t) => ({ ...t, error: cancelled ? 'Stopped.' : errorText(e), done: true }));
    } finally {
      setRunning(false);
      setThinking(false);
      abort.current = null;
    }
  }, [assistant.settings, assistant.model, backend, visibleTools, docs, files, running, names, toolMap]);

  // Start right away with the words typed in the search or on the drop screen.
  useEffect(() => {
    if (started.current || !assistant.info || !assistant.on || assistant.missing) return;
    started.current = true;
    void ask(request);
  }, [assistant.info, assistant.on, assistant.missing, ask, request]);

  useEffect(() => () => abort.current?.abort(), []);

  // Undo: most tools never change the originals, so taking back a request
  // means removing what it produced (outputs and history entries). When a step
  // did change files, this only removes results and says so.
  const [undoing, setUndoing] = useState(false);
  const undo = async () => {
    setUndoing(true);
    // Only runs that actually started have a job to discard.
    // (A run that failed to even start has a stand-in job named after the step.)
    // Steps that changed the person's files keep their History entry: it is
    // the record of what changed.
    const jobs = turns.flatMap((t) => t.steps.flatMap((s) => (s.job && s.job.id !== s.id && !changesFiles(s, toolMap) ? [s.job] : [])));
    let failed = 0;
    for (const j of jobs) {
      try {
        await backend.discardRun(j.id);
      } catch (e) {
        // Already gone counts as undone.
        if (!(e instanceof ApiError && e.status === 404)) failed++;
      }
    }
    setUndoing(false);
    if (failed) {
      toast(`Couldn’t remove ${failed} of ${jobs.length} results. Check History.`, 'error');
      return;
    }
    save(runKey, { turns: [], history: [], undone: true, partial: !canUndo });
    toast.success(canUndo ? 'Undone: the new files were removed' : 'Results removed. Changes to your files stay as they are.');
    window.history.length > 1 ? window.history.back() : navigate(href.home);
  };

  const outputs: Artifact[] = useMemo(() => turns.flatMap((t) => outputsOf(t.steps)), [turns]);
  // Steps that changed things outside Dogubako's own outputs (renamed or
  // deleted files, wrote to folders). Removing results can't reverse them.
  const changed = useMemo(() => turns.flatMap((t) => t.steps.filter((st) => changesFiles(st, toolMap)).map((st) => toolMap.get(st.toolId)!.name)), [turns, toolMap]);
  const canUndo = changed.length === 0;
  const suggestions = useMemo(
    () => matchIntent(visibleTools, docs, request).slice(0, 5),
    [visibleTools, docs, request],
  );

  const openTool = (t: ToolInfo) => {
    if (files.length) pendingInput.value = files;
    navigate(href.tool(t.id));
  };

  const back = (
    <a className="link-sm intent-back" href={href.home} onClick={(e) => { e.preventDefault(); window.history.length > 1 ? window.history.back() : navigate(href.home); }}>
      <ArrowLeft size={13} aria-hidden /> Back
    </a>
  );

  // Not set up: say what's missing, and still offer the tools the words point to.
  const blocked = assistant.info && (!assistant.on || assistant.missing);
  if (blocked) {
    return (
      <div className="page page-do">
        {back}
        <h1 className="do-request" tabIndex={-1} data-route-focus>“{request}”</h1>
        <Callout
          tone="info"
          icon={Sparkles}
          title={assistant.on ? assistant.missing : 'Let a model do this for you'}
          actions={<Button size="sm" variant="primary" icon={Settings2} onClick={() => navigate(href.settings('assistant'))}>{assistant.on ? 'Finish setup' : 'Set up'}</Button>}
        >
          {assistant.on
            ? 'Natural-language mode needs a model before it can run tools for you.'
            : 'Turn on natural-language mode and pick a model (OpenAI, Anthropic, a local Ollama model or any compatible endpoint), and requests like this one run by themselves.'}
        </Callout>
        {suggestions.length > 0 && (
          <section className="do-fallback" aria-labelledby="do-fallback-h">
            <h2 id="do-fallback-h" className="module-title">Or pick a tool</h2>
            <ul className="intent-list">
              {suggestions.map((t) => (
                <li key={t.id}>
                  <button type="button" className="intent-option" onClick={() => openTool(t)}>
                    <span className="intent-option-text"><span className="intent-option-name">{t.name}</span><span className="intent-option-hint">{shortHint(t)}</span></span>
                    <ArrowRight size={15} className="intent-go" aria-hidden />
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    );
  }

  const answer = (id: string, ok: boolean) => {
    answers.current.get(id)?.(ok);
    answers.current.delete(id);
  };

  return (
    <div className="page page-do">
      <div className="do-top">
        {back}
        <ModelChip />
      </div>
      {files.length > 0 && (
        <div className="do-files" aria-label="Files">
          {files.slice(0, 6).map((f) => <span key={f.ref} className="do-file"><FileText size={12} aria-hidden />{f.name}<span className="muted">{formatBytes(f.size)}</span></span>)}
          {files.length > 6 && <span className="do-file muted">+{files.length - 6} more</span>}
        </div>
      )}

      {saved?.undone && turns.length === 0 && (
        <>
          <h1 className="do-request" tabIndex={-1} data-route-focus>“{request}”</h1>
          <p className="muted">{saved.partial ? 'You removed this request’s result files. Changes it made to your files stay; they’re listed in History.' : 'You undid this request; what it made was removed.'}</p>
        </>
      )}
      {turns.map((t, ti) => (
        <section key={ti} className="do-turn" aria-label={t.request}>
          {ti === 0
            ? <h1 className="do-request" tabIndex={-1} data-route-focus>“{t.request}”</h1>
            : <h2 className="do-request is-follow">“{t.request}”</h2>}
          {t.steps.length > 0 && (
            <ol className="do-steps">
              {t.steps.map((s) => (
                <StepRow key={s.id} step={s} tool={toolMap.get(s.toolId)} names={names} open={!!open[s.id]}
                  onToggle={() => setOpen((o) => ({ ...o, [s.id]: !o[s.id] }))} onAnswer={(ok) => answer(s.id, ok)} />
              ))}
            </ol>
          )}
          {!t.done && thinking && ti === turns.length - 1 && (
            <p className="do-thinking" role="status"><Sparkles size={14} aria-hidden className="do-spark" />{t.steps.length ? 'Working on the next step…' : 'Working out what to do…'}</p>
          )}
          {t.reply && <p className="do-reply">{t.reply}</p>}
          {t.error && (
            <Callout tone={t.error === 'Stopped.' ? 'info' : 'danger'} actions={t.error !== 'Stopped.' && (
              <>
                <Button size="sm" variant="ghost" icon={Settings2} onClick={() => navigate(href.settings('assistant'))}>Model settings</Button>
              </>
            )}>{t.error}</Callout>
          )}
        </section>
      ))}

      {outputs.length > 0 && (
        <section className="do-outputs" aria-labelledby="do-out-h">
          <div className="do-outputs-head">
            <h2 id="do-out-h" className="module-title">Results</h2>
            {!running && (
              canUndo
                ? <Button size="sm" variant="ghost" icon={Undo2} loading={undoing} onClick={() => void undo()} title="Remove what this made and go back. Your original files were never changed.">Undo</Button>
                : <Button size="sm" variant="ghost" icon={Trash2} loading={undoing} onClick={() => void undo()} title="Remove the new result files. This doesn’t reverse changes made to your files.">Remove results</Button>
            )}
          </div>
          <p className="muted small do-originals">
            {canUndo
              ? 'Your original files weren’t changed. Every step above is in History too.'
              : `${[...new Set(changed)].join(', ')} changed files on this computer. That can’t be undone from here; every step is in History.`}
          </p>
          <ArtifactList artifacts={outputs} />
        </section>
      )}

      <form
        className="do-more"
        onSubmit={(e) => { e.preventDefault(); if (running) return; const v = more; setMore(''); void ask(v); }}
      >
        <Sparkles size={15} aria-hidden className="do-more-icon" />
        <input
          className="do-more-input"
          value={more}
          onChange={(e) => setMore(e.target.value)}
          placeholder={turns.length ? 'Anything else? Like “make them black and white too”' : 'Say what you want done'}
          aria-label="Ask for something else"
          autoComplete="off"
          disabled={!assistant.info}
        />
        {running
          ? <Button size="sm" variant="ghost" icon={Square} onClick={() => abort.current?.abort()}>Stop</Button>
          : <Button size="sm" variant="secondary" type="submit" icon={CornerDownLeft} disabled={!more.trim()}>Do it</Button>}
      </form>
    </div>
  );
}
