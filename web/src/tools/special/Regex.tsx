import { useMemo, useRef } from 'react';
import type { ParamSpec, ToolInfo } from '../../lib/types';
import type { Match } from '../../lib/renderers';
import { ParamForm } from '../../form/ParamForm';
import type { Setter } from '../../form/Field';
import { useToolForm } from '../useToolForm';
import { useLiveRun, useToolShortcuts } from '../GenericTool';
import { DataTable, KeyValue } from '../../results/DataViews';
import { ErrorView } from '../../results/ErrorView';
import { Warnings } from '../../results/Warnings';
import { CopyButton, Spinner, cx } from '../../components/ui';
import { ResetButton } from '../RunBar';
import { StaleNotice } from '../../results/Stale';


/** Multi-line pattern editor: grows with the pattern so a verbose (x) regex can be typed or pasted as is. */
function PatternField({ param, value, error, verbose, onChange }: { param: ParamSpec; value: unknown; error?: string; verbose: boolean; onChange: Setter }) {
  const s = String(value ?? '');
  const lines = s.split('\n').length;
  const hint = lines > 1 && !verbose ? 'Line breaks in the pattern match literally. Turn on the x – verbose flag to ignore whitespace and allow # comments.' : null;
  const placeholder = param.type === 'text' ? param.placeholder ?? undefined : undefined;
  const describedBy = [hint ? 'regex-pattern-hint' : '', error ? 'regex-pattern-err' : ''].filter(Boolean).join(' ') || undefined;
  return (
    <div className={cx('field', error && 'has-error')} data-param={param.name}>
      <div className="field-head">
        <label className="field-label" htmlFor="regex-pattern">
          {param.label}
          {param.required && <span className="req" aria-hidden> *</span>}
        </label>
      </div>
      <textarea
        id="regex-pattern"
        className="textarea mono code regex-pattern"
        rows={Math.min(Math.max(lines, 1), 12)}
        value={s}
        placeholder={placeholder}
        spellCheck={false}
        aria-describedby={describedBy}
        aria-invalid={error ? true : undefined}
        aria-required={param.required || undefined}
        onChange={(e) => onChange(param.name, e.target.value)}
      />
      {hint && <p className="field-desc" id="regex-pattern-hint">{hint}</p>}
      {error && <p className="field-error" id="regex-pattern-err" role="alert">{error}</p>}
    </div>
  );
}

