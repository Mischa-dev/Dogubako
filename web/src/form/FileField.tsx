import { memo, useEffect, useRef, useState } from 'react';
import { FileText, FileUp, Film, Image as ImageIcon, Music, RotateCcw, X, ClipboardPaste } from 'lucide-react';
import type { ParamSpec } from '../lib/types';
import type { FileItem } from '../lib/schema';
import { fileItemFromRef } from '../lib/schema';
import { useApp } from '../lib/app';
import { formatBytes } from '../lib/format';
import { errorText } from '../lib/errors';
import type { Backend, FileRef } from '../lib/backend';
import { cx } from '../components/ui';
import type { Setter } from './Field';

let keySeq = 0;
const newKey = () => `fi_${Date.now().toString(36)}_${++keySeq}`;

export function acceptMatches(accept: string[] | undefined, name: string, mime: string): boolean {
  if (!accept || accept.length === 0) return true;
  const n = name.toLowerCase();
  const m = (mime || '').toLowerCase();
  return accept.some((a) => {
    const x = a.toLowerCase().trim();
    if (x === '*' || x === '*/*') return true;
    if (x.startsWith('.')) return n.endsWith(x);
    if (x.endsWith('/*')) return m.startsWith(x.slice(0, -1));
    return m === x;
  });
}

export function fileKindIcon(mime: string) {
  if (mime.startsWith('image/')) return ImageIcon;
  if (mime.startsWith('video/')) return Film;
  if (mime.startsWith('audio/')) return Music;
  return FileText;
}

function acceptSummary(accept?: string[]): string {
  if (!accept?.length) return 'Any file';
  const groups = accept.filter((a) => a.endsWith('/*')).map((a) => a.slice(0, -2));
  const exts = accept.filter((a) => a.startsWith('.')).map((a) => a.slice(1).toUpperCase());
  const named = groups.map((g) => ({ image: 'Images', video: 'Video', audio: 'Audio', text: 'Text' } as Record<string, string>)[g] ?? g);
  if (named.length) return named.join(', ') + (exts.length && !groups.length ? '' : '');
  return exts.slice(0, 6).join(', ') + (exts.length > 6 ? ` +${exts.length - 6}` : '');
}

/** Shared add logic: used by browse, drop and paste. */
export function useFileAdder(param: ParamSpec & { type: 'file' }, onChange: Setter) {
  const { backend } = useApp();
  return async (files: File[]) => {
    const multiple = !!param.multiple;
    const accepted = files.filter((f) => acceptMatches(param.accept, f.name, f.type));
    const rejected = files.length - accepted.length;
    const list = multiple ? accepted : accepted.slice(0, 1);
    if (!list.length) return rejected;
    const items: FileItem[] = list.map((f) => ({
      key: newKey(),
      name: f.name || 'pasted',
      size: f.size,
      mime: f.type || 'application/octet-stream',
      status: 'uploading',
      progress: 0,
      localUrl: f.type.startsWith('image/') ? URL.createObjectURL(f) : undefined,
    }));
    const keys = new Set(items.map((i) => i.key));
    onChange(param.name, (prev: unknown) => {
      const cur = (prev as FileItem[]) ?? [];
      if (!multiple) cur.forEach((i) => i.localUrl && URL.revokeObjectURL(i.localUrl));
      return multiple ? [...cur, ...items] : items;
    });
    const update = (fn: (i: FileItem, idx: number) => FileItem) =>
      onChange(param.name, (prev: unknown) => {
        let idx = -1;
        return ((prev as FileItem[]) ?? []).map((i) => (keys.has(i.key) ? fn(i, ++idx) : i));
      });
    try {
      const refs = await backend.uploadFiles(list, (fr) => update((i) => ({ ...i, progress: fr })));
      update((i, idx) => (refs[idx] ? { ...i, ref: refs[idx].ref, size: refs[idx].size || i.size, mime: refs[idx].mime || i.mime, status: 'ready', progress: 1 } : i));
    } catch (e) {
      update((i) => ({ ...i, status: 'error', error: errorText(e) }));
    }
    return rejected;
  };
}

