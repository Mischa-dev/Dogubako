import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Settings as SettingsIcon, SlidersHorizontal, Package, Brain, Info, Sun, Moon, Monitor, RefreshCw, CircleCheck, CircleDashed,
  Download, Trash2, ExternalLink, ChevronRight, Sparkles, Puzzle, Wrench, Store, Search,
} from 'lucide-react';
import { AssistantSettingsPanel } from '../components/ModelPicker';
import { ToolsSettings } from '../components/Packs';
import type { DepInfo, ExtensionSetting, ExtensionSettings, ModelDownload, ModelInfo, StoreResult, StoreTool } from '../lib/types';
import { useApp, notifyToolsChanged } from '../lib/app';
import { asServer } from '../lib/backend';
import { href } from '../lib/router';
import { prefs, setPref, useStore, favorites, recents, clearAllToolParams, type ThemePref, type Density } from '../lib/store';
import { formatBytes } from '../lib/format';
import { errorText, reportError } from '../lib/errors';
import { Badge, Button, Callout, CopyButton, PageHeader, ProgressBar, Section, Segmented, Spinner, Switch, cx, useConfirm } from '../components/ui';
import { toast } from '../components/toast';
import { LogoMark } from '../components/Logo';

const SECTIONS = [
  { id: 'general', label: 'General', icon: SlidersHorizontal },
  { id: 'assistant', label: 'Plain words', icon: Sparkles },
  { id: 'tools', label: 'Tools', icon: Wrench },
  { id: 'dependencies', label: 'Dependencies', icon: Package },
  { id: 'models', label: 'Models', icon: Brain },
  { id: 'store', label: 'Tool store', icon: Store },
  { id: 'extensions', label: 'Extensions', icon: Puzzle },
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
          {cur.id === 'assistant' && <Section title="Plain words" description="Say what you want done instead of picking a tool."><AssistantSettingsPanel /></Section>}
          {cur.id === 'tools' && <ToolsSection />}
          {cur.id === 'dependencies' && <Dependencies />}
          {cur.id === 'models' && <Models />}
          {cur.id === 'store' && <ToolStore />}
          {cur.id === 'extensions' && <Extensions />}
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
          <Row label="Pinned tools" desc={`${favs.length} pinned tool${favs.length === 1 ? '' : 's'}.`}>
            <Button size="sm" variant="secondary" disabled={!favs.length} onClick={async () => { if (await confirm({ title: 'Unpin all tools?', confirm: 'Clear', danger: true })) favorites.set([]); }}>Clear</Button>
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

/** Desktop-only: engines the app downloads into its own folder. */
interface EngineBackend {
  engineInstall(id: string): Promise<void>;
  onEngineProgress(cb: (p: { id: string; done: number; total: number }) => void): Promise<() => void>;
}

function Dependencies() {
  const { backend, isAdmin, isServer, toolMap } = useApp();
  const [data, setData] = useState<{ dependencies: DepInfo[]; platform?: string; docker_image?: boolean } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [os, setOs] = useState<(typeof OS_KEYS)[number]['key'] | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [fetching, setFetching] = useState<{ id: string; done: number; total: number } | null>(null);
  const load = useCallback(async () => {
    try { const d = await backend.deps(); setData(d); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [backend]);
  const engines = backend as Partial<EngineBackend>;
  const download = async (d: DepInfo) => {
    if (!d.downloadable || !engines.engineInstall) return;
    const id = d.downloadable;
    setFetching({ id, done: 0, total: d.download_bytes ?? 0 });
    const stop = await engines.onEngineProgress?.((p) => { if (p.id === id) setFetching(p); });
    try { await engines.engineInstall(id); await load(); notifyToolsChanged(); toast.success(`${d.name} is ready`); } catch (e) { toast.error(errorText(e)); }
    finally { stop?.(); setFetching(null); }
  };
  useEffect(() => { void load(); }, [load]);
  const osKey = os ?? defaultOs(data?.platform);
  // The Docker image's programs come only from the image: no package-manager
  // commands, the server's hint names the build option instead.
  const inImage = !!data?.docker_image;
  const sorted = useMemo(() => [...(data?.dependencies ?? [])].sort((a, b) => Number(a.found) - Number(b.found) || (b.used_by?.length ?? 0) - (a.used_by?.length ?? 0)), [data]);
  const missing = sorted.filter((d) => !d.found).length;
  const refresh = async () => {
    const s = asServer(backend);
    try { if (s) await s.depsRefresh(); await load(); notifyToolsChanged(); toast.success('Rechecked dependencies'); } catch (e) { toast.error(errorText(e)); }
  };
  const reload = async () => {
    try {
      const r = await backend.reloadRegistry!();
      notifyToolsChanged();
      const { added, removed, changed, tools } = r.reload;
      const parts = [added.length && `${added.length} added`, removed.length && `${removed.length} removed`, changed.length && `${changed.length} updated`].filter(Boolean);
      toast.success(parts.length ? `Reloaded tools: ${parts.join(', ')}` : `Reloaded tools: no changes (${tools} tools)`);
    } catch (e) { toast.error(errorText(e)); }
  };
  return (
    <Section
      title="Dependencies"
      description={inImage ? 'External programs built into this Docker image. Tools that need a missing program stay visible but can’t run.' : isServer ? 'External programs installed on the server. Tools that need a missing program stay visible but can’t run.' : 'Optional programs that unlock more tools. Install them, then recheck.'}
      actions={(!isServer || isAdmin) && <>
        {backend.reloadRegistry && <Button size="sm" variant="ghost" icon={RefreshCw} onClick={reload} title="Pick up installed, removed or edited tools without restarting">Reload tools</Button>}
        <Button size="sm" variant="ghost" icon={RefreshCw} onClick={refresh}>Recheck</Button>
      </>}
    >
      {err && <Callout tone="danger">{err}</Callout>}
      {!data && !err && <Spinner />}
      {data && (
        <>
          <div className="deps-toolbar">
            <span className="muted small">{sorted.length - missing} of {sorted.length} found</span>
            {!inImage && <Segmented size="sm" label="Install commands for" value={osKey} onChange={setOs} options={OS_KEYS.map((o) => ({ value: o.key, label: o.label.split(' ')[0], title: o.label }))} />}
          </div>
          <ul className="dep-list">
            {sorted.map((d) => {
              const cmd = inImage ? undefined : d.install?.[osKey];
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
                    <Badge tone={d.found ? 'success' : 'neutral'}>{d.source === 'bundled' ? 'Built in' : d.source === 'downloaded' ? 'Downloaded' : d.found ? 'Found' : d.downloadable ? 'Not downloaded' : 'Missing'}</Badge>
                    <ChevronRight size={14} aria-hidden className="chev" />
                  </button>
                  {isOpen && (
                    <div className="dep-body">
                      {!d.found && d.downloadable && engines.engineInstall ? (
                        <div className="cmd">
                          <Button size="sm" disabled={fetching != null} onClick={() => void download(d)}>
                            {fetching?.id === d.downloadable ? `Downloading… ${formatBytes(fetching.done)}` : `Download (${formatBytes(d.download_bytes)})`}
                          </Button>
                          <span className="muted small">Dogubako keeps it in its own folder and updates it with the app.</span>
                        </div>
                      ) : !d.found && (cmd ? (
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

function ToolsSection() {
  const { backend, isAdmin, isServer } = useApp();
  return <ToolsSettings backend={backend} canManage={!isServer || isAdmin} />;
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
        <LogoMark size={44} tile />
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


/** Settings → Tool store: install official tools on top of the bundled ones, and update them. */
export function ToolStore() {
  const { backend, isAdmin, isServer } = useApp();
  const canEdit = !isServer || isAdmin;
  const [query, setQuery] = useState('');
  const [res, setRes] = useState<StoreResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [pending, setPending] = useState(0);
  const load = useCallback(async (q: string) => {
    if (!backend.storeSearch) { setErr('The tool store is not available here.'); return; }
    try {
      const found = await backend.storeSearch(q);
      setRes(found); setErr(null);
      const all = q && backend.storeInstalled ? (await backend.storeInstalled()).tools : found.tools;
      setPending(all.filter((t) => t.update_available).length);
    } catch (e) { setErr(errorText(e)); }
  }, [backend]);
  useEffect(() => { if (canEdit) void load(''); }, [load, canEdit]);
  const act = async (t: StoreTool, update: boolean) => {
    setBusy(t.id);
    try {
      const reports = update && backend.storeUpdate ? (await backend.storeUpdate(t.id)).updated : backend.storeInstall ? [await backend.storeInstall(t.id)] : [];
      const bad = reports.map(reportError).find(Boolean);
      if (bad) toast.error(`${t.name}: ${bad}`); else toast.success(`${update ? 'Updated' : 'Installed'} ${t.name} ${t.version}`);
      notifyToolsChanged();
      await load(query);
    } catch (e) { toast.error(errorText(e)); } finally { setBusy(null); }
  };
  const updateAll = async () => {
    if (!backend.storeUpdate) return;
    setBusy('*');
    try {
      const reports = (await backend.storeUpdate()).updated;
      const bad = reports.map((r) => { const e = reportError(r); return e ? `${r.id}: ${e}` : null; }).find(Boolean);
      if (bad) toast.error(bad); else toast.success(`Updated ${reports.length} store tool${reports.length === 1 ? '' : 's'}`);
      notifyToolsChanged();
      await load(query);
    } catch (e) { toast.error(errorText(e)); } finally { setBusy(null); }
  };
  const tools = res?.tools ?? [];
  const offline = !!res?.index.offline;
  return (
    <Section title="Tool store" description="Official tools you can add on top of the bundled ones. Each is reviewed, pinned to the SHA-256 in the store index, and tested when installed.">
      {!canEdit && <Callout tone="info">Only administrators can install tools from the store.</Callout>}
      {res?.index.offline && <Callout tone="warn">Offline: showing the store as of {new Date(res.index.fetched_at * 1000).toLocaleString()}. Installing and updating need the network.</Callout>}
      {err && <Callout tone="danger">{err}</Callout>}
      {canEdit && (
        <form className="row-gap" role="search" onSubmit={(e) => { e.preventDefault(); void load(query); }}>
          <input className="input" type="search" aria-label="Search the tool store" placeholder="Search the tool store" value={query} onChange={(e) => setQuery(e.target.value)} />
          <Button type="submit" size="sm" variant="ghost"><Search size={14} aria-hidden />Search</Button>
          {pending > 1 && <Button type="button" size="sm" disabled={!!busy || offline} onClick={() => void updateAll()}>{busy === '*' ? <Spinner /> : `Update all (${pending})`}</Button>}
        </form>
      )}
      {canEdit && !res && !err && <Spinner />}
      {res && !tools.length && <p className="muted">No store tools match.</p>}
      {tools.map((t) => (
        <div key={t.id} className="settings-card" aria-label={t.name}>
          <div className="setting-row">
            <div className="setting-text">
              <span className="setting-label">{t.name} <span className="muted mono">{t.installed_version ?? t.version}</span></span>
              <span className="setting-desc">{t.description}</span>
              <span className="setting-desc mono">{t.id}{t.permissions.length ? ` · ${t.permissions.join(', ')}` : ''}{t.requires.length ? ` · needs ${t.requires.join(', ')}` : ''}</span>
              {t.unsupported && <span className="setting-desc">{t.installed ? `Version ${t.version} needs Dogubako ${t.min_dogubako} or newer. Update Dogubako to get it.` : `Needs Dogubako ${t.min_dogubako} or newer. Update Dogubako to install it.`}</span>}
            </div>
            <div className="setting-control">
              {t.unsupported && !t.installed ? (
                <Badge tone="warn">Needs Dogubako {t.min_dogubako}</Badge>
              ) : t.update_available ? (
                <Button size="sm" disabled={!!busy || offline} onClick={() => void act(t, true)}>{busy === t.id ? <Spinner /> : `Update to ${t.version}`}</Button>
              ) : t.installed ? (
                <Badge tone="success">Installed</Badge>
              ) : (
                <Button size="sm" variant="ghost" disabled={!!busy || offline} onClick={() => void act(t, false)}>{busy === t.id ? <Spinner /> : <><Download size={14} aria-hidden />Install</>}</Button>
              )}
            </div>
          </div>
        </div>
      ))}
    </Section>
  );
}

/** Settings → Extensions: per-installation values kept outside the trusted manifests. */
export function Extensions() {
  const { backend, isAdmin, isServer } = useApp();
  const [list, setList] = useState<ExtensionSettings[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const canEdit = !isServer || isAdmin;
  const load = useCallback(async () => {
    if (!backend.extensionSettings) { setList([]); return; }
    try { setList(await backend.extensionSettings()); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [backend]);
  useEffect(() => { if (canEdit) void load(); }, [load, canEdit]);
  return (
    <Section title="Extensions" description="Settings for extension tools on this installation. They are stored outside the manifest, so changing them keeps the tool trusted and sharing a manifest never shares them.">
      {!canEdit && <Callout tone="info">Only administrators can change extension settings.</Callout>}
      {err && <Callout tone="danger">{err}</Callout>}
      {canEdit && !list && !err && <Spinner />}
      {list && !list.length && <p className="muted">No installed extension declares settings.</p>}
      {list?.map((ext) => (
        <ExtensionForm key={ext.id} ext={ext} onSaved={(next) => { setList((l) => l?.map((x) => (x.id === next.id ? next : x)) ?? null); notifyToolsChanged(); }} />
      ))}
    </Section>
  );
}

function initialValue(s: ExtensionSetting): unknown {
  if (s.secret) return '';
  return s.value ?? (s.type === 'boolean' ? undefined : '');
}

function ExtensionForm({ ext, onSaved }: { ext: ExtensionSettings; onSaved: (e: ExtensionSettings) => void }) {
  const { backend } = useApp();
  const init = useMemo(() => Object.fromEntries(ext.settings.map((s) => [s.name, initialValue(s)])), [ext]);
  const [values, setValues] = useState<Record<string, unknown>>(init);
  const [saving, setSaving] = useState(false);
  useEffect(() => setValues(init), [init]);
  // Only changed values are sent; an empty secret field keeps the stored secret, null clears it.
  const changes = Object.fromEntries(ext.settings.filter((s) => (s.secret ? values[s.name] !== '' : values[s.name] !== init[s.name])).map((s) => [s.name, values[s.name]]));
  const dirty = Object.keys(changes).length > 0;
  const save = async (body: Record<string, unknown>) => {
    if (!backend.configureExtension) return;
    setSaving(true);
    try { onSaved(await backend.configureExtension(ext.id, body)); toast.success(`Saved ${ext.name} settings`); } catch (e) { toast.error(errorText(e)); } finally { setSaving(false); }
  };
  const set = (name: string, v: unknown) => setValues((x) => ({ ...x, [name]: v }));
  const unset = ext.settings.filter((s) => s.required && !s.set);
  return (
    <form className="settings-card" aria-label={`${ext.name} settings`} onSubmit={(e) => { e.preventDefault(); if (dirty) void save(changes); }}>
      <div className="setting-row">
        <div className="setting-text">
          <span className="setting-label">{ext.name}</span>
          <span className="setting-desc mono">{ext.id}</span>
        </div>
        <div className="setting-control">
          {unset.length ? <Badge tone="warn">Needs setup</Badge> : <Badge tone="success">Ready</Badge>}
        </div>
      </div>
      {ext.settings.map((s) => {
        const id = `ext-${ext.id}-${s.name}`;
        const v = values[s.name];
        return (
          <Row key={s.name} label={`${s.label}${s.required ? ' *' : ''}`} desc={s.description || undefined} htmlFor={id}>
            {s.type === 'boolean' ? (
              <Switch id={id} checked={!!v} onChange={(b) => set(s.name, b)} />
            ) : s.type === 'enum' ? (
              <select id={id} className="input" value={String(v ?? '')} onChange={(e) => set(s.name, e.target.value)}>
                <option value="">—</option>
                {s.options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            ) : s.secret ? (
              <span className="row-gap">
                <input id={id} className="input" type="password" autoComplete="off" value={String(v ?? '')} placeholder={v === null ? 'Cleared on save' : s.set ? 'Saved (type to replace)' : 'Not set'} onChange={(e) => set(s.name, e.target.value)} />
                {s.set && v !== null && <Button type="button" size="sm" variant="ghost" disabled={saving} onClick={() => set(s.name, null)}>Clear</Button>}
              </span>
            ) : s.type === 'text' && s.multiline ? (
              <textarea id={id} className="input" rows={3} value={String(v ?? '')} placeholder={s.default != null ? String(s.default) : ''} onChange={(e) => set(s.name, e.target.value)} />
            ) : (
              <input id={id} className="input" type={s.type === 'integer' || s.type === 'number' ? 'number' : 'text'} step={s.type === 'number' ? 'any' : undefined} value={String(v ?? '')} placeholder={s.default != null ? String(s.default) : ''} onChange={(e) => set(s.name, e.target.value)} />
            )}
          </Row>
        );
      })}
      <div className="setting-row">
        <span />
        <div className="setting-control"><Button type="submit" size="sm" disabled={!dirty || saving}>Save</Button></div>
      </div>
    </form>
  );
}
