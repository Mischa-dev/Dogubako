import { describe, expect, it, vi } from 'vitest';
import { describeTool, outputsOf, riskOf, runAgent, summarizeJob, systemPrompt } from '../lib/assistant/agent';
import { missingSetup, providerConfig, targetFor, ASSISTANT_DEFAULT } from '../lib/assistant/settings';
import { makeDoc } from '../lib/search';
import type { Backend } from '../lib/backend';
import type { JobView } from '../lib/types';
import { tool } from './fixtures';

const compress = tool({
  id: 'image.compress', name: 'Compress image', category: 'image', description: 'Make images smaller',
  params: [
    { name: 'input', label: 'Image', type: 'file', required: true, multiple: true, accept: ['image/*'] },
    { name: 'quality', label: 'Quality', type: 'integer', min: 1, max: 100, default: 80 },
  ],
  output: { kind: 'files' },
});
const convert = tool({
  id: 'image.convert', name: 'Convert image', category: 'image', description: 'Change image format',
  params: [
    { name: 'input', label: 'Image', type: 'file', required: true },
    { name: 'format', label: 'Format', type: 'enum', options: [{ value: 'webp', label: 'WebP' }, { value: 'png', label: 'PNG' }] },
  ],
  output: { kind: 'files' },
});
const fetchUrl = tool({ id: 'network.headers', name: 'HTTP headers', category: 'network', permissions: ['network'] });
const tools = [compress, convert, fetchUrl];
const docs = tools.map((t) => ({ doc: makeDoc(t, t.category), item: t }));

const call = (id: string, name: string, args: unknown) => ({ id, type: 'function', function: { name, arguments: JSON.stringify(args) } });
const reply = (message: unknown) => ({ choices: [{ message }] });

function fakeBackend(answers: unknown[], job?: Partial<JobView>) {
  const chat = vi.fn(async () => answers.shift());
  const run = vi.fn((toolId: string, params: Record<string, unknown>) => ({
    done: Promise.resolve({
      id: 'j1', tool_id: toolId, status: 'succeeded', created_at: 0, params,
      result: { data: null, artifacts: [{ id: 'f_out', name: 'a.webp', mime: 'image/webp', size: 10 }], duration_ms: 5 },
      ...job,
    } as JobView),
    cancel: async () => undefined,
  }));
  return { backend: { mode: 'server', assistantChat: chat, run } as unknown as Backend, chat, run };
}

const base = {
  target: { base_url: 'https://api.openai.com/v1', api_key: 'k' }, model: 'gpt-6-luna', tools, docs,
  files: [{ ref: 'f_in', name: 'a.png', size: 100, mime: 'image/png' }], history: [], confirm: 'risky' as const,
  approve: async () => true, onStep: () => undefined,
};

