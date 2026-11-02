import { useCallback, useEffect, useMemo, useState } from 'react';
import { History as HistoryIcon, RefreshCw, Trash2, RotateCcw, CircleCheck, CircleX, Ban, Loader, Search, ChevronRight } from 'lucide-react';
import type { JobView } from '../lib/types';
import { useApp } from '../lib/app';
import { href, navigate } from '../lib/router';
import { formatDuration, formatRelative, formatDateTime } from '../lib/format';
import { prefs, useStore } from '../lib/store';
import { describeError } from '../lib/errors';
import { Button, Callout, EmptyState, IconButton, PageHeader, Spinner, cx, useConfirm } from '../components/ui';
import { ArtifactList } from '../results/Artifacts';
import { toast } from '../components/toast';
import { categoryIcon } from '../lib/icons';

const STATUS_ICON = { succeeded: CircleCheck, failed: CircleX, cancelled: Ban, running: Loader, queued: Loader } as const;

export default function HistoryPage() {
  const { backend, toolMap, isServer } = useApp();
  const p = useStore(prefs);
  const [jobs, setJobs] = useState<JobView[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState('');
  const [open, setOpen] = useState<string | null>(null);
  const [confirm, confirmNode] = useConfirm();

  const load = useCallback(async () => {
    setErr(null);
    try { setJobs(await backend.listJobs(200)); } catch (e) { setErr(e instanceof Error ? e.message : String(e)); }
  }, [backend]);
  useEffect(() => { void load(); }, [load]);

  const filtered = useMemo(() => {
    if (!jobs) return [];
    const s = q.trim().toLowerCase();
    if (!s) return jobs;
    return jobs.filter((j) => j.tool_id.includes(s) || (toolMap.get(j.tool_id)?.name.toLowerCase().includes(s) ?? false) || j.status.includes(s));
  }, [jobs, q, toolMap]);

  const remove = async (j: JobView) => {
    if (isServer && !(await confirm({ title: 'Delete this run?', message: 'Its output files will be deleted from the server too.', confirm: 'Delete', danger: true }))) return;
    try {
      await backend.deleteJob(j.id);
      setJobs((js) => js?.filter((x) => x.id !== j.id) ?? null);
    } catch (e) { toast.error(e instanceof Error ? e.message : 'Delete failed'); }
  };

  return (
    <div className="page">
      <PageHeader
        icon={HistoryIcon}
        title="History"
        subtitle={isServer ? 'Your recent runs on this server. Outputs are kept for a limited time.' : 'Runs on this device, stored locally.'}
        actions={<Button size="sm" variant="ghost" icon={RefreshCw} onClick={load}>Refresh</Button>}
      />
      {!isServer && !p.history && <Callout tone="info">Run history is turned off in Settings. New runs aren't recorded.</Callout>}
      {err && <Callout tone="danger">{err}</Callout>}
      {jobs === null && !err && <div className="page-loading"><Spinner /></div>}
      {jobs && jobs.length > 0 && (
        <div className="list-toolbar">
          <div className="input-group input-search">
            <span className="input-prefix" aria-hidden><Search size={14} /></span>
            <input className="input" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter by tool or status" aria-label="Filter history" />
          </div>
        </div>
      )}
      {jobs && jobs.length === 0 && <EmptyState icon={HistoryIcon} title="No runs yet">Run any tool and it will appear here.</EmptyState>}
      {filtered.length > 0 && (
        <ul className="history-list">
          {filtered.map((j) => {
            const t = toolMap.get(j.tool_id);
            const Icon = STATUS_ICON[j.status] ?? Loader;
            const CatIcon = categoryIcon(t?.category ?? '');
            const isOpen = open === j.id;
            const dur = j.result?.duration_ms ?? (j.finished_at ? (j.finished_at - j.created_at) * 1000 : undefined);
            const nArt = j.result?.artifacts?.length ?? 0;
            return (
              <li key={j.id} className={cx('history-item', `st-${j.status}`, isOpen && 'is-open')}>
                <div className="history-row">
                  <button type="button" className="history-main" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : j.id)}>
                    <ChevronRight size={14} aria-hidden className="chev" />
                    <span className="history-cat" aria-hidden><CatIcon size={15} /></span>
                    <span className="history-text">
                      <span className="history-name">{t?.name ?? j.tool_id}</span>
                      <span className="history-sub">
                        <span className={cx('status-dot', `st-${j.status}`)}><Icon size={12} aria-hidden />{j.status}</span>
                        <span title={formatDateTime(j.created_at)}>{formatRelative(j.created_at)}</span>
                        {dur != null && <span>{formatDuration(dur)}</span>}
                        {nArt > 0 && <span>{nArt} file{nArt === 1 ? '' : 's'}</span>}
                      </span>
                    </span>
                  </button>
                  <span className="history-actions">
                    {t && <IconButton icon={RotateCcw} label="Open with same settings" onClick={() => navigate(`${href.tool(j.tool_id)}?job=${encodeURIComponent(j.id)}`)} />}
                    {j.status !== 'running' && j.status !== 'queued' && <IconButton icon={Trash2} label="Delete run" onClick={() => void remove(j)} />}
                  </span>
                </div>
                {isOpen && (
                  <div className="history-detail">
                    {j.error && <p className="small"><strong>{describeError(j.error).title}.</strong> {describeError(j.error).message}</p>}
                    {j.params && Object.keys(j.params).length > 0 && (
                      <dl className="param-summary">
                        {Object.entries(j.params).map(([k, v]) => (
                          <div key={k}><dt>{k}</dt><dd className="mono">{typeof v === 'string' ? (v.length > 120 ? `${v.slice(0, 120)}…` : v) : JSON.stringify(v)}</dd></div>
                        ))}
                      </dl>
                    )}
                    {j.result?.text && <pre className="code-view mono small history-text-out">{j.result.text.slice(0, 2000)}{j.result.text.length > 2000 ? '…' : ''}</pre>}
                    {nArt > 0 && <ArtifactList artifacts={j.result!.artifacts} jobId={j.id} collapsed />}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {confirmNode}
    </div>
  );
}
