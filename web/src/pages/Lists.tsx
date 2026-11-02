import { useMemo, useState } from 'react';
import { Clock, Pin, Search, Trash2, X } from 'lucide-react';
import { useApp } from '../lib/app';
import { favorites as favStore, recents as recStore, useStore } from '../lib/store';
import { categoryIcon } from '../lib/icons';
import { search } from '../lib/search';
import { Button, EmptyState, PageHeader, Segmented } from '../components/ui';
import { ToolList } from '../components/ToolRow';
import { NotFound } from './NotFound';

type Filter = 'all' | 'available' | 'unavailable';

export function CategoryPage({ categoryId }: { categoryId: string }) {
  const { visibleTools, categories, docs } = useApp();
  const favs = useStore(favStore);
  const [q, setQ] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const cat = categories.find((c) => c.id === categoryId);
  const all = useMemo(() => visibleTools.filter((t) => t.category === categoryId).sort((a, b) => a.name.localeCompare(b.name)), [visibleTools, categoryId]);
  const unavailableCount = all.filter((t) => !t.availability.available).length;
  const { list, ranges } = useMemo(() => {
    let base = all;
    let ranges = new Map<string, [number, number][]>();
    if (q.trim()) {
      const hits = search(docs.filter((d) => d.item.category === categoryId), q);
      base = hits.map((h) => h.item);
      ranges = new Map(hits.map((h) => [h.item.id, h.ranges]));
    }
    if (filter === 'available') base = base.filter((t) => t.availability.available);
    if (filter === 'unavailable') base = base.filter((t) => !t.availability.available);
    return { list: base, ranges };
  }, [all, q, filter, docs, categoryId]);

  if (!cat) return <NotFound />;
  const Icon = categoryIcon(cat.id);
  return (
    <div className="page">
      <PageHeader icon={Icon} title={cat.label} subtitle={cat.description} />
      <div className="list-toolbar">
        <div className="input-group input-search">
          <span className="input-prefix" aria-hidden><Search size={14} /></span>
          <input className="input" placeholder={`Filter ${all.length} tools`} value={q} onChange={(e) => setQ(e.target.value)} aria-label={`Filter ${cat.label} tools`} />
          {q && <button className="input-addon" onClick={() => setQ('')} aria-label="Clear filter"><X size={14} /></button>}
        </div>
        {unavailableCount > 0 && (
          <Segmented<Filter>
            size="sm"
            label="Availability"
            value={filter}
            onChange={setFilter}
            options={[
              { value: 'all', label: `All ${all.length}` },
              { value: 'available', label: `Ready ${all.length - unavailableCount}` },
              { value: 'unavailable', label: `Needs setup ${unavailableCount}` },
            ]}
          />
        )}
      </div>
      {list.length ? (
        <ToolList tools={list} favorites={favs} label={`${cat.label} tools`} rangesFor={(id) => ranges.get(id)} />
      ) : (
        <EmptyState icon={Search} title="No matching tools">Try a different search term.</EmptyState>
      )}
    </div>
  );
}

export function FavoritesPage() {
  const { toolMap, categoryLabel } = useApp();
  const favs = useStore(favStore);
  const tools = favs.map((id) => toolMap.get(id)).filter((t): t is NonNullable<typeof t> => !!t && t.permitted !== false);
  return (
    <div className="page">
      <PageHeader icon={Pin} title="Pinned tools" subtitle="Pinned tools appear in the sidebar and in the Tools menu. Alt + 1 to 9 opens the first nine." />
      {tools.length ? (
        <ToolList tools={tools} favorites={favs} label="Pinned tools" showCategory categoryLabel={categoryLabel} />
      ) : (
        <EmptyState icon={Pin} title="Nothing pinned yet">Pin a tool from the Tools menu, any list or a tool page.</EmptyState>
      )}
    </div>
  );
}

export function RecentPage() {
  const { toolMap, categoryLabel } = useApp();
  const favs = useStore(favStore);
  const recs = useStore(recStore);
  const tools = recs.map((r) => toolMap.get(r.id)).filter((t): t is NonNullable<typeof t> => !!t && t.permitted !== false);
  return (
    <div className="page">
      <PageHeader
        icon={Clock}
        title="Recently used"
        subtitle="The last tools you opened on this device."
        actions={tools.length > 0 && <Button variant="ghost" size="sm" icon={Trash2} onClick={() => recStore.set([])}>Clear</Button>}
      />
      {tools.length ? (
        <ToolList tools={tools} favorites={favs} label="Recently used tools" showCategory categoryLabel={categoryLabel} />
      ) : (
        <EmptyState icon={Clock} title="Nothing yet">Tools you open will show up here.</EmptyState>
      )}
    </div>
  );
}
