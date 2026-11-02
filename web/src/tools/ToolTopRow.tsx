import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeft, Ellipsis, PanelsTopLeft, Search } from 'lucide-react';
import type { ToolInfo } from '../lib/types';
import { useApp } from '../lib/app';
import { href, navigate } from '../lib/router';
import { categoryIcon } from '../lib/icons';
import { kindOf, type BenchKind } from '../lib/bench';
import { rankTools, toolsForFiles } from '../lib/intent';
import {
  TOOL_LAYOUT_DEFAULT, favorites, pendingInput, recents, setDropped, toolLayout, useStore,
  type PendingInput, type ToolLayout,
} from '../lib/store';
import { Kbd, cx, modKey } from '../components/ui';
import { nextUses, useNextUses } from './nextUses';
import { useToolSubject } from './subject';

const KIND_WORDS: Partial<Record<BenchKind, RegExp>> = {
  image: /\b(images?|photos?)\b/gi,
  pdf: /\bpdfs?\b/gi,
  video: /\bvideos?\b/gi,
  audio: /\baudio\b/gi,
  sheet: /\b(csv|spreadsheets?)\b/gi,
};

/** "Compress Images" reads as "Compress" when the row is already about a photo. */
export function verbLabel(name: string, kind: BenchKind | null): string {
  if (!kind || / to /i.test(name)) return name;
  const re = KIND_WORDS[kind];
  if (!re) return name;
  const out = name.replace(re, '').replace(/\s{2,}/g, ' ').replace(/^[\s&]+|[\s&]+$/g, '');
  return out || name;
}

const MAX_TABS = 6;
const MAX_PINNED_TABS = 5;

/**
 * Which tools the row offers. With a file: the tools that take it. A tool that
 * takes no file (a generator, a converter of typed text) has no file to follow,
 * so in the default "tools for the file" mode it shows its neighbours from the
 * same category, pinned ones first, instead of unrelated pinned tools.
 * In "pinned tools" mode, or for a file tool still waiting for its file, the
 * row shows pinned tools (or the category when nothing is pinned).
 * The current tool is always in the row.
 */
export function rowTabs({ tool, files, row, visibleTools, toolMap, pinned, recent }: {
  tool: ToolInfo;
  files: PendingInput[];
  row: ToolLayout['row'];
  visibleTools: ToolInfo[];
  toolMap: Map<string, ToolInfo>;
  pinned: string[];
  recent: string[];
}): ToolInfo[] {
  const byFile = files.length > 0 && row === 'file';
  const sameCategory = () => visibleTools.filter((t) => t.category === tool.category);
  let list: ToolInfo[];
  if (byFile) {
    list = rankTools(toolsForFiles(visibleTools, files), files, { pinned, recent }).filter((t) => t.availability.available);
  } else if (row === 'file' && !tool.params.some((p) => p.type === 'file')) {
    const rank = (t: ToolInfo) => (pinned.includes(t.id) ? 0 : 1);
    list = sameCategory().map((t, i) => [t, i] as const).sort((a, b) => rank(a[0]) - rank(b[0]) || a[1] - b[1]).map(([t]) => t);
  } else {
    list = pinned.map((id) => toolMap.get(id)).filter((t): t is ToolInfo => !!t);
    if (!list.length) list = sameCategory();
  }
  const max = byFile ? MAX_TABS : MAX_PINNED_TABS;
  const top = list.slice(0, max);
  if (!top.some((t) => t.id === tool.id)) top.splice(Math.min(top.length, max - 1), 1, tool);
  return top;
}

/** Open the command palette (owned by the shell). */
export const openSearch = () => window.dispatchEvent(new CustomEvent('dgb:search'));

/**
 * The row above a tool: what you are working on, the other things you can do to it,
 * search, and layout. Choosing another tool takes the file (or the result) with you.
 */
