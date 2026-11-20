import { AlertTriangle } from 'lucide-react';

/** Marks a result that no longer matches the form because its current values are invalid. */
export function StaleNotice({ stale }: { stale: boolean }) {
  if (!stale) return null;
  return (
    <p className="stale-notice" role="status">
      <AlertTriangle size={14} aria-hidden />
      <span>Out of date: this result is from earlier settings. Fix the highlighted fields to update it.</span>
    </p>
  );
}