describe('natural-language agent', () => {
  it('runs a clear request and returns the reply and outputs', async () => {
    const { backend, chat, run } = fakeBackend([
      reply({ content: null, tool_calls: [call('c1', 'tool_details', { tool_id: 'image.convert' })] }),
      reply({ content: null, tool_calls: [call('c2', 'run_tool', { tool_id: 'image.convert', params: { input: 'f_in', format: 'webp' } })] }),
      reply({ content: 'Converted a.png to lossless WebP.' }),
    ]);
    const res = await runAgent({ ...base, backend, request: 'convert this png to lossless webp' });
    expect(res.reply).toBe('Converted a.png to lossless WebP.');
    expect(run).toHaveBeenCalledWith('image.convert', { input: 'f_in', format: 'webp' }, expect.anything());
    expect(outputsOf(res.steps).map((a) => a.id)).toEqual(['f_out']);
    // The model saw the tool details and the run result.
    const sent = (chat.mock.calls as any[])[2][1].messages;
    expect(sent.find((m: any) => m.tool_call_id === 'c1').content).toContain('"format"');
    expect(sent.find((m: any) => m.tool_call_id === 'c2').content).toContain('f_out');
    expect(sent[0].role).toBe('system');
    expect(sent[0].content).toContain('a.png');
  });

  it('hands vague requests to the tool page instead of guessing', async () => {
    const { backend, run } = fakeBackend([
      reply({ content: null, tool_calls: [call('c1', 'open_tool', { tool_id: 'image.convert', note: 'Pick a format' })] }),
    ]);
    const res = await runAgent({ ...base, backend, request: 'convert this file' });
    expect(res.open).toEqual({ toolId: 'image.convert', params: {}, note: 'Pick a format' });
    expect(run).not.toHaveBeenCalled();
  });

  it('asks before risky tools and respects a no', async () => {
    const { backend, run } = fakeBackend([
      reply({ content: null, tool_calls: [call('c1', 'run_tool', { tool_id: 'network.headers', params: {} })] }),
      reply({ content: 'Skipped.' }),
    ]);
    const approve = vi.fn(async () => false);
    const res = await runAgent({ ...base, backend, approve, request: 'check headers of example.com' });
    expect(approve).toHaveBeenCalledOnce();
    expect(run).not.toHaveBeenCalled();
    expect(res.steps[0].status).toBe('skipped');
  });

  it('tells the model about unknown tools and bad arguments without running anything', async () => {
    const { backend, chat, run } = fakeBackend([
      reply({ content: null, tool_calls: [call('c1', 'run_tool', { tool_id: 'nope.tool', params: {} }), { id: 'c2', type: 'function', function: { name: 'run_tool', arguments: '{oops' } }] }),
      reply({ content: 'Sorry.' }),
    ]);
    await runAgent({ ...base, backend, request: 'x' });
    expect(run).not.toHaveBeenCalled();
    const sent = (chat.mock.calls as any[])[1][1].messages;
    expect(sent.find((m: any) => m.tool_call_id === 'c1').content).toContain('no tool');
    expect(sent.find((m: any) => m.tool_call_id === 'c2').content).toContain('not valid JSON');
  });

  it('stops when cancelled', async () => {
    const { backend } = fakeBackend([reply({ content: 'never' })]);
    const ctrl = new AbortController();
    ctrl.abort();
    await expect(runAgent({ ...base, backend, signal: ctrl.signal, request: 'x' })).rejects.toThrow();
  });
});

describe('agent helpers', () => {
  it('describes tools and jobs compactly', () => {
    const d = describeTool(convert) as any;
    expect(d.params[1]).toMatchObject({ name: 'format', type: 'enum', options: ['webp', 'png'] });
    expect(summarizeJob({ id: 'j', tool_id: 't', status: 'failed', created_at: 0, error: { code: 'invalid_input', message: 'bad' } }))
      .toEqual({ status: 'failed', error: { code: 'invalid_input', message: 'bad' } });
    const long = summarizeJob({ id: 'j', tool_id: 't', status: 'succeeded', created_at: 0, result: { text: 'x'.repeat(9000), data: null, artifacts: [], duration_ms: 1 } });
    expect((long.text as string).length).toBeLessThan(4100);
  });

  it('flags risky tools', () => {
    expect(riskOf(compress, 'risky')).toBeNull();
    expect(riskOf(fetchUrl, 'risky')).toMatch(/internet/);
    expect(riskOf(compress, 'always')).not.toBeNull();
  });

  it('lists the catalog and files in the system prompt', () => {
    const p = systemPrompt(tools, [], 'desktop');
    expect(p).toContain('image.compress — Compress image');
    expect(p).toContain('absolute paths');
    expect(p).toContain('open_tool');
  });
});

describe('assistant settings', () => {
  it('uses presets and reports what is missing', () => {
    const s = { ...ASSISTANT_DEFAULT, enabled: true };
    expect(providerConfig(s).model).toBe('gpt-6-luna');
    expect(missingSetup(s)).toMatch(/API key/);
    const withKey = { ...s, configs: { openai: { baseUrl: 'https://api.openai.com/v1', apiKey: 'sk', model: 'gpt-6-luna' } } };
    expect(missingSetup(withKey)).toBeNull();
    expect(targetFor(withKey)).toEqual({ base_url: 'https://api.openai.com/v1', api_key: 'sk' });
    const local = { ...s, provider: 'ollama' as const };
    expect(missingSetup(local)).toMatch(/model/i);
    expect(missingSetup({ ...s, provider: 'server' as const }, 'm')).toBeNull();
    expect(targetFor({ ...s, provider: 'server' as const })).toEqual({ provider: 'server' });
  });
});
