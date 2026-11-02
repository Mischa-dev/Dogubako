import type { Artifact, JobView, ToolInfo } from './types';

/** What a file looks like on the bench (drives how it is drawn). */
export type BenchKind = 'image' | 'pdf' | 'video' | 'audio' | 'sheet' | 'text' | 'archive' | 'file';

export interface BenchItem {
  key: string;
  name: string;
  mime: string;
  size: number;
  kind: BenchKind;
  artifact: Artifact;
  jobId: string;
  toolId: string;
  /** Seconds since the epoch. */
  at: number;
}

const ext = (name: string) => (name.split('.').pop() ?? '').toLowerCase();

export function kindOf(mime: string, name: string): BenchKind {
  const m = (mime || '').toLowerCase();
  const e = ext(name);
  if (m.startsWith('image/')) return 'image';
  if (m === 'application/pdf' || e === 'pdf') return 'pdf';
  if (m.startsWith('video/')) return 'video';
  if (m.startsWith('audio/')) return 'audio';
  if (m === 'text/csv' || /^(csv|tsv|xlsx|xls|ods)$/.test(e) || m.includes('spreadsheet')) return 'sheet';
  if (/^(zip|tar|gz|tgz|7z|xz|zst|bz2|rar)$/.test(e) || m.includes('zip') || m.includes('compressed')) return 'archive';
  if (m.startsWith('text/') || /json|xml|yaml|toml|javascript/.test(m) || /^(txt|md|json|yaml|yml|toml|xml|srt|vtt|log)$/.test(e)) return 'text';
  return 'file';
}

const jobTime = (j: JobView) => j.finished_at ?? j.created_at;

/** Most recent distinct output files across successful runs, newest first. */
export function benchItems(jobs: JobView[], max = 6): BenchItem[] {
  const out: BenchItem[] = [];
  const seen = new Set<string>();
  const sorted = [...jobs].sort((a, b) => jobTime(b) - jobTime(a));
  for (const j of sorted) {
    if (j.status !== 'succeeded') continue;
    for (const a of j.result?.artifacts ?? []) {
      if (seen.has(a.name)) continue;
      seen.add(a.name);
      out.push({ key: `${j.id}:${a.id ?? a.path ?? a.name}`, name: a.name, mime: a.mime, size: a.size, kind: kindOf(a.mime, a.name), artifact: a, jobId: j.id, toolId: j.tool_id, at: jobTime(j) });
      if (out.length >= max) return out;
    }
  }
  return out;
}

export interface Suggestion {
  text: string;
  toolId: string;
  action: string;
}

const MB = 1024 * 1024;

/** One useful next step based on what is on the bench, or null. Only suggests tools that can run. */
export function suggestion(items: BenchItem[], toolMap: Map<string, ToolInfo>): Suggestion | null {
  const can = (id: string) => {
    const t = toolMap.get(id);
    return !!t && t.availability.available && t.permitted !== false;
  };
  for (const it of items) {
    const size = (n: number) => `${(n / MB).toFixed(1)} MB`;
    if (it.kind === 'pdf' && it.size > 3 * MB && can('pdf.compress')) {
      return { text: `${it.name} is ${size(it.size)}. Compressing it usually makes it small enough to email.`, toolId: 'pdf.compress', action: 'Compress PDF' };
    }
    if (it.kind === 'video' && it.size > 50 * MB && can('video.compress')) {
      return { text: `${it.name} is ${size(it.size)}. A compressed copy is easier to share.`, toolId: 'video.compress', action: 'Compress video' };
    }
    if (it.kind === 'image' && /heic|heif/.test(it.mime + ext(it.name)) && can('image.convert')) {
      return { text: `${it.name} is HEIC, which some apps can't open. Convert it to JPEG or WebP.`, toolId: 'image.convert', action: 'Convert image' };
    }
    if (it.kind === 'audio' && can('speech.transcribe')) {
      return { text: `Turn ${it.name} into text you can search and quote.`, toolId: 'speech.transcribe', action: 'Transcribe' };
    }
  }
  return null;
}
