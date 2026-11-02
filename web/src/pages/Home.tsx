import { useMemo } from 'react';
import { ArrowRight, Clock, Package, Search, Star } from 'lucide-react';
import { useApp } from '../lib/app';
import { favorites as favStore, recents as recStore, useStore } from '../lib/store';
import { href } from '../lib/router';
import { categoryIcon } from '../lib/icons';
import { Callout, Kbd, Section, modKey } from '../components/ui';
import { ToolList, rovingKeyDown } from '../components/ToolRow';

export function HomePage({ onSearch }: { onSearch: () => void }) {
  const { visibleTools, toolMap, categories, categoryLabel, status, isServer } = useApp();
  const favs = useStore(favStore);
  const recs = useStore(recStore);

  const favTools = useMemo(() => favs.map((id) => toolMap.get(id)).filter((t): t is NonNullable<typeof t> => !!t && t.permitted !== false), [favs, toolMap]);
  const recentTools = useMemo(() => recs.map((r) => toolMap.get(r.id)).filter((t): t is NonNullable<typeof t> => !!t && t.permitted !== false).slice(0, 6), [recs, toolMap]);
  const unavailable = useMemo(() => visibleTools.filter((t) => !t.availability.available), [visibleTools]);
  const unavailableByCat = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of unavailable) m.set(t.category, (m.get(t.category) ?? 0) + 1);
    return m;
  }, [unavailable]);
  const topMissing = useMemo(() => {
    const m = new Map<string, number>();
    for (const t of unavailable) for (const d of t.availability.missing ?? []) m.set(d.name, (m.get(d.name) ?? 0) + 1);
    return [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3);
  }, [unavailable]);

  return (
    <div className="page page-home">
      <header className="home-head">
        <h1 className="home-title" tabIndex={-1} data-route-focus>
          {isServer && status.instance_name !== 'Dogubako' ? status.instance_name : 'Your toolbox'}
        </h1>
        <p className="page-sub">{visibleTools.length} tools that run {isServer ? <>on <strong>{status.instance_name}</strong></> : 'on this device'} — no cloud services, no tracking.</p>
        <button className="home-search" onClick={onSearch}>
          <Search size={17} aria-hidden />
          <span>Search for a tool, e.g. “compress pdf” or “json”</span>
          <Kbd>{modKey} K</Kbd>
        </button>
      </header>

      {unavailable.length >= 8 && (
        <Callout
          tone="info"
          icon={Package}
          title={`${unavailable.length} tools need extra software`}
          actions={<a className="btn btn-secondary btn-sm" href={href.settings('dependencies')}>Dependencies<ArrowRight size={14} aria-hidden /></a>}
        >
          {topMissing.length > 0
            ? <>Installing {topMissing.map(([n, c]) => `${n} (${c})`).join(', ')} would enable most of them.</>
            : <>Some tools need optional programs or models.</>}
        </Callout>
      )}

      {favTools.length > 0 && (
        <Section title={<><Star size={14} aria-hidden /> Favorites</>} id="home-favs" actions={<a className="link-sm" href={href.favorites}>All favorites</a>}>
          <ToolList tools={favTools.slice(0, 8)} favorites={favs} label="Favorite tools" showCategory categoryLabel={categoryLabel} />
        </Section>
      )}

      {recentTools.length > 0 && (
        <Section title={<><Clock size={14} aria-hidden /> Recently used</>} id="home-recent" actions={<a className="link-sm" href={href.recent}>Show all</a>}>
          <ToolList tools={recentTools} favorites={favs} label="Recently used tools" showCategory categoryLabel={categoryLabel} />
        </Section>
      )}

      <Section title="Categories" id="home-cats">
        <ul className="cat-grid" onKeyDown={rovingKeyDown}>
          {categories.map((c) => {
            const Icon = categoryIcon(c.id);
            const un = unavailableByCat.get(c.id) ?? 0;
            return (
              <li key={c.id}>
                <a className="cat-row" href={href.category(c.id)} data-roving>
                  <span className="cat-icon" aria-hidden><Icon size={18} /></span>
                  <span className="cat-text">
                    <span className="cat-name">{c.label}</span>
                    <span className="cat-desc">{c.description}</span>
                  </span>
                  <span className="cat-count" aria-label={`${c.count} tools${un ? `, ${un} unavailable` : ''}`}>{c.count}</span>
                </a>
              </li>
            );
          })}
        </ul>
      </Section>
    </div>
  );
}