export function ToolTopRow({ tool }: { tool: ToolInfo }) {
  const { visibleTools, toolMap, backend, categoryLabel } = useApp();
  const subject = useToolSubject(tool.id);
  const layout = useStore(toolLayout);
  const pinned = useStore(favorites);
  const recent = useStore(recents);
  const uses = useNextUses();
  const files = subject?.files ?? [];
  const results = subject?.results ?? [];
  const kind = files.length ? kindOf(files[0].mime, files[0].name) : null;

  const tabs = useMemo(
    () => rowTabs({ tool, files, row: layout.row, visibleTools, toolMap, pinned, recent: recent.map((r) => r.id) }),
    [files, layout.row, visibleTools, pinned, recent, toolMap, tool],
  );
  const takesFiles = tool.params.some((p) => p.type === 'file');
  const rowLabel = files.length ? 'Other tools for this file' : layout.row === 'file' && !takesFiles ? 'Related tools' : 'Pinned tools';

  const handOff = (): PendingInput[] => (uses === 'result' && results.length ? results : files);
  const go = (t: ToolInfo) => (e: React.MouseEvent) => {
    if (t.id === tool.id || e.metaKey || e.ctrlKey) return;
    const hand = handOff();
    if (hand.length) pendingInput.value = hand;
  };
  const more = () => {
    const hand = handOff();
    if (hand.length) { setDropped(hand); navigate(`${href.open}?v=${Date.now().toString(36)}`); } else openSearch();
  };

  // Fade the end of the tab row only when it actually scrolls.
  const tabsRef = useRef<HTMLElement>(null);
  const [overflow, setOverflow] = useState(false);
  useLayoutEffect(() => {
    const el = tabsRef.current;
    if (!el) return;
    const check = () => setOverflow(el.scrollWidth > el.clientWidth + 1 && el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
    // Keep the current tool's tab in view when the row is too long to show it all:
    // scroll only as far as needed to clear the fade, and redo it when the row resizes.
    const reveal = () => {
      const on = el.querySelector<HTMLElement>('.tool-tab.is-on');
      el.scrollLeft = on ? Math.max(0, on.offsetLeft + on.offsetWidth - el.clientWidth + 36) : 0;
      check();
    };
    reveal();
    const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(reveal) : null;
    ro?.observe(el);
    el.addEventListener('scroll', check, { passive: true });
    return () => { ro?.disconnect(); el.removeEventListener('scroll', check); };
  }, [tabs]);

  const CatIcon = categoryIcon(tool.category);
  const thumb = files[0] && kind === 'image' ? safeUrl(() => backend.fileUrl(files[0])) : '';

  return (
    <div className="tool-row" role="toolbar" aria-label="Tool">
      <button
        type="button"
        className="icon-btn tool-back glass"
        aria-label="Back"
        title="Back"
        onClick={() => (history.length > 1 ? history.back() : navigate(href.home))}
      >
        <ArrowLeft size={16} aria-hidden />
      </button>

      {files.length ? (
        <span className="tool-subject" title={files.map((f) => f.name).join('\n')}>
          {thumb ? <img className="tool-subject-thumb" src={thumb} alt="" /> : <span className={`tool-subject-kind k-${kind}`} aria-hidden />}
          <span className="tool-subject-name">{files[0].name}</span>
          {files.length > 1 && <span className="tool-subject-more">+{files.length - 1}</span>}
        </span>
      ) : (
        <a className="tool-subject tool-subject-cat" href={href.category(tool.category)}>
          <CatIcon size={14} aria-hidden /><span className="tool-subject-name">{categoryLabel(tool.category)}</span>
        </a>
      )}

      <nav ref={tabsRef} className={cx('tool-tabs glass', overflow && 'is-overflow')} aria-label={rowLabel}>
        {tabs.map((t) => (
          <a
            key={t.id}
            href={href.tool(t.id)}
            className={cx('tool-tab', t.id === tool.id && 'is-on')}
            aria-current={t.id === tool.id ? 'page' : undefined}
            title={t.name}
            onClick={go(t)}
          >
            {verbLabel(t.name, files.length ? kind : null)}
          </a>
        ))}
        <button type="button" className="tool-tab tool-tab-more" onClick={more} aria-label={files.length ? 'All tools for this file' : 'Search tools'} title={files.length ? 'All tools for this file' : 'Search tools'}>
          <Ellipsis size={15} aria-hidden />
        </button>
      </nav>

      <span className="tool-row-gap" />

      <button type="button" className="tool-search" onClick={openSearch} aria-keyshortcuts="Control+K Meta+K">
        <Search size={14} aria-hidden /><span className="tool-search-text">Search tools and files</span><Kbd>{modKey} K</Kbd>
      </button>
      <LayoutButton layout={layout} />
    </div>
  );
}

function safeUrl(f: () => string) {
  try { return f(); } catch { return ''; }
}

const PLACES: { value: ToolLayout['controls']; label: string }[] = [
  { value: 'below', label: 'Below' },
  { value: 'left', label: 'Left' },
  { value: 'right', label: 'Right' },
];

function LayoutButton({ layout }: { layout: ToolLayout }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', down);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', down); document.removeEventListener('keydown', key); };
  }, [open]);
  const set = (patch: Partial<ToolLayout>) => toolLayout.set((l) => ({ ...l, ...patch }));
  const uses = useNextUses();

  return (
    <div className="layout-wrap" ref={box}>
      <button
        type="button"
        className={cx('icon-btn tool-layout-btn glass', open && 'is-on')}
        aria-label="Layout"
        title="Layout"
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((o) => !o)}
      >
        <PanelsTopLeft size={16} aria-hidden />
      </button>
      {open && (
        <div className="layout-pop" role="dialog" aria-label="Layout">
          <h2 className="layout-title">Layout</h2>
          <div className="layout-group">
            <span className="layout-label" id="lay-controls">Controls</span>
            <div className="layout-places" role="group" aria-labelledby="lay-controls">
              {PLACES.map((p) => (
                <button key={p.value} type="button" className={cx('layout-place', layout.controls === p.value && 'is-on')} aria-pressed={layout.controls === p.value} onClick={() => set({ controls: p.value })}>
                  <span className={`layout-pic pic-${p.value}`} aria-hidden><span /></span>
                  {p.label}
                </button>
              ))}
            </div>
          </div>
          <Choice label="Preview" value={layout.preview} onChange={(v) => set({ preview: v })} options={[['side', 'Side by side'], ['result', 'Result only']]} />
          <Choice label="Tool row shows" value={layout.row} onChange={(v) => set({ row: v })} options={[['file', 'Tools for the file'], ['pinned', 'Pinned tools']]} />
          <Choice label="Next tool works on" value={uses} onChange={(v) => nextUses.set(v)} options={[['result', 'The result'], ['original', 'The original']]} />
          <button type="button" className="link-sm layout-reset" onClick={() => { toolLayout.set(TOOL_LAYOUT_DEFAULT); nextUses.set('result'); }}>Reset to default</button>
        </div>
      )}
    </div>
  );
}

function Choice<T extends string>({ label, value, onChange, options }: { label: string; value: T; onChange: (v: T) => void; options: [T, string][] }) {
  return (
    <div className="layout-row">
      <span className="layout-row-label">{label}</span>
      <div className="mini-seg" role="group" aria-label={label}>
        {options.map(([v, l]) => (
          <button key={v} type="button" className={cx(value === v && 'is-on')} aria-pressed={value === v} onClick={() => onChange(v)}>{l}</button>
        ))}
      </div>
    </div>
  );
}
