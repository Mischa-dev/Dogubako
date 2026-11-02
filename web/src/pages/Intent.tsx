import { useEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeft, ArrowRight, ChevronDown, Pin } from 'lucide-react';
import { useApp } from '../lib/app';
import { href, navigate } from '../lib/router';
import { benchKindClass, kindOf } from '../lib/bench';
import { matchIntent, rankTools, shortHint, toolsForFiles } from '../lib/intent';
import { droppedNow, favorites, pendingInput, recents, setDropped, useStore, type PendingInput } from '../lib/store';
import { formatBytes, plural } from '../lib/format';
import type { ToolInfo } from '../lib/types';
import { FileThing } from '../components/BenchObject';
import { Kbd, cx } from '../components/ui';
import { pickAndOpen } from '../components/DropOverlay';

const TOP = 6;

function FilePreview({ files }: { files: PendingInput[] }) {
  const { backend } = useApp();
  const shown = files.slice(0, 3);
  return (
    <div className={cx('intent-files', files.length > 1 && 'is-many')} aria-hidden>
      {shown.map((f, i) => {
        let url = '';
        try { url = backend.fileUrl(f); } catch { /* preview is optional */ }
        const kind = kindOf(f.mime, f.name);
        return (
          <span key={f.ref} className={`intent-thing ${benchKindClass(kind)}`} style={{ '--i': i, '--n': shown.length } as React.CSSProperties}>
            <span className="bo-thing"><FileThing kind={kind} name={f.name} url={url} /></span>
          </span>
        );
      })}
    </div>
  );
}