/** Regex tester: highlights matches live under the test-text editor. */
export default function Regex({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const form = useToolForm(tool, { jobId });
  useToolShortcuts(form);
  useLiveRun(form, tool.availability.available, 200);
  const text = String(form.values.text ?? '');
  const pattern = String(form.values.pattern ?? '');
  // Live runs stop while the pattern or test text is empty: drop the last result rather than show it as current.
  const missing = pattern === '' ? 'pattern' : text === '' ? 'text' : null;
  const job = missing ? null : form.job.job;
  // Matches from earlier, now-invalid settings would highlight the wrong text.
  const data = (job?.status === 'succeeded' && !form.stale ? job.result?.data : null) as Record<string, any> | null;
  const matches: Match[] = Array.isArray(data?.matches) ? data!.matches : [];
  const hadResult = !!form.job.job;
  const patternError = missing === 'pattern' && (hadResult || form.shownErrors.pattern) ? 'Enter a pattern' : form.shownErrors.pattern;
  const textError = missing === 'text' && (hadResult || form.shownErrors.text) ? 'Enter some test text to match against' : form.shownErrors.text;
  // An empty field already explains the hidden result.
  const stale = form.stale && !missing;
  const flags = Array.isArray(form.values.flags) ? (form.values.flags as string[]) : [];
  const textParam = tool.params.find((p) => p.name === 'text');
  const patternParam = tool.params.find((p) => p.name === 'pattern');
  const mirror = useRef<HTMLPreElement>(null);

  const highlighted = useMemo(() => {
    const out: React.ReactNode[] = [];
    let i = 0;
    [...matches].sort((a, b) => a.start - b.start).forEach((m, k) => {
      if (m.start < i || m.start > text.length) return;
      out.push(text.slice(i, m.start));
      out.push(<mark key={k} className={cx('match', k % 2 === 1 && 'alt')}>{text.slice(m.start, m.end) || '​'}</mark>);
      i = m.end;
    });
    out.push(text.slice(i) + '\n');
    return out;
  }, [matches, text]);

  const groupNames: (string | null)[] = Array.isArray(data?.group_names) ? data!.group_names : [];
  const maxGroups = Math.max(0, ...matches.map((m) => m.groups?.length ?? 0));
  const rest = data ? Object.fromEntries(Object.entries(data).filter(([k]) => !['matches', 'group_names', 'replaced', 'table'].includes(k))) : null;

  return (
    <div className="tool-body tool-live special special-regex">
      <div className="tool-input">
        {patternParam && <PatternField param={patternParam} value={form.values.pattern} error={patternError} verbose={flags.includes('x')} onChange={form.setValue} />}
        <ParamForm resetKey={form.resetKey} params={tool.params} exclude={['pattern', 'text']} values={form.values} errors={form.shownErrors} onChange={form.setValue} mono idPrefix="regex" />
        {textParam && (
          <div className={cx('field', textError && 'has-error')} data-param="text">
            <div className="field-head">
              <label className="field-label" htmlFor="regex-text">{textParam.label}</label>
              <span className="field-meta" aria-live="polite">
                {form.job.running && <Spinner size={11} />}
                <span className="char-count">{data ? `${matches.length} match${matches.length === 1 ? '' : 'es'}` : ''}</span>
              </span>
            </div>
            <div className="hl-editor">
              <pre className="hl-mirror mono" ref={mirror} aria-hidden>{highlighted}</pre>
              <textarea
                id="regex-text"
                className="hl-input mono"
                value={text}
                spellCheck={false}
                rows={10}
                aria-invalid={textError ? true : undefined}
                aria-describedby={textError ? 'regex-text-err' : undefined}
                onChange={(e) => form.setValue('text', e.target.value)}
                onScroll={(e) => { if (mirror.current) { mirror.current.scrollTop = e.currentTarget.scrollTop; mirror.current.scrollLeft = e.currentTarget.scrollLeft; } }}
              />
            </div>
            {textError && <p className="field-error" id="regex-text-err" role="alert">{textError}</p>}
          </div>
        )}
        <ResetButton form={form} />
      </div>
      <div className={cx('tool-output', stale && 'is-stale')}>
        <StaleNotice stale={stale} />
        {job?.status === 'failed' && <ErrorView error={job.error ?? { code: 'failed' }} />}
        {job?.status === 'succeeded' && <Warnings warnings={job.result?.warnings} />}
        {data && typeof data.replaced === 'string' && (
          <div className="output-block">
            <div className="output-head"><span className="output-title">Replacement preview</span><CopyButton text={data.replaced} /></div>
            <pre className="code-view mono wrap">{data.replaced}</pre>
          </div>
        )}
        {matches.length > 0 && (
          <DataTable
            caption="Matches & groups"
            table={{
              columns: ['#', 'match', 'at', ...Array.from({ length: maxGroups }, (_, g) => groupNames[g] ? `${g + 1} · ${groupNames[g]}` : `group ${g + 1}`)],
              rows: matches.map((m, i) => [i + 1, m.text, `${m.start}–${m.end}`, ...Array.from({ length: maxGroups }, (_, g) => {
                const v = m.groups?.[g];
                return v == null ? null : typeof v === 'object' ? (v as any).text ?? JSON.stringify(v) : v;
              })]),
            }}
          />
        )}
        {rest && Object.keys(rest).length > 0 && <KeyValue value={rest} />}
        {!job && <p className="muted small">Type a pattern and some test text — matches highlight as you type.</p>}
      </div>
    </div>
  );
}
