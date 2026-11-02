import { memo, useMemo } from 'react';
import type { BenchKind } from '../lib/bench';
import { useApp } from '../lib/app';
import { benchKindClass, type BenchItem } from '../lib/bench';
import { formatBytes, formatRelative } from '../lib/format';
import { href, navigate } from '../lib/router';
import { setDropped } from '../lib/store';

/** Deterministic pseudo-random bar heights so a file always draws the same waveform. */
function bars(seed: string, n: number) {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    h = Math.imul(h ^ (h >>> 13), 1274126177);
    const r = ((h >>> 0) % 1000) / 1000;
    out.push(Math.round(8 + r * 36 * (0.45 + 0.55 * Math.sin(i / 4) ** 2)));
  }
  return out;
}

const extOf = (name: string) => (name.includes('.') ? name.split('.').pop()!.toUpperCase().slice(0, 5) : 'FILE');

/** A file drawn as the kind of thing it is: a photo, a stack of pages, a strip of film, a waveform. */
export function FileThing({ kind, name, url }: { kind: BenchKind; name: string; url?: string }) {
  switch (kind) {
    case 'image':
      return url ? <img className="bo-photo" src={url} alt="" loading="lazy" decoding="async" /> : <span className="bo-card"><span className="bo-ext">{extOf(name)}</span></span>;
    case 'pdf':
      return (
        <span className="bo-stack" aria-hidden>
          <span className="bo-sheet bo-sheet-3" /><span className="bo-sheet bo-sheet-2" />
          <span className="bo-sheet bo-page"><span className="bo-line bo-title" /><span className="bo-line bo-w40" /><span className="bo-line" /><span className="bo-line bo-w90" /><span className="bo-line" /><span className="bo-line bo-w60" /><span className="bo-line" /><span className="bo-line bo-w80" /></span>
        </span>
      );
    case 'video':
      return (
        <span className="bo-film" aria-hidden>
          <span className="bo-sprockets" />
          {url ? <video className="bo-frame" src={`${url}#t=0.1`} muted preload="metadata" playsInline /> : <span className="bo-frame" />}
          <span className="bo-sprockets" />
        </span>
      );
    case 'audio':
      return <span className="bo-wave" aria-hidden>{bars(name, 42).map((h, i) => <span key={i} style={{ height: h }} />)}</span>;
    case 'sheet':
      return <span className="bo-grid" aria-hidden />;
    case 'text':
      return <span className="bo-sheet bo-text" aria-hidden><span className="bo-code bo-w60" /><span className="bo-code bo-w80 bo-in" /><span className="bo-code bo-w70 bo-in" /><span className="bo-code bo-w50 bo-in" /><span className="bo-code bo-w30" /></span>;
    default:
      return <span className="bo-card" aria-hidden><span className="bo-ext">{extOf(name)}</span></span>;
  }
}

/** One file on the bench. Clicking it asks what to do with it next. */
export const BenchObject = memo(function BenchObject({ item }: { item: BenchItem }) {
  const { backend, toolMap } = useApp();
  const url = useMemo(() => {
    try { return backend.artifactUrl(item.artifact); } catch { return ''; }
  }, [backend, item.artifact]);
  const tool = toolMap.get(item.toolId);
  const ref = item.artifact.id ?? item.artifact.path;

  return (
    <a
      className={`bench-object ${benchKindClass(item.kind)}`}
      href={ref ? href.open : href.tool(item.toolId)}
      title={item.name}
      onClick={(e) => {
        if (!ref || e.metaKey || e.ctrlKey || e.shiftKey) return;
        e.preventDefault();
        setDropped([{ ref, name: item.name, size: item.size, mime: item.mime }]);
        navigate(href.open);
      }}
    >
      <span className="bo-thing"><FileThing kind={item.kind} name={item.name} url={url} /></span>
      <span className="bo-caption">
        <span className="bo-name">{item.name}</span>
        <span className="bo-meta">{tool ? tool.name : formatBytes(item.size)} · {formatRelative(item.at)}</span>
      </span>
    </a>
  );
});
