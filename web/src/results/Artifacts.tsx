import { memo, useEffect, useRef, useState } from 'react';
import { ChevronDown, Download, ExternalLink, FolderOpen, Columns2, Eye, Archive } from 'lucide-react';
import type { Artifact } from '../lib/types';
import { useApp } from '../lib/app';
import { asServer } from '../lib/backend';
import { formatBytes } from '../lib/format';
import { Button, IconButton, Segmented, Spinner, cx } from '../components/ui';
import { fileKindIcon } from '../form/FileField';

const TEXTY = /^(text\/|application\/(json|xml|x-subrip|yaml|toml|javascript|sql|x-ndjson))/;
const MAX_TEXT_PREVIEW = 512 * 1024;

export function previewKind(a: Artifact): 'image' | 'audio' | 'video' | 'pdf' | 'text' | null {
  const m = a.mime.toLowerCase();
  if (m.startsWith('image/')) return 'image';
  if (m.startsWith('audio/')) return 'audio';
  if (m.startsWith('video/')) return 'video';
  if (m === 'application/pdf') return 'pdf';
  if (TEXTY.test(m) && a.size <= MAX_TEXT_PREVIEW) return 'text';
  return null;
}

export const ArtifactList = memo(function ArtifactList({ artifacts, jobId, inputImage }: { artifacts: Artifact[]; jobId?: string; inputImage?: string }) {
  const { backend } = useApp();
  const server = asServer(backend);
  // Preview the first few previewable artifacts by default.
  const autoOpen = artifacts.length <= 4;
  return (
    <div className="artifacts">
      <div className="output-head">
        <span className="output-title">{artifacts.length === 1 ? 'Output file' : `${artifacts.length} output files`}</span>
        {server && jobId && artifacts.length > 1 && (
          <a className="btn btn-secondary btn-sm" href={server.zipUrl(jobId)} download>
            <Archive size={14} aria-hidden /><span>Download all (.zip)</span>
          </a>
        )}
      </div>
      <ul className="artifact-list">
        {artifacts.map((a, i) => (
          <ArtifactItem key={(a.id ?? a.path ?? a.name) + i} a={a} defaultOpen={autoOpen && i < 2} inputImage={artifacts.length === 1 || i === 0 ? inputImage : undefined} />
        ))}
      </ul>
    </div>
  );
});

function ArtifactItem({ a, defaultOpen, inputImage }: { a: Artifact; defaultOpen: boolean; inputImage?: string }) {
  const { backend } = useApp();
  const server = asServer(backend);
  const kind = previewKind(a);
  const [open, setOpen] = useState(defaultOpen && !!kind);
  const Icon = fileKindIcon(a.mime);
  const url = backend.artifactUrl(a);
  return (
    <li className="artifact">
      <div className="artifact-row">
        <span className="artifact-icon" aria-hidden><Icon size={16} /></span>
        <span className="artifact-meta">
          <span className="artifact-name" title={a.name}>{a.name}</span>
          <span className="artifact-sub">{formatBytes(a.size)} · {a.mime}{a.label ? <> · <span className="artifact-label">{a.label}</span></> : null}</span>
        </span>
        <span className="artifact-actions">
          {kind && (
            <IconButton icon={open ? ChevronDown : Eye} label={open ? 'Hide preview' : 'Preview'} onClick={() => setOpen((o) => !o)} aria-expanded={open} />
          )}
          {backend.revealArtifact && a.path && <IconButton icon={FolderOpen} label="Show in folder" onClick={() => void backend.revealArtifact!(a)} />}
          {server ? (
            <a className="btn btn-secondary btn-sm" href={server.downloadUrl(a)} download={a.name}>
              <Download size={14} aria-hidden /><span>Download</span>
            </a>
          ) : (
            <Button size="sm" icon={Download} onClick={() => void backend.downloadArtifact(a)}>Save…</Button>
          )}
        </span>
      </div>
      {open && kind && <Preview a={a} kind={kind} url={url} inputImage={inputImage} />}
    </li>
  );
}

