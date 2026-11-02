import { useEffect, useMemo, useRef, useState } from 'react';
import type { ToolInfo, ParamSpec } from '../../lib/types';
import type { FileItem } from '../../lib/schema';
import type { Region } from '../../lib/renderers';
import { useApp } from '../../lib/app';
import { Field } from '../../form/Field';
import { ParamForm } from '../../form/ParamForm';
import { ResultView } from '../../results/ResultView';
import { useToolForm } from '../useToolForm';
import { useToolShortcuts } from '../GenericTool';
import { RunBar } from '../RunBar';
import { Segmented } from '../../components/ui';
import { BoxEditor } from './BoxEditor';

const PRESETS: { value: string; label: string; r: number | null }[] = [
  { value: 'free', label: 'Free', r: null },
  { value: '1:1', label: '1:1', r: 1 },
  { value: '4:3', label: '4:3', r: 4 / 3 },
  { value: '3:2', label: '3:2', r: 3 / 2 },
  { value: '16:9', label: '16:9', r: 16 / 9 },
  { value: '9:16', label: '9:16', r: 9 / 16 },
  { value: '4:5', label: '4:5', r: 4 / 5 },
];

export default function Crop({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const { backend } = useApp();
  const form = useToolForm(tool, { jobId });
  useToolShortcuts(form);
  const isVideo = tool.id.startsWith('video.');
  const manualMode = isVideo ? 'manual' : 'rect';
  const fileParam = tool.params.find((p) => p.type === 'file') as ParamSpec & { type: 'file' };
  const singleParam = useMemo(() => ({ ...fileParam, multiple: false }) as ParamSpec, [fileParam]);
  const items = (form.values[fileParam.name] as FileItem[]) ?? [];
  const item = items.find((i) => i.status === 'ready');
  // Dimensions are tagged with the file they were measured from, so a
  // replacement starts without any until its own media has loaded.
  const [measured, setMeasured] = useState<{ w: number; h: number; ref: string } | null>(null);
  const natural = useMemo(
    () => (measured && item?.ref && measured.ref === item.ref ? { w: measured.w, h: measured.h } : null),
    [measured, item?.ref],
  );
  const onNatural = (n: { w: number; h: number }) => { if (item?.ref) setMeasured({ ...n, ref: item.ref }); };
  const [preset, setPreset] = useState('free');
  const [sel, setSel] = useState<number | null>(0);
  const aspect = PRESETS.find((p) => p.value === preset)?.r ?? null;

  const src = item ? (isVideo ? backend.fileUrl({ ref: item.ref!, name: item.name, size: item.size, mime: item.mime }) : form.input.imageUrl) : undefined;

  const v = form.values;
  const box: Region | null = natural && v.width !== '' && v.height !== '' && v.width != null && v.height != null
    ? { x: Number(v.x) || 0, y: Number(v.y) || 0, width: Number(v.width), height: Number(v.height) }
    : null;

  const setBox = (b: Region[]) => {
    const r = b[0];
    if (!r) return;
    form.setValues((prev) => ({ ...prev, mode: manualMode, x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) }));
  };

  // Initialize the crop to the full frame (or centered aspect) when media loads.
  const lastRef = useRef<string>();
  useEffect(() => {
    if (!natural || !item?.ref || lastRef.current === item.ref) return;
    const first = lastRef.current === undefined;
    lastRef.current = item.ref;
    // "Open with same settings": the first file of a restored run keeps the
    // restored crop (and mode) instead of the default inset rectangle.
    if (first && jobId) return;
    const inset = Math.round(Math.min(natural.w, natural.h) * 0.08);
    setBox([{ x: inset, y: inset, width: natural.w - inset * 2, height: natural.h - inset * 2 }]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [natural, item?.ref]);

  const applyPreset = (p: string) => {
    setPreset(p);
    const r = PRESETS.find((x) => x.value === p)?.r;
    if (!r || !natural) return;
    let w = natural.w, h = w / r;
    if (h > natural.h) { h = natural.h; w = h * r; }
    w *= 0.9; h *= 0.9;
    setBox([{ x: (natural.w - w) / 2, y: (natural.h - h) / 2, width: w, height: h }]);
  };

  const exclude = [fileParam.name, 'x', 'y', 'width', 'height', 'mode', ...(isVideo ? ['aspect'] : ['aspect', 'anchor'])];
  return (
    <div className="tool-body special special-crop">
      <div className="tool-input">
        <Field param={singleParam} value={form.values[fileParam.name]} error={form.shownErrors[fileParam.name]} onChange={form.setValue} />
        {src && (
          <div className="editor-panel">
            <div className="editor-toolbar">
              <Segmented size="sm" label="Aspect ratio" value={preset} onChange={applyPreset} options={PRESETS.map((p) => ({ value: p.value, label: p.label }))} />
            </div>
            <div className="editor-canvas">
              <BoxEditor
                src={src}
                media={isVideo ? 'video' : 'image'}
                boxes={box ? [box] : []}
                onChange={setBox}
                selected={sel}
                onSelect={(i) => setSel(i ?? 0)}
                single
                aspect={aspect}
                natural={natural}
                onNatural={onNatural}
                label={isVideo ? 'Video frame' : 'Image to crop'}
                tone="crop"
              />
            </div>
            <div className="crop-fields">
              {(['x', 'y', 'width', 'height'] as const).map((k) => {
                const p = tool.params.find((x) => x.name === k);
                return p ? (
                  <label key={k} className="crop-num">
                    <span>{k === 'width' ? 'W' : k === 'height' ? 'H' : k.toUpperCase()}</span>
                    <input className="input input-num mono" type="number" min={0} value={String(v[k] ?? '')}
                      onChange={(e) => form.setValues((prev) => ({ ...prev, mode: manualMode, [k]: e.target.value === '' ? '' : Number(e.target.value) }))}
                      aria-label={p.label} />
                  </label>
                ) : null;
              })}
              {natural && <span className="muted small">of {natural.w}×{natural.h}px</span>}
            </div>
          </div>
        )}
        <ParamForm params={tool.params} exclude={exclude} values={form.values} errors={form.shownErrors} onChange={form.setValue} idPrefix="crop" />
        <RunBar form={form} label="Crop" disabled={!tool.availability.available || !item} disabledReason={!item ? `Choose ${isVideo ? 'a video' : 'an image'} first` : undefined} />
      </div>
      <div className="tool-output">
        {form.job.job && <ResultView job={form.job.job} tool={tool} input={undefined} />}
      </div>
    </div>
  );
}
