import { useEffect, useState } from 'react';
import { Play, Square, RotateCcw } from 'lucide-react';
import type { ToolForm } from './useToolForm';
import { formatDuration } from '../lib/format';
import { Button, Kbd, ProgressBar, modKey } from '../components/ui';

function useElapsed(startedAt: number | null, running: boolean) {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!running) return;
    const t = window.setInterval(() => setNow(Date.now()), 250);
    return () => window.clearInterval(t);
  }, [running]);
  return startedAt ? Math.max(0, (running ? now : Date.now()) - startedAt) : 0;
}

export function RunBar({ form, label = 'Run', disabled, disabledReason, secondary, onRun }: {
  form: ToolForm;
  label?: string;
  disabled?: boolean;
  disabledReason?: string;
  secondary?: React.ReactNode;
  onRun?: () => void;
}) {
  const { job } = form;
  const elapsed = useElapsed(job.startedAt, job.running);
  const j = job.job;
  const status = job.running
    ? j?.status === 'queued' ? 'Queued…' : j?.message || (j?.progress != null ? `Working… ${Math.round(j.progress * 100)}%` : 'Working…')
    : '';
  return (
    <div className="run-bar">
      <div className="run-bar-main">
        {job.running ? (
          <Button variant="secondary" icon={Square} onClick={() => void job.cancel()}>Cancel</Button>
        ) : (
          <Button
            variant="primary"
            icon={Play}
            onClick={() => (onRun ? onRun() : void form.run())}
            disabled={disabled}
            title={disabledReason ?? `${label} (${modKey}+Enter)`}
            aria-keyshortcuts="Control+Enter Meta+Enter"
          >
            {label}
          </Button>
        )}
        {!job.running && <span className="run-hint" aria-hidden><Kbd>{modKey}</Kbd><Kbd>↵</Kbd></span>}
        {secondary}
        {!job.running && j && (
          <Button variant="ghost" size="sm" icon={RotateCcw} onClick={form.reset} className="run-reset">Reset</Button>
        )}
      </div>
      <div className="run-bar-status" aria-live="polite" role="status">
        {job.running && (
          <>
            <div className="run-progress">
              <ProgressBar value={j?.progress ?? undefined} label="Job progress" />
            </div>
            <span className="run-msg">{status}</span>
            <span className="run-time mono">{formatDuration(elapsed)}</span>
          </>
        )}
        {!job.running && disabledReason && disabled && <span className="run-msg muted">{disabledReason}</span>}
      </div>
    </div>
  );
}
