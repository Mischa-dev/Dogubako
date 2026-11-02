import { memo, useMemo } from 'react';
import type { Artifact, JobView } from '../lib/types';
import type { FileItem } from '../lib/schema';
import { useApp } from '../lib/app';
import { kindOf } from '../lib/bench';
import { formatBytes } from '../lib/format';
import { FileThing } from '../components/BenchObject';
import { Spinner, cx } from '../components/ui';

const ext = (name: string) => (name.includes('.') ? name.split('.').pop()!.toUpperCase() : '');

function Media({ name, mime, url, controls }: { name: string; mime: string; url: string; controls?: boolean }) {
  const kind = kindOf(mime, name);
  if (url && kind === 'image') return <img className="stage-img" src={url} alt="" decoding="async" />;
  if (url && kind === 'video') return <video className="stage-video" src={url} controls={controls} muted preload="metadata" playsInline />;
  if (url && kind === 'audio') {
    return (
      <span className="stage-audio">
        <span className="bo-thing"><FileThing kind="audio" name={name} /></span>
        {controls && <audio src={url} controls preload="metadata" />}
      </span>
    );
  }
  return <span className="stage-thing"><span className="bo-thing"><FileThing kind={kind} name={name} url={url} /></span></span>;
}

function delta(before: number, after: number) {
  if (!before || !after || before === after) return '';
  const pct = Math.round(((after - before) / before) * 100);
  if (pct === 0) return '';
  return pct < 0 ? `${pct}%` : `+${pct}%`;
}

/**
 * The file and what became of it, side by side (or just the result).
 * Detail (every output, data, downloads) stays in the result below.
 */
export const Stage = memo(function Stage({ files, job, running, live, mode }: {
  files: FileItem[];
  job: JobView | null;
  running: boolean;
  live: boolean;
  mode: 'side' | 'result';
}) {
  const { backend } = useApp();
  const first = files[0];
  const origUrl = useMemo(() => {
    if (!first) return '';
    try { return first.localUrl ?? (first.ref ? backend.fileUrl({ ref: first.ref, name: first.name, size: first.size, mime: first.mime }) : ''); } catch { return ''; }
  }, [backend, first]);
  const arts: Artifact[] = job?.status === 'succeeded' ? job.result?.artifacts ?? [] : [];
  const out = arts[0];
  const outUrl = useMemo(() => {
    if (!out) return '';
    try { return backend.artifactUrl(out); } catch { return ''; }
  }, [backend, out]);
  if (!first) return null;

  const totalIn = files.reduce((n, f) => n + f.size, 0);
  const totalOut = arts.reduce((n, a) => n + a.size, 0);
  const showOriginal = mode === 'side' || !out;
  const failed = job && (job.status === 'failed' || job.status === 'cancelled');

  return (
    <div className={cx('stage', mode === 'side' ? 'is-side' : 'is-single')}>
      {showOriginal && (
        <figure className="stage-fig">
          <div className="stage-frame"><Media name={first.name} mime={first.mime} url={origUrl} controls /></div>
          <figcaption className="stage-cap">
            <span className="stage-cap-title">{mode === 'side' || !out ? 'Original' : first.name}</span>
            <span className="stage-cap-meta">{[ext(first.name), formatBytes(first.size), files.length > 1 ? `1 of ${files.length}` : ''].filter(Boolean).join(' · ')}</span>
          </figcaption>
        </figure>
      )}
      {(mode === 'side' || out) && (
        <figure className="stage-fig">
          <div className={cx('stage-frame', !out && 'is-empty')}>
            {out ? <Media name={out.name} mime={out.mime} url={outUrl} controls /> : (
              <span className="stage-wait">
                {running ? <><Spinner size={16} /> Working…</> : failed ? 'That didn’t work. The details are below.' : live ? 'The result shows here as you change settings.' : 'Run it to see the result here.'}
              </span>
            )}
          </div>
          <figcaption className="stage-cap">
            <span className="stage-cap-title">Result</span>
            {out && (
              <span className="stage-cap-meta">
                {[ext(out.name), formatBytes(totalOut), arts.length > 1 ? `${arts.length} files` : ''].filter(Boolean).join(' · ')}
                {delta(totalIn, totalOut) && <b className={cx('stage-delta', totalOut < totalIn ? 'is-down' : 'is-up')}> {delta(totalIn, totalOut)}</b>}
              </span>
            )}
          </figcaption>
        </figure>
      )}
    </div>
  );
});
