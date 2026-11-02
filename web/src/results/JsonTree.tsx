import { memo, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import { cx } from '../components/ui';

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object';

function Scalar({ v }: { v: unknown }) {
  if (v === null) return <span className="j-null">null</span>;
  if (typeof v === 'string') return <span className="j-str">"{v}"</span>;
  if (typeof v === 'number') return <span className="j-num">{String(v)}</span>;
  if (typeof v === 'boolean') return <span className="j-bool">{String(v)}</span>;
  return <span>{String(v)}</span>;
}

const Node = memo(function Node({ k, v, depth, defaultOpen }: { k?: string; v: unknown; depth: number; defaultOpen: number }) {
  const [open, setOpen] = useState(depth < defaultOpen);
  const keyEl = k !== undefined ? <span className="j-key">{k}</span> : null;
  if (!isObj(v)) {
    return <div className="j-row">{keyEl}{keyEl && <span className="j-colon">: </span>}<Scalar v={v} /></div>;
  }
  const arr = Array.isArray(v);
  const entries = arr ? (v as unknown[]).map((x, i) => [String(i), x] as const) : Object.entries(v);
  const summary = arr ? `[${entries.length}]` : `{${entries.length}}`;
  return (
    <div className="j-node">
      <button type="button" className="j-toggle" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <ChevronRight size={12} aria-hidden className={cx('chev', open && 'is-open')} />
        {keyEl}{keyEl && <span className="j-colon">: </span>}
        <span className="j-sum">{summary}</span>
      </button>
      {open && (
        <div className="j-children">
          {entries.slice(0, 500).map(([ck, cv]) => <Node key={ck} k={ck} v={cv} depth={depth + 1} defaultOpen={defaultOpen} />)}
          {entries.length > 500 && <div className="j-row muted">… {entries.length - 500} more</div>}
        </div>
      )}
    </div>
  );
});

export function JsonTree({ value, defaultOpen = 2 }: { value: unknown; defaultOpen?: number }) {
  return <div className="json-tree mono"><Node v={value} depth={0} defaultOpen={defaultOpen} /></div>;
}
