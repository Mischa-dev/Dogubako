import type { Artifact, Category, DepInfo, JobView, Me, ModelDownload, ModelInfo, Status, ToolErrorView, ToolInfo } from '../types';
import { readJson, writeJson } from '../storage';
import { ApiError, type AssistantInfo, type AssistantTarget, type Backend, type FileRef, type PickOptions, type RunHandle, type RunOptions } from './types';

type Invoke = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

let corePromise: Promise<typeof import('@tauri-apps/api/core')> | null = null;
const core = () => (corePromise ??= import('@tauri-apps/api/core'));
const invoke: Invoke = async (cmd, args) => (await core()).invoke(cmd, args);

const HISTORY_KEY = 'dgb.desktop.history';
const HISTORY_MAX = 200;

function guessMime(name: string): string {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, string> = {
    png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', avif: 'image/avif',
    svg: 'image/svg+xml', bmp: 'image/bmp', tif: 'image/tiff', tiff: 'image/tiff', ico: 'image/x-icon',
    mp4: 'video/mp4', webm: 'video/webm', mkv: 'video/x-matroska', mov: 'video/quicktime',
    mp3: 'audio/mpeg', wav: 'audio/wav', ogg: 'audio/ogg', flac: 'audio/flac', m4a: 'audio/mp4', opus: 'audio/opus',
    pdf: 'application/pdf', json: 'application/json', txt: 'text/plain', csv: 'text/csv', md: 'text/markdown',
    html: 'text/html', zip: 'application/zip', vtt: 'text/vtt', srt: 'application/x-subrip',
  };
  return map[ext] ?? 'application/octet-stream';
}

function errorOf(e: unknown): ToolErrorView {
  if (e && typeof e === 'object' && 'code' in (e as any)) return e as ToolErrorView;
  if (typeof e === 'string') {
    try { const j = JSON.parse(e); if (j && j.code) return j; } catch { /* */ }
    return { code: 'failed', message: e };
  }
  return { code: 'failed', message: e instanceof Error ? e.message : String(e) };
}

/** Relay errors arrive as `{kind, status, message}`. */
function relayError(e: unknown): ApiError {
  const o = (e && typeof e === 'object' ? e : { message: String(e) }) as { kind?: string; status?: number; message?: string };
  return new ApiError(o.status ?? 0, `assistant_${o.kind ?? 'failed'}`, o.message ?? 'The model request failed');
}

export class TauriBackend implements Backend {
  readonly mode = 'desktop' as const;
  private info: { version: string; platform: string; tools: number; output_dir: string; data_dir: string } | null = null;
  private convert: ((p: string) => string) | null = null;

  async init() {
    const c = await core();
    this.convert = (p) => c.convertFileSrc(p);
  }

  async getStatus(): Promise<Status> {
    this.info ??= await invoke('app_info');
    return {
      instance_name: 'This device',
      version: this.info!.version,
      auth: 'none',
      setup_required: false,
      mcp_enabled: false,
      tools: this.info!.tools,
      mode: 'desktop',
      platform: this.info!.platform,
      output_dir: this.info!.output_dir,
      data_dir: this.info!.data_dir,
    };
  }

  async getMe(): Promise<Me> {
    return {
      user: { id: 'local', username: 'local', role: 'owner', is_admin: true, via: 'desktop' },
      permissions: ['host_fs_read', 'host_fs_write', 'network', 'system_info', 'process_info'],
    };
  }

  private tools: Promise<ToolInfo[]> | null = null;

  listTools() {
    const p = invoke<ToolInfo[]>('list_tools');
    this.tools = p.catch(() => []);
    return p;
  }
  async listCategories(): Promise<Category[] | null> { return null; }

