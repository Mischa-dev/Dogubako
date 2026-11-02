import { useCallback, useRef, useState } from 'react';
import type { Region } from '../../lib/renderers';
import { cx } from '../../components/ui';

type Handle = 'move' | 'n' | 's' | 'e' | 'w' | 'nw' | 'ne' | 'sw' | 'se' | 'draw';
const HANDLES: Handle[] = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

export interface BoxEditorProps {
  src: string;
  media?: 'image' | 'video';
  boxes: Region[];
  onChange: (b: Region[]) => void;
  selected: number | null;
  onSelect: (i: number | null) => void;
  /** Crop mode: only one box; drawing replaces it; area outside is dimmed. */
  single?: boolean;
  aspect?: number | null;
  natural: { w: number; h: number } | null;
  onNatural: (n: { w: number; h: number }) => void;
  label: string;
  tone?: 'redact' | 'crop';
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

export function normalizeBox(b: Region, W: number, H: number): Region {
  let { x, y, width, height } = b;
  if (width < 0) { x += width; width = -width; }
  if (height < 0) { y += height; height = -height; }
  x = clamp(Math.round(x), 0, W);
  y = clamp(Math.round(y), 0, H);
  width = Math.round(Math.min(width, W - x));
  height = Math.round(Math.min(height, H - y));
  return { ...b, x, y, width, height };
}

export function BoxEditor({ src, media = 'image', boxes, onChange, selected, onSelect, single, aspect, natural, onNatural, label, tone = 'redact' }: BoxEditorProps) {
  const stage = useRef<HTMLDivElement>(null);
  const drag = useRef<{ handle: Handle; idx: number; start: { x: number; y: number }; orig: Region } | null>(null);
  const [dragging, setDragging] = useState(false);

  const toNatural = useCallback((cx: number, cy: number) => {
    const r = stage.current!.getBoundingClientRect();
    const n = natural!;
    return { x: clamp(((cx - r.left) / r.width) * n.w, 0, n.w), y: clamp(((cy - r.top) / r.height) * n.h, 0, n.h) };
  }, [natural]);

  const begin = (e: React.PointerEvent, handle: Handle, idx: number) => {
    if (!natural || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    stage.current?.setPointerCapture(e.pointerId);
    const p = toNatural(e.clientX, e.clientY);
    if (handle === 'draw') {
      const nb: Region = { x: p.x, y: p.y, width: 0, height: 0 };
      const next = single ? [nb] : [...boxes, nb];
      idx = next.length - 1;
      onChange(next);
      onSelect(idx);
      drag.current = { handle: 'se', idx, start: p, orig: nb };
    } else {
      onSelect(idx);
      drag.current = { handle, idx, start: p, orig: { ...boxes[idx] } };
    }
    setDragging(true);
  };

  const move = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d || !natural) return;
    const p = toNatural(e.clientX, e.clientY);
    const dx = p.x - d.start.x;
    const dy = p.y - d.start.y;
    const o = d.orig;
    let { x, y, width, height } = o;
    if (d.handle === 'move') {
      x = clamp(o.x + dx, 0, natural.w - o.width);
      y = clamp(o.y + dy, 0, natural.h - o.height);
    } else {
      let x1 = o.x, y1 = o.y, x2 = o.x + o.width, y2 = o.y + o.height;
      if (d.handle.includes('w')) x1 = o.x + dx;
      if (d.handle.includes('e')) x2 = o.x + o.width + dx;
      if (d.handle.includes('n')) y1 = o.y + dy;
      if (d.handle.includes('s')) y2 = o.y + o.height + dy;
      x1 = clamp(x1, 0, natural.w); x2 = clamp(x2, 0, natural.w);
      y1 = clamp(y1, 0, natural.h); y2 = clamp(y2, 0, natural.h);
      x = Math.min(x1, x2); y = Math.min(y1, y2);
      width = Math.abs(x2 - x1); height = Math.abs(y2 - y1);
      if (aspect) {
        const hw = d.handle.includes('w');
        const hn = d.handle.includes('n');
        const ax = hw ? o.x + o.width : o.x;
        const ay = hn ? o.y + o.height : o.y;
        let w = d.handle === 'n' || d.handle === 's' ? height * aspect : width;
        if (d.handle.length === 2) w = Math.max(width, height * aspect);
        let h = w / aspect;
        const maxW = hw ? ax : natural.w - ax;
        const maxH = hn ? ay : natural.h - ay;
        if (w > maxW) { w = maxW; h = w / aspect; }
        if (h > maxH) { h = maxH; w = h * aspect; }
        width = w; height = h;
        x = hw ? ax - w : ax;
        y = hn ? ay - h : ay;
      }
    }
    const next = boxes.slice();
    next[d.idx] = { ...o, x, y, width, height };
    onChange(next);
  };

