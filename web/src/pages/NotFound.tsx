import { Compass } from 'lucide-react';
import { EmptyState } from '../components/ui';

export function NotFound() {
  return (
    <div className="page">
      <h1 className="sr-only" tabIndex={-1} data-route-focus>Page not found</h1>
      <EmptyState icon={Compass} title="Nothing here" action={<a className="btn btn-secondary" href="#/">Back to Home</a>}>
        This page doesn't exist or isn't available on this instance.
      </EmptyState>
    </div>
  );
}
