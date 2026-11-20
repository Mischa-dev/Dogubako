import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ParamSpec } from '../lib/types';
import { optionIssue, validate, valueIssue } from '../lib/schema';
import { ParamForm } from '../form/ParamForm';

vi.mock('../lib/app', () => ({ useApp: () => ({ backend: {} }) }));

const NO_JXL = "This ImageMagick install can't write JXL; install ImageMagick with JXL support or pick another format";
const params: ParamSpec[] = [
  {
    name: 'format', label: 'Format', type: 'enum', default: 'png',
    options: [
      { value: 'png', label: 'PNG' }, { value: 'mp3', label: 'MP3' }, { value: 'flac', label: 'FLAC' },
      { value: 'jpg', label: 'JPEG' }, { value: 'jxl', label: 'JPEG XL', unavailable: NO_JXL },
    ],
  },
  {
    name: 'rate', label: 'Sample rate', type: 'enum', default: 'keep',
    options: [{ value: 'keep', label: 'Keep' }, { value: '96000', label: '96 kHz', only_if: { param: 'format', values: ['flac', 'png'] } }],
  },
  { name: 'transparent', label: 'Transparent', type: 'boolean', unavailable: 'Needs pdftocairo, which this Poppler install does not include' },
];

describe('options this install cannot do', () => {
  it('explains why an option is unavailable', () => {
    const jxl = (params[0] as Extract<ParamSpec, { type: 'enum' }>).options[4];
    expect(optionIssue(jxl, { format: 'png' }, params)).toBe(NO_JXL);
    const hi = (params[1] as Extract<ParamSpec, { type: 'enum' }>).options[1];
    expect(optionIssue(hi, { format: 'flac' }, params)).toBeNull();
    expect(optionIssue(hi, { format: 'mp3' }, params)).toBe('Not available with MP3');
  });

  it('blocks Run with the reason instead of letting the engine fail', () => {
    expect(validate({ params }, { format: 'jxl', rate: 'keep' }).format).toBe(`JPEG XL: ${NO_JXL}`);
    expect(validate({ params }, { format: 'mp3', rate: '96000' }).rate).toBe('96 kHz: Not available with MP3');
    expect(validate({ params }, { format: 'png', rate: 'keep', transparent: true }).transparent).toMatch(/pdftocairo/);
    expect(validate({ params }, { format: 'flac', rate: '96000', transparent: false })).toEqual({});
    expect(valueIssue(params[2], { transparent: false })).toBeNull();
  });

  it('disables unavailable choices in the form and warns about a picked one', () => {
    const { rerender } = render(<ParamForm params={params} values={{ format: 'png', rate: 'keep' }} errors={{}} onChange={() => {}} />);
    const jxl = screen.getByRole('option', { name: 'JPEG XL (unavailable)' }) as HTMLOptionElement;
    expect(jxl.disabled).toBe(true);
    expect(jxl.title).toBe(NO_JXL);
    expect((screen.getByRole('radio', { name: '96 kHz' }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole('switch') as HTMLInputElement | HTMLButtonElement).disabled).toBe(true);

    rerender(<ParamForm params={params} values={{ format: 'mp3', rate: '96000' }} errors={{}} onChange={() => {}} />);
    expect(screen.getByText('96 kHz: Not available with MP3')).toBeTruthy();
    expect((screen.getByRole('radio', { name: 'Keep' }) as HTMLButtonElement).disabled).toBe(false);
  });
});
