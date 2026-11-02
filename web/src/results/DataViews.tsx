import { memo, useMemo, useState } from 'react';
import { ArrowDown, ArrowUp, ArrowUpDown, Download } from 'lucide-react';
import {
  compareCells, selectDataViews, tableToCsv,
  type ColorData, type DiffSeg, type Match, type Region, type TableData,
} from '../lib/renderers';
import { humanizeKey } from '../lib/format';
import { downloadText } from '../lib/clipboard';
import { Button, CopyButton, cx } from '../components/ui';
import { JsonTree } from './JsonTree';

export interface InputContext {
  imageUrl?: string;
  text?: string;
}

const MAX_ROWS = 2000;

export const DataTable = memo(function DataTable({ table, caption }: { table: TableData; caption?: string }) {
  const [sort, setSort] = useState<{ col: number; dir: 1 | -1 } | null>(null);
  const rows = useMemo(() => {
    if (!sort) return table.rows;
    return [...table.rows].sort((a, b) => compareCells(a[sort.col], b[sort.col]) * sort.dir);
  }, [table, sort]);
  const toggle = (i: number) => setSort((s) => (s?.col === i ? (s.dir === 1 ? { col: i, dir: -1 } : null) : { col: i, dir: 1 }));
  if (!table.rows.length) return <p className="muted small">No rows.</p>;
  return (
    <div className="data-block">
      <div className="data-block-head">
        <span className="data-block-title">{caption ?? 'Table'} <span className="muted">· {table.rows.length.toLocaleString()} {table.rows.length === 1 ? 'row' : 'rows'}</span></span>
        <span className="data-block-actions">
          <CopyButton text={() => tableToCsv(table)} label="Copy CSV" />
          <Button size="sm" variant="ghost" icon={Download} onClick={() => downloadText(tableToCsv(table), 'table.csv', 'text/csv')}>CSV</Button>
        </span>
      </div>
      <div className="table-wrap">
        <table className="table">
          <thead>
            <tr>
              {table.columns.map((c, i) => {
                const active = sort?.col === i;
                const Icon = active ? (sort!.dir === 1 ? ArrowUp : ArrowDown) : ArrowUpDown;
                return (
                  <th key={i} scope="col" aria-sort={active ? (sort!.dir === 1 ? 'ascending' : 'descending') : 'none'}>
                    <button type="button" className={cx('th-btn', active && 'is-active')} onClick={() => toggle(i)}>
                      {String(c)}
                      <Icon size={12} aria-hidden />
                    </button>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {rows.slice(0, MAX_ROWS).map((r, ri) => (
              <tr key={ri}>
                {table.columns.map((_, ci) => <td key={ci}><Cell v={r[ci]} /></td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {rows.length > MAX_ROWS && <p className="muted small">Showing first {MAX_ROWS.toLocaleString()} rows. Copy or download for all.</p>}
    </div>
  );
});

/** Group digits only for large magnitudes so ports, years and ids stay readable. */
export function fmtNum(v: number): string {
  if (Number.isInteger(v)) return Math.abs(v) >= 1e6 ? v.toLocaleString() : String(v);
  return v.toLocaleString(undefined, { maximumFractionDigits: 6, useGrouping: Math.abs(v) >= 1e6 });
}

function Cell({ v }: { v: unknown }) {
  if (v === true) return <span className="cell-bool is-true">Yes</span>;
  if (v === false) return <span className="cell-bool is-false">No</span>;
  if (v == null) return <span className="muted">—</span>;
  if (typeof v === 'number') return <span className="num">{fmtNum(v)}</span>;
  if (typeof v === 'object') return <span className="mono small">{JSON.stringify(v)}</span>;
  const s = String(v);
  if (/^#[0-9a-f]{6}$/i.test(s)) return <span className="cell-color"><span className="dot" style={{ background: s }} />{s}</span>;
  return <>{s}</>;
}

export function Swatches({ colors }: { colors: ColorData[] }) {
  return (
    <div className="data-block">
      <div className="data-block-head">
        <span className="data-block-title">Colors <span className="muted">· {colors.length}</span></span>
        <span className="data-block-actions"><CopyButton text={() => colors.map((c) => c.hex).join('\n')} label="Copy all" /></span>
      </div>
      <ul className="swatches">
        {colors.map((c, i) => (
          <li key={i} className="swatch">
            <span className="swatch-color" style={{ background: c.hex }} aria-hidden />
            <span className="swatch-meta">
              <span className="swatch-hex mono">{c.hex}</span>
              {c.name && <span className="swatch-name" title={c.name}>{c.name}</span>}
              {extraColorInfo(c as ColorData & Record<string, unknown>)}
            </span>
            <CopyButton text={c.hex} iconOnly label={`Copy ${c.hex}`} />
          </li>
        ))}
      </ul>
    </div>
  );
}

function extraColorInfo(c: ColorData & Record<string, unknown>) {
  const bits: string[] = [];
  if (typeof c.ratio === 'number') bits.push(`${c.ratio}:1`);
  if (typeof c.share === 'number') { const sh = c.share as number; bits.push(`${(sh <= 1 ? sh * 100 : sh).toFixed(1)}%`); }
  if (typeof c.percent === 'number') bits.push(`${c.percent}%`);
  return bits.length ? <span className="swatch-name">{bits.join(' · ')}</span> : null;
}

export function InlineDiff({ diff }: { diff: DiffSeg[] }) {
  const stats = useMemo(() => {
    let ins = 0;
    let del = 0;
    for (const s of diff) { if (s.tag === 'insert') ins += s.text.length; else if (s.tag === 'delete') del += s.text.length; }
    return { ins, del };
  }, [diff]);
  return (
    <div className="data-block">
      <div className="data-block-head">
        <span className="data-block-title">Differences <span className="diff-stat ins">+{stats.ins}</span> <span className="diff-stat del">−{stats.del}</span></span>
      </div>
      {stats.ins === 0 && stats.del === 0 ? (
        <p className="muted small">No differences.</p>
      ) : (
        <pre className="diff mono">
          {diff.map((s, i) => s.tag === 'equal' ? <span key={i}>{s.text}</span> : s.tag === 'insert'
            ? <ins key={i} className="d-ins">{s.text}</ins>
            : <del key={i} className="d-del">{s.text}</del>)}
        </pre>
      )}
    </div>
  );
}

export function RegionOverlay({ regions, imageUrl }: { regions: Region[]; imageUrl?: string }) {
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);
  return (
    <div className="data-block">
      <div className="data-block-head"><span className="data-block-title">Regions <span className="muted">· {regions.length}</span></span></div>
      {imageUrl ? (
        <div className="region-stage">
          <img src={imageUrl} alt="Input with detected regions" onLoad={(e) => setSize({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })} />
          {size && regions.map((r, i) => (
            <span
              key={i}
              className="region-box"
              style={{ left: `${(r.x / size.w) * 100}%`, top: `${(r.y / size.h) * 100}%`, width: `${(r.width / size.w) * 100}%`, height: `${(r.height / size.h) * 100}%` }}
            >
              {(r.label || r.score != null) && <span className="region-label">{r.label ?? ''}{r.score != null ? ` ${Number(r.score).toFixed(2)}` : ''}</span>}
            </span>
          ))}
        </div>
      ) : null}
      {regions.length > 0 && (
        <DataTable caption="Coordinates" table={{ columns: ['#', 'x', 'y', 'width', 'height', 'label', 'score'], rows: regions.map((r, i) => [i + 1, r.x, r.y, r.width, r.height, r.label ?? null, r.score ?? null]) }} />
      )}
    </div>
  );
}

export function MatchHighlight({ matches, text }: { matches: Match[]; text?: string }) {
  const body = useMemo(() => {
    if (!text) return null;
    const out: React.ReactNode[] = [];
    let i = 0;
    const sorted = [...matches].sort((a, b) => a.start - b.start);
    // Positions may be byte or char offsets; we treat them as JS string indices.
    sorted.forEach((m, k) => {
      if (m.start < i) return;
      if (m.start > i) out.push(text.slice(i, m.start));
      out.push(<mark key={k} className={cx('match', k % 2 === 1 && 'alt')} title={`Match ${k + 1}`}>{text.slice(m.start, m.end) || '​'}</mark>);
      i = m.end;
    });
    out.push(text.slice(i));
    return out;
  }, [matches, text]);
  const maxGroups = Math.max(0, ...matches.map((m) => m.groups?.length ?? 0));
  return (
    <div className="data-block">
      <div className="data-block-head"><span className="data-block-title">Matches <span className="muted">· {matches.length}</span></span></div>
      {body && <pre className="match-text mono">{body}</pre>}
      {matches.length > 0 && (
        <DataTable
          caption="Match details"
          table={{
            columns: ['#', 'start', 'end', 'match', ...Array.from({ length: maxGroups }, (_, i) => `group ${i + 1}`)],
            rows: matches.map((m, i) => [i + 1, m.start, m.end, m.text, ...Array.from({ length: maxGroups }, (_, g) => groupText(m.groups?.[g]))]),
          }}
        />
      )}
    </div>
  );
}

const groupText = (g: unknown) => (g == null ? null : typeof g === 'object' ? ((g as any).text ?? JSON.stringify(g)) : g);

/** Render a value inside the key/value list. */
function KvValue({ v }: { v: unknown }) {
  if (v && typeof v === 'object' && !Array.isArray(v) && 'human' in (v as any)) return <>{String((v as any).human)}</>;
  if (v === true) return <span className="cell-bool is-true">Yes</span>;
  if (v === false) return <span className="cell-bool is-false">No</span>;
  if (v == null) return <span className="muted">—</span>;
  if (typeof v === 'number') return <span className="num">{fmtNum(v)}</span>;
  if (typeof v === 'string') {
    if (/^#[0-9a-f]{6}$/i.test(v)) return <span className="cell-color"><span className="dot" style={{ background: v }} />{v}</span>;
    if (v.includes('\n') || v.length > 140) return <pre className="kv-pre mono">{v}</pre>;
    return <span className="kv-str">{v}</span>;
  }
  if (Array.isArray(v) && v.every((x) => typeof x !== 'object' || x === null) && v.length <= 30) {
    return v.length ? <span className="kv-list">{v.map((x, i) => <span key={i} className="kv-pill">{String(x)}</span>)}</span> : <span className="muted">none</span>;
  }
  return <JsonTree value={v} defaultOpen={1} />;
}

export function KeyValue({ value }: { value: Record<string, unknown> }) {
  const entries = Object.entries(value);
  const copyable = (v: unknown) => typeof v === 'string' || typeof v === 'number';
  return (
    <div className="data-block">
      <dl className="kv">
        {entries.map(([k, v]) => (
          <div className="kv-row" key={k}>
            <dt>{humanizeKey(k)}</dt>
            <dd>
              <KvValue v={v} />
              {copyable(v) && String(v).length > 6 && <CopyButton text={String(v)} iconOnly label={`Copy ${humanizeKey(k)}`} className="kv-copy" />}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

export function DataViews({ data, input }: { data: unknown; input?: InputContext }) {
  const views = useMemo(() => selectDataViews(data), [data]);
  if (!views.length) return null;
  return (
    <div className="data-views">
      {views.map((v, i) => {
        switch (v.kind) {
          case 'table': return <DataTable key={i} table={v.table} />;
          case 'colors': return <Swatches key={i} colors={v.colors} />;
          case 'diff': return <InlineDiff key={i} diff={v.diff} />;
          case 'regions': return <RegionOverlay key={i} regions={v.regions} imageUrl={input?.imageUrl} />;
          case 'matches': return <MatchHighlight key={i} matches={v.matches} text={input?.text} />;
          case 'kv': return <KeyValue key={i} value={v.value} />;
          default: return <div key={i} className="data-block"><JsonTree value={v.value} /></div>;
        }
      })}
    </div>
  );
}
