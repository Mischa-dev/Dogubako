import { describe, expect, it } from 'vitest';
import { mergeDetected, restoredRegions } from '../tools/special/FaceRedact';

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

describe('mergeDetected (automatic face detection)', () => {
  it('keeps regions drawn while detection was running', () => {
    const old = { x: 0, y: 0, width: 5, height: 5 };
    const manual = { x: 50, y: 50, width: 10, height: 10 };
    const face = { x: 1, y: 1, width: 9, height: 9, label: 'face 1' };
    expect(mergeDetected([face], [old, manual], new Set([old]))).toEqual([face, manual]);
    // No faces found: manual regions survive.
    expect(mergeDetected([], [manual], new Set())).toEqual([manual]);
  });
});
