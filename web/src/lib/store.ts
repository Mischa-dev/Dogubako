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

/** File(s) chosen elsewhere (Files page, drop screen, bench) to prefill the next tool's input. */
export interface PendingInput { ref: string; name: string; size: number; mime: string }
export const pendingInput = { value: null as PendingInput | PendingInput[] | null };
/** Settings a plain-words request filled in for the next tool page (natural-language mode). */
export const pendingParams = { value: null as { toolId: string; params: Record<string, unknown> } | null };

/** Files dropped or picked and waiting on the drop screen. Session only, so a reload keeps them. */
const DROPPED_KEY = 'dgb.dropped';
export function getDropped(): PendingInput[] {
  try { const v = JSON.parse(sessionStorage.getItem(DROPPED_KEY) ?? '[]'); return Array.isArray(v) ? v : []; } catch { return []; }
}
export function setDropped(files: PendingInput[]) {
  try { sessionStorage.setItem(DROPPED_KEY, JSON.stringify(files)); } catch { /* storage unavailable: drop screen still works for this view */ }
  droppedMem = files;
}
let droppedMem: PendingInput[] = [];
export const droppedNow = () => (droppedMem.length ? droppedMem : getDropped());

/** Tool page layout: where the controls sit, how the preview shows, what the tool row lists. */
export interface ToolLayout { controls: 'below' | 'left' | 'right'; preview: 'side' | 'result'; row: 'file' | 'pinned' }
export const TOOL_LAYOUT_DEFAULT: ToolLayout = { controls: 'below', preview: 'side', row: 'file' };
const isToolLayout = (v: any): v is ToolLayout =>
  !!v && ['below', 'left', 'right'].includes(v.controls) && ['side', 'result'].includes(v.preview) && ['file', 'pinned'].includes(v.row);
export const toolLayout = new Store<ToolLayout>('dgb.toolLayout', TOOL_LAYOUT_DEFAULT, isToolLayout);

/** Home page: which sections show, in what order, and a few layout choices. Saved on this device. */
export type HomeModuleId = 'bench' | 'pinned' | 'runs' | 'tip' | 'categories';
export interface HomeLayout {
  sidebar: 'left' | 'right';
  shelf: 'compact' | 'large';
  glass: boolean;
  modules: { id: HomeModuleId; on: boolean }[];
}
export const HOME_MODULES: HomeModuleId[] = ['bench', 'pinned', 'runs', 'tip', 'categories'];
export const HOME_LAYOUT_DEFAULT: HomeLayout = {
  sidebar: 'left',
  shelf: 'compact',
  glass: true,
  modules: [{ id: 'bench', on: true }, { id: 'runs', on: true }, { id: 'tip', on: true }, { id: 'pinned', on: false }, { id: 'categories', on: false }],
};
const isHomeLayout = (v: any): v is HomeLayout =>
  !!v && (v.sidebar === 'left' || v.sidebar === 'right') && (v.shelf === 'compact' || v.shelf === 'large')
  && typeof v.glass === 'boolean' && Array.isArray(v.modules) && v.modules.every((m: any) => HOME_MODULES.includes(m?.id));
export const homeLayout = new Store<HomeLayout>('dgb.home', HOME_LAYOUT_DEFAULT, isHomeLayout);

/** Modules in saved order, with any new ones (added in a later version) appended switched off. */
export function homeModules(l: HomeLayout): { id: HomeModuleId; on: boolean }[] {
  const seen = new Set(l.modules.map((m) => m.id));
  return [...l.modules, ...HOME_MODULES.filter((id) => !seen.has(id)).map((id) => ({ id, on: false }))];
}

function applyGlass() {
  if (typeof document === 'undefined') return;
  if (homeLayout.get().glass) delete document.documentElement.dataset.glass;
  else document.documentElement.dataset.glass = 'off';
}
applyGlass();
homeLayout.subscribe(applyGlass);
