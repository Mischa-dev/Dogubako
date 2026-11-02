import { acceptMatches } from '../form/FileField';
import { kindOf, type BenchKind } from './bench';
import { search, type SearchDoc } from './search';
import type { ToolInfo } from './types';

/** A file someone dropped or picked, waiting for them to choose what happens to it. */
export interface IntentFile { ref: string; name: string; size: number; mime: string }

/** The likeliest actions per kind of file, most useful first. */
const PRIMARY: Record<BenchKind, string[]> = {
  image: ['image.compress', 'image.convert', 'image.resize', 'image.strip_metadata', 'image.crop', 'image.remove_background', 'image.redact', 'image.rotate_flip', 'image.ocr', 'image.watermark'],
  pdf: ['pdf.compress', 'pdf.merge', 'pdf.split', 'pdf.to_images', 'pdf.extract_text', 'pdf.ocr', 'pdf.rotate', 'pdf.protect', 'pdf.page_numbers'],
  video: ['video.compress', 'video.trim', 'video.convert', 'video.to_gif', 'video.extract_audio', 'video.resize', 'video.thumbnail', 'video.subtitles_burn'],
  audio: ['audio.convert', 'speech.transcribe', 'audio.trim', 'audio.normalize', 'audio.compress', 'audio.remove_silence', 'audio.denoise', 'audio.waveform'],
  sheet: ['data.csv_view', 'data.clean', 'data.csv_filter_sort', 'data.stats', 'data.convert', 'data.csv_to_json', 'data.duplicates'],
  text: ['dev.json', 'text.extract', 'text.count', 'text.case', 'text.diff', 'dev.convert', 'text.cleanup'],
  archive: ['files.archive_list', 'files.archive_extract', 'files.hash', 'files.decompress'],
  file: ['files.type_detect', 'files.hash', 'files.binary_info', 'files.compress'],
};

/** Plain words people type, mapped to fragments of tool ids they usually mean. */
const WORDS: Record<string, string[]> = {
  smaller: ['compress', 'resize', 'convert', 'strip_metadata'], shrink: ['compress', 'resize'], reduce: ['compress', 'resize'],
  email: ['compress', 'resize'], web: ['compress', 'convert', 'resize'], size: ['compress', 'resize'],
  jpg: ['convert'], jpeg: ['convert'], png: ['convert'], webp: ['convert'], avif: ['convert'], heic: ['convert'], mp3: ['convert'], mp4: ['convert'], gif: ['to_gif', 'convert'],
  text: ['transcribe', 'ocr', 'extract_text', 'extract'], words: ['transcribe', 'ocr', 'count'], transcript: ['transcribe'], subtitles: ['subtitles', 'transcribe'],
  location: ['strip_metadata', 'metadata'], gps: ['strip_metadata', 'metadata'], privacy: ['strip_metadata', 'redact', 'face_blur'], private: ['strip_metadata', 'redact'],
  blur: ['redact', 'face_blur'], hide: ['redact'], faces: ['face_blur', 'face_detect'], background: ['remove_background'],
  combine: ['merge', 'combine'], join: ['merge'], together: ['merge'], separate: ['split'], pages: ['split', 'extract_pages', 'reorder'],
  turn: ['rotate'], cut: ['trim', 'cut', 'crop'], shorter: ['trim', 'cut'], bigger: ['upscale', 'resize'], sharper: ['upscale'],
  password: ['protect', 'unlock'], lock: ['protect'], unlock: ['unlock'], sound: ['extract_audio', 'audio'], quieter: ['volume', 'normalize'], louder: ['volume', 'normalize'],
};

/** Tools with a file input that takes every one of these files. */
export function toolsForFiles(tools: ToolInfo[], files: IntentFile[]): ToolInfo[] {
  if (!files.length) return [];
  return tools.filter((t) => t.permitted !== false && t.params.some((p) =>
    p.type === 'file' && (files.length === 1 || p.multiple) && files.every((f) => acceptMatches(p.accept, f.name, f.mime))));
}

function hasSpecificAccept(t: ToolInfo) {
  return t.params.some((p) => p.type === 'file' && !!p.accept?.length);
}

/** Order candidates: the usual actions for this kind of file first, nudged by what is pinned or recently used. */
export function rankTools(cands: ToolInfo[], files: IntentFile[], ctx: { pinned: string[]; recent: string[] }): ToolInfo[] {
  const kind = files.length ? kindOf(files[0].mime, files[0].name) : 'file';
  const primary = PRIMARY[kind];
  const score = (t: ToolInfo) => {
    let s = 0;
    const pi = primary.indexOf(t.id);
    if (pi >= 0) s += 300 - pi * 10;
    if (ctx.pinned.includes(t.id)) s += 45;
    const ri = ctx.recent.indexOf(t.id);
    if (ri >= 0) s += 25 - Math.min(ri, 25);
    if (hasSpecificAccept(t)) s += 40;
    if (files.length > 1 && t.params.some((p) => p.type === 'file' && p.multiple)) s += 30;
    if (!t.availability.available) s -= 400;
    return s;
  };
  return [...cands].sort((a, b) => score(b) - score(a) || a.name.localeCompare(b.name));
}

/** Filter candidates by what someone typed, understanding a few plain words ("smaller", "jpg", "no location"). */
export function matchIntent(cands: ToolInfo[], docs: { doc: SearchDoc; item: ToolInfo }[], query: string): ToolInfo[] {
  const q = query.trim().toLowerCase();
  if (!q) return cands;
  const allowed = new Set(cands.map((t) => t.id));
  const pool = docs.filter((d) => allowed.has(d.item.id));
  const scores = new Map<string, number>();
  for (const hit of search(pool, q, { limit: 60 })) scores.set(hit.item.id, (scores.get(hit.item.id) ?? 0) + hit.score + 100);
  const words = q.split(/[^a-z0-9]+/).filter(Boolean);
  words.forEach((w) => {
    for (const frag of WORDS[w] ?? []) {
      cands.forEach((t, i) => {
        if (t.id.split('.')[1]?.includes(frag) || t.id.endsWith(frag)) scores.set(t.id, (scores.get(t.id) ?? 0) + 200 - i);
      });
    }
  });
  return cands.filter((t) => scores.has(t.id)).sort((a, b) => (scores.get(b.id)! - scores.get(a.id)!));
}

/** A short reason line for a tool, from the first clause of its description. */
export function shortHint(t: ToolInfo): string {
  const d = t.description.split(/[:;(]| — /)[0].trim();
  return d.length > 64 ? `${d.slice(0, 62).trimEnd()}…` : d;
}
