import { Suspense, lazy, useCallback, useEffect, useRef, useState } from 'react';
import { ChevronDown, History, Menu as MenuIcon, PanelLeftOpen, Search } from 'lucide-react';
import { Sidebar } from './components/Sidebar';
import { CommandPalette } from './components/CommandPalette';
import { ToolsMenu } from './components/ToolsMenu';
import { DropOverlay } from './components/DropOverlay';
import { href, navigate, useRoute } from './lib/router';
import { favorites as favStore, homeLayout, sidebarCollapsed, useStore } from './lib/store';
import { useApp } from './lib/app';
import { LogoMark } from './components/Logo';
import { Kbd, Spinner, cx, modKey } from './components/ui';
import { HomePage } from './pages/Home';
import { CategoryPage, FavoritesPage, RecentPage } from './pages/Lists';
import { NotFound } from './pages/NotFound';

const ToolPage = lazy(() => import('./tools/ToolPage'));
const IntentPage = lazy(() => import('./pages/Intent'));
const DoPage = lazy(() => import('./pages/Do'));
const HistoryPage = lazy(() => import('./pages/History'));
const FilesPage = lazy(() => import('./pages/Files'));
const SettingsPage = lazy(() => import('./pages/Settings'));
const AccountPage = lazy(() => import('./pages/Account'));
const AdminPage = lazy(() => import('./pages/admin/Admin'));

function PageFallback() {
  return <div className="page-loading" aria-busy="true"><Spinner size={18} /></div>;
}

/** A dialog or popover other than the Tools menu is open (Customize, Layout, a confirm). */
export const otherDialogOpen = () => !!document.querySelector('[role="dialog"]:not(.tools-menu), [role="alertdialog"]');

export function Shell() {
  const route = useRoute();
  const { isServer, isAdmin, toolMap } = useApp();
  const [palette, setPalette] = useState(false);
  const [query, setQuery] = useState('');
  const [drawer, setDrawer] = useState(false);
  const [toolsOpen, setToolsOpen] = useState(false);
  const hidden = useStore(sidebarCollapsed);
  const favs = useStore(favStore);
  const side = useStore(homeLayout).sidebar;
  const main = useRef<HTMLElement>(null);

  const openPalette = useCallback((q?: string) => { setDrawer(false); setToolsOpen(false); setQuery(typeof q === 'string' ? q : ''); setPalette(true); }, []);
  const closePalette = useCallback(() => setPalette(false), []);
  const closeTools = useCallback(() => setToolsOpen(false), []);
  const toggleSidebar = useCallback(() => sidebarCollapsed.set(!sidebarCollapsed.get()), []);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = !!t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setQuery('');
        setPalette((p) => !p);
      } else if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
        e.preventDefault();
        toggleSidebar();
      } else if (e.key === '/' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        setQuery('');
        setPalette(true);
      } else if (e.key.toLowerCase() === 't' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey && !palette && !otherDialogOpen()) {
        e.preventDefault();
        setToolsOpen((o) => !o);
      } else if (e.altKey && !e.metaKey && !e.ctrlKey && /^Digit[1-9]$/.test(e.code)) {
        const id = favs[Number(e.code.slice(5)) - 1];
        if (id && toolMap.has(id)) { e.preventDefault(); navigate(href.tool(id)); }
      } else if (e.key === 'Escape' && drawer) {
        setDrawer(false);
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [drawer, palette, favs, toolMap, toggleSidebar]);

  // The tool row's search button opens the palette.
  useEffect(() => {
    const h = () => openPalette();
    window.addEventListener('dgb:search', h);
    return () => window.removeEventListener('dgb:search', h);
  }, [openPalette]);

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
    case 'open': page = <IntentPage key={route.query.get('v') ?? ''} />; break;
    case 'do': page = <DoPage key={route.query.get('t') ?? ''} request={route.query.get('q') ?? ''} withFiles={route.query.get('files') === '1'} runKey={route.query.get('t') ?? ''} />; break;
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
    <div className={cx('app', drawer && 'drawer-open', hidden && 'sidebar-hidden', side === 'right' && 'sidebar-right')}>
      <a href="#main" className="skip-link" onClick={(e) => { e.preventDefault(); main.current?.focus(); }}>Skip to content</a>
      <header className="topbar">
        <button className="icon-btn" onClick={() => setDrawer(true)} aria-label="Open navigation" aria-expanded={drawer}>
          <MenuIcon size={18} aria-hidden />
        </button>
        <a href="#/" className="brand brand-sm"><LogoMark size={22} /><span className="brand-name">Dogubako</span></a>
        <button className="icon-btn" onClick={() => openPalette()} aria-label="Search tools"><Search size={18} aria-hidden /></button>
      </header>
      <div className="sidebar-wrap">
        <Sidebar onSearch={() => openPalette()} onNavigate={() => setDrawer(false)} onHide={toggleSidebar} />
      </div>
      {hidden && (
        <div className="mini-bar glass" role="toolbar" aria-label="Navigation">
          <button className="icon-btn" onClick={toggleSidebar} aria-label="Show sidebar" title={`Show sidebar (${modKey} \\)`}><PanelLeftOpen size={16} aria-hidden /></button>
          <a href="#/" className="brand brand-sm" aria-label="Dogubako home"><LogoMark size={17} /><span className="brand-name">Dogubako</span></a>
          <button className={cx('mini-btn', toolsOpen && 'is-on')} onClick={() => setToolsOpen((o) => !o)} aria-expanded={toolsOpen} aria-haspopup="dialog" aria-keyshortcuts="T">Tools <ChevronDown size={13} aria-hidden /></button>
          <button className="mini-btn" onClick={() => openPalette()} aria-keyshortcuts="Control+K Meta+K">Search <Kbd>{modKey} K</Kbd></button>
          <a className="mini-btn" href={href.history}><History size={14} aria-hidden />History</a>
        </div>
      )}
      {drawer && <div className="scrim" onClick={() => setDrawer(false)} aria-hidden />}
      <main className="main" id="main" ref={main} tabIndex={-1}>
        <Suspense fallback={<PageFallback />}>{page}</Suspense>
      </main>
      <CommandPalette open={palette} onClose={closePalette} initialQuery={query} />
      <DropOverlay />
      <ToolsMenu open={toolsOpen} onClose={closeTools} anchor={hidden ? { left: 16, top: 70 } : undefined} />
    </div>
  );
}
