import { useCallback, useEffect, useState } from 'react';
import type { Backend, PackInfo, PacksState, UnusedDownload } from '../lib/backend/types';
import { formatBytes } from '../lib/format';
import { errorText } from '../lib/errors';
import { Button, Callout, Section, Spinner, Switch, cx, useConfirm } from './ui';
import { LogoMark } from './Logo';
import { toast } from './toast';

function sizeLine(p: PackInfo) {
  const n = `${p.tools} tool${p.tools === 1 ? '' : 's'}`;
  if (!p.download_bytes) return `${n} · nothing extra to download`;
  return `${n} · up to ${formatBytes(p.download_bytes)} download, ${formatBytes(p.disk_bytes)} on disk`;
}

/** One row per pack with an on/off switch. */
export function PackList({ packs, isOn, onToggle, disabled }: {
  packs: PackInfo[];
  isOn: (p: PackInfo) => boolean;
  onToggle: (p: PackInfo, on: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <ul className="pack-list">
      {packs.map((p) => (
        <li key={p.id} className={cx('pack-row', !isOn(p) && 'is-off')}>
          <div className="pack-text">
            <label className="pack-name" htmlFor={`pack-${p.id}`}>{p.name}</label>
            <span className="muted small">{p.description}</span>
            <span className="pack-meta small">{sizeLine(p)}</span>
          </div>
          <Switch id={`pack-${p.id}`} checked={isOn(p)} disabled={disabled} label={`${p.name} pack`} onChange={(v) => onToggle(p, v)} />
        </li>
      ))}
    </ul>
  );
}

const total = (u: UnusedDownload[]) => u.reduce((a, x) => a + x.bytes, 0);
const listText = (u: UnusedDownload[]) => u.map((x) => `${x.name} (${formatBytes(x.bytes)})`).join(', ');

/** Settings → Tools: turn packs on and off; offers to delete what's left unused. */
export function ToolsSettings({ backend, canManage }: { backend: Backend; canManage: boolean }) {
  const [data, setData] = useState<PacksState | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, confirmNode] = useConfirm();
  const load = useCallback(async () => {
    if (!backend.packs || !canManage) return;
    try { setData(await backend.packs()); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [backend, canManage]);
  useEffect(() => { void load(); }, [load]);

  const cleanup = async (unused: UnusedDownload[], why: string) => {
    if (!unused.length || !backend.packsCleanup) return;
    const ok = await confirm({
      title: `Free ${formatBytes(total(unused))}?`,
      message: `${why} No turned-on pack uses ${unused.length === 1 ? 'this download' : 'these downloads'}: ${listText(unused)}. Delete ${unused.length === 1 ? 'it' : 'them'}? You can download again later.`,
      confirm: `Delete and free ${formatBytes(total(unused))}`,
      danger: true,
    });
    if (!ok) return;
    const r = await backend.packsCleanup(unused.map((u) => u.id));
    setData(r);
    toast.success(`Freed ${formatBytes(r.freed_bytes ?? 0)}`);
  };

  const toggle = async (p: PackInfo, on: boolean) => {
    if (!backend.setPack) return;
    setBusy(true);
    try {
      const r = await backend.setPack(p.id, on);
      setData(r);
      toast.success(`${p.name} pack turned ${on ? 'on' : 'off'}`);
      if (!on) await cleanup(r.unused, `${p.name} is off.`);
    } catch (e) { toast.error(errorText(e)); } finally { setBusy(false); }
  };

  return (
    <Section title="Tools" description="Tools come in packs. Turn off packs you don't need to keep the catalog short; changes apply right away and anything turned off can be turned back on later.">
      {!canManage && <Callout tone="info">Only administrators can change which tool packs this server offers.</Callout>}
      {err && <Callout tone="danger">{err}</Callout>}
      {canManage && !data && !err && <Spinner />}
      {data && (
        <>
          <PackList packs={data.packs} isOn={(p) => p.enabled} onToggle={toggle} disabled={busy} />
          {data.unused.length > 0 && (
            <Callout tone="info" actions={<Button size="sm" variant="secondary" disabled={busy} onClick={() => void cleanup(data.unused, '')}>Free {formatBytes(data.unused_bytes)}</Button>}>
              {listText(data.unused)} {data.unused.length === 1 ? 'is' : 'are'} downloaded but no turned-on pack uses {data.unused.length === 1 ? 'it' : 'them'}.
            </Callout>
          )}
        </>
      )}
      {confirmNode}
    </Section>
  );
}

/** First launch: keep Everything (default) or pick packs. */
export function InstallChoice({ backend, initial, onDone }: { backend: Backend; initial: PacksState; onDone: () => void }) {
  const [mode, setMode] = useState<'everything' | 'custom'>('everything');
  const [on, setOn] = useState<Set<string>>(() => new Set(initial.packs.map((p) => p.id)));
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const save = async (enabled: string[] | null) => {
    setBusy(true);
    try { await backend.choosePacks?.(enabled); onDone(); } catch (e) { setErr(errorText(e)); setBusy(false); }
  };
  const packs = initial.packs;
  const chosen = packs.filter((p) => on.has(p.id));
  const tools = (ps: PackInfo[]) => ps.reduce((a, p) => a + p.tools, 0);
  const bytes = (ps: PackInfo[]) => ps.reduce((a, p) => a + p.download_bytes, 0);
  return (
    <div className="install-choice" role="dialog" aria-labelledby="install-h">
      <LogoMark size={36} />
      <h1 id="install-h">Choose what to install</h1>
      <p className="muted">You can change this any time in Settings → Tools.</p>
      <div className="install-options" role="radiogroup" aria-label="Install">
        <button type="button" role="radio" aria-checked={mode === 'everything'} className={cx('install-option', mode === 'everything' && 'is-active')} onClick={() => setMode('everything')}>
          <strong>Everything <span className="badge badge-accent">Recommended</span></strong>
          <span className="muted small">All {tools(packs)} tools in {packs.length} packs.</span>
        </button>
        <button type="button" role="radio" aria-checked={mode === 'custom'} className={cx('install-option', mode === 'custom' && 'is-active')} onClick={() => setMode('custom')}>
          <strong>Custom</strong>
          <span className="muted small">Pick the packs you want.</span>
        </button>
      </div>
      {mode === 'custom' && (
        <>
          <PackList packs={packs} isOn={(p) => on.has(p.id)} disabled={busy} onToggle={(p, v) => setOn((s) => { const n = new Set(s); if (v) n.add(p.id); else n.delete(p.id); return n; })} />
          <p className="muted small">{chosen.length} of {packs.length} packs · {tools(chosen)} tools · up to {formatBytes(bytes(chosen))} of optional downloads</p>
        </>
      )}
      {err && <Callout tone="danger">{err}</Callout>}
      <div className="install-actions">
        <Button variant="ghost" disabled={busy} onClick={() => void save(null)}>Skip</Button>
        <Button disabled={busy || (mode === 'custom' && chosen.length === 0)} onClick={() => void save(mode === 'everything' || chosen.length === packs.length ? null : chosen.map((p) => p.id))}>Continue</Button>
      </div>
    </div>
  );
}
