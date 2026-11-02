import { memo } from 'react';
import { Star, TriangleAlert, Lock } from 'lucide-react';
import type { ToolInfo } from '../lib/types';
import { href } from '../lib/router';
import { categoryIcon } from '../lib/icons';
import { toggleFavorite } from '../lib/store';
import { Highlight, cx } from './ui';

export function unavailableLabel(t: ToolInfo): string | null {
  const a = t.availability;
  if (a.available) return null;
  if (!a.platform_supported) return 'Not on this OS';
  if (a.missing?.length) return `Needs ${a.missing.map((m) => m.name).join(', ')}`;
  if (a.missing_model) return 'Needs model';
  return a.reason ?? 'Unavailable';
}

export const ToolRow = memo(function ToolRow({ tool, favorite, ranges, showCategory, categoryLabel }: {
  tool: ToolInfo;
  favorite: boolean;
  ranges?: [number, number][];
  showCategory?: boolean;
  categoryLabel?: string;
}) {
  const Icon = categoryIcon(tool.category);
  const unavailable = unavailableLabel(tool);
  const denied = tool.permitted === false;
  return (
    <li className={cx('tool-row', unavailable && 'is-unavailable')}>
      <a className="tool-row-link" href={href.tool(tool.id)} data-roving>
        <span className="tool-row-icon" aria-hidden><Icon size={16} /></span>
        <span className="tool-row-text">
          <span className="tool-row-name">
            <Highlight text={tool.name} ranges={ranges ?? []} />
            {showCategory && categoryLabel && <span className="tool-row-cat">{categoryLabel}</span>}
          </span>
          <span className="tool-row-desc">{tool.description}</span>
        </span>
        {unavailable && (
          <span className="tool-row-badge" title={tool.availability.reason ?? unavailable}>
            <TriangleAlert size={12} aria-hidden />
            {unavailable}
          </span>
        )}
        {denied && <span className="tool-row-badge"><Lock size={12} aria-hidden />No access</span>}
      </a>
      <button
        type="button"
        className={cx('star-btn', favorite && 'is-on')}
        aria-pressed={favorite}
        aria-label={favorite ? `Remove ${tool.name} from favorites` : `Add ${tool.name} to favorites`}
        title={favorite ? 'Remove from favorites' : 'Add to favorites'}
        onClick={() => toggleFavorite(tool.id)}
      >
        <Star size={15} aria-hidden fill={favorite ? 'currentColor' : 'none'} />
      </button>
    </li>
  );
});

/** Arrow-key navigation among [data-roving] links inside a container. */
export function rovingKeyDown(e: React.KeyboardEvent<HTMLElement>) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
  const items = Array.from(e.currentTarget.querySelectorAll<HTMLElement>('[data-roving]'));
  if (!items.length) return;
  const i = items.indexOf(document.activeElement as HTMLElement);
  if (i < 0 && !(e.key === 'Home' || e.key === 'End')) return;
  e.preventDefault();
  let n = i;
  if (e.key === 'ArrowDown') n = Math.min(items.length - 1, i + 1);
  if (e.key === 'ArrowUp') n = Math.max(0, i - 1);
  if (e.key === 'Home') n = 0;
  if (e.key === 'End') n = items.length - 1;
  items[n].focus();
  items[n].scrollIntoView({ block: 'nearest' });
}

export function ToolList({ tools, favorites, label, rangesFor, showCategory, categoryLabel }: {
  tools: ToolInfo[];
  favorites: string[];
  label: string;
  rangesFor?: (id: string) => [number, number][] | undefined;
  showCategory?: boolean;
  categoryLabel?: (id: string) => string;
}) {
  const fav = new Set(favorites);
  return (
    <ul className="tool-list" aria-label={label} onKeyDown={rovingKeyDown}>
      {tools.map((t) => (
        <ToolRow
          key={t.id}
          tool={t}
          favorite={fav.has(t.id)}
          ranges={rangesFor?.(t.id)}
          showCategory={showCategory}
          categoryLabel={categoryLabel?.(t.category)}
        />
      ))}
    </ul>
  );
}
