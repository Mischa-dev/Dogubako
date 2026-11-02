import { useMemo, useRef } from 'react';
import type { ToolInfo } from '../../lib/types';
import type { Match } from '../../lib/renderers';
import { ParamForm } from '../../form/ParamForm';
import { Field } from '../../form/Field';
import { useToolForm } from '../useToolForm';
import { useLiveRun, useToolShortcuts } from '../GenericTool';
import { DataTable, KeyValue } from '../../results/DataViews';
import { ErrorView } from '../../results/ErrorView';
import { Warnings } from '../../results/Warnings';
import { CopyButton, Spinner, cx } from '../../components/ui';

/** Regex tester: highlights matches live under the test-text editor. */
export default function Regex({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const form = useToolForm(tool, { jobId });
  useToolShortcuts(form);
  useLiveRun(form, tool.availability.available, 200);
  const job = form.job.job;
  const data = (job?.status === 'succeeded' ? job.result?.data : null) as Record<string, any> | null;
  const matches: Match[] = Array.isArray(data?.matches) ? data!.matches : [];
  const text = String(form.values.text ?? '');
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
        {patternParam && <Field param={patternParam} value={form.values.pattern} error={form.shownErrors.pattern} onChange={form.setValue} mono />}
        <ParamForm params={tool.params} exclude={['pattern', 'text']} values={form.values} errors={form.shownErrors} onChange={form.setValue} mono idPrefix="regex" />
        {textParam && (
          <div className={cx('field', form.shownErrors.text && 'has-error')} data-param="text">
            <div className="field-head">
              <label className="field-label" htmlFor="regex-text">{textParam.label}</label>
              <span className="field-meta" aria-live="polite">
                {form.job.running && <Spinner size={11} />}
                <span className="char-count">{job?.status === 'succeeded' ? `${matches.length} match${matches.length === 1 ? '' : 'es'}` : ''}</span>
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
                onChange={(e) => form.setValue('text', e.target.value)}
                onScroll={(e) => { if (mirror.current) { mirror.current.scrollTop = e.currentTarget.scrollTop; mirror.current.scrollLeft = e.currentTarget.scrollLeft; } }}
              />
            </div>
          </div>
        )}
      </div>
      <div className="tool-output">
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
