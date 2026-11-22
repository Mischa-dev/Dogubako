import { useEffect, useState } from 'react';
import { ChevronRight, CircleAlert, Ban, Package } from 'lucide-react';
import type { ToolErrorView, ToolInfo } from '../lib/types';
import { GetMissing } from '../components/GetMissing';
import { notifyToolsChanged } from '../lib/app';
import { describeError } from '../lib/errors';
import { href } from '../lib/router';
import { CopyButton, cx } from '../components/ui';

/** `tool` lets a missing engine or model error offer its download in place. */
export function ErrorView({ error, tool }: { error: ToolErrorView; tool?: ToolInfo }) {
  const f = describeError(error);
  const [open, setOpen] = useState(false);
  const cancelled = error.code === 'cancelled';
  const missing = error.code === 'missing_dependency' || error.code === 'missing_model';
  // The tool list said it was available: refresh it so the download offer reflects what is gone.
  const stale = !!tool && missing && tool.availability.available;
  useEffect(() => { if (stale) notifyToolsChanged(); }, [stale]);
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
        {tool && (error.code === 'missing_dependency' || error.code === 'missing_model') && <GetMissing availability={tool.availability} />}
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