function Preview({ a, kind, url, inputImage }: { a: Artifact; kind: NonNullable<ReturnType<typeof previewKind>>; url: string; inputImage?: string }) {
  const [mode, setMode] = useState<'result' | 'compare'>(inputImage && kind === 'image' ? 'compare' : 'result');
  switch (kind) {
    case 'image':
      return (
        <div className="preview">
          {inputImage && (
            <div className="preview-toolbar">
              <Segmented size="sm" label="Image view" value={mode} onChange={setMode} options={[
                { value: 'compare', label: 'Before / after', icon: Columns2 },
                { value: 'result', label: 'Result' },
              ]} />
            </div>
          )}
          {mode === 'compare' && inputImage ? <BeforeAfter before={inputImage} after={url} /> : (
            <div className="img-preview checker"><img src={url} alt={a.name} /></div>
          )}
        </div>
      );
    case 'audio':
      return <div className="preview"><audio controls src={url} preload="metadata" className="media" aria-label={a.name} /></div>;
    case 'video':
      return <div className="preview"><video controls src={url} preload="metadata" className="media media-video" aria-label={a.name} /></div>;
    case 'pdf':
      return (
        <div className="preview">
          <iframe className="pdf-preview" src={url} title={`PDF preview: ${a.name}`} />
          <a className="link-sm" href={url} target="_blank" rel="noopener noreferrer"><ExternalLink size={12} aria-hidden /> Open in new tab</a>
        </div>
      );
    case 'text':
      return <TextFilePreview url={url} />;
  }
}

function TextFilePreview({ url }: { url: string }) {
  const [text, setText] = useState<string | null>(null);
  const [err, setErr] = useState(false);
  useEffect(() => {
    let alive = true;
    fetch(url, { credentials: 'same-origin' })
      .then((r) => (r.ok ? r.text() : Promise.reject()))
      .then((t) => alive && setText(t))
      .catch(() => alive && setErr(true));
    return () => { alive = false; };
  }, [url]);
  if (err) return <p className="muted small preview">Preview unavailable.</p>;
  if (text == null) return <div className="preview"><Spinner /></div>;
  return <pre className="code-view mono preview-text">{text}</pre>;
}

/** Draggable before/after comparison slider. */
export function BeforeAfter({ before, after }: { before: string; after: string }) {
  const [pos, setPos] = useState(50);
  const stage = useRef<HTMLDivElement>(null);
  const drag = useRef(false);
  const update = (clientX: number) => {
    const r = stage.current?.getBoundingClientRect();
    if (!r) return;
    setPos(Math.max(0, Math.min(100, ((clientX - r.left) / r.width) * 100)));
  };
  return (
    <div
      className="ba checker"
      ref={stage}
      onPointerDown={(e) => { drag.current = true; (e.target as HTMLElement).setPointerCapture?.(e.pointerId); update(e.clientX); }}
      onPointerMove={(e) => drag.current && update(e.clientX)}
      onPointerUp={() => { drag.current = false; }}
    >
      <img src={after} alt="After" className="ba-img" draggable={false} />
      <div className="ba-before" style={{ clipPath: `inset(0 ${100 - pos}% 0 0)` }}>
        <img src={before} alt="Before" className="ba-img" draggable={false} />
      </div>
      <span className="ba-tag ba-tag-l" aria-hidden>Before</span>
      <span className="ba-tag ba-tag-r" aria-hidden>After</span>
      <div className="ba-handle" style={{ left: `${pos}%` }}>
        <input
          type="range"
          min={0}
          max={100}
          value={Math.round(pos)}
          onChange={(e) => setPos(Number(e.target.value))}
          aria-label="Comparison position"
          className={cx('ba-range')}
        />
      </div>
    </div>
  );
}
