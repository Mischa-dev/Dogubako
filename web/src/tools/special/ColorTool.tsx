import type { ToolInfo } from '../../lib/types';
import { ParamForm } from '../../form/ParamForm';
import { useToolForm } from '../useToolForm';
import { useLiveRun, useToolShortcuts } from '../GenericTool';
import { ResultView } from '../../results/ResultView';
import { RunBar } from '../RunBar';
import { Spinner } from '../../components/ui';

/** Color tools: live swatches as you pick (file-based tools run on demand). */
export default function ColorTool({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const form = useToolForm(tool, { jobId });
  useToolShortcuts(form);
  const hasFile = tool.params.some((p) => p.type === 'file');
  useLiveRun(form, tool.availability.available, hasFile ? 300 : 150);
  const job = form.job.job;
  const data = job?.result?.data as Record<string, any> | undefined;
  const contrast = tool.id === 'design.contrast' && data && typeof data.ratio === 'number';
  const fg = String(form.values.foreground ?? '#000');
  const bg = String(form.values.background ?? '#fff');
  return (
    <div className="tool-body tool-live special special-color">
      <div className="tool-input">
        <ParamForm params={tool.params} values={form.values} errors={form.shownErrors} onChange={form.setValue} idPrefix="color" />
        {hasFile && <RunBar form={form} label="Analyze" disabled={!tool.availability.available} />}
      </div>
      <div className="tool-output">
        {contrast && (
          <div className="contrast-card" style={{ background: bg, color: fg }}>
            <span className="contrast-ratio">{data!.ratio_text ?? `${data!.ratio}:1`}</span>
            <span className="contrast-sample">The quick brown fox jumps over the lazy dog.</span>
            <span className="contrast-sample-lg">Large text sample</span>
          </div>
        )}
        {!job && form.job.running && <Spinner />}
        {job && <ResultView job={job} tool={tool} input={form.input} live={!hasFile} />}
      </div>
    </div>
  );
}
