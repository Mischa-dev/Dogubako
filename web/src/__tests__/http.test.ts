import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { HttpBackend } from '../lib/backend/http';
import { ApiError } from '../lib/backend/types';
import type { JobView } from '../lib/types';

type Handler = (url: string, init: RequestInit) => { status: number; body: unknown };

function mockFetch(h: Handler) {
  const calls: { url: string; init: RequestInit }[] = [];
  globalThis.fetch = vi.fn(async (url: any, init: any = {}) => {
    calls.push({ url: String(url), init });
    const r = h(String(url), init);
    return new Response(JSON.stringify(r.body), { status: r.status, headers: { 'content-type': 'application/json' } });
  }) as any;
  return calls;
}

class FakeEventSource {
  static last: FakeEventSource | null = null;
  url: string;
  listeners: Record<string, ((e: MessageEvent) => void)[]> = {};
  onerror: (() => void) | null = null;
  closed = false;
  constructor(url: string) { this.url = url; FakeEventSource.last = this; }
  addEventListener(name: string, cb: (e: MessageEvent) => void) { (this.listeners[name] ??= []).push(cb); }
  emit(job: Partial<JobView>) { this.listeners.job?.forEach((cb) => cb({ data: JSON.stringify(job) } as MessageEvent)); }
  close() { this.closed = true; }
}

const job = (p: Partial<JobView>): JobView => ({ id: 'j_1', tool_id: 'text.case', status: 'running', created_at: 1, ...p });

describe('HttpBackend', () => {
  beforeEach(() => { (globalThis as any).EventSource = FakeEventSource; FakeEventSource.last = null; });
  afterEach(() => { vi.restoreAllMocks(); });

  it('returns immediately when the job finishes within the request', async () => {
    const calls = mockFetch(() => ({ status: 200, body: job({ status: 'succeeded', result: { data: null, artifacts: [], duration_ms: 3, text: 'HI' } }) }));
    const b = new HttpBackend('/api/v1');
    b.csrf = 'tok';
    const updates: string[] = [];
    const h = b.run('text.case', { text: 'hi' }, { onUpdate: (j) => updates.push(j.status), wait: 10 });
    const final = await h.done;
    expect(final.result?.text).toBe('HI');
    expect(calls[0].url).toBe('/api/v1/tools/text.case/run');
    expect(JSON.parse(calls[0].init.body as string)).toEqual({ params: { text: 'hi' }, wait_seconds: 10, ephemeral: false });
    expect((calls[0].init.headers as Record<string, string>)['X-CSRF-Token']).toBe('tok');
    expect(updates).toEqual(['succeeded']);
    expect(FakeEventSource.last).toBeNull();
  });

  it('follows progress over SSE until a terminal status', async () => {
    mockFetch(() => ({ status: 202, body: job({ status: 'queued' }) }));
    const b = new HttpBackend('/api/v1');
    const seen: (number | undefined)[] = [];
    const h = b.run('video.compress', {}, { onUpdate: (j) => seen.push(j.progress) });
    await vi.waitFor(() => expect(FakeEventSource.last).not.toBeNull());
    const es = FakeEventSource.last!;
    expect(es.url).toBe('/api/v1/jobs/j_1/events');
    es.emit(job({ status: 'running', progress: 0.5 }));
    es.emit(job({ status: 'succeeded', progress: 1, result: { data: {}, artifacts: [], duration_ms: 9 } }));
    const final = await h.done;
    expect(final.status).toBe('succeeded');
    expect(seen).toEqual([undefined, 0.5, 1]);
    expect(es.closed).toBe(true);
  });

  it('falls back to polling when the event stream errors', async () => {
    let polls = 0;
    mockFetch((url) => {
      if (url.endsWith('/run')) return { status: 202, body: job({ status: 'running' }) };
      polls++;
      return { status: 200, body: job({ status: polls < 2 ? 'running' : 'failed', error: { code: 'timeout', seconds: 5 } }) };
    });
    const b = new HttpBackend('/api/v1');
    const h = b.run('x.y', {});
    await vi.waitFor(() => expect(FakeEventSource.last).not.toBeNull());
    FakeEventSource.last!.onerror?.();
    const final = await h.done;
    expect(final.status).toBe('failed');
    expect(final.error?.code).toBe('timeout');
    expect(polls).toBe(2);
  });

  it('cancels a running job', async () => {
    const calls = mockFetch((url) => (url.endsWith('/cancel') ? { status: 200, body: job({ status: 'cancelled' }) } : { status: 202, body: job({ status: 'running' }) }));
    const b = new HttpBackend('/api/v1');
    const h = b.run('x.y', {});
    await vi.waitFor(() => expect(FakeEventSource.last).not.toBeNull());
    await h.cancel();
    expect(calls.some((c) => c.url === '/api/v1/jobs/j_1/cancel' && c.init.method === 'POST')).toBe(true);
    FakeEventSource.last!.emit(job({ status: 'cancelled' }));
    expect((await h.done).status).toBe('cancelled');
  });

  it('turns API errors into ApiError with structured tool error detail', async () => {
    mockFetch(() => ({ status: 400, body: { error: { code: 'invalid_input', message: 'bad', detail: { code: 'invalid_input', param: 'text', message: 'empty' } } } }));
    const b = new HttpBackend('/api/v1');
    const err = await b.run('x.y', {}).done.catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).toToolError()).toEqual({ code: 'invalid_input', param: 'text', message: 'empty' });
  });

  it('notifies listeners on 401 and returns null from getMe', async () => {
    mockFetch(() => ({ status: 401, body: { error: { code: 'unauthorized', message: 'login' } } }));
    const b = new HttpBackend('/api/v1');
    const spy = vi.fn();
    b.onUnauthorized(spy);
    expect(await b.getMe()).toBeNull();
    expect(spy).not.toHaveBeenCalled();
    await expect(b.listTools()).rejects.toBeInstanceOf(ApiError);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('builds inline preview and download URLs', () => {
    const b = new HttpBackend('api/v1');
    expect(b.fileUrl({ ref: 'f_1', name: 'a', size: 1, mime: 'image/png' })).toBe('api/v1/files/f_1/content?inline=true');
    expect(b.downloadUrl({ id: 'f_2', name: 'b', mime: 'x', size: 1 })).toBe('api/v1/files/f_2/content');
    expect(b.downloadUrl({ id: 'f_2', name: 'b', mime: 'x', size: 1, url: '/api/v1/files/f_2/content' })).toBe('/api/v1/files/f_2/content');
  });
});
