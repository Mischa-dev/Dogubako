import { describe, expect, it } from 'vitest';
import { restoredRegions } from '../tools/special/FaceRedact';

describe('restoredRegions (redaction editor, "open with same settings")', () => {
  it('reads image.redact regions and image.face_blur extra_regions', () => {
    const r = [{ x: 1, y: 2, width: 30, height: 40 }];
    expect(restoredRegions({ regions: r })).toEqual(r);
    expect(restoredRegions({ extra_regions: r })).toEqual(r);
    expect(restoredRegions({ regions: JSON.stringify(r) })).toEqual(r);
  });
  it('ignores missing or malformed regions', () => {
    expect(restoredRegions(undefined)).toBeNull();
    expect(restoredRegions({ regions: [] })).toBeNull();
    expect(restoredRegions({ regions: 'not json' })).toBeNull();
    expect(restoredRegions({ regions: [{ x: 'a' }] })).toBeNull();
  });
});
