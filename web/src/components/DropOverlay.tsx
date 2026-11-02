import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { useApp } from '../lib/app';
import type { Backend, FileRef } from '../lib/backend';
import { href, navigate } from '../lib/router';
import { setDropped } from '../lib/store';
import { setNativeDropFallback } from '../form/FileField';
import { errorText } from '../lib/errors';
import { toast } from './toast';
import { ProgressBar } from './ui';

/* Upload state shared by the overlay and anything that starts an upload (Choose files buttons). */
let uploading: { count: number; fraction: number } | null = null;
const subs = new Set<() => void>();
const emit = () => subs.forEach((s) => s());
const setUploading = (u: typeof uploading) => { uploading = u; emit(); };
let backendRef: Backend | null = null;

/** Show the drop screen for these files. */
export function openFiles(refs: FileRef[]) {
  if (!refs.length) return;
  setDropped(refs.map(({ ref, name, size, mime }) => ({ ref, name, size, mime })));
  navigate(`${href.open}?v=${Date.now().toString(36)}`);
}

async function uploadAndOpen(files: File[]) {
  const backend = backendRef;
  if (!backend || !files.length) return;
  setUploading({ count: files.length, fraction: 0 });
  try {
    const refs = await backend.uploadFiles(files, (fraction) => setUploading({ count: files.length, fraction }));
    openFiles(refs);
  } catch (e) {
    toast.error(errorText(e));
  } finally {
    setUploading(null);
  }
}

/** Ask for files (native picker on desktop, browser picker otherwise), then show the drop screen. */
export async function pickAndOpen() {
  const backend = backendRef;
  if (!backend) return;
  if (backend.pickFiles) {
    const refs = await backend.pickFiles({ multiple: true }).catch((e) => { toast.error(errorText(e)); return null; });
    if (refs) openFiles(refs);
    return;
  }
  const el = document.createElement('input');
  el.type = 'file';
  el.multiple = true;
  el.style.display = 'none';
  el.addEventListener('change', () => { void uploadAndOpen(Array.from(el.files ?? [])); el.remove(); }, { once: true });
  document.body.appendChild(el);
  el.click();
}

const hasFiles = (e: DragEvent) => !!e.dataTransfer && Array.from(e.dataTransfer.types).includes('Files');

/**
 * Drop a file anywhere: a sheet covers the window while dragging, the files upload,
 * and the drop screen asks what to do. Fields that take files handle their own drops
 * first (they call preventDefault), so this only catches drops that land elsewhere.
 */
export function DropOverlay() {
  const { backend } = useApp();
  const [over, setOver] = useState(false);
  const depth = useRef(0);
  const up = useSyncExternalStore((l) => { subs.add(l); return () => { subs.delete(l); }; }, () => uploading);

  useEffect(() => {
    backendRef = backend;
    // Desktop: the OS hands over paths; anything no file field takes opens the drop screen.
    if (backend.onNativeDrop) setNativeDropFallback(backend, openFiles);
    return () => { if (backend.onNativeDrop) setNativeDropFallback(backend, null); };
  }, [backend]);

  useEffect(() => {
    if (backend.onNativeDrop) return; // the desktop webview never sees file drags
    // A page with its own file box (a tool) takes drops there; the sheet would only get in the way.
    const hasField = () => !!document.querySelector('.file-field');
    const enter = (e: DragEvent) => { if (!hasFiles(e)) return; depth.current++; if (!hasField()) setOver(true); };
    const leave = (e: DragEvent) => { if (!hasFiles(e)) return; depth.current = Math.max(0, depth.current - 1); if (!depth.current) setOver(false); };
    const overH = (e: DragEvent) => { if (!hasFiles(e) || e.defaultPrevented) return; e.preventDefault(); if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy'; };
    const drop = (e: DragEvent) => {
      depth.current = 0;
      setOver(false);
      if (!hasFiles(e) || e.defaultPrevented) return;
      e.preventDefault();
      if (hasField()) { toast('Drop it on the file box to use it here'); return; }
      void uploadAndOpen(Array.from(e.dataTransfer?.files ?? []));
    };
    const reset = () => { depth.current = 0; setOver(false); };
    window.addEventListener('dragenter', enter);
    window.addEventListener('dragleave', leave);
    window.addEventListener('dragover', overH);
    window.addEventListener('drop', drop);
    window.addEventListener('dragend', reset);
    return () => {
      window.removeEventListener('dragenter', enter);
      window.removeEventListener('dragleave', leave);
      window.removeEventListener('dragover', overH);
      window.removeEventListener('drop', drop);
      window.removeEventListener('dragend', reset);
    };
  }, [backend]);

  if (!over && !up) return null;
  return createPortal(
    <div className={`drop-overlay${up ? ' is-uploading' : ''}`} aria-live="polite">
      <div className="drop-card">
        {up ? (
          <>
            <p className="drop-title">Bringing in {up.count === 1 ? 'your file' : `${up.count} files`}…</p>
            <ProgressBar value={up.fraction} label="Upload progress" />
          </>
        ) : (
          <>
            <span className="drop-tray" aria-hidden><span className="drop-sheet" /></span>
            <p className="drop-title">Drop to put it on the bench</p>
            <p className="drop-sub">Then choose what should happen to it.</p>
          </>
        )}
      </div>
    </div>,
    document.body,
  );
}
