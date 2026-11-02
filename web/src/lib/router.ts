import { useSyncExternalStore } from 'react';

/** Hash routes: #/tool/<id>, #/category/<id>, #/history, #/settings/<section>, #/admin/<section> … */
export interface Route {
  path: string;
  parts: string[];
  query: URLSearchParams;
}

function parse(hash: string): Route {
  const raw = hash.replace(/^#/, '') || '/';
  const [path, qs] = raw.split('?');
  const clean = path.startsWith('/') ? path : `/${path}`;
  return { path: clean, parts: clean.split('/').filter(Boolean).map(decodeURIComponent), query: new URLSearchParams(qs ?? '') };
}

let current = parse(typeof location !== 'undefined' ? location.hash : '');
const listeners = new Set<() => void>();
if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    current = parse(location.hash);
    listeners.forEach((l) => l());
  });
}

export function useRoute(): Route {
  return useSyncExternalStore(
    (l) => { listeners.add(l); return () => { listeners.delete(l); }; },
    () => current,
    () => current,
  );
}

export function navigate(to: string, opts: { replace?: boolean } = {}) {
  const hash = to.startsWith('#') ? to : `#${to}`;
  if (opts.replace) {
    history.replaceState(null, '', hash);
    current = parse(hash);
    listeners.forEach((l) => l());
  } else if (location.hash !== hash) {
    location.hash = hash;
  }
}

export const href = {
  home: '#/',
  tool: (id: string) => `#/tool/${encodeURIComponent(id)}`,
  category: (id: string) => `#/category/${encodeURIComponent(id)}`,
  favorites: '#/favorites',
  recent: '#/recent',
  history: '#/history',
  files: '#/files',
  settings: (s = '') => `#/settings${s ? `/${s}` : ''}`,
  account: '#/account',
  admin: (s = '') => `#/admin${s ? `/${s}` : ''}`,
};
