import { memo, useMemo } from 'react';
import { useApp } from '../lib/app';
import type { BenchItem } from '../lib/bench';
import { formatBytes, formatRelative } from '../lib/format';
import { href } from '../lib/router';

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

/** One file on the bench, drawn as the kind of thing it is. */
export const BenchObject = memo(function BenchObject({ item }: { item: BenchItem }) {
  const { backend, toolMap } = useApp();
  const url = useMemo(() => {
    try { return backend.artifactUrl(item.artifact); } catch { return ''; }
  }, [backend, item.artifact]);
  const tool = toolMap.get(item.toolId);

  let thing: React.ReactNode;
  switch (item.kind) {
    case 'image':
      thing = url ? <img className="bo-photo" src={url} alt="" loading="lazy" decoding="async" /> : <span className="bo-card"><span className="bo-ext">{extOf(item.name)}</span></span>;
      break;
    case 'pdf':
      thing = (
        <span className="bo-stack" aria-hidden>
          <span className="bo-sheet bo-sheet-3" /><span className="bo-sheet bo-sheet-2" />
          <span className="bo-sheet bo-page"><span className="bo-line bo-title" /><span className="bo-line bo-w40" /><span className="bo-line" /><span className="bo-line bo-w90" /><span className="bo-line" /><span className="bo-line bo-w60" /><span className="bo-line" /><span className="bo-line bo-w80" /></span>
        </span>
      );
      break;
    case 'video':
      thing = (
        <span className="bo-film" aria-hidden>
          <span className="bo-sprockets" />
          {url ? <video className="bo-frame" src={`${url}#t=0.1`} muted preload="metadata" playsInline /> : <span className="bo-frame" />}
          <span className="bo-sprockets" />
        </span>
      );
      break;
    case 'audio':
      thing = <span className="bo-wave" aria-hidden>{bars(item.name, 42).map((h, i) => <span key={i} style={{ height: h }} />)}</span>;
      break;
    case 'sheet':
      thing = <span className="bo-grid" aria-hidden />;
      break;
    case 'text':
      thing = <span className="bo-sheet bo-text" aria-hidden><span className="bo-code bo-w60" /><span className="bo-code bo-w80 bo-in" /><span className="bo-code bo-w70 bo-in" /><span className="bo-code bo-w50 bo-in" /><span className="bo-code bo-w30" /></span>;
      break;
    default:
      thing = <span className="bo-card" aria-hidden><span className="bo-ext">{extOf(item.name)}</span></span>;
  }

  return (
    <a className={`bench-object bo-${item.kind}`} href={href.tool(item.toolId)} title={item.name}>
      <span className="bo-thing">{thing}</span>
      <span className="bo-caption">
        <span className="bo-name">{item.name}</span>
        <span className="bo-meta">{tool ? tool.name : formatBytes(item.size)} · {formatRelative(item.at)}</span>
      </span>
    </a>
  );
});
