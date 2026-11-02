import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { Segmented, Highlight } from '../components/ui';
import { acceptMatches } from '../form/FileField';

describe('Segmented', () => {
  it('supports arrow-key navigation with roving tabindex', () => {
    const onChange = vi.fn();
    render(<Segmented label="Mode" value="a" onChange={onChange} options={[{ value: 'a', label: 'A' }, { value: 'b', label: 'B' }]} />);
    const radios = screen.getAllByRole('radio');
    expect(radios[0].getAttribute('tabindex')).toBe('0');
    expect(radios[1].getAttribute('tabindex')).toBe('-1');
    fireEvent.keyDown(screen.getByRole('radiogroup'), { key: 'ArrowRight' });
    expect(onChange).toHaveBeenCalledWith('b');
  });
});

describe('Highlight', () => {
  it('wraps ranges in <mark>', () => {
    const { container } = render(<Highlight text="Compress PDF" ranges={[[0, 4]]} />);
    expect(container.querySelector('mark')?.textContent).toBe('Comp');
  });
});

describe('acceptMatches', () => {
  it('matches mime wildcards and extensions', () => {
    expect(acceptMatches(['image/*'], 'a.png', 'image/png')).toBe(true);
    expect(acceptMatches(['.pdf'], 'A.PDF', '')).toBe(true);
    expect(acceptMatches(['image/*', '.svg'], 'doc.pdf', 'application/pdf')).toBe(false);
    expect(acceptMatches([], 'x', '')).toBe(true);
  });
});
