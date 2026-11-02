import type { ParamSpec, ToolInfo } from './types';
import type { FileRef } from './backend/types';

/** A file in a form value: either uploading or ready (with ref). */
export interface FileItem {
  key: string;
  name: string;
  size: number;
  mime: string;
  ref?: string;
  status: 'uploading' | 'ready' | 'error';
  progress?: number;
  error?: string;
  /** Local object URL for instant previews (revoked on removal). */
  localUrl?: string;
}

export type FormValues = Record<string, unknown>;
export type FormErrors = Record<string, string>;

export function fileItemFromRef(r: FileRef): FileItem {
  return { key: `${r.ref}:${Math.random().toString(36).slice(2, 7)}`, name: r.name, size: r.size, mime: r.mime, ref: r.ref, status: 'ready' };
}

/** Initial value for one param. */
export function defaultFor(p: ParamSpec): unknown {
  switch (p.type) {
    case 'file':
      return [];
    case 'boolean':
      return p.default ?? false;
    case 'enum':
      if (p.multi) return Array.isArray(p.default) ? p.default : p.default != null ? [p.default] : [];
      return p.default ?? (p.required && p.options[0] ? p.options[0].value : '');
    case 'integer':
    case 'number':
      return p.default ?? '';
    case 'json':
      return p.default != null ? (typeof p.default === 'string' ? p.default : JSON.stringify(p.default, null, 2)) : '';
    case 'color':
      return p.default ?? '';
    default:
      return p.default ?? '';
  }
}

export function defaultValues(tool: Pick<ToolInfo, 'params'>): FormValues {
  const v: FormValues = {};
  for (const p of tool.params) v[p.name] = defaultFor(p);
  return v;
}

const looseEq = (a: unknown, b: unknown) => a === b || String(a) === String(b);

/** Whether a param should be shown given current values (show_if, recursively). */
export function isVisible(p: ParamSpec, values: FormValues, all?: ParamSpec[], depth = 0): boolean {
  if (!p.show_if) return true;
  const cur = values[p.show_if.param];
  const vals = p.show_if.values ?? [];
  const match = Array.isArray(cur) ? cur.some((c) => vals.some((v) => looseEq(c, v))) : vals.some((v) => looseEq(cur, v));
  if (!match) return false;
  if (all && depth < 5) {
    const parent = all.find((x) => x.name === p.show_if!.param);
    if (parent) return isVisible(parent, values, all, depth + 1);
  }
  return true;
}

export function isEmpty(v: unknown): boolean {
  return v == null || v === '' || (Array.isArray(v) && v.length === 0);
}

/** Validate visible params. Returns map param -> message. */
export function validate(tool: Pick<ToolInfo, 'params'>, values: FormValues): FormErrors {
  const errs: FormErrors = {};
  for (const p of tool.params) {
    if (!isVisible(p, values, tool.params)) continue;
    const v = values[p.name];
    if (p.type === 'file') {
      const items = (v as FileItem[] | undefined) ?? [];
      if (items.some((i) => i.status === 'uploading')) { errs[p.name] = 'Waiting for upload to finish'; continue; }
      if (items.some((i) => i.status === 'error')) { errs[p.name] = 'Remove files that failed to upload'; continue; }
      if (p.required && !items.some((i) => i.status === 'ready')) errs[p.name] = p.multiple ? 'Add at least one file' : 'Choose a file';
      continue;
    }
    if (isEmpty(v)) {
      if (p.required && p.type !== 'boolean') errs[p.name] = 'Required';
      continue;
    }
    if (p.type === 'integer' || p.type === 'number') {
      const n = Number(v);
      if (!Number.isFinite(n)) { errs[p.name] = 'Enter a number'; continue; }
      if (p.type === 'integer' && !Number.isInteger(n)) { errs[p.name] = 'Enter a whole number'; continue; }
      if (p.min != null && n < p.min) { errs[p.name] = `Must be at least ${p.min}`; continue; }
      if (p.max != null && n > p.max) { errs[p.name] = `Must be at most ${p.max}`; continue; }
    }
    if (p.type === 'json') {
      try { JSON.parse(String(v)); } catch (e) { errs[p.name] = `Invalid JSON: ${(e as Error).message}`; }
    }
    if (p.type === 'color' && !/^#?([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(String(v).trim())) {
      errs[p.name] = 'Use a hex color like #3366ff';
    }
    if (p.type === 'text' && p.max_len && String(v).length > p.max_len) {
      errs[p.name] = `At most ${p.max_len} characters`;
    }
  }
  return errs;
}

/** Convert form values into API params: drop hidden/empty, coerce numbers, parse JSON, map files to refs. */
export function toRunParams(tool: Pick<ToolInfo, 'params'>, values: FormValues): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const p of tool.params) {
    if (!isVisible(p, values, tool.params)) continue;
    const v = values[p.name];
    switch (p.type) {
      case 'file': {
        const refs = ((v as FileItem[]) ?? []).filter((i) => i.status === 'ready' && i.ref).map((i) => i.ref!);
        if (!refs.length) break;
        out[p.name] = p.multiple ? refs : refs[0];
        break;
      }
      case 'integer':
      case 'number':
        if (!isEmpty(v) && Number.isFinite(Number(v))) out[p.name] = Number(v);
        break;
      case 'boolean':
        out[p.name] = !!v;
        break;
      case 'json':
        if (!isEmpty(v)) { try { out[p.name] = JSON.parse(String(v)); } catch { out[p.name] = v; } }
        break;
      case 'enum':
        if (p.multi) { if (Array.isArray(v) && v.length) out[p.name] = v; }
        else if (!isEmpty(v)) out[p.name] = v;
        break;
      case 'text':
      case 'secret':
        if (typeof v === 'string' && (v !== '' || p.required)) out[p.name] = v;
        break;
      default:
        if (!isEmpty(v)) out[p.name] = typeof v === 'string' ? v.trim() : v;
    }
  }
  return out;
}

/** Reverse of toRunParams for re-opening a job: file refs stay as pending refs to be described. */
export function fromRunParams(tool: Pick<ToolInfo, 'params'>, params: Record<string, unknown>): { values: FormValues; fileRefs: Record<string, string[]> } {
  const values = defaultValues(tool);
  const fileRefs: Record<string, string[]> = {};
  for (const p of tool.params) {
    if (!(p.name in params)) continue;
    const v = params[p.name];
    if (p.type === 'file') {
      fileRefs[p.name] = (Array.isArray(v) ? v : [v]).filter((x): x is string => typeof x === 'string');
    } else if (p.type === 'json') {
      values[p.name] = typeof v === 'string' ? v : JSON.stringify(v, null, 2);
    } else {
      values[p.name] = v;
    }
  }
  return { values, fileRefs };
}

/** Values without file params (for persisting "remembered" params). */
export function persistableValues(tool: Pick<ToolInfo, 'params'>, values: FormValues): FormValues {
  const out: FormValues = {};
  for (const p of tool.params) if (p.type !== 'file' && p.type !== 'secret') out[p.name] = values[p.name];
  return out;
}

/** Whether a numeric param is a bounded range suitable for a slider. */
export function isSliderRange(p: ParamSpec): boolean {
  if (p.type !== 'integer' && p.type !== 'number') return false;
  if (p.min == null || p.max == null) return false;
  const span = p.max - p.min;
  return span > 0 && span <= 1000 && (p.type === 'number' || span >= 4);
}
