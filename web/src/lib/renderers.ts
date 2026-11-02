export interface TableData { columns: string[]; rows: unknown[][] }
export interface ColorData { hex: string; name?: string }
export interface DiffSeg { tag: 'equal' | 'insert' | 'delete'; text: string }
export interface Region { x: number; y: number; width: number; height: number; label?: string; score?: number }
export interface Match { start: number; end: number; text: string; groups?: unknown[] }

export type DataView =
  | { kind: 'table'; key: string; table: TableData }
  | { kind: 'colors'; key: string; colors: ColorData[] }
  | { kind: 'diff'; key: string; diff: DiffSeg[] }
  | { kind: 'regions'; key: string; regions: Region[] }
  | { kind: 'matches'; key: string; matches: Match[] }
  | { kind: 'kv'; key: string; value: Record<string, unknown> }
  | { kind: 'json'; key: string; value: unknown };

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);

export function isTable(v: unknown): v is TableData {
  return isObj(v) && Array.isArray(v.columns) && Array.isArray(v.rows) && v.rows.every((r) => Array.isArray(r));
}
export function isColors(v: unknown): v is ColorData[] {
  return Array.isArray(v) && v.length > 0 && v.every((c) => isObj(c) && typeof c.hex === 'string');
}
export function isDiff(v: unknown): v is DiffSeg[] {
  return Array.isArray(v) && v.every((s) => isObj(s) && typeof s.text === 'string' && ['equal', 'insert', 'delete'].includes(String(s.tag)));
}
export function isRegions(v: unknown): v is Region[] {
  return Array.isArray(v) && v.every((r) => isObj(r) && ['x', 'y', 'width', 'height'].every((k) => typeof r[k] === 'number'));
}
export function isMatches(v: unknown): v is Match[] {
  return Array.isArray(v) && v.every((m) => isObj(m) && typeof m.start === 'number' && typeof m.end === 'number');
}

/** Pick renderers for a result's `data` (AGENT_BRIEF §9). Special keys first, the rest as key/value. */
export function selectDataViews(data: unknown): DataView[] {
  if (data == null) return [];
  if (!isObj(data)) {
    if (Array.isArray(data) && data.length === 0) return [];
    if (isColors(data)) return [{ kind: 'colors', key: '', colors: data }];
    return [{ kind: 'json', key: '', value: data }];
  }
  const views: DataView[] = [];
  const rest: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(data)) {
    if (k === 'table' && isTable(v)) views.push({ kind: 'table', key: k, table: v });
    else if (k === 'colors' && isColors(v)) views.push({ kind: 'colors', key: k, colors: v });
    else if (k === 'diff' && isDiff(v)) views.push({ kind: 'diff', key: k, diff: v });
    else if (k === 'regions' && isRegions(v)) views.push({ kind: 'regions', key: k, regions: v });
    else if (k === 'matches' && isMatches(v)) views.push({ kind: 'matches', key: k, matches: v });
    else rest[k] = v;
  }
  if (Object.keys(rest).length) views.push({ kind: 'kv', key: '', value: rest });
  return views;
}

export function tableToCsv(t: TableData): string {
  const cell = (v: unknown) => {
    const s = v == null ? '' : typeof v === 'object' ? JSON.stringify(v) : String(v);
    return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  return [t.columns.map(cell).join(','), ...t.rows.map((r) => r.map(cell).join(','))].join('\n');
}

/** Compare for table sorting: numbers numerically, otherwise locale string. */
export function compareCells(a: unknown, b: unknown): number {
  const na = typeof a === 'number' ? a : typeof a === 'string' && a.trim() !== '' && !isNaN(Number(a)) ? Number(a) : NaN;
  const nb = typeof b === 'number' ? b : typeof b === 'string' && b.trim() !== '' && !isNaN(Number(b)) ? Number(b) : NaN;
  if (!isNaN(na) && !isNaN(nb)) return na - nb;
  if (a == null) return b == null ? 0 : 1;
  if (b == null) return -1;
  return String(a).localeCompare(String(b), undefined, { numeric: true, sensitivity: 'base' });
}

/** Extension for downloading text output by mime. */
export function extForMime(mime?: string): string {
  if (!mime) return 'txt';
  const m = mime.split(';')[0].trim();
  const map: Record<string, string> = {
    'application/json': 'json', 'text/markdown': 'md', 'text/html': 'html', 'image/svg+xml': 'svg',
    'text/csv': 'csv', 'application/xml': 'xml', 'text/xml': 'xml', 'application/yaml': 'yaml', 'text/yaml': 'yaml',
    'text/x-yaml': 'yaml', 'application/toml': 'toml', 'text/vtt': 'vtt', 'application/x-subrip': 'srt',
    'text/css': 'css', 'application/javascript': 'js', 'text/javascript': 'js', 'application/sql': 'sql',
  };
  return map[m] ?? 'txt';
}
