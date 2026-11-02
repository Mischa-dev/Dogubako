import { useSyncExternalStore } from 'react';
import { readJson, removeKey, writeJson } from './storage';

/** Minimal observable store persisted to localStorage. */
export class Store<T> {
  private value: T;
  private listeners = new Set<() => void>();
  constructor(private key: string, initial: T, validate?: (v: any) => v is T) {
    const stored = readJson<unknown>(key, undefined);
    this.value = stored !== undefined && (!validate || validate(stored)) ? (stored as T) : initial;
  }
  get = () => this.value;
  set = (next: T | ((prev: T) => T)) => {
    const v = typeof next === 'function' ? (next as (p: T) => T)(this.value) : next;
    if (Object.is(v, this.value)) return;
    this.value = v;
    writeJson(this.key, v);
    this.listeners.forEach((l) => l());
  };
  subscribe = (l: () => void) => {
    this.listeners.add(l);
    return () => { this.listeners.delete(l); };
  };
}

export function useStore<T>(s: Store<T>): T {
  return useSyncExternalStore(s.subscribe, s.get, s.get);
}

export type ThemePref = 'system' | 'light' | 'dark';
export type Density = 'comfortable' | 'compact';

export interface Prefs {
  theme: ThemePref;
  density: Density;
  history: boolean;
  rememberParams: boolean;
}

export const prefs = new Store<Prefs>('dgb.prefs', { theme: 'system', density: 'comfortable', history: true, rememberParams: false },
  (v): v is Prefs => !!v && typeof v === 'object' && 'theme' in v);
export const favorites = new Store<string[]>('dgb.favorites', [], Array.isArray as any);
export const recents = new Store<{ id: string; at: number }[]>('dgb.recents', [], Array.isArray as any);
export const sidebarCollapsed = new Store<boolean>('dgb.sidebar.collapsed', false);

export const RECENTS_MAX = 20;

export function toggleFavorite(id: string) {
  favorites.set((f) => (f.includes(id) ? f.filter((x) => x !== id) : [...f, id]));
}

export function pushRecent(id: string) {
  recents.set((r) => [{ id, at: Date.now() }, ...r.filter((x) => x.id !== id)].slice(0, RECENTS_MAX));
}

export function setPref<K extends keyof Prefs>(k: K, v: Prefs[K]) {
  prefs.set((p) => ({ ...p, [k]: v }));
  if (k === 'history') writeJson('dgb.prefs.history', v);
}

const paramKey = (toolId: string) => `dgb.params.${toolId}`;
export const loadToolParams = (toolId: string) => readJson<Record<string, unknown> | null>(paramKey(toolId), null);
export const saveToolParams = (toolId: string, v: Record<string, unknown>) => writeJson(paramKey(toolId), v);
export const clearToolParams = (toolId: string) => removeKey(paramKey(toolId));

export function clearAllToolParams() {
  try {
    const keys: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k?.startsWith('dgb.params.')) keys.push(k);
    }
    keys.forEach(removeKey);
  } catch { /* ignore */ }
}

// ---------------------------------------------------------------- theme
const mq = typeof window !== 'undefined' && window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null;

export function resolvedTheme(p: ThemePref = prefs.get().theme): 'light' | 'dark' {
  if (p === 'system') return mq?.matches ? 'dark' : 'light';
  return p;
}

export function applyTheme() {
  const root = document.documentElement;
  const t = resolvedTheme();
  root.dataset.theme = t;
  root.dataset.density = prefs.get().density;
  root.style.colorScheme = t;
}

export function initTheme() {
  applyTheme();
  prefs.subscribe(applyTheme);
  mq?.addEventListener?.('change', applyTheme);
}

export function cycleTheme() {
  const cur = resolvedTheme();
  setPref('theme', cur === 'dark' ? 'light' : 'dark');
}

/** A stored file chosen on the Files page to prefill the next tool's input (session only). */
export interface PendingInput { ref: string; name: string; size: number; mime: string }
export const pendingInput = { value: null as PendingInput | null };
