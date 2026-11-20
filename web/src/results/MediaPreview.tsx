import { useEffect, useState, type ImgHTMLAttributes } from 'react';
import { stillFormat, stillToObjectUrl } from '../lib/stillImage';

/**
 * `src` for an image an <img> can show: the file itself, or a PNG redraw for TIFF, TGA,
 * QOI and Netpbm. `failed` is set when the file can't be shown here.
 */
export function useStillImage(url: string | undefined, mime: string, name: string): { src?: string; failed: boolean } {
  const format = url ? stillFormat(mime, name) : null;
  const [state, setState] = useState<{ key: string; src?: string; failed: boolean }>({ key: '', failed: false });
  const key = `${format}:${url}`;
  useEffect(() => {
    if (!url || !format) return;
    let alive = true;
    let made: string | undefined;
    setState({ key, failed: false });
    stillToObjectUrl(url, format)
      .then((u) => { made = u; if (alive) setState({ key, src: u, failed: false }); else URL.revokeObjectURL(u); })
      .catch(() => alive && setState({ key, failed: true }));
    return () => { alive = false; if (made) URL.revokeObjectURL(made); };
  }, [url, format, key]);
  if (!url) return { failed: false };
  if (!format) return { src: url, failed: false };
  return state.key === key ? { src: state.src, failed: state.failed } : { failed: false };
}

export const PREVIEW_UNSUPPORTED = 'Preview not supported for this format. Download the file to view it.';

type StillProps = { url: string; mime: string; name: string; fallback?: React.ReactNode } & Omit<ImgHTMLAttributes<HTMLImageElement>, 'src'>;

/** An image preview that never shows a broken-image icon. */
export function StillImage({ url, mime, name, fallback, ...img }: StillProps) {
  const { src, failed } = useStillImage(url, mime, name);
  const [broken, setBroken] = useState<string | null>(null);
  if (failed || (src && broken === src)) return <>{fallback ?? <p className="muted small preview-note">{PREVIEW_UNSUPPORTED}</p>}</>;
  if (!src) return null;
  return <img {...img} src={src} onError={(e) => { setBroken(src); img.onError?.(e); }} />;
}

const NO_PLAYBACK = 'Playback isn’t available here: there may be no audio output device, or the built-in player can’t play this format. The file itself may be fine; download it to play it.';

/** <audio>/<video> that replaces the player's bare "Error" with an explanation. */
export function PlayableMedia({ kind, url, name, className, ...rest }: { kind: 'audio' | 'video'; url: string; name: string; className?: string; muted?: boolean; playsInline?: boolean; controls?: boolean }) {
  const [failed, setFailed] = useState<string | null>(null);
  if (failed === url) return <p className="muted small preview-note" role="status">{NO_PLAYBACK}</p>;
  const props = { src: url, preload: 'metadata' as const, className, 'aria-label': name, onError: () => setFailed(url), ...rest };
  return kind === 'audio' ? <audio {...props} /> : <video {...props} />;
}
