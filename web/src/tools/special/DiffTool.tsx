import type { ToolInfo } from '../../lib/types';
import { Field } from '../../form/Field';
import { ParamForm } from '../../form/ParamForm';
import { useToolForm } from '../useToolForm';
import { useLiveRun, useToolShortcuts } from '../GenericTool';
import { ResultView } from '../../results/ResultView';
import { RunBar } from '../RunBar';

/** Two side-by-side inputs (texts or files) and a rendered diff. Text inputs re-run live. */
export default function DiffTool({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const form = useToolForm(tool, { jobId });
  useToolShortcuts(form);
  const pair = tool.params.filter((p) => !p.advanced && (p.type === 'file' || (p.type === 'text' && p.multiline))).slice(0, 2);
  const textMode = pair.length === 2 && pair.every((p) => p.type === 'text');
  useLiveRun(form, textMode && tool.availability.available, 350);
  const job = form.job.job;
  return (
    <div className="tool-body special special-diff">
      <div className="diff-pair">
        {pair.map((p) => (
          <Field key={p.name} param={p} value={form.values[p.name]} error={form.shownErrors[p.name]} onChange={form.setValue} mono={textMode} />
        ))}
      </div>
      <div className="diff-options">
        <ParamForm params={tool.params} exclude={pair.map((p) => p.name)} values={form.values} errors={form.shownErrors} onChange={form.setValue} idPrefix="diff" />
        {!textMode && <RunBar form={form} label="Compare" disabled={!tool.availability.available} />}
      </div>
      <div className="tool-output">
        {job && <ResultView job={job} tool={tool} input={form.input} live={textMode} dataFirst />}
      </div>
    </div>
  );
}
