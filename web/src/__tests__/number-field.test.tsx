import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import type { ParamSpec } from '../lib/types';

vi.mock('../lib/app', () => ({ useApp: () => ({ backend: { mode: 'server' } }) }));
const { Field } = await import('../form/Field');

const decimal = { name: 'speed', label: 'Speed', type: 'number', min: -10, max: 1000 } as unknown as ParamSpec;
const whole = { name: 'count', label: 'Count', type: 'integer', min: 1, max: 10 } as unknown as ParamSpec;

function Harness({ param, initial, seen }: { param: ParamSpec; initial?: unknown; seen: unknown[] }) {
  const [v, setV] = useState<unknown>(initial ?? '');
  return (
    <>
      <Field param={param} value={v} onChange={(_, nv) => { seen.push(nv); setV(nv); }} />
      <button onClick={() => setV(7)}>external</button>
    </>
  );
}

/** Type one character at a time, the way a keyboard does. */
function typeKeys(el: HTMLInputElement, text: string) {
  for (const ch of text) fireEvent.change(el, { target: { value: el.value + ch } });
}

const input = (label: string) => screen.getByLabelText(label) as HTMLInputElement;

describe('number field typing', () => {
  it.each([
    ['1.5', 1.5],
    ['0.01', 0.01],
    ['5.05', 5.05],
    ['-1', -1],
    ['-3', -3],
    ['-0.25', -0.25],
  ])('keeps %s typed key by key', (text, expected) => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} seen={seen} />);
    typeKeys(input('Speed'), text);
    expect(input('Speed').value).toBe(text);
    expect(seen.at(-1)).toBe(expected);
  });

  it('keeps intermediate text such as "-", "." and "0." visible', () => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} seen={seen} />);
    const el = input('Speed');
    fireEvent.change(el, { target: { value: '-' } });
    expect(el.value).toBe('-');
    fireEvent.change(el, { target: { value: '.' } });
    expect(el.value).toBe('.');
    fireEvent.change(el, { target: { value: '0.' } });
    expect(el.value).toBe('0.');
    expect(seen.at(-1)).toBe(0);
  });

  it('commits a pasted value in one change', () => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} seen={seen} />);
    fireEvent.change(input('Speed'), { target: { value: '12.75' } });
    expect(seen).toEqual([12.75]);
    expect(input('Speed').value).toBe('12.75');
  });

  it('commits text that is not a number as-is so validation can flag it', () => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} seen={seen} />);
    fireEvent.change(input('Speed'), { target: { value: '1.2.3' } });
    expect(seen.at(-1)).toBe('1.2.3');
    expect(input('Speed').value).toBe('1.2.3');
  });

  it('clears to an empty value', () => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} initial={2} seen={seen} />);
    fireEvent.change(input('Speed'), { target: { value: '' } });
    expect(seen.at(-1)).toBe('');
  });

  it('normalizes a trailing dot on blur', () => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} seen={seen} />);
    typeKeys(input('Speed'), '2.');
    fireEvent.blur(input('Speed'));
    expect(input('Speed').value).toBe('2');
    expect(seen.at(-1)).toBe(2);
  });

  it('follows values set from outside the input', () => {
    const seen: unknown[] = [];
    render(<Harness param={decimal} initial={1.5} seen={seen} />);
    expect(input('Speed').value).toBe('1.5');
    fireEvent.click(screen.getByText('external'));
    expect(input('Speed').value).toBe('7');
  });

  it('steps with the arrow keys within min and max', () => {
    const seen: unknown[] = [];
    render(<Harness param={whole} initial={9} seen={seen} />);
    const el = input('Count');
    fireEvent.keyDown(el, { key: 'ArrowUp' });
    expect(seen.at(-1)).toBe(10);
    fireEvent.keyDown(el, { key: 'ArrowUp' });
    expect(seen.at(-1)).toBe(10);
    fireEvent.keyDown(el, { key: 'ArrowDown' });
    expect(el.value).toBe('9');
  });

  it('steps decimals without float noise', () => {
    const seen: unknown[] = [];
    const p = { ...decimal, step: 0.1 } as unknown as ParamSpec;
    render(<Harness param={p} initial={0.2} seen={seen} />);
    fireEvent.keyDown(input('Speed'), { key: 'ArrowUp' });
    expect(seen.at(-1)).toBe(0.3);
  });
});
