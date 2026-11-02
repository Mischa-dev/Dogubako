import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { ArrowDown, ArrowUp, X } from 'lucide-react';
import {
  HOME_LAYOUT_DEFAULT, homeLayout, homeModules, prefs, setPref, sidebarCollapsed, useStore,
  type HomeLayout, type HomeModuleId, type ThemePref,
} from '../lib/store';
import { Switch, cx } from './ui';

export const MODULE_INFO: Record<HomeModuleId, { name: string; hint: string }> = {
  bench: { name: 'On your bench', hint: 'Files you worked on lately, ready to pick up' },
  runs: { name: 'Recent runs', hint: 'What ran, how it went, how long ago' },
  tip: { name: 'Worth doing', hint: 'A suggestion based on your files' },
  pinned: { name: 'Pinned tools', hint: 'Your pins as quick buttons' },
  categories: { name: 'All tools', hint: 'Every category at a glance' },
};

function Seg<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: [T, string][]; onChange: (v: T) => void }) {
  return (
    <div className="cz-row">
      <span className="cz-row-label">{label}</span>
      <div className="mini-seg" role="group" aria-label={label}>
        {options.map(([v, l]) => <button key={v} type="button" className={cx(value === v && 'is-on')} aria-pressed={value === v} onClick={() => onChange(v)}>{l}</button>)}
      </div>
    </div>
  );
}

/** A side sheet for arranging the home page. Curated: a few choices that matter, nothing fiddly. */
export function CustomizeHome({ open, onClose }: { open: boolean; onClose: () => void }) {
  const layout = useStore(homeLayout);
  const hidden = useStore(sidebarCollapsed);
  const theme = useStore(prefs).theme;
  const sheet = useRef<HTMLDivElement>(null);
  const set = (patch: Partial<HomeLayout>) => homeLayout.set((l) => ({ ...l, ...patch }));
  const modules = homeModules(layout);

  useEffect(() => {
    if (!open) return;
    const prev = document.activeElement as HTMLElement | null;
    sheet.current?.querySelector<HTMLElement>('button')?.focus();
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } };
    window.addEventListener('keydown', key, true);
    return () => { window.removeEventListener('keydown', key, true); prev?.focus?.(); };
  }, [open, onClose]);

  if (!open) return null;

  const move = (i: number, d: -1 | 1) => {
    const next = [...modules];
    const j = i + d;
    if (j < 0 || j >= next.length) return;
    [next[i], next[j]] = [next[j], next[i]];
    set({ modules: next });
  };
  const toggle = (id: HomeModuleId, on: boolean) => set({ modules: modules.map((m) => (m.id === id ? { ...m, on } : m)) });
  const sidebar = hidden ? 'hidden' : layout.sidebar;

  return createPortal(
    <>
      <div className="cz-scrim" onClick={onClose} aria-hidden />
      <div className="cz-sheet" role="dialog" aria-modal="true" aria-labelledby="cz-title" ref={sheet}>
        <div className="cz-head">
          <h2 id="cz-title" className="cz-title">Customize home</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Close"><X size={16} aria-hidden /></button>
        </div>

        <section className="cz-section" aria-labelledby="cz-sections">
          <h3 id="cz-sections" className="cz-label">Sections</h3>
          <ul className="cz-modules">
            {modules.map((m, i) => (
              <li key={m.id} className={cx('cz-module', !m.on && 'is-off')}>
                <span className="cz-module-text">
                  <span className="cz-module-name">{MODULE_INFO[m.id].name}</span>
                  <span className="cz-module-hint">{MODULE_INFO[m.id].hint}</span>
                </span>
                <span className="cz-move">
                  <button type="button" className="icon-btn" disabled={i === 0} onClick={() => move(i, -1)} aria-label={`Move ${MODULE_INFO[m.id].name} up`}><ArrowUp size={14} aria-hidden /></button>
                  <button type="button" className="icon-btn" disabled={i === modules.length - 1} onClick={() => move(i, 1)} aria-label={`Move ${MODULE_INFO[m.id].name} down`}><ArrowDown size={14} aria-hidden /></button>
                </span>
                <Switch checked={m.on} onChange={(on) => toggle(m.id, on)} label={`Show ${MODULE_INFO[m.id].name}`} />
              </li>
            ))}
          </ul>
        </section>

        <section className="cz-section" aria-labelledby="cz-layout">
          <h3 id="cz-layout" className="cz-label">Layout</h3>
          <Seg<'left' | 'right' | 'hidden'>
            label="Sidebar"
            value={sidebar}
            options={[['left', 'Left'], ['right', 'Right'], ['hidden', 'Hidden']]}
            onChange={(v) => { if (v === 'hidden') sidebarCollapsed.set(true); else { sidebarCollapsed.set(false); set({ sidebar: v }); } }}
          />
          <Seg<'compact' | 'large'> label="Bench files" value={layout.shelf} options={[['compact', 'Compact'], ['large', 'Large']]} onChange={(v) => set({ shelf: v })} />
        </section>

        <section className="cz-section" aria-labelledby="cz-look">
          <h3 id="cz-look" className="cz-label">Appearance</h3>
          <Seg<ThemePref> label="Theme" value={theme} options={[['system', 'System'], ['light', 'Light'], ['dark', 'Dark']]} onChange={(v) => setPref('theme', v)} />
          <div className="cz-row">
            <span className="cz-row-label" id="cz-glass">Frosted panels<span className="cz-row-hint">Turn off for plain paper, or on slower machines</span></span>
            <Switch checked={layout.glass} onChange={(glass) => set({ glass })} describedBy="cz-glass" label="Frosted panels" />
          </div>
        </section>

        <div className="cz-foot">
          <button type="button" className="link-sm cz-reset" onClick={() => { homeLayout.set(HOME_LAYOUT_DEFAULT); sidebarCollapsed.set(false); }}>Reset to default</button>
          <span className="cz-saved">Saved on this device</span>
        </div>
      </div>
    </>,
    document.body,
  );
}
