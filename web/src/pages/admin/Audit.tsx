import { useCallback, useEffect, useMemo, useState } from 'react';
import { Search, RefreshCw } from 'lucide-react';
import type { AuditRow } from '../../lib/types';
import { useApp } from '../../lib/app';
import { asServer } from '../../lib/backend';
import { formatDateTime } from '../../lib/format';
import { errorText } from '../../lib/errors';
import { Badge, Button, Section, Spinner } from '../../components/ui';
import { toast } from '../../components/toast';

const PAGE = 100;

export function AuditPanel() {
  const { backend } = useApp();
  const server = asServer(backend)!;
  const [rows, setRows] = useState<AuditRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [more, setMore] = useState(true);
  const [q, setQ] = useState('');
  const [outcome, setOutcome] = useState('');

  const load = useCallback(async (before?: number) => {
    setLoading(true);
    try {
      const r = await server.adminAudit(PAGE, before);
      setRows((prev) => (before ? [...prev, ...r] : r));
      setMore(r.length === PAGE);
    } catch (e) { toast.error(errorText(e)); }
    setLoading(false);
  }, [server]);
  useEffect(() => { void load(); }, [load]);

  const outcomes = useMemo(() => [...new Set(rows.map((r) => r.outcome))].sort(), [rows]);
  const filtered = useMemo(() => {
    const s = q.trim().toLowerCase();
    return rows.filter((r) => (!outcome || r.outcome === outcome) && (!s || [r.actor, r.action, r.target, r.ip, r.detail].some((x) => x?.toLowerCase().includes(s))));
  }, [rows, q, outcome]);

  return (
    <Section title="Audit log" description="Sign-ins, account changes, tool runs and other security-relevant events."
      actions={<Button size="sm" variant="ghost" icon={RefreshCw} onClick={() => load()}>Refresh</Button>}>
      <div className="list-toolbar">
        <div className="input-group input-search">
          <span className="input-prefix" aria-hidden><Search size={14} /></span>
          <input className="input" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter by actor, action, target, IP…" aria-label="Filter audit log" />
        </div>
        <div className="select-wrap select-sm">
          <select className="select" value={outcome} onChange={(e) => setOutcome(e.target.value)} aria-label="Outcome">
            <option value="">All outcomes</option>
            {outcomes.map((o) => <option key={o} value={o}>{o}</option>)}
          </select>
        </div>
      </div>
      <div className="table-wrap">
        <table className="table audit-table">
          <thead><tr><th scope="col">Time</th><th scope="col">Actor</th><th scope="col">Action</th><th scope="col">Target</th><th scope="col">Outcome</th><th scope="col">IP</th><th scope="col">Detail</th></tr></thead>
          <tbody>
            {filtered.map((r) => (
              <tr key={r.id}>
                <td className="small nowrap">{formatDateTime(r.ts)}</td>
                <td>{r.actor}</td>
                <td className="mono small">{r.action}</td>
                <td className="small trunc" title={r.target}>{r.target}</td>
                <td><Badge tone={r.outcome === 'ok' ? 'success' : r.outcome === 'started' ? 'neutral' : 'danger'}>{r.outcome}</Badge></td>
                <td className="mono small">{r.ip}</td>
                <td className="small trunc" title={r.detail}>{r.detail}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {loading && <Spinner />}
      {!loading && filtered.length === 0 && <p className="muted small">No matching entries.</p>}
      {more && !loading && rows.length > 0 && <Button variant="secondary" onClick={() => load(rows[rows.length - 1].id)}>Load more</Button>}
    </Section>
  );
}
