import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { JobView } from '../lib/types';
import type { FileItem } from '../lib/schema';

const pickFiles = vi.fn(async () => []);
vi.mock('../lib/app', () => ({
  useApp: () => ({ backend: { pickFiles, uploadFiles: async () => [], fileUrl: () => '', artifactUrl: () => '' } }),
}));

const { Segmented } = await import('../components/ui');
const { FileField } = await import('../form/FileField');
const { typeAheadMatch } = await import('../form/Field');
const { Stage } = await import('../tools/Stage');

// Viewport the regressions were reported at (DB-41): 1280×800, where a half-width settings column
// is too narrow for some four-option segmented controls.
describe('Segmented wrap', () => {
  it('wraps instead of hiding options behind a scroll with no scrollbar', () => {
    render(<Segmented wrap label="To" value="lf" onChange={() => undefined} options={[
      { value: 'lf', label: 'LF (Unix)' }, { value: 'crlf', label: 'CRLF (Windows)' }, { value: 'cr', label: 'CR (old Mac)' },
    ]} />);
    const group = screen.getByRole('radiogroup');
    expect(group.className).toContain('segmented-wrap');
    expect(screen.getAllByRole('radio')).toHaveLength(3);
  });

  it('keeps the scrolling layout when wrap is not asked for', () => {
    render(<Segmented label="Mode" value="a" onChange={() => undefined} options={[{ value: 'a', label: 'A' }]} />);
    expect(screen.getByRole('radiogroup').className).not.toContain('segmented-wrap');
  });
});

describe('FileField drop zone vs a dropdown over it', () => {
  const param = { name: 'icc', label: 'ICC profile', type: 'file' as const };

  it('ignores a click whose press began elsewhere (a native dropdown list released over the zone)', () => {
    pickFiles.mockClear();
    const { container } = render(<FileField param={param} items={[]} onChange={() => undefined} labelledBy="x" />);
    fireEvent.click(container.querySelector('.dropzone')!, { detail: 1 });
    expect(pickFiles).not.toHaveBeenCalled();
  });

  it('browses on a real press and release on the zone, and on keyboard activation', () => {
    pickFiles.mockClear();
    const { container } = render(<FileField param={param} items={[]} onChange={() => undefined} labelledBy="x" />);
    const zone = container.querySelector('.dropzone')!;
    fireEvent.pointerDown(zone);
    fireEvent.click(zone, { detail: 1 });
    expect(pickFiles).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(zone, { key: 'Enter' });
    expect(pickFiles).toHaveBeenCalledTimes(2);
  });
});

describe('typeAheadMatch', () => {
  const select = () => {
    const s = document.createElement('select');
    for (const [v, l] of [['png', 'PNG'], ['jpeg', 'JPEG'], ['jxl', 'JPEG XL'], ['webp', 'WebP'], ['tiff', 'TIFF']]) s.add(new Option(l, v));
    return s;
  };

  it('picks the first option starting with the typed letters', () => {
    const s = select();
    const a = typeAheadMatch(s, 'w', false, { text: '', at: 0 }, 1000)!;
    expect(a.value).toBe('webp');
    const b = typeAheadMatch(s, 'j', false, { text: '', at: 0 }, 1000)!;
    s.value = b.value;
    const c = typeAheadMatch(s, 'p', false, b.buffer, 1100)!;
    expect(c.value).toBe('jpeg');
  });

  it('cycles through matches when one letter repeats, and resets after a pause', () => {
    const s = select();
    s.value = 'jpeg';
    const a = typeAheadMatch(s, 'j', false, { text: 'j', at: 1000 }, 1100)!;
    expect(a.value).toBe('jxl');
    const b = typeAheadMatch(s, 't', false, { text: 'jj', at: 1000 }, 5000)!;
    expect(b.value).toBe('tiff');
  });

  it('lets Space open the dropdown again once the typed text has expired', () => {
    const s = select();
    expect(typeAheadMatch(s, ' ', false, { text: 'w', at: 1000 }, 5000)).toBeNull();
    expect(typeAheadMatch(s, ' ', false, { text: 'w', at: 1000 }, 1200)).not.toBeNull();
  });

  it('leaves modified and non-printable keys to the browser', () => {
    const s = select();
    expect(typeAheadMatch(s, 'ArrowDown', false, { text: '', at: 0 })).toBeNull();
    expect(typeAheadMatch(s, 'a', true, { text: '', at: 0 })).toBeNull();
  });
});

describe('Stage with a result that has no file', () => {
  const file: FileItem = { id: '1', name: 'd.csv', size: 4, mime: 'text/csv', status: 'ready', ref: 'f_1' } as unknown as FileItem;
  const job = { id: 'j', status: 'succeeded', result: { artifacts: [] } } as unknown as JobView;

  it('says the result is below and offers a way to get to it', () => {
    const onShowResult = vi.fn();
    render(<Stage files={[file]} job={job} running={false} live={false} mode="side" onShowResult={onShowResult} />);
    expect(screen.queryByText('Run it to see the result here.')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: /show result/i }));
    expect(onShowResult).toHaveBeenCalled();
  });
});
