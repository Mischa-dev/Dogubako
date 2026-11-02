import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Pin, Search } from 'lucide-react';
import { useApp } from '../lib/app';
import { href, navigate } from '../lib/router';
import { search } from '../lib/search';
import { favorites as favStore, toggleFavorite, useStore } from '../lib/store';
import type { ToolInfo } from '../lib/types';
import { unavailableLabel } from './ToolRow';
import { cx } from './ui';

/**
 * Every tool, by category, with a pin toggle on each row. Opened from the
 * collapsed sidebar's "Tools" button or with the T key from anywhere.
 */
export function ToolsMenu({ open, onClose, anchor }: { open: boolean; onClose: () => void; anchor?: { left: number; top: number } }) {
  const { categories, visibleTools, docs, toolMap } = useApp();
  const favs = useStore(favStore);
  const [q, setQ] = useState('');
  const [cat, setCat] = useState<string>(categories[0]?.id ?? '');
  const panel = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const prevFocus = useRef<Element | null>(null);

  useLayoutEffect(() => {
    if (open) {
      prevFocus.current = document.activeElement;
      setQ('');
      requestAnimationFrame(() => input.current?.focus());
    } else {
      (prevFocus.current as HTMLElement | null)?.focus?.();
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => { if (!panel.current?.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); onClose(); } };
    document.addEventListener('mousedown', down);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', down); document.removeEventListener('keydown', key); };
  }, [open, onClose]);

  const shown: ToolInfo[] = useMemo(() => {
    if (q.trim()) return search(docs, q, { limit: 40 }).map((r) => r.item);
    return visibleTools.filter((t) => t.category === cat).sort((a, b) => a.name.localeCompare(b.name));
  }, [q, docs, visibleTools, cat]);

  const pinned = useMemo(() => favs.map((id) => toolMap.get(id)).filter((t): t is ToolInfo => !!t), [favs, toolMap]);

  if (!open) return null;
  const go = (id: string) => { onClose(); navigate(href.tool(id)); };

  return createPortal(
    <div className="tools-menu" role="dialog" aria-label="All tools" ref={panel} style={anchor ? { left: anchor.left, top: anchor.top, transform: 'none' } : undefined}>
      <label className="tools-menu-search">
        <Search size={16} aria-hidden />
        <input ref={input} value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Filter ${visibleTools.length} tools`} aria-label="Filter tools" />
      </label>
      {pinned.length > 0 && (
        <div className="tools-menu-pins">
          <span className="tools-menu-label">Pinned</span>
          {pinned.map((t) => <button key={t.id} type="button" className="tools-menu-chip" onClick={() => go(t.id)}>{t.name}</button>)}
        </div>
      )}
      <div className={cx('tools-menu-body', q.trim() && 'is-searching')}>
        {!q.trim() && (
          <nav className="tools-menu-cats" aria-label="Categories">
            {categories.map((c) => (
              <button key={c.id} type="button" className={cx('tools-menu-cat', c.id === cat && 'is-on')} aria-pressed={c.id === cat}
                onClick={() => setCat(c.id)} onMouseEnter={() => setCat(c.id)}>
                <span>{c.label}</span><span className="tools-menu-count">{c.count}</span>
              </button>
            ))}
          </nav>
        )}
        <ul className="tools-menu-list" aria-label={q.trim() ? 'Matching tools' : 'Tools'}>
          {shown.map((t) => {
            const on = favs.includes(t.id);
            const why = unavailableLabel(t);
            return (
              <li key={t.id} className={cx('tools-menu-row', on && 'is-pinned')}>
                <a href={href.tool(t.id)} onClick={(e) => { e.preventDefault(); go(t.id); }} className={cx(why && 'is-unavailable')}>
                  {t.name}{why && <span className="tools-menu-why"> · {why}</span>}
                </a>
                <button type="button" className="pin-btn" aria-pressed={on} aria-label={on ? `Unpin ${t.name}` : `Pin ${t.name}`} title={on ? 'Unpin' : 'Pin'}
                  onClick={() => toggleFavorite(t.id)}>
                  <Pin size={14} aria-hidden fill={on ? 'currentColor' : 'none'} />
                </button>
              </li>
            );
          })}
          {shown.length === 0 && <li className="tools-menu-empty">No tools match “{q}”.</li>}
        </ul>
      </div>
      <div className="tools-menu-foot"><span>↵ open · click the pin to keep a tool in the sidebar</span><span>T opens this menu</span></div>
    </div>,
    document.body,
  );
}
