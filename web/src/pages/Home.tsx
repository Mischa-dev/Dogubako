import { useCallback, useEffect, useMemo, useState } from 'react';
import { ArrowRight, FileUp, SlidersHorizontal } from 'lucide-react';
import { CustomizeHome } from '../components/CustomizeHome';
import { categoryIcon } from '../lib/icons';
import { favorites, homeLayout, homeModules, useStore, type HomeModuleId } from '../lib/store';
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
  const { backend, visibleTools, toolMap, status, isServer, categories } = useApp();
  const layout = useStore(homeLayout);
  const pinned = useStore(favorites);
  const [customize, setCustomize] = useState(false);
  const closeCustomize = useCallback(() => setCustomize(false), []);
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

  const pinnedTools = pinned.map((id) => toolMap.get(id)).filter((t): t is NonNullable<typeof t> => !!t).slice(0, 8);

  const render: Record<HomeModuleId, () => React.ReactNode> = {
    bench: () => (
      <section key="bench" className="home-block is-wide bench" aria-labelledby="bench-title">
        <div className="home-section-head">
          <h2 id="bench-title" className="home-section-title">On your bench</h2>
          <a className="link-sm" href={href.history}>History <ArrowRight size={13} aria-hidden /></a>
        </div>
        {jobs === null ? (
          <div className="bench-shelf is-loading" aria-busy="true"><span className="bench-ghost" /><span className="bench-ghost" /><span className="bench-ghost" /></div>
        ) : items.length === 0 ? (
          <p className="bench-empty">Files you work on show up here, so you can pick them up again. Drop one anywhere to start.</p>
        ) : (
          <div className={`bench-shelf is-${layout.shelf}`}>{items.map((it) => <BenchObject key={it.key} item={it} />)}</div>
        )}
      </section>
    ),
    runs: () => recent.length > 0 && (
      <section key="runs" className="home-block module glass" aria-labelledby="runs-title">
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
    ),
    tip: () => tip && (
      <section key="tip" className="home-block module glass" aria-labelledby="tip-title">
        <div className="module-head"><h2 id="tip-title" className="module-title">Worth doing</h2></div>
        <p className="tip-text">{tip.text}</p>
        <a className="btn btn-primary btn-sm tip-action" href={href.tool(tip.toolId)}>{tip.action}</a>
      </section>
    ),
    pinned: () => (
      <section key="pinned" className="home-block module glass" aria-labelledby="pins-title">
        <div className="module-head"><h2 id="pins-title" className="module-title">Pinned tools</h2><a className="link-sm" href={href.favorites}>Edit</a></div>
        {pinnedTools.length ? (
          <ul className="pin-grid">
            {pinnedTools.map((t) => {
              const I = categoryIcon(t.category);
              return <li key={t.id}><a className="pin-chip" href={href.tool(t.id)}><I size={14} aria-hidden /><span>{t.name}</span></a></li>;
            })}
          </ul>
        ) : <p className="muted small">Pin tools from their page or the Tools menu to keep them here.</p>}
      </section>
    ),
    categories: () => (
      <section key="categories" className="home-block is-wide" aria-labelledby="cats-title">
        <div className="home-section-head"><h2 id="cats-title" className="home-section-title">All tools</h2></div>
        <ul className="cat-grid">
          {categories.map((c) => {
            const I = categoryIcon(c.id);
            return (
              <li key={c.id}>
                <a className="cat-tile glass" href={href.category(c.id)}>
                  <I size={16} aria-hidden className="cat-tile-icon" />
                  <span className="cat-tile-name">{c.label}</span>
                  <span className="cat-tile-count">{c.count}</span>
                </a>
              </li>
            );
          })}
        </ul>
      </section>
    ),
  };

  return (
    <div className="page page-home">
      <section className="home-hero">
        <div className="home-label-row">
          <label className="home-label" htmlFor="home-q">{visibleTools.length} tools on {where}</label>
          <button type="button" className="link-sm home-customize" onClick={() => setCustomize(true)} aria-haspopup="dialog">
            <SlidersHorizontal size={13} aria-hidden />Customize
          </button>
        </div>
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

      <div className="home-flow">
        {homeModules(layout).filter((m) => m.on).map((m) => render[m.id]())}
      </div>
      <CustomizeHome open={customize} onClose={closeCustomize} />
    </div>
  );
}