interface DropTarget { accept?: string[]; deliver: (refs: FileRef[]) => void }
const dropTargets: DropTarget[] = [];
let dropUnlisten: Promise<() => void> | null = null;

function registerDropTarget(backend: Backend, t: DropTarget) {
  dropTargets.push(t);
  if (!dropUnlisten && backend.onNativeDrop) {
    dropUnlisten = backend.onNativeDrop((refs) => {
      for (const target of dropTargets) {
        const ok = refs.filter((r) => acceptMatches(target.accept, r.name, r.mime));
        if (ok.length) { target.deliver(ok); return; }
      }
    });
  }
  return () => {
    const i = dropTargets.indexOf(t);
    if (i >= 0) dropTargets.splice(i, 1);
    if (!dropTargets.length && dropUnlisten) { const u = dropUnlisten; dropUnlisten = null; void u.then((f) => f()); }
  };
}

export const FileField = memo(function FileField({ param, items, onChange, labelledBy, describedBy, invalid }: {
  param: ParamSpec & { type: 'file' };
  items: FileItem[];
  onChange: Setter;
  labelledBy: string;
  describedBy?: string;
  invalid?: boolean;
}) {
  const { backend } = useApp();
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const add = useFileAdder(param, onChange);
  const multiple = !!param.multiple;

  const addFiles = async (files: File[]) => {
    setNote(null);
    const rejected = await add(files);
    if (rejected) setNote(`${rejected} file${rejected > 1 ? 's' : ''} skipped — not an accepted type.`);
  };

  const remove = (key: string) =>
    onChange(param.name, (prev: unknown) => ((prev as FileItem[]) ?? []).filter((i) => {
      if (i.key === key && i.localUrl) URL.revokeObjectURL(i.localUrl);
      return i.key !== key;
    }));

  const browse = async () => {
    if (backend.pickFiles) {
      const refs = await backend.pickFiles({ multiple, accept: param.accept });
      if (refs?.length) {
        const its = refs.map(fileItemFromRef);
        onChange(param.name, (prev: unknown) => (multiple ? [...((prev as FileItem[]) ?? []), ...its] : its.slice(0, 1)));
      }
      return;
    }
    input.current?.click();
  };

  const onPaste = async (e: React.ClipboardEvent) => {
    const files = Array.from(e.clipboardData.files);
    if (files.length) { e.preventDefault(); await addFiles(files); return; }
    const text = e.clipboardData.getData('text/plain');
    if (text && acceptMatches(param.accept, 'pasted.txt', 'text/plain')) {
      e.preventDefault();
      await addFiles([new File([text], 'pasted.txt', { type: 'text/plain' })]);
    }
  };

  const pasteFromClipboard = async () => {
    try {
      const its = await navigator.clipboard.read();
      const files: File[] = [];
      for (const it of its) {
        const type = it.types.find((t) => t.startsWith('image/')) ?? (it.types.includes('text/plain') ? 'text/plain' : null);
        if (!type) continue;
        const blob = await it.getType(type);
        const ext = type === 'text/plain' ? 'txt' : type.split('/')[1].replace('jpeg', 'jpg');
        files.push(new File([blob], `pasted.${ext}`, { type }));
      }
      if (files.length) await addFiles(files);
      else setNote('Clipboard has no image or text.');
    } catch {
      setNote(`Press ${navigator.platform.includes('Mac') ? '⌘' : 'Ctrl'}+V while the drop zone is focused to paste.`);
    }
  };

  // Desktop: OS-level drag & drop delivers paths to the first mounted file field that accepts them.
  useEffect(() => {
    if (!backend.onNativeDrop) return;
    return registerDropTarget(backend, {
      accept: param.accept,
      deliver: (refs) => {
        const its = refs.map(fileItemFromRef);
        onChange(param.name, (prev: unknown) => (multiple ? [...((prev as FileItem[]) ?? []), ...its] : its.slice(0, 1)));
      },
    });
  }, [backend, param.accept, param.name, multiple, onChange]);

  const showZone = multiple || items.length === 0;
  const canPasteImages = !param.accept?.length || param.accept.some((a) => a.startsWith('image/') || a === 'text/plain' || a.startsWith('text/'));

  return (
    <div className="file-field" onPaste={onPaste}>
      {showZone && (
        <div
          className={cx('dropzone', over && 'is-over', invalid && 'is-invalid', items.length > 0 && 'is-compact')}
          role="button"
          tabIndex={0}
          aria-labelledby={labelledBy}
          aria-describedby={describedBy}
          onClick={browse}
          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); void browse(); } }}
          onDragEnter={(e) => { e.preventDefault(); setOver(true); }}
          onDragOver={(e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; }}
          onDragLeave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) setOver(false); }}
          onDrop={(e) => { e.preventDefault(); setOver(false); void addFiles(Array.from(e.dataTransfer.files)); }}
        >
          <FileUp size={items.length ? 16 : 20} aria-hidden className="dropzone-icon" />
          <div className="dropzone-text">
            <span className="dropzone-title">
              {items.length ? (multiple ? 'Add more files' : 'Replace file') : <>Drop {multiple ? 'files' : 'a file'} here or <u>browse</u></>}
            </span>
            {!items.length && <span className="dropzone-sub">{acceptSummary(param.accept)}{multiple ? ' · multiple allowed' : ''} · paste with {navigator.platform.includes('Mac') ? '⌘' : 'Ctrl'}+V</span>}
          </div>
          {canPasteImages && !items.length && typeof navigator.clipboard?.read === 'function' && (
            <button type="button" className="btn btn-ghost btn-sm dropzone-paste" onClick={(e) => { e.stopPropagation(); void pasteFromClipboard(); }}>
              <ClipboardPaste size={14} aria-hidden /><span>Paste</span>
            </button>
          )}
          <input
            ref={input}
            type="file"
            hidden
            multiple={multiple}
            accept={param.accept?.join(',')}
            onChange={(e) => { const f = Array.from(e.target.files ?? []); e.target.value = ''; void addFiles(f); }}
          />
        </div>
      )}
      {note && <p className="field-note" role="status">{note}</p>}
      {items.length > 0 && (
        <ul className="file-items" aria-label="Selected files">
          {items.map((it) => <FileChip key={it.key} item={it} onRemove={() => remove(it.key)} onReplace={!multiple ? browse : undefined} />)}
        </ul>
      )}
    </div>
  );
});

