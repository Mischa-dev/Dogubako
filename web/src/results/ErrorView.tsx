import { useState } from 'react';
import { ChevronRight, CircleAlert, Ban, Package } from 'lucide-react';
import type { ToolErrorView } from '../lib/types';
import { describeError } from '../lib/errors';
import { href } from '../lib/router';
import { CopyButton, cx } from '../components/ui';

export function ErrorView({ error }: { error: ToolErrorView }) {
  const f = describeError(error);
  const [open, setOpen] = useState(false);
  const cancelled = error.code === 'cancelled';
  const Icon = cancelled ? Ban : error.code === 'missing_dependency' || error.code === 'missing_model' ? Package : CircleAlert;
  return (
    <div className={cx('error-view', cancelled && 'is-cancelled')} role={cancelled ? 'status' : 'alert'}>
      <Icon size={17} aria-hidden className="error-icon" />
      <div className="error-body">
        <div className="error-title">{f.title}</div>
        <div className="error-msg">{f.message}</div>
        {f.hint && (
          <div className="error-hint">
            <code className="mono">{f.hint}</code>
            <CopyButton text={f.hint} iconOnly label="Copy hint" />
          </div>
        )}
        {(error.code === 'missing_dependency' || error.code === 'missing_model') && (
          <a className="link-sm" href={href.settings(error.code === 'missing_model' ? 'models' : 'dependencies')}>
            Open {error.code === 'missing_model' ? 'Models' : 'Dependencies'} settings
          </a>
        )}
        {f.details && (
          <div className="error-details">
            <button type="button" className="advanced-toggle" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
              <ChevronRight size={13} aria-hidden className="chev" /> Program output
            </button>
            {open && <pre className="code-view mono wrap stderr">{f.details}</pre>}
          </div>
        )}
      </div>
    </div>
  );
}
