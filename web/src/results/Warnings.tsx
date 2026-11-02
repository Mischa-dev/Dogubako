import { TriangleAlert } from 'lucide-react';

/** Backend warnings for a result (e.g. a QR code that failed scan verification). */
export function Warnings({ warnings }: { warnings?: string[] | null }) {
  if (!warnings || warnings.length === 0) return null;
  return (
    <ul className="warnings" role="status">
      {warnings.map((w, i) => <li key={i}><TriangleAlert size={13} aria-hidden />{w}</li>)}
    </ul>
  );
}