function FileChip({ item, onRemove, onReplace }: { item: FileItem; onRemove: () => void; onReplace?: () => void }) {
  const { backend } = useApp();
  const Icon = fileKindIcon(item.mime);
  const thumb = item.mime.startsWith('image/')
    ? item.localUrl ?? (item.ref ? backend.fileUrl({ ref: item.ref, name: item.name, size: item.size, mime: item.mime }) : undefined)
    : undefined;
  return (
    <li className={cx('file-item', item.status === 'error' && 'is-error')}>
      <span className="file-thumb" aria-hidden>
        {thumb ? <img src={thumb} alt="" loading="lazy" /> : <Icon size={16} />}
      </span>
      <span className="file-meta">
        <span className="file-name" title={item.name}>{item.name}</span>
        <span className="file-sub">
          {item.status === 'uploading' ? `Uploading… ${Math.round((item.progress ?? 0) * 100)}%` : item.status === 'error' ? item.error ?? 'Upload failed' : formatBytes(item.size)}
        </span>
        {item.status === 'uploading' && (
          <span className="file-progress" role="progressbar" aria-label={`Uploading ${item.name}`} aria-valuenow={Math.round((item.progress ?? 0) * 100)} aria-valuemin={0} aria-valuemax={100}>
            <span style={{ width: `${Math.round((item.progress ?? 0) * 100)}%` }} />
          </span>
        )}
      </span>
      {onReplace && (
        <button type="button" className="icon-btn" onClick={onReplace} aria-label={`Replace ${item.name}`} title="Replace"><RotateCcw size={14} aria-hidden /></button>
      )}
      <button type="button" className="icon-btn" onClick={onRemove} aria-label={`Remove ${item.name}`} title="Remove"><X size={14} aria-hidden /></button>
    </li>
  );
}
