import { useEffect, useMemo, useState } from 'react';
import { ExternalLink, ChevronRight, Search } from 'lucide-react';
import { useApp } from '../../lib/app';
import { asServer } from '../../lib/backend';
import { errorText } from '../../lib/errors';
import { Callout, CopyButton, Section, Spinner, cx } from '../../components/ui';
import { mcpConfig } from './TokenReveal';

interface Op { method: string; path: string; summary: string; tag: string }

export function ApiPanel() {
  const { backend, status } = useApp();
  const server = asServer(backend)!;
  const [spec, setSpec] = useState<any>(null);
  const [err, setErr] = useState<string | null>(null);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [q, setQ] = useState('');
  useEffect(() => { server.openapi().then(setSpec).catch((e) => setErr(errorText(e))); }, [server]);

  const groups = useMemo(() => {
    if (!spec?.paths) return [];
    const ops: Op[] = [];
    for (const [path, item] of Object.entries<any>(spec.paths)) {
      for (const [method, op] of Object.entries<any>(item)) {
        if (!['get', 'post', 'put', 'patch', 'delete'].includes(method)) continue;
        ops.push({ method: method.toUpperCase(), path, summary: op.summary ?? op.description ?? '', tag: op.tags?.[0] ?? (path.includes('/tools/') && path.endsWith('/run') ? 'tools' : 'other') });
      }
    }
    const s = q.trim().toLowerCase();
    const filtered = s ? ops.filter((o) => o.path.toLowerCase().includes(s) || o.summary.toLowerCase().includes(s)) : ops;
    const m = new Map<string, Op[]>();
    for (const o of filtered) m.set(o.tag, [...(m.get(o.tag) ?? []), o]);
    return [...m.entries()].sort((a, b) => a[1].length - b[1].length);
  }, [spec, q]);

  const origin = location.origin;
  return (
    <>
      <Section title="HTTP API" description="Everything in this UI is available over a versioned REST API. Authenticate with an API token in the Authorization header."
        actions={<a className="btn btn-secondary btn-sm" href="api/v1/openapi.json" target="_blank" rel="noopener noreferrer"><ExternalLink size={14} aria-hidden /><span>openapi.json</span></a>}>
        {err && <Callout tone="danger">{err}</Callout>}
        {!spec && !err && <Spinner />}
        {spec && (
          <>
            <div className="list-toolbar">
              <div className="input-group input-search">
                <span className="input-prefix" aria-hidden><Search size={14} /></span>
                <input className="input" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter endpoints" aria-label="Filter endpoints" />
              </div>
              <span className="muted small">{spec.info?.title} {spec.info?.version}</span>
            </div>
            {groups.map(([tag, ops]) => {
              const many = ops.length > 12;
              const isOpen = open[tag] ?? (!many || !!q);
              return (
                <div key={tag} className="api-group">
                  <button type="button" className="api-group-head" aria-expanded={isOpen} onClick={() => setOpen((o) => ({ ...o, [tag]: !isOpen }))}>
                    <ChevronRight size={14} aria-hidden className="chev" />
                    <span className="api-tag">{tag}</span>
                    <span className="muted small">{ops.length} endpoint{ops.length === 1 ? '' : 's'}</span>
                  </button>
                  {isOpen && (
                    <ul className="api-ops">
                      {ops.map((o) => (
                        <li key={o.method + o.path} className="api-op">
                          <span className={cx('method', `m-${o.method.toLowerCase()}`)}>{o.method}</span>
                          <code className="mono api-path">{o.path}</code>
                          <span className="api-sum">{o.summary}</span>
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              );
            })}
          </>
        )}
      </Section>
      {status.mcp_enabled && (
        <Section title="MCP" description="Connect AI assistants (Claude, IDE agents…) over the Model Context Protocol. Create an agent with a scoped policy, then a token for it.">
          <div className="token-examples">
            <div className="token-examples-head"><span className="small muted">Client configuration</span><CopyButton text={mcpConfig(origin, '<token>')} /></div>
            <pre className="code-view mono small">{mcpConfig(origin, '<token>')}</pre>
          </div>
        </Section>
      )}
    </>
  );
}
