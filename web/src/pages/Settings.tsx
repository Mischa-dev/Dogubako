import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Settings as SettingsIcon, SlidersHorizontal, Package, Brain, Info, Sun, Moon, Monitor, RefreshCw, CircleCheck, CircleDashed,
  Download, Trash2, ExternalLink, ChevronRight,
} from 'lucide-react';
import type { DepInfo, ModelDownload, ModelInfo } from '../lib/types';
import { useApp, notifyToolsChanged } from '../lib/app';
import { asServer } from '../lib/backend';
import { href } from '../lib/router';
import { prefs, setPref, useStore, favorites, recents, clearAllToolParams, type ThemePref, type Density } from '../lib/store';
import { formatBytes } from '../lib/format';
import { errorText } from '../lib/errors';
import { Badge, Button, Callout, CopyButton, PageHeader, ProgressBar, Section, Segmented, Spinner, Switch, cx, useConfirm } from '../components/ui';
import { toast } from '../components/toast';
import { LogoMark } from '../components/Logo';

const SECTIONS = [
  { id: 'general', label: 'General', icon: SlidersHorizontal },
  { id: 'dependencies', label: 'Dependencies', icon: Package },
  { id: 'models', label: 'Models', icon: Brain },
  { id: 'about', label: 'About', icon: Info },
] as const;

export default function SettingsPage({ section = 'general' }: { section?: string }) {
  const cur = SECTIONS.find((s) => s.id === section) ?? SECTIONS[0];
  return (
    <div className="page page-settings">
      <PageHeader icon={SettingsIcon} title="Settings" />
      <div className="settings-layout">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map((s) => (
            <a key={s.id} href={href.settings(s.id === 'general' ? '' : s.id)} className={cx('settings-link', cur.id === s.id && 'is-active')} aria-current={cur.id === s.id ? 'page' : undefined}>
              <s.icon size={15} aria-hidden />{s.label}
            </a>
          ))}
        </nav>
        <div className="settings-body">
          {cur.id === 'general' && <General />}
          {cur.id === 'dependencies' && <Dependencies />}
          {cur.id === 'models' && <Models />}
          {cur.id === 'about' && <About />}
        </div>
      </div>
    </div>
  );
}

