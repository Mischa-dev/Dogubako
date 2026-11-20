import { describe, expect, it } from 'vitest';
import { qrCapacityError } from '../lib/qr';
import { checkQr } from '../tools/special/Qr';

describe('qrCapacityError', () => {
  it('rejects near-max text at the default Medium level', () => {
    expect(qrCapacityError('x'.repeat(4296), 'M')).toMatch(/Medium error correction: 4,296 of 2,331 bytes/);
    expect(qrCapacityError('x'.repeat(2331), 'M')).toBeNull();
    expect(qrCapacityError('https://example.com', 'M')).toBeNull();
  });

  it('uses the denser digit and alphanumeric capacities', () => {
    expect(qrCapacityError('7'.repeat(7089), 'L')).toBeNull();
    expect(qrCapacityError('7'.repeat(7090), 'L')).toMatch(/7,089 digits/);
    expect(qrCapacityError('A1'.repeat(2148), 'L')).toBeNull();
    expect(qrCapacityError('A'.repeat(1853), 'H')).toMatch(/1,852 characters/);
  });

  it('counts UTF-8 bytes for non-ASCII text', () => {
    expect(qrCapacityError('é'.repeat(1200), 'M')).toMatch(/2,400 of 2,331 bytes/);
  });
});

describe('checkQr', () => {
  it('follows the chosen error correction and forces High with a logo', () => {
    const text = 'x'.repeat(2000);
    expect(checkQr({ text, error_correction: 'L' })).toEqual({});
    expect(checkQr({ text, error_correction: 'H' }).text).toMatch(/High error correction/);
    const logo = [{ key: 'k', name: 'l.png', size: 1, mime: 'image/png', status: 'ready' }];
    expect(checkQr({ text: 'x'.repeat(1300), error_correction: 'L', logo }).text).toMatch(/remove the logo/);
  });
});
