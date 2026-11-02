import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react';
import { asServer, type Backend } from './backend';
import type { Category, Me, Status, ToolInfo } from './types';
import { makeDoc, type SearchDoc } from './search';
import { clearAssistantKeys } from './assistant/settings';
import { CATEGORY_DESCRIPTIONS, CATEGORY_LABELS, CATEGORY_ORDER } from './icons';

export interface AppState {
  backend: Backend;
  status: Status;
  me: Me | null;
  tools: ToolInfo[];
  /** Tools the current user can see (permitted). */
  visibleTools: ToolInfo[];
  toolMap: Map<string, ToolInfo>;
  categories: Category[];
  categoryLabel: (id: string) => string;
  docs: { doc: SearchDoc; item: ToolInfo }[];
  isServer: boolean;
  isAdmin: boolean;
  reloadTools: () => Promise<void>;
  refreshMe: () => Promise<void>;
  signOut: () => Promise<void>;
}

const Ctx = createContext<AppState | null>(null);

export function useApp(): AppState {
  const v = useContext(Ctx);
  if (!v) throw new Error('useApp outside provider');
  return v;
}

/** Normalizes ToolInfo from either host (CLI JSON flattens availability). */
export function normalizeTool(t: any): ToolInfo {
  const availability = t.availability ?? {
    available: t.available ?? true,
    platform_supported: t.platform_supported ?? true,
    missing: t.missing,
    missing_model: t.missing_model,
    reason: t.reason,
  };
  return {
    keywords: [], params: [], requires: [], permissions: [], platforms: [], destructive: false, source: 'builtin',
    reports_progress: false, execution: 'native', version: '', timeout_secs: 0, output: { kind: 'data' },
    ...t,
    availability,
  };
}

export function deriveCategories(tools: ToolInfo[], fromApi: Category[] | null): Category[] {
  const counts = new Map<string, number>();
  for (const t of tools) counts.set(t.category, (counts.get(t.category) ?? 0) + 1);
  const base: Category[] = fromApi ?? CATEGORY_ORDER.map((id) => ({
    id: id as Category['id'], label: CATEGORY_LABELS[id], description: CATEGORY_DESCRIPTIONS[id], count: 0,
  }));
  return base.map((c) => ({ ...c, count: counts.get(c.id) ?? 0 })).filter((c) => c.count > 0);
}

export function AppProvider(props: {
  backend: Backend;
  status: Status;
  me: Me | null;
  tools: ToolInfo[];
  categories: Category[] | null;
  onSignedOut: () => void;
  children: ReactNode;
}) {
  const { backend, status } = props;
  const [me, setMe] = useState(props.me);
  const [tools, setTools] = useState(props.tools);
  const [apiCats, setApiCats] = useState(props.categories);

  const reloadTools = useCallback(async () => {
    const [t, c] = await Promise.all([backend.listTools(), backend.listCategories().catch(() => null)]);
    setTools(t.map(normalizeTool));
    setApiCats(c);
  }, [backend]);

  const refreshMe = useCallback(async () => {
    setMe(await backend.getMe());
  }, [backend]);

  const signOut = useCallback(async () => {
    const s = asServer(backend);
    if (s) await s.logout().catch(() => undefined);
    clearAssistantKeys();
    props.onSignedOut();
  }, [backend, props.onSignedOut]);

  const value = useMemo<AppState>(() => {
    const visibleTools = tools.filter((t) => t.permitted !== false);
    const categories = deriveCategories(visibleTools, apiCats);
    const labels = new Map(categories.map((c) => [c.id as string, c.label]));
    const categoryLabel = (id: string) => labels.get(id) ?? CATEGORY_LABELS[id] ?? id;
    return {
      backend,
      status,
      me,
      tools,
      visibleTools,
      toolMap: new Map(tools.map((t) => [t.id, t])),
      categories,
      categoryLabel,
      docs: visibleTools.map((t) => ({ doc: makeDoc(t, categoryLabel(t.category)), item: t })),
      isServer: backend.mode === 'server',
      isAdmin: !!me?.user.is_admin,
      reloadTools,
      refreshMe,
      signOut,
    };
  }, [backend, status, me, tools, apiCats, reloadTools, refreshMe, signOut]);

  // Keep the tool list fresh when deps/models change elsewhere.
  useEffect(() => {
    const h = () => { void reloadTools(); };
    window.addEventListener('dgb:tools-changed', h);
    return () => window.removeEventListener('dgb:tools-changed', h);
  }, [reloadTools]);

  return <Ctx.Provider value={value}>{props.children}</Ctx.Provider>;
}

export const notifyToolsChanged = () => window.dispatchEvent(new Event('dgb:tools-changed'));
