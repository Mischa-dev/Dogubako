import { memo } from 'react';
import {
  Clock, Folder, History, House, KeyRound, LogOut, Search, Settings, Shield, Star, UserRound, Moon, Sun, ChevronsUpDown, type LucideIcon,
} from 'lucide-react';
import { useApp } from '../lib/app';
import { href, useRoute } from '../lib/router';
import { categoryIcon } from '../lib/icons';
import { cycleTheme, favorites as favStore, resolvedTheme, useStore, prefs } from '../lib/store';
import { LogoMark } from './Logo';
import { Kbd, Menu, MenuItem, cx, modKey } from './ui';

function NavLink({ to, icon: Icon, label, active, count, onNavigate }: {
  to: string; icon: LucideIcon; label: string; active: boolean; count?: number; onNavigate?: () => void;
}) {
  return (
    <a href={to} className={cx('nav-link', active && 'is-active')} aria-current={active ? 'page' : undefined} onClick={onNavigate}>
      <Icon size={16} aria-hidden />
      <span className="nav-label">{label}</span>
      {count != null && count > 0 && <span className="nav-count">{count}</span>}
    </a>
  );
}

export const Sidebar = memo(function Sidebar({ onSearch, onNavigate }: { onSearch: () => void; onNavigate?: () => void }) {
  const { categories, isServer, isAdmin, status, me, signOut } = useApp();
  const route = useRoute();
  const favs = useStore(favStore);
  useStore(prefs);
  const [p0, p1] = route.parts;
  const is = (a: string, b?: string) => p0 === a && (b === undefined || p1 === b);
  const showUserMenu = isServer && status.auth === 'required' && me;
  const dark = resolvedTheme() === 'dark';

  return (
    <nav className="sidebar" aria-label="Main">
      <div className="sidebar-brand">
        <a href={href.home} className="brand" onClick={onNavigate} aria-label="Dogubako home">
          <LogoMark size={24} />
          <span className="brand-name">Dogubako</span>
        </a>
        <button className="icon-btn" onClick={cycleTheme} aria-label={dark ? 'Switch to light theme' : 'Switch to dark theme'} title="Toggle theme">
          {dark ? <Sun size={15} aria-hidden /> : <Moon size={15} aria-hidden />}
        </button>
      </div>

      <button className="search-trigger" onClick={onSearch} aria-keyshortcuts="Control+K Meta+K">
        <Search size={15} aria-hidden />
        <span>Search tools</span>
        <Kbd>{modKey} K</Kbd>
      </button>

      <div className="sidebar-scroll">
        <div className="nav-group">
          <NavLink to={href.home} icon={House} label="Home" active={route.parts.length === 0} onNavigate={onNavigate} />
          <NavLink to={href.favorites} icon={Star} label="Favorites" active={is('favorites')} count={favs.length} onNavigate={onNavigate} />
          <NavLink to={href.recent} icon={Clock} label="Recent" active={is('recent')} onNavigate={onNavigate} />
        </div>
        <div className="nav-group">
          <div className="nav-heading" id="nav-cats">Categories</div>
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
        <NavLink to={href.settings()} icon={Settings} label="Settings" active={is('settings')} onNavigate={onNavigate} />
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
