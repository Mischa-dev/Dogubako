import type { ToolInfo } from './types';

export interface SearchDoc {
  id: string;
  name: string;
  nameLc: string;
  idLc: string;
  keywords: string[];
  category: string;
  categoryLabel: string;
  description: string;
}

export interface SearchHit<T = ToolInfo> {
  item: T;
  score: number;
  /** [start, end) ranges in the name to highlight. */
  ranges: [number, number][];
}

const norm = (s: string) => s.toLowerCase().normalize('NFKD').replace(/[̀-ͯ]/g, '');

export function makeDoc(t: ToolInfo, categoryLabel = ''): SearchDoc {
  return {
    id: t.id,
    name: t.name,
    nameLc: norm(t.name),
    idLc: norm(t.id),
    keywords: (t.keywords ?? []).map(norm),
    category: t.category,
    categoryLabel: norm(categoryLabel),
    description: norm(t.description ?? ''),
  };
}

const isBoundary = (s: string, i: number) => i === 0 || /[\s._\-/()&,]/.test(s[i - 1]);

/** Subsequence match with bonuses for consecutive and word-boundary characters. */
export function fuzzyMatch(text: string, q: string): { score: number; positions: number[] } | null {
  if (!q) return { score: 0, positions: [] };
  const positions: number[] = [];
  let ti = 0;
  let score = 0;
  let prev = -2;
  for (let qi = 0; qi < q.length; qi++) {
    const ch = q[qi];
    let found = -1;
    // Prefer a word-boundary occurrence close ahead.
    for (let j = ti; j < text.length; j++) {
      if (text[j] === ch) {
        if (found < 0) found = j;
        if (isBoundary(text, j) && j - ti < 12) { found = j; break; }
        if (j === prev + 1) { found = j; break; }
      }
    }
    if (found < 0) return null;
    let s = 1;
    if (found === prev + 1) s += 3;
    if (isBoundary(text, found)) s += 4;
    score += s;
    positions.push(found);
    prev = found;
    ti = found + 1;
  }
  // Penalize spread.
  const spread = positions[positions.length - 1] - positions[0] + 1 - q.length;
  score -= Math.min(spread, 20) * 0.3;
  return { score, positions };
}

function toRanges(positions: number[]): [number, number][] {
  const out: [number, number][] = [];
  for (const p of positions) {
    const last = out[out.length - 1];
    if (last && last[1] === p) last[1] = p + 1;
    else out.push([p, p + 1]);
  }
  return out;
}

/** Score one query token against a document. Returns null when the token matches nothing. */
function scoreToken(d: SearchDoc, tok: string): { score: number; ranges: [number, number][] } | null {
  const name = d.nameLc;
  const idx = name.indexOf(tok);
  if (name === tok) return { score: 120, ranges: [[0, name.length]] };
  if (idx === 0) return { score: 90, ranges: [[0, tok.length]] };
  if (idx > 0) {
    // word-start match inside the name
    let wi = -1;
    let from = 0;
    while (from <= name.length) {
      const k = name.indexOf(tok, from);
      if (k < 0) break;
      if (isBoundary(name, k)) { wi = k; break; }
      from = k + 1;
    }
    if (wi >= 0) return { score: 70, ranges: [[wi, wi + tok.length]] };
    return { score: 45, ranges: [[idx, idx + tok.length]] };
  }
  if (d.idLc === tok) return { score: 100, ranges: [] };
  let best = 0;
  if (d.idLc.includes(tok)) best = d.idLc.split(/[._]/).some((p) => p.startsWith(tok)) ? 55 : 40;
  for (const k of d.keywords) {
    if (k === tok) best = Math.max(best, 60);
    else if (k.startsWith(tok)) best = Math.max(best, 48);
    else if (k.split(/\s+/).some((w) => w.startsWith(tok))) best = Math.max(best, 40);
    else if (k.includes(tok)) best = Math.max(best, 28);
  }
  if (d.category === tok || d.categoryLabel.startsWith(tok)) best = Math.max(best, 30);
  else if (d.categoryLabel.includes(tok)) best = Math.max(best, 18);
  if (!best && tok.length >= 2) {
    if (d.description.split(/[^a-z0-9]+/).some((w) => w.startsWith(tok))) best = 16;
    else if (tok.length >= 3 && d.description.includes(tok)) best = 9;
  }
  const fz = tok.length >= 2 ? fuzzyMatch(name, tok) : null;
  if (fz && fz.score >= tok.length * 3) {
    const fs = Math.min(38, fz.score * 1.3);
    if (fs > best) return { score: fs, ranges: toRanges(fz.positions) };
  }
  return best ? { score: best, ranges: [] } : null;
}

function mergeRanges(r: [number, number][]): [number, number][] {
  const s = [...r].sort((a, b) => a[0] - b[0]);
  const out: [number, number][] = [];
  for (const x of s) {
    const last = out[out.length - 1];
    if (last && x[0] <= last[1]) last[1] = Math.max(last[1], x[1]);
    else out.push([x[0], x[1]]);
  }
  return out;
}

export function scoreDoc(d: SearchDoc, query: string): { score: number; ranges: [number, number][] } | null {
  const q = norm(query).trim();
  if (!q) return { score: 0, ranges: [] };
  const toks = q.split(/\s+/).filter(Boolean);
  let total = 0;
  const ranges: [number, number][] = [];
  for (const t of toks) {
    const r = scoreToken(d, t);
    if (!r) return null;
    total += r.score;
    ranges.push(...r.ranges);
  }
  // Whole-phrase bonus.
  if (toks.length > 1 && d.nameLc.includes(q)) total += 40;
  return { score: total, ranges: mergeRanges(ranges) };
}

export interface SearchOptions {
  limit?: number;
  /** Extra score per item (e.g. recency/favorites boosts). */
  boost?: (id: string) => number;
}

export function search<T extends { id: string }>(
  docs: { doc: SearchDoc; item: T }[],
  query: string,
  opts: SearchOptions = {},
): SearchHit<T>[] {
  const out: SearchHit<T>[] = [];
  for (const { doc, item } of docs) {
    const r = scoreDoc(doc, query);
    if (!r) continue;
    out.push({ item, score: r.score + (opts.boost?.(doc.id) ?? 0), ranges: r.ranges });
  }
  out.sort((a, b) => b.score - a.score || a.item.id.localeCompare(b.item.id));
  return opts.limit ? out.slice(0, opts.limit) : out;
}
