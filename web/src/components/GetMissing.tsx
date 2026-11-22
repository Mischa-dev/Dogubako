import { useEffect, useRef, useState } from 'react';
import { Download } from 'lucide-react';
import type { Availability } from '../lib/types';
import type { Backend } from '../lib/backend/types';
import { useApp, notifyToolsChanged } from '../lib/app';
import { href } from '../lib/router';
import { formatBytes } from '../lib/format';
import { errorText } from '../lib/errors';
import { Button, ProgressBar } from './ui';
import { toast } from './toast';

type Progress = { id: string; done: number; total: number };

/** Desktop-only: engines the app downloads into its own folder. */
interface EngineBackend {
  engineInstall(id: string): Promise<void>;
  onEngineProgress(cb: (p: Progress) => void): Promise<() => void>;
}
interface ModelProgressBackend {
  onModelProgress(cb: (p: Progress) => void): Promise<() => void>;
}

interface Offer {
  kind: 'engine' | 'model';
  id: string;
  name: string;
  bytes?: number;
}

/** What the app can fetch itself for an unavailable tool: managed engines and the recommended model. */
export function downloadOffers(a: Availability | undefined, backend: Backend): Offer[] {
  if (!a || a.available) return [];
  const engines = backend as Partial<EngineBackend>;
  const out: Offer[] = [];
  if (engines.engineInstall) {
    for (const m of a.missing ?? []) {
      if (m.downloadable) out.push({ kind: 'engine', id: m.downloadable, name: m.name, bytes: m.download_bytes });
    }
  }
  const r = a.recommended_model;
  if (r) out.push({ kind: 'model', id: r.id, name: r.name, bytes: r.size_bytes });
  return out;
}

const sleep = (ms: number) => new Promise((r) => window.setTimeout(r, ms));

/** Download one engine or model, reporting progress; resolves once it is installed. */
async function fetchOffer(backend: Backend, o: Offer, onProgress: (p: Progress) => void, alive: () => boolean) {
  if (o.kind === 'engine') {
    const engines = backend as unknown as EngineBackend;
    const stop = await engines.onEngineProgress?.((p) => { if (p.id === o.id) onProgress(p); });
    try { await engines.engineInstall(o.id); } finally { stop?.(); }
    return;
  }
  if (backend.mode === 'desktop') {
    const stop = await (backend as Partial<ModelProgressBackend>).onModelProgress?.((p) => { if (p.id === o.id) onProgress(p); });
    try { await backend.modelDownload(o.id); } finally { stop?.(); }
    return;
  }
  // The server downloads in the background; follow it until it finishes.
  await backend.modelDownload(o.id);
  while (alive()) {
    await sleep(1000);
    const { downloads } = await backend.models();
    const d = downloads.find((x) => x.model === o.id);
    if (!d) continue;
    onProgress({ id: o.id, done: d.done, total: d.total });
    if (d.status === 'done') return;
    if (d.status === 'failed') throw new Error(d.error ?? 'Download failed');
  }
}

/** One-click downloads for the engines and model a tool is missing, in place on the tool page and error view. */
export function GetMissing({ availability }: { availability: Availability | undefined }) {
  const { backend, isServer, isAdmin, me } = useApp();
  const [busy, setBusy] = useState<Progress | null>(null);
  const [failed, setFailed] = useState<{ id: string; message: string } | null>(null);
  const mounted = useRef(true);
  useEffect(() => () => { mounted.current = false; }, []);
  const offers = downloadOffers(availability, backend);
  if (!offers.length) return null;

  if (isServer && !isAdmin) {
    const admins = me?.admins ?? [];
    const what = offers.map((o) => `${o.name} (${formatBytes(o.bytes)})`).join(' and ');
    return (
      <div className="get-missing muted small">
        {admins.length ? `Ask ${admins.join(' or ')} to download ${what}.` : `Ask an administrator of this instance to download ${what}.`}
      </div>
    );
  }

  const get = async (o: Offer) => {
    setFailed(null);
    setBusy({ id: o.id, done: 0, total: o.bytes ?? 0 });
    try {
      await fetchOffer(backend, o, (p) => { if (mounted.current) setBusy(p); }, () => mounted.current);
      toast.success(`${o.name} is ready`);
      notifyToolsChanged();
    } catch (e) {
      if (mounted.current) setFailed({ id: o.id, message: errorText(e) });
    } finally {
      if (mounted.current) setBusy(null);
    }
  };

  return (
    <div className="get-missing">
      {offers.map((o) => {
        const mine = busy?.id === o.id;
        const prog = mine && busy!.total ? busy!.done / busy!.total : undefined;
        return (
          <div key={`${o.kind}:${o.id}`} className="get-missing-item">
            <div className="get-missing-row">
              <Button size="sm" variant="primary" icon={Download} loading={mine} disabled={!!busy && !mine} onClick={() => void get(o)}>
                Download {o.name} ({formatBytes(o.bytes)})
              </Button>
              {o.kind === 'model' && <a className="link-sm" href={href.settings('models')}>Choose another model</a>}
            </div>
            {mine && (
              <div className="model-progress">
                <ProgressBar value={prog} indeterminate={prog == null} label={`Downloading ${o.name}`} />
                <span className="muted small">{formatBytes(busy!.done)}{prog != null ? ` · ${Math.round(prog * 100)}%` : ''}</span>
              </div>
            )}
            {failed?.id === o.id && <span className="field-error" role="alert">Couldn't download {o.name}: {failed.message}</span>}
          </div>
        );
      })}
    </div>
  );
}