function Row({ label, desc, children, htmlFor }: { label: string; desc?: string; children: React.ReactNode; htmlFor?: string }) {
  return (
    <div className="setting-row">
      <div className="setting-text">
        {htmlFor ? <label className="setting-label" htmlFor={htmlFor}>{label}</label> : <span className="setting-label">{label}</span>}
        {desc && <span className="setting-desc">{desc}</span>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

function General() {
  const p = useStore(prefs);
  const favs = useStore(favorites);
  const recs = useStore(recents);
  const { isServer } = useApp();
  const [confirm, confirmNode] = useConfirm();
  return (
    <>
      <Section title="Appearance">
        <div className="settings-card">
          <Row label="Theme" desc="Follow your system or pick one.">
            <Segmented<ThemePref> label="Theme" value={p.theme} onChange={(v) => setPref('theme', v)} options={[
              { value: 'system', label: 'System', icon: Monitor }, { value: 'light', label: 'Light', icon: Sun }, { value: 'dark', label: 'Dark', icon: Moon },
            ]} />
          </Row>
          <Row label="Density" desc="Compact shows more tools per screen.">
            <Segmented<Density> label="Density" value={p.density} onChange={(v) => setPref('density', v)} options={[
              { value: 'comfortable', label: 'Comfortable' }, { value: 'compact', label: 'Compact' },
            ]} />
          </Row>
        </div>
      </Section>
      <Section title="Behavior">
        <div className="settings-card">
          <Row label="Remember tool options" desc="Restore each tool's last settings (never files or secrets)." htmlFor="pref-remember">
            <Switch id="pref-remember" checked={p.rememberParams} onChange={(v) => setPref('rememberParams', v)} />
          </Row>
          {!isServer && (
            <Row label="Keep run history" desc="Store a local list of past runs on this device." htmlFor="pref-history">
              <Switch id="pref-history" checked={p.history} onChange={(v) => setPref('history', v)} />
            </Row>
          )}
        </div>
      </Section>
      <Section title="Data on this device">
        <div className="settings-card">
          <Row label="Favorites" desc={`${favs.length} starred tool${favs.length === 1 ? '' : 's'}.`}>
            <Button size="sm" variant="secondary" disabled={!favs.length} onClick={async () => { if (await confirm({ title: 'Clear favorites?', confirm: 'Clear', danger: true })) favorites.set([]); }}>Clear</Button>
          </Row>
          <Row label="Recently used" desc={`${recs.length} tool${recs.length === 1 ? '' : 's'}.`}>
            <Button size="sm" variant="secondary" disabled={!recs.length} onClick={() => recents.set([])}>Clear</Button>
          </Row>
          <Row label="Remembered options" desc="Forget saved settings for every tool.">
            <Button size="sm" variant="secondary" onClick={() => { clearAllToolParams(); toast.success('Saved tool options cleared'); }}>Clear</Button>
          </Row>
        </div>
      </Section>
      {confirmNode}
    </>
  );
}

const OS_KEYS: { key: 'apt' | 'dnf' | 'pacman' | 'brew' | 'winget'; label: string }[] = [
  { key: 'apt', label: 'Debian / Ubuntu' }, { key: 'dnf', label: 'Fedora' }, { key: 'pacman', label: 'Arch' }, { key: 'brew', label: 'macOS (Homebrew)' }, { key: 'winget', label: 'Windows' },
];

function defaultOs(platform?: string): (typeof OS_KEYS)[number]['key'] {
  const p = (platform ?? navigator.platform ?? '').toLowerCase();
  if (p.includes('mac')) return 'brew';
  if (p.includes('win')) return 'winget';
  return 'apt';
}

function Dependencies() {
  const { backend, isAdmin, isServer, toolMap } = useApp();
  const [data, setData] = useState<{ dependencies: DepInfo[]; platform?: string } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [os, setOs] = useState<(typeof OS_KEYS)[number]['key'] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const load = useCallback(async () => {
    try { const d = await backend.deps(); setData(d); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [backend]);
  useEffect(() => { void load(); }, [load]);
  const osKey = os ?? defaultOs(data?.platform);
  const sorted = useMemo(() => [...(data?.dependencies ?? [])].sort((a, b) => Number(a.found) - Number(b.found) || (b.used_by?.length ?? 0) - (a.used_by?.length ?? 0)), [data]);
  const missing = sorted.filter((d) => !d.found).length;
  const refresh = async () => {
    const s = asServer(backend);
    try { if (s) await s.depsRefresh(); await load(); notifyToolsChanged(); toast.success('Rechecked dependencies'); } catch (e) { toast.error(errorText(e)); }
  };
  return (
    <Section
      title="Dependencies"
      description={isServer ? 'External programs installed on the server. Tools that need a missing program stay visible but can’t run.' : 'Optional programs that unlock more tools. Install them, then recheck.'}
      actions={(!isServer || isAdmin) && <Button size="sm" variant="ghost" icon={RefreshCw} onClick={refresh}>Recheck</Button>}
    >
      {err && <Callout tone="danger">{err}</Callout>}
      {!data && !err && <Spinner />}
      {data && (
        <>
          <div className="deps-toolbar">
            <span className="muted small">{sorted.length - missing} of {sorted.length} found</span>
            <Segmented size="sm" label="Install commands for" value={osKey} onChange={setOs} options={OS_KEYS.map((o) => ({ value: o.key, label: o.label.split(' ')[0], title: o.label }))} />
          </div>
          <ul className="dep-list">
            {sorted.map((d) => {
              const cmd = d.install?.[osKey];
              const isOpen = open === d.id;
              const tools = d.used_by ?? [];
              return (
                <li key={d.id} className={cx('dep', !d.found && 'is-missing')}>
                  <button type="button" className="dep-head" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : d.id)}>
                    {d.found ? <CircleCheck size={16} aria-hidden className="ok-icon" /> : <CircleDashed size={16} aria-hidden className="muted" />}
                    <span className="dep-text">
                      <span className="dep-name">{d.name}{d.version && <span className="dep-ver mono">{d.version}</span>}</span>
                      <span className="dep-purpose">{d.purpose}</span>
                    </span>
                    {tools.length > 0 && <span className="dep-count">{tools.length} tool{tools.length === 1 ? '' : 's'}</span>}
                    <Badge tone={d.found ? 'success' : 'neutral'}>{d.found ? 'Found' : 'Missing'}</Badge>
                    <ChevronRight size={14} aria-hidden className="chev" />
                  </button>
                  {isOpen && (
                    <div className="dep-body">
                      {!d.found && (cmd ? (
                        <div className="cmd"><code className="mono">{cmd}</code><CopyButton text={cmd} iconOnly label="Copy install command" /></div>
                      ) : d.install_hint ? <p className="small">{d.install_hint}</p> : null)}
                      {!d.found && d.install?.url && (
                        <a className="link-sm" href={d.install.url} target="_blank" rel="noopener noreferrer"><ExternalLink size={12} aria-hidden /> Project website</a>
                      )}
                      {d.license && <p className="muted small">License: {d.license}</p>}
                      {tools.length > 0 && (
                        <div className="dep-tools">
                          <span className="muted small">Used by</span>
                          <span className="kv-list">
                            {tools.map((id) => <a key={id} className="kv-pill" href={href.tool(id)}>{toolMap.get(id)?.name ?? id}</a>)}
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </>
      )}
    </Section>
  );
}

function Models() {
  const { backend, isAdmin, isServer } = useApp();
  const [data, setData] = useState<{ models: ModelInfo[]; downloads: ModelDownload[] } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [local, setLocal] = useState<Record<string, { done: number; total: number }>>({});
  const [confirm, confirmNode] = useConfirm();
  const canManage = !isServer || isAdmin;
  const load = useCallback(async () => {
    try { setData(await backend.models()); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [backend]);
  useEffect(() => { void load(); }, [load]);
  const active = (data?.downloads ?? []).some((d) => d.status === 'downloading') || Object.keys(local).length > 0;
  useEffect(() => {
    if (!active) return;
    const t = window.setInterval(() => void load(), 1000);
    return () => window.clearInterval(t);
  }, [active, load]);
  useEffect(() => {
    const b = backend as any;
    if (!b.onModelProgress) return;
    let un: (() => void) | undefined;
    void b.onModelProgress((p: { id: string; done: number; total: number }) => setLocal((l) => ({ ...l, [p.id]: { done: p.done, total: p.total } }))).then((u: () => void) => { un = u; });
    return () => un?.();
  }, [backend]);

  const download = async (m: ModelInfo) => {
    try {
      if (backend.mode === 'desktop') setLocal((l) => ({ ...l, [m.id]: { done: 0, total: m.size_bytes } }));
      const p = backend.modelDownload(m.id);
      if (backend.mode === 'server') { await p; await load(); }
      else { await p; setLocal((l) => { const n = { ...l }; delete n[m.id]; return n; }); await load(); notifyToolsChanged(); toast.success(`${m.name} installed`); }
    } catch (e) {
      setLocal((l) => { const n = { ...l }; delete n[m.id]; return n; });
      toast.error(errorText(e));
    }
  };
  const remove = async (m: ModelInfo) => {
    if (!(await confirm({ title: `Remove ${m.name}?`, message: `Frees ${formatBytes(m.size_bytes)}. Tools that need it become unavailable.`, confirm: 'Remove', danger: true }))) return;
    try { await backend.modelRemove(m.id); await load(); notifyToolsChanged(); } catch (e) { toast.error(errorText(e)); }
  };
  // When a server download finishes, refresh tool availability.
  const doneIds = (data?.downloads ?? []).filter((d) => d.status === 'done').map((d) => d.model).join(',');
  useEffect(() => { if (doneIds) notifyToolsChanged(); }, [doneIds]);

  const groups = useMemo(() => {
    const m = new Map<string, ModelInfo[]>();
    for (const x of data?.models ?? []) m.set(x.group, [...(m.get(x.group) ?? []), x]);
    return [...m.entries()];
  }, [data]);

  return (
    <Section title="Models" description="Optional machine-learning models that run locally — for transcription, face detection and voices. Nothing is sent to the cloud.">
      {!canManage && <Callout tone="info">Only administrators can install or remove models on this server.</Callout>}
      {err && <Callout tone="danger">{err}</Callout>}
      {!data && !err && <Spinner />}
      {groups.map(([g, ms]) => (
        <div key={g} className="model-group">
          <h3 className="model-group-title">{g}</h3>
          <ul className="model-list">
            {ms.map((m) => {
              const dl = data!.downloads.find((d) => d.model === m.id);
              const loc = local[m.id];
              const downloading = dl?.status === 'downloading' || !!loc;
              const prog = loc ? (loc.total ? loc.done / loc.total : undefined) : dl && dl.total ? dl.done / dl.total : undefined;
              return (
                <li key={m.id} className="model">
                  <div className="model-text">
                    <span className="model-name">{m.name} {m.installed && <Badge tone="success">Installed</Badge>}</span>
                    {m.description && <span className="model-desc">{m.description}</span>}
                    <span className="model-meta">{formatBytes(m.size_bytes)}{m.license ? ` · ${m.license}` : ''} · <span className="mono">{m.id}</span></span>
                    {downloading && (
                      <div className="model-progress">
                        <ProgressBar value={prog} label={`Downloading ${m.name}`} />
                        <span className="muted small">{loc ? formatBytes(loc.done) : dl ? formatBytes(dl.done) : ''}{prog != null ? ` · ${Math.round(prog * 100)}%` : ''}</span>
                      </div>
                    )}
                    {dl?.status === 'failed' && <span className="field-error">{dl.error ?? 'Download failed'}</span>}
                  </div>
                  {canManage && (
                    <div className="model-actions">
                      {m.installed
                        ? <Button size="sm" variant="ghost" icon={Trash2} onClick={() => void remove(m)}>Remove</Button>
                        : <Button size="sm" variant="secondary" icon={Download} onClick={() => void download(m)} loading={downloading}>Install</Button>}
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      ))}
      {data && data.models.length === 0 && <p className="muted">No models are available for this build.</p>}
      {confirmNode}
    </Section>
  );
}

function About() {
  const { status, visibleTools, isServer } = useApp();
  return (
    <Section title="About">
      <div className="about">
        <LogoMark size={44} />
        <div>
          <div className="about-name">Dogubako <span className="muted">道具箱</span></div>
          <div className="muted small">Version {status.version} · {isServer ? `server “${status.instance_name}”` : 'desktop'} · {visibleTools.length} tools</div>
        </div>
      </div>
      <div className="settings-card">
        <Row label="License" desc="Free and open-source software."><span className="mono small">AGPL-3.0-or-later</span></Row>
        <Row label="Privacy" desc={isServer ? 'Files are processed by this server only and deleted after the retention period.' : 'Everything runs on this device; nothing is uploaded.'}><span /></Row>
        {isServer && <Row label="API" desc="Automate tools over HTTP or connect AI agents via MCP."><a className="link-sm" href="api/v1/openapi.json" target="_blank" rel="noopener noreferrer">OpenAPI <ExternalLink size={12} aria-hidden /></a></Row>}
        <Row label="Source code & docs"><a className="link-sm" href="https://github.com/mischa-dev/dogubako" target="_blank" rel="noopener noreferrer">GitHub <ExternalLink size={12} aria-hidden /></a></Row>
      </div>
      <p className="muted small about-notice">
        Dogubako builds on many open-source projects. External programs (such as FFmpeg, ImageMagick, Ghostscript or Tesseract) are separate works
        used under their own licenses and are not bundled unless stated by your distribution. Downloadable models are distributed under the
        licenses listed in Settings → Models. Full third-party notices ship with each release.
      </p>
    </Section>
  );
}
