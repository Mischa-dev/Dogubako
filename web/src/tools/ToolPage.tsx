import { Suspense, lazy, useEffect, useMemo, useRef, useState } from 'react';
import {
  Pin, Cpu, Server, Globe, Brain, Plug, TriangleAlert, Package, ShieldAlert, LayoutPanelLeft, ListChecks,
} from 'lucide-react';
import type { ToolInfo } from '../lib/types';
import { useApp } from '../lib/app';
import { href, useRoute } from '../lib/router';
import { ToolTopRow } from './ToolTopRow';
import { favorites as favStore, pushRecent, toggleFavorite, useStore } from '../lib/store';
import { Callout, CopyButton, Segmented, Spinner, cx } from '../components/ui';
import { NotFound } from '../pages/NotFound';
import { GenericTool } from './GenericTool';

/** Props every special editor gets. The visual editors (crop, redact) lay
 *  themselves out like the generic tool page, so they take the panel head and
 *  footer too; the others keep their own layout under a standalone head. */
export interface SpecialProps { tool: ToolInfo; jobId?: string | null; head?: React.ReactNode; meta?: React.ReactNode }

/** Specials that sit in the shared tool-page layout (stage, settings panel, result). */
const STAGED = new Set(['face-redact', 'crop']);

const SPECIAL: Record<string, React.LazyExoticComponent<React.ComponentType<SpecialProps>>> = {
  'face-redact': lazy(() => import('./special/FaceRedact')),
  crop: lazy(() => import('./special/Crop')),
  regex: lazy(() => import('./special/Regex')),
  qr: lazy(() => import('./special/Qr')),
  color: lazy(() => import('./special/ColorTool')),
  system: lazy(() => import('./special/SystemDash')),
  diff: lazy(() => import('./special/DiffTool')),
};

export function specialUiFor(tool: ToolInfo): string | null {
  if (tool.id === 'image.face_detect') return 'face-redact';
  if (tool.ui && tool.ui in SPECIAL) return tool.ui;
  return null;
}

export default function ToolPage({ toolId }: { toolId: string }) {
  const { toolMap } = useApp();
  const tool = toolMap.get(toolId);
  const route = useRoute();
  const jobId = route.query.get('job');
  useEffect(() => { if (tool) pushRecent(tool.id); }, [tool]);
  if (!tool || tool.permitted === false) return <NotFound />;
  return <ToolPageInner tool={tool} jobId={jobId} />;
}

function ToolPageInner({ tool, jobId }: { tool: ToolInfo; jobId: string | null }) {
  const special = specialUiFor(tool);
  const [mode, setMode] = useState<'editor' | 'form'>('editor');
  const Special = special && mode === 'editor' ? SPECIAL[special] : null;
  const root = useRef<HTMLDivElement>(null);

  const staged = !!special && STAGED.has(special);
  const uiSwitch = staged && (
    <div className="ui-switch">
      <Segmented<'editor' | 'form'>
        size="sm"
        label="Interface"
        value={mode}
        onChange={setMode}
        options={[{ value: 'editor', label: 'Visual', icon: LayoutPanelLeft }, { value: 'form', label: 'All options', icon: ListChecks }]}
      />
    </div>
  );
  const head = <ToolHead tool={tool}>{uiSwitch}</ToolHead>;

  return (
    <div className={cx('page tool-page', tool.ui === 'live' && 'is-live', Special && !staged && 'is-special')} ref={root}>
      <ToolTopRow tool={tool} />
      <Availability tool={tool} />
      {Special && !staged && <ToolHead tool={tool} standalone />}
      {Special ? (
        <Suspense fallback={<div className="page-loading"><Spinner /></div>}>
          {staged ? <Special tool={tool} jobId={jobId} head={head} meta={<ToolMeta tool={tool} />} /> : <Special tool={tool} jobId={jobId} />}
        </Suspense>
      ) : (
        <GenericTool tool={tool} jobId={jobId} head={head} meta={<ToolMeta tool={tool} />} />
      )}
    </div>
  );
}

const EXEC_REQUIRES_LABEL: Record<string, string> = {
  ffmpeg: 'FFmpeg', ffprobe: 'FFprobe', imagemagick: 'ImageMagick', vips: 'libvips', exiftool: 'ExifTool', qpdf: 'qpdf',
  poppler: 'Poppler', ghostscript: 'Ghostscript', tesseract: 'Tesseract', whisper: 'whisper.cpp', piper: 'Piper',
  rembg: 'rembg', realesrgan: 'Real-ESRGAN', libreoffice: 'LibreOffice', pandoc: 'Pandoc', '7zip': '7-Zip', bwrap: 'Bubblewrap',
};

