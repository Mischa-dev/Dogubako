import { useEffect, useRef, useState } from 'react';
import { Pause, Play, RefreshCw, Info } from 'lucide-react';
import type { ToolInfo } from '../../lib/types';
import { ParamForm } from '../../form/ParamForm';
import { useToolForm } from '../useToolForm';
import { DataTable, KeyValue, fmtNum } from '../../results/DataViews';
import { ErrorView } from '../../results/ErrorView';
import { Warnings } from '../../results/Warnings';
import { Button, Meter, Segmented, Spinner } from '../../components/ui';
import { humanizeKey, formatRelative } from '../../lib/format';
import { isTable } from '../../lib/renderers';

const isObj = (v: unknown): v is Record<string, any> => !!v && typeof v === 'object' && !Array.isArray(v);
const isHuman = (v: unknown): v is { human: string } => isObj(v) && 'human' in v;
const PCT = /(percent|_pct$|^usage$|^charge|health)/i;

function percentEntries(o: Record<string, any>) {
  return Object.entries(o).filter(([k, v]) => typeof v === 'number' && PCT.test(k) && v >= 0 && v <= 100.0001);
}

function titleOf(o: Record<string, any>, i: number): string {
  return String(o.name ?? o.mount_point ?? o.label ?? o.model ?? o.component ?? o.brand ?? `#${i + 1}`);
}

function ValueText({ v }: { v: unknown }) {
  if (isHuman(v)) return <>{v.human}</>;
  if (typeof v === 'number') return <span className="num">{Number.isInteger(v) ? fmtNum(v) : v.toFixed(2)}</span>;
  if (typeof v === 'boolean') return <>{v ? 'Yes' : 'No'}</>;
  if (v == null) return <span className="muted">—</span>;
  if (Array.isArray(v)) return <>{v.length <= 8 && v.every((x) => typeof x !== 'object') ? v.join(', ') : `${v.length} items`}</>;
  if (isObj(v)) return <>{Object.entries(v).filter(([, x]) => typeof x !== 'object').map(([k, x]) => `${k} ${x}`).join(' · ')}</>;
  return <>{String(v)}</>;
}

function StatCard({ title, obj }: { title: string; obj: Record<string, any> }) {
  const pcts = percentEntries(obj);
  const rest = Object.entries(obj).filter(([k, v]) => !pcts.some(([pk]) => pk === k) && !(Array.isArray(v) && v.length > 8) && !(isObj(v) && !isHuman(v) && Object.keys(v).length > 6));
  return (
    <section className="stat-card">
      <h3 className="stat-title">{title}</h3>
      {pcts.map(([k, v]) => (
        <div className="stat-meter" key={k}>
          <div className="stat-meter-head"><span>{humanizeKey(k).replace(/ percent$/i, '')}</span><span className="num">{Number(v).toFixed(1)}%</span></div>
          <Meter value={Number(v)} label={`${title} ${humanizeKey(k)}`} tone="auto" />
        </div>
      ))}
      <dl className="stat-kv">
        {rest.slice(0, 12).map(([k, v]) => (
          <div key={k}><dt>{humanizeKey(k)}</dt><dd><ValueText v={v} /></dd></div>
        ))}
      </dl>
    </section>
  );
}

