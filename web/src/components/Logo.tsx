/**
 * The Dogubako mark: the toolbox drawn as one line, a lid seam across it, and a
 * small vermilion latch — the only place the brand colour appears. Inline use
 * follows the text colour; `tile` draws the app-icon version on a dark squircle.
 */
export function LogoMark({ size = 22, title, tile = false }: { size?: number; title?: string; tile?: boolean }) {
  const a11y = title ? { role: 'img' as const, 'aria-label': title } : { 'aria-hidden': true as const };
  if (tile) {
    return (
      <svg width={size} height={size} viewBox="0 0 100 100" {...a11y}>
        <rect x="2" y="2" width="96" height="96" rx="22" fill="#18181a" />
        <rect x="25" y="31" width="50" height="40" rx="7" fill="none" stroke="#f1f0ed" strokeWidth="3.6" />
        <line x1="25" y1="46" x2="75" y2="46" stroke="#f1f0ed" strokeWidth="3.6" />
        <rect x="45" y="42" width="10" height="8" rx="1.6" fill="#e0533d" />
      </svg>
    );
  }
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" {...a11y}>
      <rect x="10" y="24" width="80" height="58" rx="10" fill="none" stroke="currentColor" strokeWidth="8" />
      <line x1="10" y1="46" x2="90" y2="46" stroke="currentColor" strokeWidth="8" />
      <rect x="41" y="39" width="18" height="14" rx="2" fill="#e0533d" />
    </svg>
  );
}
