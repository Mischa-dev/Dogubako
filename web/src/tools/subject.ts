import { useSyncExternalStore } from 'react';
import type { PendingInput } from '../lib/store';

/**
 * What the open tool is working on, so the tool row can show the file and hand it
 * (or the result) to the next tool. Set by the tool body, read by the top row.
 */
export interface ToolSubject { toolId: string; files: PendingInput[]; results: PendingInput[] }

let subject: ToolSubject | null = null;
const subs = new Set<() => void>();

export function setToolSubject(next: ToolSubject | null) {
  const same = subject && next && subject.toolId === next.toolId
    && JSON.stringify(subject.files) === JSON.stringify(next.files)
    && JSON.stringify(subject.results) === JSON.stringify(next.results);
  if (same || (!subject && !next)) return;
  subject = next;
  subs.forEach((s) => s());
}

export function useToolSubject(toolId: string): ToolSubject | null {
  const s = useSyncExternalStore((l) => { subs.add(l); return () => { subs.delete(l); }; }, () => subject, () => subject);
  return s && s.toolId === toolId ? s : null;
}