  const end = () => {
    const d = drag.current;
    drag.current = null;
    setDragging(false);
    if (!d || !natural) return;
    const next = boxes.map((b) => normalizeBox(b, natural.w, natural.h)).filter((b) => b.width >= 4 && b.height >= 4);
    if (next.length !== boxes.length) onSelect(next.length ? Math.min(d.idx, next.length - 1) : null);
    onChange(next);
  };

  const onBoxKey = (e: React.KeyboardEvent, i: number) => {
    if (!natural) return;
    const step = e.shiftKey ? 10 : 1;
    const b = { ...boxes[i] };
    let handled = true;
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (single) return;
      onChange(boxes.filter((_, k) => k !== i));
      onSelect(null);
      e.preventDefault();
      return;
    }
    if (e.altKey) {
      if (e.key === 'ArrowRight') b.width += step; else if (e.key === 'ArrowLeft') b.width -= step;
      else if (e.key === 'ArrowDown') b.height += step; else if (e.key === 'ArrowUp') b.height -= step;
      else handled = false;
      if (aspect) b.height = b.width / aspect;
    } else {
      if (e.key === 'ArrowRight') b.x += step; else if (e.key === 'ArrowLeft') b.x -= step;
      else if (e.key === 'ArrowDown') b.y += step; else if (e.key === 'ArrowUp') b.y -= step;
      else handled = false;
    }
    if (!handled) return;
    e.preventDefault();
    b.width = clamp(b.width, 4, natural.w); b.height = clamp(b.height, 4, natural.h);
    b.x = clamp(b.x, 0, natural.w - b.width); b.y = clamp(b.y, 0, natural.h - b.height);
    const next = boxes.slice();
    next[i] = normalizeBox(b, natural.w, natural.h);
    onChange(next);
  };

  const pct = (v: number, of: number) => `${(v / of) * 100}%`;
  return (
    <div className={cx('box-editor', `tone-${tone}`, dragging && 'is-dragging')}>
      <div className="box-stage" ref={stage} onPointerMove={move} onPointerUp={end} onPointerCancel={end}>
        {media === 'video' ? (
          <video src={src} muted playsInline preload="metadata" className="box-media"
            onLoadedMetadata={(e) => onNatural({ w: e.currentTarget.videoWidth, h: e.currentTarget.videoHeight })} />
        ) : (
          <img src={src} alt={label} className="box-media" draggable={false}
            onLoad={(e) => onNatural({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })} />
        )}
        {natural && (
          <div className="box-layer" onPointerDown={(e) => begin(e, 'draw', -1)} aria-label={`${label}: drag to draw a ${single ? 'crop area' : 'region'}`}>
            {single && boxes[0] && (
              <svg className="crop-dim" viewBox={`0 0 ${natural.w} ${natural.h}`} preserveAspectRatio="none" aria-hidden>
                <path fillRule="evenodd" d={`M0 0H${natural.w}V${natural.h}H0Z M${boxes[0].x} ${boxes[0].y}h${boxes[0].width}v${boxes[0].height}h${-boxes[0].width}Z`} />
              </svg>
            )}
            {boxes.map((b, i) => (
              <div
                key={i}
                className={cx('box', selected === i && 'is-selected')}
                style={{ left: pct(b.x, natural.w), top: pct(b.y, natural.h), width: pct(b.width, natural.w), height: pct(b.height, natural.h) }}
                onPointerDown={(e) => begin(e, 'move', i)}
                tabIndex={0}
                role="button"
                aria-label={`${single ? 'Crop area' : `Region ${i + 1}`}: ${Math.round(b.x)}, ${Math.round(b.y)}, ${Math.round(b.width)} by ${Math.round(b.height)} pixels. Arrow keys move, Alt+arrows resize${single ? '' : ', Delete removes'}.`}
                aria-pressed={selected === i}
                onFocus={() => onSelect(i)}
                onKeyDown={(e) => onBoxKey(e, i)}
              >
                {!single && <span className="box-num" aria-hidden>{b.label?.replace(/^face /, '') ?? i + 1}</span>}
                {single && <span className="crop-grid" aria-hidden />}
                {selected === i && HANDLES.map((h) => (
                  <span key={h} className={`box-h h-${h}`} onPointerDown={(e) => begin(e, h, i)} aria-hidden />
                ))}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
