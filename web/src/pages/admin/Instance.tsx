import { useCallback, useEffect, useState } from 'react';
import { RefreshCw, CircleCheck, CircleAlert } from 'lucide-react';
import type { AdminConfig, JobView } from '../../lib/types';
import { useApp } from '../../lib/app';
import { asServer } from '../../lib/backend';
import { formatUptime, formatRelative } from '../../lib/format';
import { errorText } from '../../lib/errors';
import { Badge, Button, Callout, Section, Spinner } from '../../components/ui';
import { KeyValue } from '../../results/DataViews';

export function InstancePanel() {
  const { backend, status, toolMap } = useApp();
  const server = asServer(backend)!;
  const [cfg, setCfg] = useState<AdminConfig | null>(null);
  const [jobs, setJobs] = useState<JobView[]>([]);
  const [ext, setExt] = useState<{ extensions: any[]; dir?: string } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      const [c, j, e] = await Promise.all([server.adminConfig(), server.adminJobs(50).catch(() => ({ jobs: [] as JobView[] })), server.extensions().catch(() => null)]);
      setCfg(c); setJobs(j.jobs.filter((x) => x.status === 'running' || x.status === 'queued')); setExt(e); setErr(null);
    } catch (x) { setErr(errorText(x)); }
  }, [server]);
  useEffect(() => { void load(); const t = window.setInterval(load, 5000); return () => window.clearInterval(t); }, [load]);
  if (err) return <Callout tone="danger">{err}</Callout>;
  if (!cfg) return <Spinner />;
  return (
    <>
      <Section title="Overview" actions={<Button size="sm" variant="ghost" icon={RefreshCw} onClick={load}>Refresh</Button>}>
        <div className="stat-tiles">
          <div className="stat-tile"><span className="stat-tile-label">Instance</span><span className="stat-tile-value">{status.instance_name}</span></div>
          <div className="stat-tile"><span className="stat-tile-label">Version</span><span className="stat-tile-value">{status.version}</span></div>
          <div className="stat-tile"><span className="stat-tile-label">Uptime</span><span className="stat-tile-value">{formatUptime(cfg.uptime_secs)}</span></div>
          <div className="stat-tile"><span className="stat-tile-label">Running jobs</span><span className="stat-tile-value">{cfg.running_jobs}</span></div>
          <div className="stat-tile"><span className="stat-tile-label">Tools</span><span className="stat-tile-value">{status.tools}</span></div>
          <div className="stat-tile"><span className="stat-tile-label">MCP endpoint</span><span className="stat-tile-value">{status.mcp_enabled ? 'Enabled' : 'Off'}</span></div>
        </div>
      </Section>
      {jobs.length > 0 && (
        <Section title="Active jobs">
          <div className="table-wrap">
            <table className="table">
              <thead><tr><th scope="col">Tool</th><th scope="col">Owner</th><th scope="col">Status</th><th scope="col">Started</th><th scope="col">Progress</th></tr></thead>
              <tbody>{jobs.map((j) => (
                <tr key={j.id}><td>{toolMap.get(j.tool_id)?.name ?? j.tool_id}</td><td>{j.owner}</td><td>{j.status}</td><td>{formatRelative(j.created_at)}</td><td className="num">{j.progress != null ? `${Math.round(j.progress * 100)}%` : '—'}</td></tr>
              ))}</tbody>
            </table>
          </div>
        </Section>
      )}
      <Section title="Extensions" description={ext?.dir ? <>Loaded from <code className="mono">{ext.dir}</code></> : undefined}>
        {ext && ext.extensions.length > 0 ? (
          <ul className="ext-list">
            {ext.extensions.map((e: any, i: number) => (
              <li key={i} className="ext">
                {e.error ? <CircleAlert size={15} aria-hidden className="danger-icon" /> : <CircleCheck size={15} aria-hidden className="ok-icon" />}
                <span className="ext-name">{e.id ?? e.name ?? `extension ${i + 1}`}</span>
                {e.version && <span className="mono small muted">{e.version}</span>}
                {e.tools != null && <Badge>{Array.isArray(e.tools) ? e.tools.length : e.tools} tools</Badge>}
                {e.error && <span className="small field-error">{String(e.error)}</span>}
              </li>
            ))}
          </ul>
        ) : <p className="muted small">No extensions installed.</p>}
      </Section>
      <Section title="Configuration" description="Read-only. Change settings in the server's config file or command-line flags.">
        <KeyValue value={cfg.config} />
      </Section>
    </>
  );
}
