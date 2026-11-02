import type {
  AdminConfig, Artifact, AuditRow, Category, DepInfo, FileView, JobView, Me, ModelDownload, ModelInfo,
  NewToken, Policy, Role, Status, ToolInfo, TokenRow, UserRow,
} from '../types';
import { ApiError, type Backend, type FileRef, type RunHandle, type RunOptions } from './types';

const TERMINAL = new Set(['succeeded', 'failed', 'cancelled']);
export const isTerminal = (s: string) => TERMINAL.has(s);

export type UnauthorizedListener = () => void;

export class HttpBackend implements Backend {
  readonly mode = 'server' as const;
  base: string;
  csrf: string | null = null;
  private unauthorized = new Set<UnauthorizedListener>();

  constructor(base = 'api/v1') {
    this.base = base.replace(/\/$/, '');
  }

  onUnauthorized(cb: UnauthorizedListener) {
    this.unauthorized.add(cb);
    return () => this.unauthorized.delete(cb);
  }

  url(path: string) {
    return `${this.base}${path.startsWith('/') ? path : `/${path}`}`;
  }

  async request<T>(method: string, path: string, body?: unknown, opts: { quiet401?: boolean } = {}): Promise<T> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (method !== 'GET' && this.csrf) headers['X-CSRF-Token'] = this.csrf;
    let res: Response;
    try {
      res = await fetch(this.url(path), {
        method,
        headers,
        credentials: 'same-origin',
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch (e) {
      throw new ApiError(0, 'network', 'Could not reach the server. Check your connection.');
    }
    return this.parse<T>(res, opts.quiet401);
  }

  private async parse<T>(res: Response, quiet401?: boolean): Promise<T> {
    const text = await res.text();
    let json: any = null;
    if (text) {
      try { json = JSON.parse(text); } catch { json = null; }
    }
    if (!res.ok && res.status !== 202) {
      const err = json?.error ?? {};
      if (res.status === 401 && !quiet401) this.unauthorized.forEach((cb) => cb());
      throw new ApiError(res.status, err.code ?? `http_${res.status}`, err.message ?? (res.statusText || 'Request failed'), err.detail);
    }
    return json as T;
  }

  get<T>(p: string) { return this.request<T>('GET', p); }
  post<T>(p: string, b: unknown = {}) { return this.request<T>('POST', p, b); }
  patch<T>(p: string, b: unknown) { return this.request<T>('PATCH', p, b); }
  del<T>(p: string) { return this.request<T>('DELETE', p); }

  // ---------------------------------------------------------------- core
  getStatus() { return this.get<Status>('/status'); }

  async getMe(): Promise<Me | null> {
    try {
      const me = await this.request<Me>('GET', '/auth/me', undefined, { quiet401: true });
      if (me.csrf_token) this.csrf = me.csrf_token;
      return me;
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) return null;
      throw e;
    }
  }

  async listTools(): Promise<ToolInfo[]> {
    const r = await this.get<{ tools: ToolInfo[] }>('/tools');
    return r.tools;
  }

  async listCategories(): Promise<Category[]> {
    const r = await this.get<{ categories: Category[] }>('/categories');
    return r.categories;
  }

  run(toolId: string, params: Record<string, unknown>, opts: RunOptions = {}): RunHandle {
    let jobId: string | null = null;
    let settle: ((j: JobView) => void) | null = null;
    let cancelled = false;
    const done = (async () => {
      const first = await this.post<JobView>(`/tools/${encodeURIComponent(toolId)}/run`, { params, wait_seconds: opts.wait ?? 0, ephemeral: opts.ephemeral ?? false });
      jobId = first.id;
      opts.onUpdate?.(first);
      if (cancelled && !isTerminal(first.status)) {
        try { await this.post(`/jobs/${first.id}/cancel`); } catch { /* ignore */ }
      }
      if (isTerminal(first.status)) return first;
      return this.follow(first, opts, (finish) => { settle = finish; });
    })();
    return {
      done,
      cancel: async () => {
        if (cancelled) return;
        cancelled = true;
        if (!jobId) return; // the run request is still in flight; it cancels once it has an id
        // Keep following the job: the event stream (or polling) delivers the
        // terminal status. The cancel response usually already carries it.
        try {
          const j = await this.post<JobView>(`/jobs/${jobId}/cancel`);
          if (j && isTerminal(j.status)) {
            opts.onUpdate?.(j);
            (settle as ((j: JobView) => void) | null)?.(j);
          }
        } catch { /* ignore: the stream reports the outcome */ }
      },
    };
  }

  /** Follow a job via SSE, falling back to long-polling. */
  follow(initial: JobView, opts: RunOptions, onFinisher?: (finish: (j: JobView) => void) => void): Promise<JobView> {
    return new Promise<JobView>((resolve, reject) => {
      let last = initial;
      let settled = false;
      let es: EventSource | null = null;
      const finish = (j: JobView) => {
        if (settled) return;
        settled = true;
        es?.close();
        resolve(j);
      };
      onFinisher?.(finish);
      const poll = async () => {
        try {
          while (!settled) {
            const j = await this.get<JobView>(`/jobs/${initial.id}?wait_seconds=15`);
            last = j;
            opts.onUpdate?.(j);
            if (isTerminal(j.status)) return finish(j);
          }
        } catch (e) {
          if (!settled) { settled = true; reject(e); }
        }
      };
      if (typeof EventSource === 'undefined') { void poll(); return; }
      const source = new EventSource(this.url(`/jobs/${initial.id}/events`), { withCredentials: true });
      es = source;
      source.addEventListener('job', (ev) => {
        try {
          const j = JSON.parse((ev as MessageEvent).data) as JobView;
          last = j;
          opts.onUpdate?.(j);
          if (isTerminal(j.status)) finish(j);
        } catch { /* ignore malformed */ }
      });
      source.onerror = () => {
        source.close();
        if (settled) return;
        if (isTerminal(last.status)) return finish(last);
        void poll();
      };
    });
  }

  // ---------------------------------------------------------------- files
  uploadFiles(files: File[], onProgress?: (f: number) => void, signal?: AbortSignal): Promise<FileRef[]> {
    return new Promise((resolve, reject) => {
      const form = new FormData();
      for (const f of files) form.append('file', f, f.name || 'pasted');
      const xhr = new XMLHttpRequest();
      xhr.open('POST', this.url('/files'));
      xhr.withCredentials = true;
      if (this.csrf) xhr.setRequestHeader('X-CSRF-Token', this.csrf);
      xhr.setRequestHeader('Accept', 'application/json');
      xhr.upload.onprogress = (e) => { if (e.lengthComputable) onProgress?.(e.loaded / e.total); };
      xhr.onload = () => {
        let json: any = null;
        try { json = JSON.parse(xhr.responseText); } catch { /* */ }
        if (xhr.status >= 200 && xhr.status < 300 && json?.files) {
          resolve((json.files as FileView[]).map(fileViewToRef));
        } else {
          if (xhr.status === 401) this.unauthorized.forEach((cb) => cb());
          const err = json?.error ?? {};
          reject(new ApiError(xhr.status, err.code ?? 'upload_failed', err.message ?? 'Upload failed'));
        }
      };
      xhr.onerror = () => reject(new ApiError(0, 'network', 'Upload failed: network error'));
      xhr.onabort = () => reject(new ApiError(0, 'aborted', 'Upload cancelled'));
      signal?.addEventListener('abort', () => xhr.abort());
      xhr.send(form);
    });
  }

  async describeFile(ref: string): Promise<FileRef> {
    const f = await this.get<FileView>(`/files/${encodeURIComponent(ref)}`);
    return fileViewToRef(f);
  }

  fileUrl(ref: FileRef): string {
    return this.url(`/files/${encodeURIComponent(ref.ref)}/content?inline=true`);
  }

  artifactUrl(a: Artifact): string {
    if (a.id) return this.url(`/files/${encodeURIComponent(a.id)}/content?inline=true`);
    return a.url ?? '';
  }

  downloadUrl(a: Artifact): string {
    if (a.url) return a.url;
    if (a.id) return this.url(`/files/${encodeURIComponent(a.id)}/content`);
    return '';
  }

  async downloadArtifact(a: Artifact): Promise<void> {
    triggerDownload(this.downloadUrl(a), a.name);
  }

  zipUrl(jobId: string) { return this.url(`/jobs/${encodeURIComponent(jobId)}/artifacts.zip`); }

  async listFiles() { return this.get<{ files: FileView[]; used: number; quota: number }>('/files'); }
  async deleteFile(id: string) { await this.del(`/files/${encodeURIComponent(id)}`); }

  // ---------------------------------------------------------------- jobs
  async listJobs(limit = 100): Promise<JobView[]> {
    const r = await this.get<{ jobs: JobView[] }>(`/jobs?limit=${limit}`);
    return r.jobs;
  }
  async deleteJob(id: string) { await this.del(`/jobs/${encodeURIComponent(id)}`); }

  // ---------------------------------------------------------------- deps/models
  deps() { return this.get<{ dependencies: DepInfo[]; platform?: string }>('/deps'); }
  models() { return this.get<{ models: ModelInfo[]; downloads: ModelDownload[] }>('/models'); }
  async modelDownload(id: string) { await this.post(`/models/${encodeURIComponent(id)}/download`); }
  async modelRemove(id: string) { await this.del(`/models/${encodeURIComponent(id)}`); }
  async depsRefresh() { await this.post('/deps/refresh'); }

  // ---------------------------------------------------------------- auth
  async login(username: string, password: string) {
    const r = await this.request<{ csrf_token: string }>('POST', '/auth/login', { username, password }, { quiet401: true });
    this.csrf = r.csrf_token;
    return r;
  }
  async setup(setup_token: string, username: string, password: string) {
    const r = await this.post<{ csrf_token: string }>('/setup', { setup_token, username, password });
    this.csrf = r.csrf_token;
    return r;
  }
  async logout() { try { await this.post('/auth/logout'); } finally { this.csrf = null; } }
  changePassword(current_password: string, new_password: string) {
    return this.post<{ ok: boolean; relogin?: boolean }>('/auth/password', { current_password, new_password });
  }

  // ---------------------------------------------------------------- tokens
  async myTokens() { return (await this.get<{ tokens: TokenRow[] }>('/tokens')).tokens; }
  createToken(body: { name: string; expires_days?: number; scope?: Policy }) { return this.post<NewToken>('/tokens', body); }
  async revokeToken(id: string) { await this.del(`/tokens/${encodeURIComponent(id)}`); }

  // ---------------------------------------------------------------- admin
  async adminUsers() { return (await this.get<{ users: UserRow[] }>('/admin/users')).users; }
  adminCreateUser(body: { username: string; password?: string; role: Role; policy?: Policy; description?: string }) {
    return this.post<UserRow>('/admin/users', body);
  }
  adminUpdateUser(id: string, body: Partial<{ role: Role; policy: Policy; disabled: boolean; description: string }>) {
    return this.patch<UserRow>(`/admin/users/${encodeURIComponent(id)}`, body);
  }
  async adminDeleteUser(id: string) { await this.del(`/admin/users/${encodeURIComponent(id)}`); }
  async adminSetPassword(id: string, password: string) { await this.post(`/admin/users/${encodeURIComponent(id)}/password`, { password }); }
  async adminTokens() { return (await this.get<{ tokens: TokenRow[] }>('/admin/tokens')).tokens; }
  adminCreateToken(body: { user_id: string; name: string; expires_days?: number; scope?: Policy }) {
    return this.post<NewToken>('/admin/tokens', body);
  }
  async adminAudit(limit = 100, before?: number) {
    const q = `?limit=${limit}${before ? `&before=${before}` : ''}`;
    return (await this.get<{ entries: AuditRow[] }>(`/admin/audit${q}`)).entries;
  }
  adminConfig() { return this.get<AdminConfig>('/admin/config'); }
  adminJobs(limit = 50) { return this.get<{ jobs: JobView[] }>(`/jobs?all=true&limit=${limit}`); }
  extensions() { return this.get<{ extensions: any[]; dir?: string }>('/extensions'); }
  async openapi() {
    const res = await fetch(this.url('/openapi.json'), { credentials: 'same-origin' });
    if (!res.ok) throw new ApiError(res.status, 'http', 'Could not load the OpenAPI document');
    return res.json();
  }
}

export function fileViewToRef(f: FileView): FileRef {
  return { ref: f.id, name: f.name, size: f.size, mime: f.mime };
}

export function triggerDownload(url: string, name: string) {
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
}
