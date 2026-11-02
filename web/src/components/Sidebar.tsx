import { memo, useMemo } from 'react';
import {
  Folder, History, KeyRound, LogOut, Search, Settings, Shield, UserRound, Moon, Sun, ChevronsUpDown, PanelLeftClose, type LucideIcon,
} from 'lucide-react';
import { useApp } from '../lib/app';
import { href, useRoute } from '../lib/router';
import { categoryIcon } from '../lib/icons';
import { cycleTheme, favorites as favStore, resolvedTheme, useStore, prefs } from '../lib/store';
import { LogoMark } from './Logo';
import { Kbd, Menu, MenuItem, cx, modKey } from './ui';

function NavLink({ to, icon: Icon, label, active, count, hint, onNavigate }: {
  to: string; icon: LucideIcon; label: string; active: boolean; count?: number; hint?: React.ReactNode; onNavigate?: () => void;
}) {
  return (
    <a href={to} className={cx('nav-link', active && 'is-active')} aria-current={active ? 'page' : undefined} onClick={onNavigate}>
      <Icon size={16} aria-hidden />
      <span className="nav-label">{label}</span>
      {count != null && count > 0 && <span className="nav-count">{count}</span>}
      {hint}
    </a>
  );
}

export const Sidebar = memo(function Sidebar({ onSearch, onNavigate, onHide }: {
  onSearch: () => void; onNavigate?: () => void; onHide?: () => void;
}) {
  const { categories, isServer, isAdmin, status, me, signOut, toolMap, visibleTools } = useApp();
  const route = useRoute();
  const favs = useStore(favStore);
  useStore(prefs);
  const [p0, p1] = route.parts;
  const is = (a: string, b?: string) => p0 === a && (b === undefined || p1 === b);
  const showUserMenu = isServer && status.auth === 'required' && me;
  const dark = resolvedTheme() === 'dark';

  const pinned = useMemo(
    () => favs.map((id) => toolMap.get(id)).filter((t): t is NonNullable<typeof t> => !!t && t.permitted !== false),
    [favs, toolMap],
  );
  const missing = useMemo(() => visibleTools.filter((t) => !t.availability.available).length, [visibleTools]);

  return (
    <nav className="sidebar" aria-label="Main">
      <div className="sidebar-brand">
        <a href={href.home} className="brand" onClick={onNavigate} aria-label="Dogubako home">
          <LogoMark size={20} />
          <span className="brand-name">Dogubako</span>
        </a>
        <span className="sidebar-brand-actions">
          <button className="icon-btn" onClick={cycleTheme} aria-label={dark ? 'Switch to light theme' : 'Switch to dark theme'} title="Toggle theme">
            {dark ? <Sun size={15} aria-hidden /> : <Moon size={15} aria-hidden />}
          </button>
          {onHide && (
            <button className="icon-btn hide-md" onClick={onHide} aria-label="Hide sidebar" title={`Hide sidebar (${modKey} \\)`} aria-keyshortcuts="Control+Backslash Meta+Backslash">
              <PanelLeftClose size={16} aria-hidden />
            </button>
          )}
        </span>
      </div>

      <button className="search-trigger" onClick={onSearch} aria-keyshortcuts="Control+K Meta+K">
        <Search size={15} aria-hidden />
        <span>Search tools and files</span>
        <Kbd>{modKey} K</Kbd>
      </button>

      <div className="sidebar-scroll">
        <div className="nav-group">
          <div className="nav-heading-row">
            <span className="nav-heading" id="nav-pins">Pinned</span>
            {pinned.length > 0 && <a className="nav-heading-link" href={href.favorites} onClick={onNavigate}>Edit</a>}
          </div>
          {pinned.length === 0 ? (
            <p className="nav-empty">Pin tools you use often from the Tools menu or any tool page.</p>
          ) : (
            <div role="list" aria-labelledby="nav-pins">
              {pinned.slice(0, 9).map((t, i) => (
                <div role="listitem" key={t.id}>
                  <NavLink to={href.tool(t.id)} icon={categoryIcon(t.category)} label={t.name} active={is('tool', t.id)} onNavigate={onNavigate}
                    hint={i < 9 ? <span className="nav-key" aria-hidden>{isMacKey(i + 1)}</span> : undefined} />
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="nav-group">
          <div className="nav-heading-row">
            <span className="nav-heading" id="nav-cats">All tools</span>
            <span className="nav-heading-count">{visibleTools.length}</span>
          </div>
          <div role="list" aria-labelledby="nav-cats">
            {categories.map((c) => (
              <div role="listitem" key={c.id}>
                <NavLink to={href.category(c.id)} icon={categoryIcon(c.id)} label={c.label} active={is('category', c.id)} count={c.count} onNavigate={onNavigate} />
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="sidebar-foot">
        <NavLink to={href.history} icon={History} label="History" active={is('history')} onNavigate={onNavigate} />
        {isServer && <NavLink to={href.files} icon={Folder} label="Files" active={is('files')} onNavigate={onNavigate} />}
        <NavLink to={href.settings()} icon={Settings} label="Settings" active={is('settings')} onNavigate={onNavigate}
          hint={missing > 0 ? <span className="nav-engines" title={`${missing} tools need extra software`}><span className="engine-dot is-partial" aria-hidden />{missing} need software</span> : undefined} />
        {isServer && isAdmin && <NavLink to={href.admin()} icon={Shield} label="Admin" active={is('admin')} onNavigate={onNavigate} />}
        {showUserMenu && (
          <Menu
            label="Account"
            trigger={(t) => (
              <button className="user-btn" {...t}>
                <span className="avatar" aria-hidden>{me!.user.username.slice(0, 1).toUpperCase()}</span>
                <span className="user-meta">
                  <span className="user-name">{me!.user.username}</span>
                  <span className="user-role">{me!.user.role} · {status.instance_name}</span>
                </span>
                <ChevronsUpDown size={14} aria-hidden />
              </button>
            )}
          >
            {(close) => (
              <>
                <MenuItem icon={UserRound} onSelect={() => { close(); location.hash = href.account; onNavigate?.(); }}>Account & password</MenuItem>
                <MenuItem icon={KeyRound} onSelect={() => { close(); location.hash = `${href.account}?tab=tokens`; onNavigate?.(); }}>API tokens</MenuItem>
                <div className="menu-sep" role="separator" />
                <MenuItem icon={LogOut} onSelect={() => { close(); void signOut(); }}>Sign out</MenuItem>
              </>
            )}
          </Menu>
        )}
      </div>
    </nav>
  );
});

/** Alt/Option + number opens a pinned tool. */
function isMacKey(n: number) {
  return typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent) ? `⌥${n}` : `Alt ${n}`;
}
