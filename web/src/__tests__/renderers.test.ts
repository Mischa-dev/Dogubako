import { describe, expect, it } from 'vitest';
import { compareCells, extForMime, selectDataViews, tableToCsv } from '../lib/renderers';

describe('selectDataViews', () => {
  it('returns nothing for null or empty arrays', () => {
    expect(selectDataViews(null)).toEqual([]);
    expect(selectDataViews([])).toEqual([]);
  });
  it('detects special keys and puts the rest into a key/value view', () => {
    const v = selectDataViews({
      table: { columns: ['a'], rows: [[1]] },
      colors: [{ hex: '#ffffff' }],
      diff: [{ tag: 'equal', text: 'x' }, { tag: 'insert', text: 'y' }],
      regions: [{ x: 1, y: 2, width: 3, height: 4 }],
      matches: [{ start: 0, end: 1, text: 'a' }],
      count: 3,
    });
    expect(v.map((x) => x.kind)).toEqual(['table', 'colors', 'diff', 'regions', 'matches', 'kv']);
    expect(v[5]).toMatchObject({ kind: 'kv', value: { count: 3 } });
  });
  it('falls back to kv when a special key has the wrong shape', () => {
    expect(selectDataViews({ table: 'nope' }).map((x) => x.kind)).toEqual(['kv']);
    expect(selectDataViews({ regions: [{ x: 1 }] }).map((x) => x.kind)).toEqual(['kv']);
  });
  it('renders bare color arrays as swatches and other arrays as JSON', () => {
    expect(selectDataViews([{ hex: '#000' }])[0].kind).toBe('colors');
    expect(selectDataViews([1, 2])[0].kind).toBe('json');
  });
});

describe('helpers', () => {
  it('converts tables to CSV with quoting', () => {
    expect(tableToCsv({ columns: ['a', 'b'], rows: [['x,y', 'q"t'], [1, null]] })).toBe('a,b\n"x,y","q""t"\n1,');
  });
  it('sorts numbers numerically and strings naturally', () => {
    expect([10, 9, 100].sort(compareCells)).toEqual([9, 10, 100]);
    expect(['file10', 'file9'].sort(compareCells)).toEqual(['file9', 'file10']);
    expect(['2', '10'].sort(compareCells)).toEqual(['2', '10']);
  });
  it('maps mime types to extensions', () => {
    expect(extForMime('application/json')).toBe('json');
    expect(extForMime('text/markdown; charset=utf-8')).toBe('md');
    expect(extForMime(undefined)).toBe('txt');
  });
});
