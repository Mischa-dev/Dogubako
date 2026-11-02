import { useEffect, useMemo, useState } from 'react';
import { ArrowRight, FileUp } from 'lucide-react';
import { pickAndOpen } from '../components/DropOverlay';
import { useApp } from '../lib/app';
import { href } from '../lib/router';
import { benchItems, suggestion } from '../lib/bench';
import { formatBytes, formatDuration, formatRelative, plural } from '../lib/format';
import type { JobView } from '../lib/types';
import { BenchObject } from '../components/BenchObject';
import { Kbd, modKey } from '../components/ui';

function runSummary(j: JobView): string {
  if (j.status === 'failed') return j.error?.message ?? 'Failed';
  if (j.status === 'cancelled') return 'Cancelled';
  if (j.status !== 'succeeded') return 'Running';
  const arts = j.result?.artifacts ?? [];
  if (arts.length === 1) return formatBytes(arts[0].size);
  if (arts.length > 1) return plural(arts.length, 'file');
  return formatDuration(j.result?.duration_ms);
}

export function HomePage({ onSearch }: { onSearch: (query?: string) => void }) {
  const { backend, visibleTools, toolMap, status, isServer } = useApp();
  const [jobs, setJobs] = useState<JobView[] | null>(null);

  useEffect(() => {
    let live = true;
    backend.listJobs(40).then((j) => { if (live) setJobs(j); }).catch(() => { if (live) setJobs([]); });
    return () => { live = false; };
  }, [backend]);

  const items = useMemo(() => benchItems(jobs ?? [], 6), [jobs]);
  const tip = useMemo(() => suggestion(items, toolMap), [items, toolMap]);
  const recent = useMemo(() => (jobs ?? []).slice(0, 5), [jobs]);
  const where = !isServer ? 'this device' : status.instance_name && status.instance_name !== 'Dogubako' ? status.instance_name : 'this server';

  return (
    <div className="page page-home">
      <section className="home-hero">
        <label className="home-label" htmlFor="home-q">{visibleTools.length} tools on {where}</label>
        <div className="home-input-row">
          <input
            id="home-q"
            className="home-input"
            placeholder="Drop a file, or say what you need"
            autoComplete="off"
            data-route-focus
            onChange={(e) => { const v = e.target.value; e.target.value = ''; onSearch(v); }}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === 'ArrowDown') { e.preventDefault(); onSearch(''); } }}
          />
          <Kbd>{modKey} K</Kbd>
          <button type="button" className="btn btn-secondary home-choose" onClick={() => void pickAndOpen()}><FileUp size={15} aria-hidden />Choose files</button>
        </div>
      </section>

      <section className="bench" aria-labelledby="bench-title">
        <div className="home-section-head">
          <h2 id="bench-title" className="home-section-title">On your bench</h2>
          <a className="link-sm" href={href.history}>History <ArrowRight size={13} aria-hidden /></a>
        </div>
        {jobs === null ? (
          <div className="bench-shelf is-loading" aria-busy="true"><span className="bench-ghost" /><span className="bench-ghost" /><span className="bench-ghost" /></div>
        ) : items.length === 0 ? (
          <p className="bench-empty">Files you work on show up here, so you can pick them up again. Drop one anywhere to start.</p>
        ) : (
          <div className="bench-shelf">{items.map((it) => <BenchObject key={it.key} item={it} />)}</div>
        )}
      </section>

      {(recent.length > 0 || tip) && (
        <div className="home-modules">
          {recent.length > 0 && (
            <section className="module glass" aria-labelledby="runs-title">
              <div className="module-head"><h2 id="runs-title" className="module-title">Recent runs</h2><a className="link-sm" href={href.history}>All</a></div>
              <ul className="runs">
                {recent.map((j) => (
                  <li key={j.id}>
                    <a className="run-row" href={href.tool(j.tool_id)}>
                      <span className="run-name">{toolMap.get(j.tool_id)?.name ?? j.tool_id}</span>
                      <span className={`run-result st-${j.status}`}>{runSummary(j)}</span>
                      <span className="run-when">{formatRelative(j.finished_at ?? j.created_at)}</span>
                    </a>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {tip && (
            <section className="module glass" aria-labelledby="tip-title">
              <div className="module-head"><h2 id="tip-title" className="module-title">Worth doing</h2></div>
              <p className="tip-text">{tip.text}</p>
              <a className="btn btn-primary btn-sm tip-action" href={href.tool(tip.toolId)}>{tip.action}</a>
            </section>
          )}
        </div>
      )}
    </div>
  );
}
