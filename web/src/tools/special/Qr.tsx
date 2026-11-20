import { useEffect, useState } from 'react';
import { QrCode } from 'lucide-react';
import type { ToolInfo, Artifact } from '../../lib/types';
import { useApp } from '../../lib/app';
import { ParamForm } from '../../form/ParamForm';
import { useToolForm } from '../useToolForm';
import { useLiveRun, useToolShortcuts } from '../GenericTool';
import { ArtifactList } from '../../results/Artifacts';
import { ErrorView } from '../../results/ErrorView';
import { Warnings } from '../../results/Warnings';
import { Spinner, cx } from '../../components/ui';
import { ResetButton } from '../RunBar';
import { StaleNotice } from '../../results/Stale';
import { qrCapacityError } from '../../lib/qr';
import type { FileItem, FormErrors, FormValues } from '../../lib/schema';

/** Content must fit a QR code at the chosen error correction (a logo forces High). */
export function checkQr(values: FormValues): FormErrors {
  const logo = ((values.logo as FileItem[] | undefined) ?? []).length > 0;
  const err = qrCapacityError(String(values.text ?? ''), logo ? 'H' : String(values.error_correction ?? 'M'), logo);
  return err ? { text: err } : {};
}


/** QR generator with a live preview of the produced SVG/PNG. */
export default function Qr({ tool, jobId }: { tool: ToolInfo; jobId?: string | null }) {
  const { backend } = useApp();
  const form = useToolForm(tool, { jobId, check: checkQr });
  useToolShortcuts(form);
  useLiveRun(form, tool.availability.available, 400);
  const job = form.job.job;
  const arts: Artifact[] = job?.status === 'succeeded' ? job.result?.artifacts ?? [] : [];
  const svg = arts.find((a) => a.mime === 'image/svg+xml');
  const img = svg ?? arts.find((a) => a.mime.startsWith('image/'));
  const [shown, setShown] = useState<string | null>(null);
  const url = img ? backend.artifactUrl(img) : null;
  // Keep the previous image until the new one loads (no flicker while typing).
  useEffect(() => {
    if (!url) return;
    const im = new Image();
    im.onload = () => setShown(url);
    im.src = url;
  }, [url]);
  useEffect(() => setShown(null), [form.resetKey]);
  const textSvg = job?.result?.text_mime === 'image/svg+xml' ? job.result.text : null;

  return (
    <div className="tool-body tool-live special special-qr">
      <div className="tool-input">
        <ParamForm resetKey={form.resetKey} params={tool.params} values={form.values} errors={form.shownErrors} onChange={form.setValue} idPrefix="qr" />
        <ResetButton form={form} />
      </div>
      <div className={cx('tool-output', form.stale && 'is-stale')}>
        <StaleNotice stale={form.stale} />
        <div className="qr-preview checker" aria-live="polite">
          {textSvg ? <img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(textSvg)}`} alt="QR code preview" />
            : shown ? <img src={shown} alt="QR code preview" />
            : <span className="qr-empty"><QrCode size={28} aria-hidden /><span>Type text or a URL to generate a code</span></span>}
          {form.job.running && <span className="qr-busy"><Spinner size={12} /></span>}
        </div>
        {job?.status === 'failed' && <ErrorView error={job.error ?? { code: 'failed' }} />}
        {job?.status === 'succeeded' && <Warnings warnings={job.result?.warnings} />}
        {arts.length > 0 && <ArtifactList artifacts={arts} jobId={job?.id} collapsed />}
      </div>
    </div>
  );
}
