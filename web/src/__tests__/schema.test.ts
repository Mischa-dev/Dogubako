import { describe, expect, it } from 'vitest';
import { defaultValues, fromRunParams, isSliderRange, isVisible, toRunParams, validate, type FileItem } from '../lib/schema';
import type { ParamSpec } from '../lib/types';
import { tool } from './fixtures';
import { redactSecrets } from '../lib/backend/tauri';

const params: ParamSpec[] = [
  { name: 'input', label: 'Images', type: 'file', multiple: true, accept: ['image/*'], required: true },
  { name: 'format', label: 'Format', type: 'enum', options: [{ value: 'png', label: 'PNG' }, { value: 'jpg', label: 'JPEG' }], default: 'png' },
  { name: 'quality', label: 'Quality', type: 'integer', min: 1, max: 100, default: 85, show_if: { param: 'format', values: ['jpg'] } },
  { name: 'progressive', label: 'Progressive', type: 'boolean', default: false, show_if: { param: 'quality', values: [85, 90] } },
  { name: 'regions', label: 'Regions', type: 'json' },
  { name: 'note', label: 'Note', type: 'text' },
  { name: 'tags', label: 'Tags', type: 'enum', multi: true, options: [{ value: 'a', label: 'A' }, { value: 'b', label: 'B' }] },
  { name: 'bg', label: 'Background', type: 'color', default: '#ffffff' },
];
const t = tool({ id: 'image.x', name: 'X', params });
const ready = (ref: string): FileItem => ({ key: ref, name: `${ref}.png`, size: 1, mime: 'image/png', ref, status: 'ready' });

describe('schema defaults', () => {
  it('prefills defaults per type', () => {
    expect(defaultValues(t)).toEqual({ input: [], format: 'png', quality: 85, progressive: false, regions: '', note: '', tags: [], bg: '#ffffff' });
  });
});

describe('show_if', () => {
  it('hides params unless the controlling value matches', () => {
    const v = defaultValues(t);
    expect(isVisible(params[2], v)).toBe(false);
    expect(isVisible(params[2], { ...v, format: 'jpg' })).toBe(true);
  });
  it('is transitive: a param depending on a hidden param is hidden', () => {
    const v = { ...defaultValues(t), format: 'png', quality: 85 };
    expect(isVisible(params[3], v)).toBe(true);
    expect(isVisible(params[3], v, params)).toBe(false);
    expect(isVisible(params[3], { ...v, format: 'jpg' }, params)).toBe(true);
  });
  it('compares loosely (numbers vs strings) and supports multi values', () => {
    expect(isVisible({ ...params[5], show_if: { param: 'n', values: ['3'] } } as ParamSpec, { n: 3 })).toBe(true);
    expect(isVisible({ ...params[5], show_if: { param: 'tags', values: ['b'] } } as ParamSpec, { tags: ['a', 'b'] })).toBe(true);
  });
});

describe('validate', () => {
  it('requires files and reports upload state', () => {
    expect(validate(t, defaultValues(t)).input).toMatch(/at least one/);
    expect(validate(t, { ...defaultValues(t), input: [{ ...ready('f_1'), status: 'uploading' }] }).input).toMatch(/upload/);
    expect(validate(t, { ...defaultValues(t), input: [ready('f_1')] })).toEqual({});
  });
  it('checks numeric bounds only for visible params', () => {
    const base = { ...defaultValues(t), input: [ready('f_1')], quality: 500 };
    expect(validate(t, base)).toEqual({});
    expect(validate(t, { ...base, format: 'jpg' }).quality).toBe('Must be at most 100');
    expect(validate(t, { ...base, format: 'jpg', quality: 2.5 }).quality).toBe('Enter a whole number');
  });
  it('validates JSON and colors', () => {
    const base = { ...defaultValues(t), input: [ready('f_1')] };
    expect(validate(t, { ...base, regions: '[{' }).regions).toMatch(/Invalid JSON/);
    expect(validate(t, { ...base, regions: '[]' }).regions).toBeUndefined();
    expect(validate(t, { ...base, bg: 'red' }).bg).toMatch(/hex/);
    expect(validate(t, { ...base, bg: '#abc' }).bg).toBeUndefined();
  });
});

describe('toRunParams', () => {
  it('maps files to refs, drops hidden and empty values, parses JSON', () => {
    const v = { ...defaultValues(t), input: [ready('f_1'), ready('f_2')], regions: '[{"x":1}]', tags: ['a'] };
    expect(toRunParams(t, v)).toEqual({ input: ['f_1', 'f_2'], format: 'png', progressive: undefined, regions: [{ x: 1 }], tags: ['a'], bg: '#ffffff' }.progressive === undefined
      ? { input: ['f_1', 'f_2'], format: 'png', regions: [{ x: 1 }], tags: ['a'], bg: '#ffffff' } : {});
    const jpg = toRunParams(t, { ...v, format: 'jpg', quality: '70' });
    expect(jpg.quality).toBe(70);
  });
  it('single file params yield a string', () => {
    const single = tool({ id: 'x', name: 'x', params: [{ name: 'f', label: 'F', type: 'file' }] });
    expect(toRunParams(single, { f: [ready('f_9')] })).toEqual({ f: 'f_9' });
  });
  it('round-trips through fromRunParams', () => {
    const { values, fileRefs } = fromRunParams(t, { input: ['f_1'], format: 'jpg', quality: 60, regions: [{ x: 1 }] });
    expect(fileRefs).toEqual({ input: ['f_1'] });
    expect(values.quality).toBe(60);
    expect(JSON.parse(values.regions as string)).toEqual([{ x: 1 }]);
  });

  it('keeps an explicitly empty multi-choice selection', () => {
    const mt = tool({ id: 'pdf.protect', name: 'P', params: [{ name: 'allow', label: 'Allow', type: 'enum', multi: true, options: [{ value: 'print', label: 'Print' }], default: ['print'] }] });
    expect(toRunParams(mt, { allow: [] })).toEqual({ allow: [] });
    expect(toRunParams(mt, { allow: ['print'] })).toEqual({ allow: ['print'] });
  });

  it('never restores a (masked) secret from history', () => {
    const st = tool({ id: 'files.encrypt', name: 'E', params: [{ name: 'password', label: 'Password', type: 'secret' }, { name: 'level', label: 'Level', type: 'integer' }] });
    const { values } = fromRunParams(st, { password: '••••••', level: 5 });
    expect(values.password).toBe('');
    expect(values.level).toBe(5);
  });
});

describe('redactSecrets (desktop history)', () => {
  const st = tool({ id: 'dev.hmac', name: 'H', params: [{ name: 'key', label: 'Key', type: 'secret' }, { name: 'text', label: 'Text', type: 'text' }] });
  it('masks secret params and keeps the rest', () => {
    expect(redactSecrets(st, { key: 'hunter2', text: 'hi' })).toEqual({ key: '••••••', text: 'hi' });
  });
  it('drops all params when the tool spec is unknown', () => {
    expect(redactSecrets(undefined, { key: 'hunter2' })).toEqual({});
  });
});

describe('isSliderRange', () => {
  it('uses sliders for bounded ranges only', () => {
    expect(isSliderRange(params[2])).toBe(true);
    expect(isSliderRange({ name: 'w', label: 'W', type: 'integer', min: 1 })).toBe(false);
    expect(isSliderRange({ name: 'w', label: 'W', type: 'integer', min: 1, max: 100000 })).toBe(false);
  });
});
