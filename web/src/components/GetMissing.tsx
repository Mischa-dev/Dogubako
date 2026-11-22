import { useEffect, useState, useSyncExternalStore } from 'react';
import { Download } from 'lucide-react';
import type { Availability, ToolErrorView, ToolInfo } from '../lib/types';
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

export interface Offer {
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

/** Downloads in flight, kept outside components so one survives navigation and is never started twice. */
interface Job { done: number; total: number; running: boolean; error?: string }
const jobs = new Map<string, Job>();
const running = new Map<string, Promise<void>>();
const listeners = new Set<() => void>();
let version = 0;
const keyOf = (o: Offer) => `${o.kind}:${o.id}`;
function setJob(key: string, job: Job | null) {
  if (job) jobs.set(key, job); else jobs.delete(key);
  version++;
  listeners.forEach((l) => l());
}
function useJobs() {
  useSyncExternalStore((l) => { listeners.add(l); return () => { listeners.delete(l); }; }, () => version);
  return jobs;
}

/** Download one engine or model, reporting progress; resolves once it is installed. */
async function fetchOffer(backend: Backend, o: Offer, onProgress: (p: Progress) => void) {
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
  // The server downloads in the background (or already is, started elsewhere); follow it until it finishes.
  try { await backend.modelDownload(o.id); } catch (e) { if (!/already downloading/i.test(errorText(e))) throw e; }
  for (;;) {
    await sleep(1000);
    const { downloads } = await backend.models();
    const d = downloads.find((x) => x.model === o.id);
    if (!d) continue;
    onProgress({ id: o.id, done: d.done, total: d.total });
    if (d.status === 'done') return;
    if (d.status === 'failed') throw new Error(d.error ?? 'Download failed');
  }
}

/** Start (or join) the download of `o`. */
export function startDownload(backend: Backend, o: Offer): Promise<void> {
  const key = keyOf(o);
  const cur = running.get(key);
  if (cur) return cur;
  setJob(key, { done: 0, total: o.bytes ?? 0, running: true });
  const p = fetchOffer(backend, o, (pr) => setJob(key, { done: pr.done, total: pr.total, running: true }))
    .then(() => { setJob(key, null); toast.success(`${o.name} is ready`); notifyToolsChanged(); })
    .catch((e) => { setJob(key, { done: 0, total: 0, running: false, error: errorText(e) }); })
    .finally(() => { running.delete(key); });
  running.set(key, p);
  return p;
}

/** What a failed run needs: the model or engine it named (an option may need its own), else the tool's own status. */
export function useErrorOffer(error: ToolErrorView, tool: ToolInfo | undefined): Availability | undefined {
  const { backend } = useApp();
  const [named, setNamed] = useState<Availability | null>(null);
  const model = error.code === 'missing_model' ? error.model : undefined;
  const dep = error.code === 'missing_dependency' ? error.dependency : undefined;
  // Look again once something is installed, so a finished download stops being offered.
  const [installs, setInstalls] = useState(0);
  useEffect(() => {
    const bump = () => setInstalls((n) => n + 1);
    window.addEventListener('dgb:tools-changed', bump);
    return () => window.removeEventListener('dgb:tools-changed', bump);
  }, []);
  useEffect(() => {
    let live = true;
    setNamed(null);
    if (model) {
      void backend.models().then(({ models }) => {
        const m = models.find((x) => x.id === model && !x.installed);
        if (live && m) setNamed({ available: false, platform_supported: true, recommended_model: { id: m.id, name: m.name, size_bytes: m.size_bytes } });
      }).catch(() => {});
    } else if (dep && (backend as Partial<EngineBackend>).engineInstall) {
      void backend.deps().then(({ dependencies }) => {
        const d = dependencies.find((x) => x.id === dep && x.downloadable && !x.found);
        if (live && d) setNamed({ available: false, platform_supported: true, missing: [{ id: d.id, name: d.name, hint: d.install_hint ?? '', downloadable: d.downloadable, download_bytes: d.download_bytes }] });
      }).catch(() => {});
    }
    return () => { live = false; };
  }, [backend, model, dep, installs]);
  return named ?? tool?.availability;
}

/** One-click downloads for the engines and model a tool is missing, in place on the tool page and error view. */
export function GetMissing({ availability }: { availability: Availability | undefined }) {
  const { backend, isServer, isAdmin, me } = useApp();
  const jobs = useJobs();
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

  return (
    <div className="get-missing">
      {offers.map((o) => {
        const job = jobs.get(keyOf(o));
        const mine = !!job?.running;
        const prog = mine && job.total ? job.done / job.total : undefined;
        return (
          <div key={`${o.kind}:${o.id}`} className="get-missing-item">
            <div className="get-missing-row">
              <Button size="sm" variant="primary" icon={Download} loading={mine} onClick={() => void startDownload(backend, o)}>
                Download {o.name} ({formatBytes(o.bytes)})
              </Button>
              {o.kind === 'model' && <a className="link-sm" href={href.settings('models')}>Choose another model</a>}
            </div>
            {mine && (
              <div className="model-progress">
                <ProgressBar value={prog} indeterminate={prog == null} label={`Downloading ${o.name}`} />
                <span className="muted small">{formatBytes(job.done)}{prog != null ? ` · ${Math.round(prog * 100)}%` : ''}</span>
              </div>
            )}
            {job?.error && <span className="field-error" role="alert">Couldn't download {o.name}: {job.error}</span>}
          </div>
        );
      })}
    </div>
  );
}
