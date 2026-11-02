import { memo } from 'react';
import { CircleCheck, TriangleAlert } from 'lucide-react';
import type { JobView, ToolInfo } from '../lib/types';
import { formatDuration } from '../lib/format';
import { TextOutput } from './TextOutput';
import { DataViews, type InputContext } from './DataViews';
import { ArtifactList } from './Artifacts';
import { ErrorView } from './ErrorView';

export const ResultView = memo(function ResultView({ job, tool, input, live, hideData, dataFirst }: {
  job: JobView;
  tool: ToolInfo;
  input?: InputContext;
  live?: boolean;
  hideData?: boolean;
  dataFirst?: boolean;
}) {
  if (job.status === 'failed' || job.status === 'cancelled') {
    return <div className="result"><ErrorView error={job.error ?? { code: job.status === 'cancelled' ? 'cancelled' : 'failed' }} /></div>;
  }
  const r = job.result;
  if (!r) return null;
  const hasText = r.text != null && r.text !== '';
  const hasArtifacts = r.artifacts?.length > 0;
  const filename = tool.id.split('.').pop() ?? 'output';
  return (
    <div className="result">
      {!live && (
        <div className="result-status" role="status">
          <CircleCheck size={15} aria-hidden className="ok-icon" />
          <span>Done in {formatDuration(r.duration_ms)}</span>
        </div>
      )}
      {r.warnings && r.warnings.length > 0 && (
        <ul className="warnings">
          {r.warnings.map((w, i) => <li key={i}><TriangleAlert size={13} aria-hidden />{w}</li>)}
        </ul>
      )}
      {dataFirst && !hideData && <DataViews data={r.data} input={input} />}
      {hasText && <TextOutput text={r.text!} mime={r.text_mime ?? tool.output.text_mime} filename={filename} live={live} />}
      {!dataFirst && !hideData && <DataViews data={r.data} input={input} />}
      {hasArtifacts && <ArtifactList artifacts={r.artifacts} jobId={job.id || undefined} inputImage={input?.imageUrl} />}
      {!hasText && !hasArtifacts && (r.data == null || (typeof r.data === 'object' && Object.keys(r.data as object).length === 0)) && (
        <p className="muted small">The tool finished without output.</p>
      )}
    </div>
  );
});