/** Title, pin and one-line purpose, at the top of the settings panel. */
function ToolHead({ tool, standalone, children }: { tool: ToolInfo; standalone?: boolean; children?: React.ReactNode }) {
  const favs = useStore(favStore);
  const fav = favs.includes(tool.id);
  return (
    <header className={cx('tool-head', standalone && 'is-standalone')}>
      <div className="tool-title-row">
        <h1 className="tool-title" tabIndex={-1} data-route-focus>{tool.name}</h1>
        <button
          type="button"
          className={cx('star-btn star-lg', fav && 'is-on')}
          aria-pressed={fav}
          aria-label={fav ? `Unpin ${tool.name}` : `Pin ${tool.name}`}
          title={fav ? 'Unpin' : 'Pin to the sidebar'}
          onClick={() => toggleFavorite(tool.id)}
        >
          <Pin size={15} aria-hidden fill={fav ? 'currentColor' : 'none'} />
        </button>
      </div>
      <p className="tool-desc">{tool.description}</p>
      {children}
    </header>
  );
}

/** Where it runs and what it touches: kept, but quiet, under the run button. */
function ToolMeta({ tool }: { tool: ToolInfo }) {
  const { status, isServer } = useApp();
  const items: { icon: typeof Server; text: string; title: string; warn?: boolean }[] = [
    isServer
      ? { icon: Server, text: `Runs on ${status.instance_name && status.instance_name !== 'Dogubako' ? status.instance_name : 'this server'}`, title: 'Your files are uploaded to and processed by this server' }
      : { icon: Cpu, text: 'Runs on this device', title: 'Nothing leaves this computer' },
  ];
  if (tool.permissions.includes('network')) items.push({ icon: Globe, text: 'Uses network', title: 'This tool makes outbound network requests' });
  if (tool.execution === 'model') items.push({ icon: Brain, text: 'Local model', title: `Uses a locally installed ${tool.requires_model ?? ''} model` });
  if (tool.requires.length) items.push({ icon: Plug, text: tool.requires.map((r) => EXEC_REQUIRES_LABEL[r] ?? r).join(', '), title: 'External program used by this tool' });
  if (tool.destructive) items.push({ icon: ShieldAlert, text: 'Modifies files', title: 'Can modify files outside this job', warn: true });
  return (
    <footer className="tool-meta">
      {items.map(({ icon: I, text, title, warn }) => (
        <span key={text} className={cx('tool-meta-item', warn && 'is-warn')} title={title}><I size={12} aria-hidden />{text}</span>
      ))}
      <span className="tool-id mono" title="Tool id (CLI / API / MCP)">{tool.id}</span>
    </footer>
  );
}

function Availability({ tool }: { tool: ToolInfo }) {
  const a = tool.availability;
  const { isAdmin, isServer } = useApp();
  const hints = useMemo(() => (a.missing ?? []).map((m) => ({ name: m.name, hint: m.hint })), [a.missing]);
  if (a.available) return null;
  if (!a.platform_supported) {
    return <Callout tone="warn" icon={TriangleAlert} title="Not available on this operating system">{a.reason}</Callout>;
  }
  return (
    <Callout
      tone="warn"
      icon={Package}
      title={hints.length ? `Needs ${hints.map((h) => h.name).join(' and ')}` : 'Needs a model'}
      actions={(!isServer || isAdmin) && (
        <a className="btn btn-secondary btn-sm" href={href.settings(a.missing_model && !hints.length ? 'models' : 'dependencies')}>
          {a.missing_model && !hints.length ? 'Models' : 'Dependencies'}
        </a>
      )}
    >
      {hints.map((h) => (
        <div key={h.name} className="hint-line">
          <span>{h.hint}</span>
          <CopyButton text={h.hint} iconOnly label={`Copy install hint for ${h.name}`} />
        </div>
      ))}
      {a.missing_model && <div className="hint-line"><span>{a.missing_model}</span></div>}
      {isServer && !isAdmin && <div className="muted small">Ask an administrator of this instance to install it.</div>}
    </Callout>
  );
}