  run(toolId: string, params: Record<string, unknown>, opts: RunOptions = {}): RunHandle {
    const runId = `r_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
    const created = Math.floor(Date.now() / 1000);
    const base: JobView = { id: runId, tool_id: toolId, status: 'running', created_at: created, params };
    // A cancel that arrives while the listener is still being set up must
    // stop the run from starting; once run_tool is invoked the backend
    // honours cancel_run even if it lands before the run registers.
    let cancelled = false;
    let started = false;
    let finished = false;
    const cancelledJob = (): JobView => ({
      ...base,
      status: 'cancelled',
      error: { code: 'cancelled', message: 'cancelled' },
      finished_at: Math.floor(Date.now() / 1000),
    });
    const done = (async () => {
      opts.onUpdate?.(base);
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<{ run_id: string; progress?: number; message?: string }>('dogubako://progress', (e) => {
        if (e.payload.run_id !== runId) return;
        opts.onUpdate?.({ ...base, progress: e.payload.progress, message: e.payload.message });
      });
      let job: JobView;
      if (cancelled) {
        unlisten();
        job = cancelledJob();
      } else {
        started = true;
        try {
          job = await invoke<JobView>('run_tool', { runId, toolId, params });
        } catch (e) {
          const err = errorOf(e);
          job = { ...base, status: err.code === 'cancelled' ? 'cancelled' : 'failed', error: err, finished_at: Math.floor(Date.now() / 1000) };
        } finally {
          finished = true;
          unlisten();
        }
      }
      job = { ...job, params: job.params ?? params, created_at: job.created_at ?? created };
      if (!opts?.ephemeral) await this.recordHistory(job);
      return job;
    })();
    return {
      done,
      cancel: async () => {
        cancelled = true;
        if (started && !finished) await invoke('cancel_run', { runId }).catch(() => undefined);
      },
    };
  }

  private async recordHistory(job: JobView) {
    if (readJson<boolean>('dgb.prefs.history', true) === false) return;
    this.tools ??= invoke<ToolInfo[]>('list_tools').catch(() => []);
    const tool = (await this.tools).find((t) => t.id === job.tool_id);
    const list = readJson<JobView[]>(HISTORY_KEY, []);
    const slim: JobView = { ...job, params: redactSecrets(tool, job.params) };
    if (slim.result?.text && slim.result.text.length > 100_000) {
      slim.result = { ...slim.result, text: slim.result.text.slice(0, 100_000) + '\n…(truncated)' };
    }
    writeJson(HISTORY_KEY, [slim, ...list.filter((j) => j.id !== job.id)].slice(0, HISTORY_MAX));
  }

  async uploadFiles(files: File[], onProgress?: (f: number) => void): Promise<FileRef[]> {
    const out: FileRef[] = [];
    let i = 0;
    for (const f of files) {
      const bytes = Array.from(new Uint8Array(await f.arrayBuffer()));
      const r = await invoke<{ path: string; name: string; size: number }>('stage_file', { name: f.name || 'pasted', bytes });
      out.push({ ref: r.path, name: r.name, size: r.size, mime: f.type || guessMime(r.name) });
      onProgress?.(++i / files.length);
    }
    return out;
  }

  async pickFiles(opts: PickOptions): Promise<FileRef[] | null> {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const exts = (opts.accept ?? [])
      .flatMap((a) => (a.startsWith('.') ? [a.slice(1)] : []));
    const res = await open({
      multiple: !!opts.multiple,
      directory: !!opts.directory,
      filters: exts.length ? [{ name: 'Supported files', extensions: exts }, { name: 'All files', extensions: ['*'] }] : undefined,
    });
    if (!res) return null;
    const paths = Array.isArray(res) ? res : [res];
    return Promise.all(paths.map((p) => this.describeFile(String(p))));
  }

  async pickPath(kind: 'file' | 'dir' | 'any'): Promise<string | null> {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const res = await open({ multiple: false, directory: kind === 'dir' });
    return res ? String(res) : null;
  }

  async onNativeDrop(cb: (refs: FileRef[]) => void) {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview');
    return getCurrentWebview().onDragDropEvent(async (e) => {
      if (e.payload.type === 'drop') {
        cb(await Promise.all(e.payload.paths.map((p) => this.describeFile(p))));
      }
    });
  }

  async describeFile(path: string): Promise<FileRef> {
    try {
      const r = await invoke<{ path: string; name: string; size: number; mime: string }>('file_info', { path });
      return { ref: r.path, name: r.name, size: r.size, mime: r.mime || guessMime(r.name) };
    } catch {
      const name = path.split(/[\\/]/).pop() || path;
      return { ref: path, name, size: 0, mime: guessMime(name) };
    }
  }

  fileUrl(ref: FileRef) { return this.convert ? this.convert(ref.ref) : ref.ref; }
  artifactUrl(a: Artifact) { return a.path && this.convert ? this.convert(a.path) : a.url ?? ''; }

  async downloadArtifact(a: Artifact) {
    if (!a.path) return;
    const { save } = await import('@tauri-apps/plugin-dialog');
    const dest = await save({ defaultPath: a.name });
    if (dest) await invoke('save_artifact', { src: a.path, dest });
  }

  // ---------------------------------------------------------------- assistant
  async assistantInfo(): Promise<AssistantInfo> {
    return { enabled: true, server_provider: null, private_network: true };
  }
  async assistantChat(target: AssistantTarget, body: Record<string, unknown>, signal?: AbortSignal) {
    // The relay call itself cannot be aborted; drop its answer when cancelled.
    const call = invoke<any>('assistant_chat', { baseUrl: target.base_url ?? '', apiKey: target.api_key || null, body }).catch((e) => { throw relayError(e); });
    if (!signal) return call;
    return new Promise((resolve, reject) => {
      const abort = () => reject(new DOMException('Cancelled', 'AbortError'));
      if (signal.aborted) return abort();
      signal.addEventListener('abort', abort, { once: true });
      call.then(resolve, reject).finally(() => signal.removeEventListener('abort', abort));
    });
  }
  async assistantModels(target: AssistantTarget) {
    try {
      return (await invoke<{ models: { id: string; owned_by?: string }[] }>('assistant_models', { baseUrl: target.base_url ?? '', apiKey: target.api_key || null })).models;
    } catch (e) { throw relayError(e); }
  }

  async revealArtifact(a: Artifact) {
    if (a.path) await invoke('reveal_path', { path: a.path });
  }

  async listJobs(limit = 200) { return readJson<JobView[]>(HISTORY_KEY, []).slice(0, limit); }
  async deleteJob(id: string) {
    writeJson(HISTORY_KEY, readJson<JobView[]>(HISTORY_KEY, []).filter((j) => j.id !== id));
  }
  clearHistory() { writeJson(HISTORY_KEY, []); }

  async discardRun(id: string) {
    await invoke('discard_run_outputs', { runId: id });
    await this.deleteJob(id);
  }

  async deps() { return { dependencies: await invoke<DepInfo[]>('deps_status'), platform: this.info?.platform }; }
  async models() {
    const r = await invoke<{ models: ModelInfo[]; downloads?: ModelDownload[] } | ModelInfo[]>('models_list');
    return Array.isArray(r) ? { models: r, downloads: [] } : { models: r.models, downloads: r.downloads ?? [] };
  }
  async modelDownload(id: string) { await invoke('model_download', { id }); }
  async modelRemove(id: string) { await invoke('model_remove', { id }); }
  /** Download an app-managed engine (pinned, checksum-verified) into the app's folder. */
  async engineInstall(id: string) { await invoke('engine_install', { id }); }
  async onEngineProgress(cb: (p: { id: string; done: number; total: number }) => void) {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<{ id: string; done: number; total: number }>('dogubako://engine-progress', (e) => cb(e.payload));
  }
  async onModelProgress(cb: (p: { id: string; done: number; total: number }) => void) {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<{ id: string; done: number; total: number }>('dogubako://model-progress', (e) => cb(e.payload));
  }
}

/** Mask secret parameters (passwords, keys) before persisting history, as the
 *  server does. Without the tool's spec nothing is known to be safe, so all
 *  parameters are dropped. */
export function redactSecrets(tool: Pick<ToolInfo, 'params'> | undefined, params: Record<string, unknown> | undefined): Record<string, unknown> {
  if (!params) return {};
  if (!tool) return {};
  const out = { ...params };
  for (const p of tool.params) if (p.type === 'secret' && p.name in out) out[p.name] = '••••••';
  return out;
}
