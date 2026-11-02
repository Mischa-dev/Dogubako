import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Folder, Upload, Download, Trash2, Eye, RefreshCw, Search, X, ExternalLink } from 'lucide-react';
import type { FileView } from '../lib/types';
import { useApp } from '../lib/app';
import { asServer } from '../lib/backend';
import { formatBytes, formatRelative, formatDateTime } from '../lib/format';
import { Badge, Button, Callout, EmptyState, IconButton, Meter, Modal, PageHeader, ProgressBar, Segmented, Spinner, useConfirm } from '../components/ui';
import { fileKindIcon } from '../form/FileField';
import { previewKind } from '../results/Artifacts';
import { toast } from '../components/toast';
import { errorText } from '../lib/errors';
import { copyText } from '../lib/clipboard';

type Src = 'all' | 'upload' | 'artifact';

export default function FilesPage() {
  const { backend, refreshMe } = useApp();
  const server = asServer(backend)!;
  const [data, setData] = useState<{ files: FileView[]; used: number; quota: number } | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState('');
  const [src, setSrc] = useState<Src>('all');
  const [preview, setPreview] = useState<FileView | null>(null);
  const [upload, setUpload] = useState<number | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const [confirm, confirmNode] = useConfirm();

  const load = useCallback(async () => {
    try { setData(await server.listFiles()); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [server]);
  useEffect(() => { void load(); }, [load]);

  const files = useMemo(() => {
    let f = data?.files ?? [];
    if (src !== 'all') f = f.filter((x) => x.source === src);
    const s = q.trim().toLowerCase();
    if (s) f = f.filter((x) => x.name.toLowerCase().includes(s) || x.mime.includes(s));
    return [...f].sort((a, b) => b.created_at - a.created_at);
  }, [data, q, src]);

  const doUpload = async (list: File[]) => {
    if (!list.length) return;
    setUpload(0);
    try {
      await server.uploadFiles(list, setUpload);
      toast.success(`Uploaded ${list.length} file${list.length === 1 ? '' : 's'}`);
      await load();
      void refreshMe();
    } catch (e) { toast.error(errorText(e)); }
    setUpload(null);
  };

  const remove = async (f: FileView) => {
    if (!(await confirm({ title: `Delete “${f.name}”?`, message: 'This cannot be undone.', confirm: 'Delete', danger: true }))) return;
    try { await server.deleteFile(f.id); await load(); } catch (e) { toast.error(errorText(e)); }
  };

  const pct = data && data.quota ? (data.used / data.quota) * 100 : 0;
  return (
    <div className="page">
      <PageHeader
        icon={Folder}
        title="Files"
        subtitle="Uploads and tool outputs stored for you on this server."
        actions={
          <>
            <Button size="sm" variant="ghost" icon={RefreshCw} onClick={load}>Refresh</Button>
            <Button size="sm" variant="primary" icon={Upload} onClick={() => input.current?.click()} loading={upload != null}>Upload</Button>
            <input ref={input} type="file" multiple hidden onChange={(e) => { const l = Array.from(e.target.files ?? []); e.target.value = ''; void doUpload(l); }} />
          </>
        }
      />
      {data && (
        <div className="quota">
          <div className="quota-head"><span>Storage</span><span className="num">{formatBytes(data.used)} of {formatBytes(data.quota)}</span></div>
          <Meter value={pct} label="Storage used" tone="auto" />
        </div>
      )}
      {upload != null && <ProgressBar value={upload} label="Upload progress" />}
      {err && <Callout tone="danger">{err}</Callout>}
      {!data && !err && <div className="page-loading"><Spinner /></div>}
      {data && data.files.length > 0 && (
        <div className="list-toolbar">
          <div className="input-group input-search">
            <span className="input-prefix" aria-hidden><Search size={14} /></span>
            <input className="input" value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter files" aria-label="Filter files" />
            {q && <button className="input-addon" onClick={() => setQ('')} aria-label="Clear"><X size={14} /></button>}
          </div>
          <Segmented<Src> size="sm" label="Source" value={src} onChange={setSrc} options={[{ value: 'all', label: 'All' }, { value: 'upload', label: 'Uploads' }, { value: 'artifact', label: 'Outputs' }]} />
        </div>
      )}
      {data && data.files.length === 0 && (
        <EmptyState icon={Folder} title="No files yet" action={<Button icon={Upload} onClick={() => input.current?.click()}>Upload files</Button>}>
          Files you upload to tools and the outputs they produce appear here.
        </EmptyState>
      )}
      {files.length > 0 && (
        <div className="table-wrap files-table">
          <table className="table">
            <thead><tr><th scope="col">Name</th><th scope="col">Size</th><th scope="col">Source</th><th scope="col">Added</th><th scope="col"><span className="sr-only">Actions</span></th></tr></thead>
            <tbody>
              {files.map((f) => {
                const Icon = fileKindIcon(f.mime);
                const art = { id: f.id, name: f.name, mime: f.mime, size: f.size };
                return (
                  <tr key={f.id}>
                    <td>
                      <span className="file-cell">
                        <Icon size={15} aria-hidden className="muted" />
                        <span className="file-cell-name" title={f.name}>{f.name}</span>
                        <span className="muted small hide-sm">{f.mime}</span>
                      </span>
                    </td>
                    <td className="num">{formatBytes(f.size)}</td>
                    <td><Badge tone={f.source === 'upload' ? 'neutral' : 'accent'}>{f.source === 'upload' ? 'Upload' : 'Output'}</Badge></td>
                    <td title={formatDateTime(f.created_at)}>{formatRelative(f.created_at)}{f.expires_at ? <span className="muted small"> · expires {formatRelative(f.expires_at)}</span> : null}</td>
                    <td className="row-actions">
                      {previewKind(art) && <IconButton icon={Eye} label={`Preview ${f.name}`} onClick={() => setPreview(f)} />}
                      <IconButton icon={ExternalLink} label="Copy file id (use as tool input via API)" onClick={async () => { if (await copyText(f.id)) toast.success(`Copied ${f.id}`); }} />
                      <a className="icon-btn" href={server.downloadUrl(art)} download={f.name} aria-label={`Download ${f.name}`} title="Download"><Download size={16} aria-hidden /></a>
                      <IconButton icon={Trash2} label={`Delete ${f.name}`} onClick={() => void remove(f)} />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <Modal open={!!preview} onClose={() => setPreview(null)} title={preview?.name} width={860}>
        {preview && <FilePreview f={preview} />}
      </Modal>
      {confirmNode}
    </div>
  );
}

function FilePreview({ f }: { f: FileView }) {
  const { backend } = useApp();
  const url = backend.artifactUrl({ id: f.id, name: f.name, mime: f.mime, size: f.size });
  const k = previewKind({ name: f.name, mime: f.mime, size: f.size });
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    if (k === 'text') fetch(url, { credentials: 'same-origin' }).then((r) => r.text()).then(setText).catch(() => setText('(preview unavailable)'));
  }, [k, url]);
  if (k === 'image') return <div className="img-preview checker"><img src={url} alt={f.name} /></div>;
  if (k === 'audio') return <audio controls src={url} className="media" />;
  if (k === 'video') return <video controls src={url} className="media media-video" />;
  if (k === 'pdf') return <iframe className="pdf-preview" src={url} title={f.name} />;
  if (k === 'text') return text == null ? <Spinner /> : <pre className="code-view mono preview-text">{text}</pre>;
  return <p className="muted">No preview for this file type.</p>;
}
