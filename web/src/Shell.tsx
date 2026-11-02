import { Suspense, lazy, useCallback, useEffect, useRef, useState } from 'react';
import { Menu as MenuIcon, Search } from 'lucide-react';
import { Sidebar } from './components/Sidebar';
import { CommandPalette } from './components/CommandPalette';
import { useRoute } from './lib/router';
import { useApp } from './lib/app';
import { LogoMark } from './components/Logo';
import { Spinner, cx } from './components/ui';
import { HomePage } from './pages/Home';
import { CategoryPage, FavoritesPage, RecentPage } from './pages/Lists';
import { NotFound } from './pages/NotFound';

const ToolPage = lazy(() => import('./tools/ToolPage'));
const HistoryPage = lazy(() => import('./pages/History'));
const FilesPage = lazy(() => import('./pages/Files'));
const SettingsPage = lazy(() => import('./pages/Settings'));
const AccountPage = lazy(() => import('./pages/Account'));
const AdminPage = lazy(() => import('./pages/admin/Admin'));

function PageFallback() {
  return <div className="page-loading" aria-busy="true"><Spinner size={18} /></div>;
}

export function Shell() {
  const route = useRoute();
  const { isServer, isAdmin } = useApp();
  const [palette, setPalette] = useState(false);
  const [drawer, setDrawer] = useState(false);
  const main = useRef<HTMLElement>(null);

  const openPalette = useCallback(() => { setDrawer(false); setPalette(true); }, []);
  const closePalette = useCallback(() => setPalette(false), []);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = !!t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPalette((p) => !p);
      } else if (e.key === '/' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        setPalette(true);
      } else if (e.key === 'Escape' && drawer) {
        setDrawer(false);
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [drawer]);

  // Focus management on route change: move focus to the page heading and reset scroll.
  const first = useRef(true);
  useEffect(() => {
    setDrawer(false);
    if (first.current) { first.current = false; return; }
    main.current?.scrollTo?.({ top: 0 });
    const id = window.setTimeout(() => {
      const h = main.current?.querySelector<HTMLElement>('[data-route-focus]');
      h?.focus({ preventScroll: true });
    }, 60);
    return () => window.clearTimeout(id);
  }, [route.path]);

  const [p0, p1] = route.parts;
  let page: React.ReactNode;
  switch (p0) {
    case undefined: page = <HomePage onSearch={openPalette} />; break;
    case 'tool': page = <ToolPage key={p1} toolId={p1} />; break;
    case 'category': page = <CategoryPage key={p1} categoryId={p1} />; break;
    case 'favorites': page = <FavoritesPage />; break;
    case 'recent': page = <RecentPage />; break;
    case 'history': page = <HistoryPage />; break;
    case 'files': page = isServer ? <FilesPage /> : <NotFound />; break;
    case 'settings': page = <SettingsPage section={p1} />; break;
    case 'account': page = isServer ? <AccountPage /> : <NotFound />; break;
    case 'admin': page = isServer && isAdmin ? <AdminPage section={p1} /> : <NotFound />; break;
    default: page = <NotFound />;
  }

  return (
    <div className={cx('app', drawer && 'drawer-open')}>
      <a href="#main" className="skip-link" onClick={(e) => { e.preventDefault(); main.current?.focus(); }}>Skip to content</a>
      <header className="topbar">
        <button className="icon-btn" onClick={() => setDrawer(true)} aria-label="Open navigation" aria-expanded={drawer}>
          <MenuIcon size={18} aria-hidden />
        </button>
        <a href="#/" className="brand brand-sm"><LogoMark size={22} /><span className="brand-name">Dogubako</span></a>
        <button className="icon-btn" onClick={openPalette} aria-label="Search tools"><Search size={18} aria-hidden /></button>
      </header>
      <div className="sidebar-wrap">
        <Sidebar onSearch={openPalette} onNavigate={() => setDrawer(false)} />
      </div>
      {drawer && <div className="scrim" onClick={() => setDrawer(false)} aria-hidden />}
      <main className="main" id="main" ref={main} tabIndex={-1}>
        <Suspense fallback={<PageFallback />}>{page}</Suspense>
      </main>
      <CommandPalette open={palette} onClose={closePalette} />
    </div>
  );
}
