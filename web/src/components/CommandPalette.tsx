import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  Clock, CornerDownLeft, Folder, History, House, Moon, Search, Settings, Shield, Pin, Sun, Package, KeyRound, Sparkles, type LucideIcon,
} from 'lucide-react';
import { useApp } from '../lib/app';
import { navigate, href } from '../lib/router';
import { search, fuzzyMatch } from '../lib/search';
import { matchIntent } from '../lib/intent';
import { cycleTheme, favorites as favStore, recents as recStore, resolvedTheme, useStore } from '../lib/store';
import { categoryIcon } from '../lib/icons';
import type { ToolInfo } from '../lib/types';
import { Highlight, Kbd, cx, modKey } from './ui';
import { unavailableLabel } from './ToolRow';
import { looksLikeRequest, useAssistant } from '../lib/assistant/useAssistant';

interface Item {
  key: string;
  kind: 'tool' | 'action' | 'category';
  label: string;
  ranges: [number, number][];
  sub?: string;
  icon: LucideIcon;
  hint?: string;
  section: string;
  run: () => void;
  tool?: ToolInfo;
}

export function CommandPalette({ open, onClose, initialQuery = '' }: { open: boolean; onClose: () => void; initialQuery?: string }) {
  const { docs, toolMap, categories, isServer, isAdmin, categoryLabel } = useApp();
  const favs = useStore(favStore);
  const recs = useStore(recStore);
  const assistant = useAssistant();
  const [q, setQ] = useState('');
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const prevFocus = useRef<Element | null>(null);

  useLayoutEffect(() => {
    if (open) {
      prevFocus.current = document.activeElement;
      setQ(initialQuery);
      setActive(0);
      input.current?.focus();
    } else {
      (prevFocus.current as HTMLElement | null)?.focus?.();
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const actions = useMemo<Item[]>(() => {
    const dark = resolvedTheme() === 'dark';
    const a: Omit<Item, 'ranges' | 'section'>[] = [
      { key: 'a:home', kind: 'action', label: 'Go to Home', icon: House, run: () => navigate(href.home) },
      { key: 'a:theme', kind: 'action', label: dark ? 'Switch to light theme' : 'Switch to dark theme', icon: dark ? Sun : Moon, run: cycleTheme },
      { key: 'a:history', kind: 'action', label: 'Open run history', icon: History, run: () => navigate(href.history) },
      { key: 'a:settings', kind: 'action', label: 'Open settings', icon: Settings, run: () => navigate(href.settings()) },
      { key: 'a:deps', kind: 'action', label: 'Dependencies', sub: 'Settings', icon: Package, run: () => navigate(href.settings('dependencies')) },
      { key: 'a:favs', kind: 'action', label: 'Pinned tools', icon: Pin, run: () => navigate(href.favorites) },
      { key: 'a:recent', kind: 'action', label: 'Recently used', icon: Clock, run: () => navigate(href.recent) },
    ];
    if (isServer) {
      a.push({ key: 'a:files', kind: 'action', label: 'My files', icon: Folder, run: () => navigate(href.files) });
      a.push({ key: 'a:tokens', kind: 'action', label: 'API tokens', icon: KeyRound, run: () => navigate(`${href.account}?tab=tokens`) });
    }
    if (isServer && isAdmin) {
      a.push({ key: 'a:admin', kind: 'action', label: 'Admin: users & agents', icon: Shield, run: () => navigate(href.admin('users')) });
      a.push({ key: 'a:audit', kind: 'action', label: 'Admin: audit log', icon: Shield, run: () => navigate(href.admin('audit')) });
    }
    return a.map((x) => ({ ...x, ranges: [], section: 'Actions' }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isServer, isAdmin, open]);

  const items = useMemo<Item[]>(() => {
    const toolItem = (t: ToolInfo, ranges: [number, number][], section: string): Item => ({
      key: `t:${t.id}`, kind: 'tool', label: t.name, ranges, sub: t.description, icon: categoryIcon(t.category),
      hint: categoryLabel(t.category), section, tool: t, run: () => navigate(href.tool(t.id)),
    });
    const query = q.trim();
    if (!query) {
      const out: Item[] = [];
      const seen = new Set<string>();
      for (const r of recs.slice(0, 6)) {
        const t = toolMap.get(r.id);
        if (t && t.permitted !== false) { out.push(toolItem(t, [], 'Recent')); seen.add(t.id); }
      }
      for (const id of favs) {
        const t = toolMap.get(id);
        if (t && !seen.has(id) && t.permitted !== false) { out.push(toolItem(t, [], 'Pinned')); seen.add(id); }
        if (out.length >= 12) break;
      }
      return [...out, ...actions];
    }
    const recentIds = new Map(recs.map((r, i) => [r.id, i]));
    const favSet = new Set(favs);
    const hits = search(docs, query, {
      limit: 40,
      boost: (id) => (favSet.has(id) ? 6 : 0) + (recentIds.has(id) ? Math.max(0, 8 - recentIds.get(id)!) : 0),
    });
    const tools = hits.map((h) => toolItem(h.item, h.ranges, 'Tools'));
    // Home invites "say what you need": understand plain words ("make this smaller",
    // "no location") that name no tool, and offer what they usually mean.
    const found = new Set(hits.map((h) => h.item.id));
    const meant = matchIntent(docs.map((d) => d.item).filter((t) => t.permitted !== false), docs, query)
      .filter((t) => !found.has(t.id))
      .slice(0, Math.max(0, 8 - tools.length))
      .map((t) => toolItem(t, [], 'Suggested'));
    const ql = query.toLowerCase();
    const cats: Item[] = categories
      .filter((c) => c.label.toLowerCase().includes(ql) || c.id.startsWith(ql))
      .map((c) => ({
        key: `c:${c.id}`, kind: 'category', label: c.label, ranges: [], sub: `${c.count} tools`, icon: categoryIcon(c.id),
        section: 'Categories', run: () => navigate(href.category(c.id)),
      }));
    const acts = actions
      .map((a) => ({ a, m: fuzzyMatch(a.label.toLowerCase(), ql) }))
      .filter((x) => x.m && x.m.score >= ql.length * 2.5)
      .sort((x, y) => y.m!.score - x.m!.score)
      .slice(0, 4)
      .map(({ a, m }) => ({ ...a, ranges: toRanges(m!.positions) }));
    // Natural-language mode: carry out the words themselves. A sentence
    // goes first; a word or two stays below the tools it names.
    const sentence = looksLikeRequest(query);
    const doIt: Item[] = assistant.on
      ? [{
        key: 'a:do', kind: 'action', label: `Do it: “${query}”`, ranges: [], icon: Sparkles,
        sub: assistant.missing ? `${assistant.missing} first` : `With ${assistant.label}`, hint: `${modKey} ↵`,
        section: 'In plain words', run: () => navigate(href.do(query)),
      }]
      : sentence && !tools.length
        ? [{
          key: 'a:do-setup', kind: 'action', label: 'Do this in plain words…', ranges: [], icon: Sparkles,
          sub: 'Set up a model to run requests like this for you', section: 'In plain words', run: () => navigate(href.do(query)),
        }]
        : [];
    return sentence || !tools.length
      ? [...doIt, ...tools, ...meant, ...cats, ...acts]
      : [...tools, ...doIt, ...meant, ...cats, ...acts];
  }, [q, docs, recs, favs, toolMap, actions, categories, categoryLabel, assistant.on, assistant.missing, assistant.label]);

  useEffect(() => { setActive(0); }, [q]);
  useEffect(() => {
    list.current?.querySelector(`[data-idx="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  }, [active]);

  if (!open) return null;

  const choose = (it: Item | undefined) => {
    if (!it) return;
    onClose();
    it.run();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setActive((a) => Math.min(items.length - 1, a + 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActive((a) => Math.max(0, a - 1)); }
    else if (e.key === 'PageDown') { e.preventDefault(); setActive((a) => Math.min(items.length - 1, a + 8)); }
    else if (e.key === 'PageUp') { e.preventDefault(); setActive((a) => Math.max(0, a - 8)); }
    else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && assistant.on && q.trim()) { e.preventDefault(); onClose(); navigate(href.do(q.trim())); }
    else if (e.key === 'Enter') { e.preventDefault(); choose(items[active]); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); }
    else if (e.key === 'Tab') e.preventDefault();
  };

  let lastSection = '';
  return createPortal(
    <div className="overlay overlay-top" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="palette" role="dialog" aria-modal="true" aria-label="Command palette" onKeyDown={onKey}>
        <div className="palette-input-row">
          <Search size={17} aria-hidden className="palette-search-icon" />
          <input
            ref={input}
            className="palette-input"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={assistant.on ? 'Search tools, or say what you want done…' : 'Search tools and actions…'}
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={items[active] ? `pal-${active}` : undefined}
            aria-autocomplete="list"
            spellCheck={false}
            autoComplete="off"
          />
          <Kbd>Esc</Kbd>
        </div>
        <div className="palette-list" id="palette-list" role="listbox" ref={list} aria-label="Results">
          {items.length === 0 && <div className="palette-empty">No tools match “{q}”.</div>}
          {items.map((it, i) => {
            const header = it.section !== lastSection ? it.section : null;
            lastSection = it.section;
            const Icon = it.icon;
            const unavailable = it.tool ? unavailableLabel(it.tool) : null;
            return (
              <div key={it.key} role="presentation">
                {header && <div className="palette-section" role="presentation">{header}</div>}
                <div
                  id={`pal-${i}`}
                  data-idx={i}
                  role="option"
                  aria-selected={i === active}
                  className={cx('palette-item', i === active && 'is-active', unavailable && 'is-unavailable')}
                  onMouseMove={() => { if (active !== i) setActive(i); }}
                  onClick={() => choose(it)}
                >
                  <span className="palette-item-icon" aria-hidden><Icon size={16} /></span>
                  <span className="palette-item-text">
                    <span className="palette-item-label"><Highlight text={it.label} ranges={it.ranges} /></span>
                    {it.sub && <span className="palette-item-sub">{it.sub}</span>}
                  </span>
                  {unavailable && <span className="palette-item-warn">{unavailable}</span>}
                  {it.hint && <span className="palette-item-hint">{it.hint}</span>}
                  {i === active && <CornerDownLeft size={14} aria-hidden className="palette-enter" />}
                </div>
              </div>
            );
          })}
        </div>
        <div className="palette-foot" aria-hidden>
          <span><Kbd>↑</Kbd><Kbd>↓</Kbd> navigate</span>
          <span><Kbd>↵</Kbd> open</span>
          {assistant.on && <span><Kbd>{modKey} ↵</Kbd> do it</span>}
          <span><Kbd>Esc</Kbd> close</span>
        </div>
      </div>
    </div>,
    document.body,
  );
}

function toRanges(positions: number[]): [number, number][] {
  const out: [number, number][] = [];
  for (const p of positions) {
    const l = out[out.length - 1];
    if (l && l[1] === p) l[1] = p + 1;
    else out.push([p, p + 1]);
  }
  return out;
}