export function Dashboard({ data }: { data: Record<string, any> }) {
  const scalars: [string, unknown][] = [];
  const cards: { title: string; obj: Record<string, any> }[] = [];
  const tables: [string, any][] = [];
  const lists: [string, Record<string, any>[]][] = [];
  for (const [k, v] of Object.entries(data)) {
    if (['host_note', 'environment', 'errors', 'message'].includes(k)) continue;
    if (isTable(v)) tables.push([k, v]);
    else if (Array.isArray(v) && v.length && v.every(isObj)) lists.push([k, v]);
    else if (isObj(v) && !isHuman(v)) cards.push({ title: humanizeKey(k), obj: v });
    else if (!Array.isArray(v) || v.length) scalars.push([k, v]);
  }
  return (
    <div className="dash">
      {data.host_note && <p className="dash-note"><Info size={13} aria-hidden /> {data.host_note}</p>}
      {data.message && <p className="muted">{String(data.message)}</p>}
      {scalars.length > 0 && (
        <section className="stat-card stat-summary">
          <dl className="stat-kv stat-kv-wide">
            {scalars.map(([k, v]) => <div key={k}><dt>{humanizeKey(k)}</dt><dd><ValueText v={v} /></dd></div>)}
          </dl>
        </section>
      )}
      <div className="stat-grid">
        {cards.map((c) => <StatCard key={c.title} title={c.title} obj={c.obj} />)}
        {lists.flatMap(([k, arr]) => (arr.length <= 12 ? arr.map((o, i) => <StatCard key={`${k}${i}`} title={`${humanizeKey(k).replace(/s$/, '')} · ${titleOf(o, i)}`} obj={o} />) : []))}
      </div>
      {lists.filter(([, arr]) => arr.length > 12).map(([k, arr]) => (
        <KeyValue key={k} value={{ [k]: arr }} />
      ))}
      {tables.map(([k, t]) => <DataTable key={k} table={t} caption={k === 'table' ? 'Details' : humanizeKey(k)} />)}
    </div>
  );
}

export default function SystemDash({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const form = useToolForm(tool, { jobId });
  const [paused, setPaused] = useState(false);
  const [interval, setIntervalS] = useState<'2' | '3' | '5'>('3');
  const [updated, setUpdated] = useState<number | null>(null);
  const [lastGood, setLastGood] = useState<Record<string, any> | null>(null);
  const formRef = useRef(form);
  formRef.current = form;

  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    const tick = async () => {
      if (!alive) return;
      if (!document.hidden) {
        const j = await formRef.current.run({ silent: true, live: true });
        if (!alive) return;
        if (j?.status === 'succeeded' && j.result) { setLastGood((j.result.data as any) ?? {}); setUpdated(Date.now() / 1000); }
      }
      if (alive && !paused) timer = window.setTimeout(tick, Number(interval) * 1000);
    };
    void tick();
    return () => { alive = false; window.clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paused, interval, form.values]);

  const job = form.job.job;
  const text = job?.result?.text;
  return (
    <div className="tool-body special special-system">
      {tool.params.length > 0 && (
        <div className="tool-input">
          <ParamForm params={tool.params} values={form.values} errors={form.shownErrors} onChange={form.setValue} idPrefix="sys" />
        </div>
      )}
      <div className="dash-toolbar">
        <Button size="sm" variant="secondary" icon={paused ? Play : Pause} onClick={() => setPaused((p) => !p)} aria-pressed={paused}>
          {paused ? 'Resume' : 'Pause'}
        </Button>
        <Button size="sm" variant="ghost" icon={RefreshCw} onClick={() => void form.run({ silent: true, live: true }).then((j) => { if (j?.result) { setLastGood(j.result.data as any); setUpdated(Date.now() / 1000); } })}>Refresh</Button>
        <Segmented size="sm" label="Refresh interval" value={interval} onChange={setIntervalS} options={[{ value: '2', label: '2 s' }, { value: '3', label: '3 s' }, { value: '5', label: '5 s' }]} />
        <span className="muted small dash-updated" aria-live="off">
          {form.job.running && <Spinner size={11} />} {updated ? (paused ? 'Paused · ' : 'Live · ') + `updated ${formatRelative(updated)}` : 'Loading…'}
        </span>
      </div>
      {job?.status === 'failed' && <ErrorView error={job.error ?? { code: 'failed' }} />}
      {job?.status === 'succeeded' && <Warnings warnings={job.result?.warnings} />}
      {lastGood && Object.keys(lastGood).length > 0 ? <Dashboard data={lastGood} /> : text ? <pre className="code-view mono">{text}</pre> : null}
    </div>
  );
}
