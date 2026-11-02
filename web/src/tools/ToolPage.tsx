import { Suspense, lazy, useEffect, useMemo, useRef, useState } from 'react';
import {
  Star, Cpu, Server, Globe, Brain, Plug, TriangleAlert, Package, ShieldAlert, LayoutPanelLeft, ListChecks,
} from 'lucide-react';
import type { ToolInfo } from '../lib/types';
import { useApp } from '../lib/app';
import { href, useRoute } from '../lib/router';
import { categoryIcon } from '../lib/icons';
import { favorites as favStore, pushRecent, toggleFavorite, useStore } from '../lib/store';
import { Badge, Callout, CopyButton, Segmented, Spinner, cx } from '../components/ui';
import { NotFound } from '../pages/NotFound';
import { GenericTool } from './GenericTool';

const SPECIAL: Record<string, React.LazyExoticComponent<React.ComponentType<{ tool: ToolInfo; jobId?: string | null }>>> = {
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

  return (
    <div className={cx('page tool-page', tool.ui === 'live' && 'is-live')} ref={root}>
      <ToolHeader tool={tool} />
      <Availability tool={tool} />
      {special && special !== 'system' && (
        <div className="ui-switch">
          <Segmented<'editor' | 'form'>
            size="sm"
            label="Interface"
            value={mode}
            onChange={setMode}
            options={[{ value: 'editor', label: 'Visual', icon: LayoutPanelLeft }, { value: 'form', label: 'All options', icon: ListChecks }]}
          />
        </div>
      )}
      {Special ? (
        <Suspense fallback={<div className="page-loading"><Spinner /></div>}>
          <Special tool={tool} jobId={jobId} />
        </Suspense>
      ) : (
        <GenericTool tool={tool} jobId={jobId} />
      )}
    </div>
  );
}

const EXEC_REQUIRES_LABEL: Record<string, string> = {
  ffmpeg: 'FFmpeg', ffprobe: 'FFprobe', imagemagick: 'ImageMagick', vips: 'libvips', exiftool: 'ExifTool', qpdf: 'qpdf',
  poppler: 'Poppler', ghostscript: 'Ghostscript', tesseract: 'Tesseract', whisper: 'whisper.cpp', piper: 'Piper',
  rembg: 'rembg', realesrgan: 'Real-ESRGAN', libreoffice: 'LibreOffice', pandoc: 'Pandoc', '7zip': '7-Zip', bwrap: 'Bubblewrap',
};

function ToolHeader({ tool }: { tool: ToolInfo }) {
  const { status, isServer, categoryLabel } = useApp();
  const favs = useStore(favStore);
  const fav = favs.includes(tool.id);
  const Icon = categoryIcon(tool.category);
  return (
    <header className="tool-head">
      <nav className="crumbs" aria-label="Breadcrumb">
        <a href={href.category(tool.category)}><Icon size={13} aria-hidden />{categoryLabel(tool.category)}</a>
      </nav>
      <div className="tool-title-row">
        <h1 className="page-title" tabIndex={-1} data-route-focus>{tool.name}</h1>
        <button
          type="button"
          className={cx('star-btn star-lg', fav && 'is-on')}
          aria-pressed={fav}
          aria-label={fav ? 'Remove from favorites' : 'Add to favorites'}
          title={fav ? 'Remove from favorites' : 'Add to favorites'}
          onClick={() => toggleFavorite(tool.id)}
        >
          <Star size={17} aria-hidden fill={fav ? 'currentColor' : 'none'} />
        </button>
      </div>
      <p className="page-sub">{tool.description}</p>
      <div className="tool-badges">
        {isServer
          ? <Badge icon={Server} tone="outline" title="Your files are uploaded to and processed by this server">Processed on {status.instance_name} server</Badge>
          : <Badge icon={Cpu} tone="outline" title="Nothing leaves this computer">Runs on this device</Badge>}
        {tool.permissions.includes('network') && <Badge icon={Globe} tone="outline" title="This tool makes outbound network requests">Uses network</Badge>}
        {tool.execution === 'model' && <Badge icon={Brain} tone="outline" title={`Uses a locally installed ${tool.requires_model ?? ''} model`}>Local model</Badge>}
        {tool.requires.length > 0 && (
          <Badge icon={Plug} tone="outline" title="External program used by this tool">
            External: {tool.requires.map((r) => EXEC_REQUIRES_LABEL[r] ?? r).join(', ')}
          </Badge>
        )}
        {tool.destructive && <Badge icon={ShieldAlert} tone="warn" title="Can modify files outside this job">Modifies files</Badge>}
        <span className="tool-id mono" title="Tool id (CLI / API / MCP)">{tool.id}</span>
      </div>
    </header>
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