/** "What should happen to it?" — shown after a file is dropped, picked, or taken from the bench. */
export default function IntentPage() {
  const { visibleTools, docs } = useApp();
  const pinned = useStore(favorites);
  const recent = useStore(recents);
  const [files] = useState<PendingInput[]>(() => droppedNow());
  const [q, setQ] = useState('');
  const [active, setActive] = useState(0);
  const [all, setAll] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const cands = useMemo(() => toolsForFiles(visibleTools, files), [visibleTools, files]);
  const ranked = useMemo(() => rankTools(cands, files, { pinned, recent: recent.map((r) => r.id) }), [cands, files, pinned, recent]);
  const list = useMemo(() => (q.trim() ? matchIntent(ranked, docs, q) : ranked), [ranked, docs, q]);
  const top = list.slice(0, TOP);
  const rest = list.slice(TOP);

  useEffect(() => { setActive(0); }, [q]);

  const choose = (t: ToolInfo) => {
    pendingInput.value = files;
    navigate(href.tool(t.id));
  };

  const onKey = (e: React.KeyboardEvent) => {
    const visible = all || q.trim() ? list : top;
    if (e.key === 'ArrowDown') { e.preventDefault(); setActive((a) => Math.min(a + 1, visible.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setActive((a) => Math.max(a - 1, 0)); }
    else if (e.key === 'Enter' && visible[active]) { e.preventDefault(); choose(visible[active]); }
    else if (e.key === 'Escape') { e.preventDefault(); if (q) setQ(''); else history.back(); }
    else if (!q && /^[1-9]$/.test(e.key) && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const t = top[Number(e.key) - 1];
      if (t) { e.preventDefault(); choose(t); }
    }
  };

  useEffect(() => {
    const el = document.querySelector<HTMLElement>(`[data-intent-i="${active}"]`);
    el?.scrollIntoView?.({ block: 'nearest' });
  }, [active]);

  if (!files.length) {
    return (
      <div className="page page-intent is-empty">
        <h1 className="intent-title" tabIndex={-1} data-route-focus>Nothing to work on yet</h1>
        <p className="intent-sub">Drop files anywhere on this window, or choose some.</p>
        <button className="btn btn-primary" onClick={() => void pickAndOpen()}>Choose files</button>
      </div>
    );
  }

  const total = files.reduce((n, f) => n + (f.size || 0), 0);
  const label = files.length === 1 ? files[0].name : plural(files.length, 'file');
  const row = (t: ToolInfo, i: number, n?: number) => (
    <li key={t.id}>
      <button
        type="button"
        className={cx('intent-option', i === active && 'is-active', !t.availability.available && 'is-unavailable')}
        data-intent-i={i}
        onMouseEnter={() => setActive(i)}
        onClick={() => choose(t)}
      >
        {n !== undefined && <span className="intent-num" aria-hidden>{n}</span>}
        <span className="intent-option-text">
          <span className="intent-option-name">{t.name}{pinned.includes(t.id) && <Pin size={11} className="intent-pin" aria-label="Pinned" />}</span>
          <span className="intent-option-hint">{t.availability.available ? shortHint(t) : 'Needs software that isn’t installed'}</span>
        </span>
        <ArrowRight size={15} className="intent-go" aria-hidden />
      </button>
    </li>
  );

  return (
    <div className="page page-intent">
      <a className="link-sm intent-back" href={href.home} onClick={(e) => { e.preventDefault(); history.length > 1 ? history.back() : navigate(href.home); }}>
        <ArrowLeft size={13} aria-hidden /> Back
      </a>
      <div className="intent-layout">
        <figure className="intent-figure">
          <FilePreview files={files} />
          <figcaption className="intent-caption">
            <span className="intent-name" title={files.map((f) => f.name).join('\n')}>{label}</span>
            <span className="intent-meta">{formatBytes(total)}{files.length > 1 ? ` · ${files.slice(0, 3).map((f) => f.name).join(', ')}${files.length > 3 ? '…' : ''}` : ''}</span>
          </figcaption>
        </figure>

        <section className="intent-choices" aria-labelledby="intent-title">
          <h1 id="intent-title" className="intent-title">What should happen to {files.length === 1 ? 'it' : 'them'}?</h1>
          <div className="intent-input-row">
            <input
              ref={input}
              className="intent-input"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              onKeyDown={onKey}
              placeholder="Type what you want, like “smaller” or “jpg”"
              aria-label="Say what should happen"
              aria-controls="intent-list"
              autoComplete="off"
              data-route-focus
            />
            {!q && top.length > 0 && <span className="intent-keys"><Kbd>1</Kbd>–<Kbd>{Math.min(top.length, 9)}</Kbd></span>}
          </div>

          {list.length === 0 ? (
            <p className="intent-none">
              {cands.length === 0 ? `No tool takes ${files.length === 1 ? 'this kind of file' : 'all of these files together'}.` : `Nothing matches “${q}”.`}{' '}
              <button type="button" className="link-sm" onClick={() => setQ('')}>{q ? 'Clear' : ''}</button>
            </p>
          ) : (
            <ul id="intent-list" className="intent-list" role="list">
              {(q.trim() ? list : top).map((t, i) => row(t, i, !q.trim() && i < 9 ? i + 1 : undefined))}
              {!q.trim() && all && rest.map((t, i) => row(t, TOP + i))}
            </ul>
          )}

          {!q.trim() && rest.length > 0 && (
            <button type="button" className="intent-more" onClick={() => setAll((a) => !a)} aria-expanded={all}>
              {all ? 'Fewer' : `All ${cands.length} tools for ${files.length === 1 ? 'this file' : 'these files'}`}
              <ChevronDown size={14} className={cx(all && 'is-flipped')} aria-hidden />
            </button>
          )}

          <p className="intent-foot">
            <button type="button" className="link-sm" onClick={() => void pickAndOpen()}>Choose different files</button>
            <span aria-hidden> · </span>
            <button type="button" className="link-sm" onClick={() => { setDropped([]); navigate(href.home); }}>Put {files.length === 1 ? 'it' : 'them'} away</button>
          </p>
        </section>
      </div>
    </div>
  );
}
