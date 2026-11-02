import { useEffect, useMemo, useRef, useState } from 'react';
import { ScanFace, Trash2, Wand2, Info } from 'lucide-react';
import type { ParamSpec } from '../../lib/types';
import type { SpecialProps } from '../ToolPage';
import { toolLayout, useStore } from '../../lib/store';
import { useApp } from '../../lib/app';
import type { FileItem } from '../../lib/schema';
import type { Region } from '../../lib/renderers';
import { Field } from '../../form/Field';
import { ParamForm } from '../../form/ParamForm';
import { ResultView } from '../../results/ResultView';
import { useToolForm } from '../useToolForm';
import { useJob } from '../useJob';
import { useToolShortcuts } from '../GenericTool';
import { RunBar } from '../RunBar';
import { Button, Callout, IconButton, Spinner, cx } from '../../components/ui';
import { describeError } from '../../lib/errors';
import { BoxEditor } from './BoxEditor';

/** Image redaction editor: detect faces (optional) → edit boxes → image.redact. */
export default function FaceRedact({ tool, jobId, head, meta }: SpecialProps) {
  const { toolMap, backend } = useApp();
  const layout = useStore(toolLayout);
  const redact = toolMap.get('image.redact');
  const detect = toolMap.get('image.face_detect');
  const canDetect = !!detect && detect.availability.available && detect.permitted !== false;
  // The final step always uses image.redact (single image, explicit regions).
  const target = redact && redact.permitted !== false ? redact : tool;
  const form = useToolForm(target, { jobId });
  useToolShortcuts(form, false);
  const detectJob = useJob(backend);
  const [boxes, setBoxes] = useState<Region[]>([]);
  const [selected, setSelected] = useState<number | null>(null);
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [detected, setDetected] = useState<string | null>(null);

  const fileParam = target.params.find((p) => p.type === 'file') as ParamSpec & { type: 'file' };
  const singleParam = useMemo(() => ({ ...fileParam, multiple: false, label: 'Image' }) as ParamSpec, [fileParam]);
  const item = ((form.values[fileParam.name] as FileItem[]) ?? []).find((i) => i.status === 'ready');
  const imageUrl = form.input.imageUrl;

  const boxesRef = useRef<Region[]>(boxes);
  boxesRef.current = boxes;
  const currentRef = useRef<string | undefined>();
  currentRef.current = item?.ref;

  const runDetect = async (startBoxes: Region[] = boxesRef.current) => {
    if (!item?.ref || !canDetect) return;
    const ref = item.ref;
    const before = new Set(startBoxes);
    const j = await detectJob.run('image.face_detect', { input: ref });
    if (currentRef.current !== ref) return; // the image changed meanwhile
    if (j?.status === 'succeeded') {
      const regs = ((j.result?.data as any)?.regions as Region[]) ?? [];
      const faces = regs.map((r, i) => ({ ...r, label: r.label ?? `face ${i + 1}` }));
      // Replace only the boxes that existed when detection started; regions
      // drawn or edited meanwhile are kept (over-redacting is the safe side).
      setBoxes((prev) => mergeDetected(faces, prev, before));
      setSelected(null);
      setDetected(regs.length ? `${regs.length} face${regs.length === 1 ? '' : 's'} found` : 'No faces found — draw boxes manually');
    }
  };

  // Auto-detect when a new image is ready.
  const lastRef = useRef<string | undefined>();
  useEffect(() => {
    if (item?.ref && item.ref !== lastRef.current) {
      const first = lastRef.current === undefined;
      lastRef.current = item.ref;
      // "Open with same settings": the restored run's image keeps the
      // regions the user selected (image.redact `regions` or
      // image.face_blur `extra_regions`) instead of re-detecting.
      const restored = first && jobId ? restoredRegions(form.job.job?.params) : null;
      if (restored) {
        setBoxes(restored);
        setSelected(null);
        setDetected(null);
        return;
      }
      setBoxes([]);
      setNatural(null);
      setDetected(null);
      form.job.reset();
      if (canDetect) void runDetect([]);
    }
    if (!item) { lastRef.current = undefined; setBoxes([]); setNatural(null); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item?.ref]);

  const apply = () => {
    if (!boxes.length || form.job.running) return;
    const regions = boxes.map(({ x, y, width, height }) => ({ x, y, width, height }));
    if (target.id === 'image.redact') void form.run({ overrides: { regions } });
    else void form.run({ overrides: { extra_regions: regions } });
  };

  const applyRef = useRef(apply);
  applyRef.current = apply;
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); applyRef.current(); }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  const optionParams = target.params.filter((p) => !['input', 'regions', 'extra_regions'].includes(p.name));
  const detectErr = detectJob.job?.status === 'failed' ? describeError(detectJob.job.error) : null;

  const staged = !!(imageUrl && item);
  return (
    <div className={cx('tool-body tool-work special special-redact', `place-${layout.controls}`, staged && 'has-stage')}>
      {staged && (
        <div className="tool-stage editor-stage">
          <div className="editor-panel">
            <div className="editor-toolbar">
              <span className="editor-hint">Drag on the image to add a box. Select a box to move or resize it.</span>
              <span className="editor-actions">
                {canDetect && (
                  <Button size="sm" variant="ghost" icon={ScanFace} onClick={() => void runDetect()} loading={detectJob.running}>Detect faces</Button>
                )}
                <Button size="sm" variant="ghost" icon={Trash2} onClick={() => { setBoxes([]); setSelected(null); }} disabled={!boxes.length}>Clear</Button>
              </span>
            </div>
            <div className="editor-canvas">
              <BoxEditor
                src={imageUrl!}
                boxes={boxes}
                onChange={setBoxes}
                selected={selected}
                onSelect={setSelected}
                natural={natural}
                onNatural={setNatural}
                label="Image to redact"
              />
              {detectJob.running && <div className="editor-busy" role="status"><Spinner size={14} /> Detecting faces…</div>}
            </div>
            <div className="editor-foot" aria-live="polite">
              {detectErr ? <span className="muted small">{detectErr.title}: {detectErr.message}</span> : detected && <span className="muted small">{detected}</span>}
              <span className="muted small">{boxes.length} region{boxes.length === 1 ? '' : 's'}{natural ? ` · ${natural.w}×${natural.h}px` : ''}</span>
            </div>
          </div>
        </div>
      )}
      <section className="tool-input tool-controls" aria-label="Settings">
        {head}
        <Field param={singleParam} value={form.values[fileParam.name]} error={form.shownErrors[fileParam.name]} onChange={form.setValue} />
        {!canDetect && (
          <Callout tone="info" icon={Info}>
            Automatic face detection is unavailable{detect?.availability.missing_model ? ' (no face-detection model installed)' : ''}. Draw boxes over the areas to hide.
          </Callout>
        )}
        {staged && boxes.length > 0 && (
          <ul className="region-list" aria-label="Regions">
            {boxes.map((b, i) => (
              <li key={i} className={cx(selected === i && 'is-selected')}>
                <button type="button" className="region-pick" onClick={() => setSelected(i)} aria-pressed={selected === i}>
                  <span className="region-idx">{i + 1}</span>
                  <span className="mono small">{b.x},{b.y} · {b.width}×{b.height}</span>
                  {b.score != null && <span className="muted small">score {Number(b.score).toFixed(2)}</span>}
                </button>
                <IconButton icon={Trash2} label={`Remove region ${i + 1}`} size={14} onClick={() => { setBoxes(boxes.filter((_, k) => k !== i)); setSelected(null); }} />
              </li>
            ))}
          </ul>
        )}
        <ParamForm params={optionParams} values={form.values} errors={form.shownErrors} onChange={form.setValue} idPrefix="redact" />
        <RunBar
          form={form}
          label="Apply redaction"
          disabled={!boxes.length || !target.availability.available}
          disabledReason={!item ? 'Choose an image first' : !boxes.length ? 'Draw a box over what to hide' : undefined}
          onRun={apply}
        />
        {meta}
      </section>
      <div className="tool-output" aria-label="Result">
        {form.job.job && <ResultView job={form.job.job} tool={target} input={{ imageUrl }} hideData />}
        {!form.job.job && item && boxes.length > 0 && (
          <p className="muted small"><Wand2 size={13} aria-hidden /> Choose a style, then apply to see the result here.</p>
        )}
      </div>
    </div>
  );
}

export function restoredRegions(params: Record<string, unknown> | undefined): Region[] | null {
  const raw = params?.regions ?? params?.extra_regions;
  const list = typeof raw === 'string' ? (() => { try { return JSON.parse(raw); } catch { return null; } })() : raw;
  if (!Array.isArray(list)) return null;
  const regs = list.filter((r): r is Region =>
    r && ['x', 'y', 'width', 'height'].every((k) => Number.isFinite(Number((r as any)[k]))))
    .map((r) => ({ x: Number(r.x), y: Number(r.y), width: Number(r.width), height: Number(r.height) }));
  return regs.length ? regs : null;
}

/** Detection results replace the boxes that existed when detection started;
 *  boxes drawn or edited since then are kept. */
export function mergeDetected(faces: Region[], current: Region[], atStart: Set<Region>): Region[] {
  return [...faces, ...current.filter((b) => !atStart.has(b))];
}
