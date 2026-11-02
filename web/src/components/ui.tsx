import {
  forwardRef, useCallback, useEffect, useId, useLayoutEffect, useRef, useState,
  type ButtonHTMLAttributes, type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { Check, Copy, X, type LucideIcon } from 'lucide-react';
import { copyText } from '../lib/clipboard';

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(' ');
export { cx };

type BtnVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'subtle';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: BtnVariant;
  size?: 'sm' | 'md';
  icon?: LucideIcon;
  iconRight?: LucideIcon;
  loading?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'secondary', size = 'md', icon: Icon, iconRight: IconR, loading, className, children, disabled, type = 'button', ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cx('btn', `btn-${variant}`, size === 'sm' && 'btn-sm', className)}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      {...rest}
    >
      {loading ? <Spinner size={14} /> : Icon ? <Icon size={size === 'sm' ? 14 : 15} aria-hidden /> : null}
      {children != null && <span>{children}</span>}
      {IconR && <IconR size={14} aria-hidden />}
    </button>
  );
});

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: LucideIcon;
  label: string;
  size?: number;
  active?: boolean;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { icon: Icon, label, size = 16, active, className, type = 'button', ...rest },
  ref,
) {
  return (
    <button ref={ref} type={type} className={cx('icon-btn', active && 'is-active', className)} aria-label={label} title={label} {...rest}>
      <Icon size={size} aria-hidden />
    </button>
  );
});

export function Spinner({ size = 16 }: { size?: number }) {
  return <span className="spinner" style={{ width: size, height: size }} aria-hidden />;
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
export const modKey = isMac ? '⌘' : 'Ctrl';

export function Badge({ tone = 'neutral', icon: Icon, children, title }: {
  tone?: 'neutral' | 'accent' | 'warn' | 'danger' | 'success' | 'outline';
  icon?: LucideIcon;
  children: ReactNode;
  title?: string;
}) {
  return (
    <span className={cx('badge', `badge-${tone}`)} title={title}>
      {Icon && <Icon size={12} aria-hidden />}
      {children}
    </span>
  );
}

export function Switch({ checked, onChange, id, disabled, label, describedBy }: {
  checked: boolean;
  onChange: (v: boolean) => void;
  id?: string;
  disabled?: boolean;
  label?: string;
  describedBy?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      id={id}
      aria-checked={checked}
      aria-label={label}
      aria-describedby={describedBy}
      disabled={disabled}
      className={cx('switch', checked && 'is-on')}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-thumb" />
    </button>
  );
}

/** Radio-group styled as a segmented control, with arrow-key navigation. */
export function Segmented<T extends string>({ value, options, onChange, label, id, size }: {
  value: T;
  options: { value: T; label: ReactNode; icon?: LucideIcon; title?: string }[];
  onChange: (v: T) => void;
  label?: string;
  id?: string;
  size?: 'sm';
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const idx = Math.max(0, options.findIndex((o) => o.value === value));
  const onKey = (e: React.KeyboardEvent) => {
    let n = -1;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') n = (idx + 1) % options.length;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') n = (idx - 1 + options.length) % options.length;
    if (e.key === 'Home') n = 0;
    if (e.key === 'End') n = options.length - 1;
    if (n >= 0) {
      e.preventDefault();
      onChange(options[n].value);
      refs.current[n]?.focus();
    }
  };
  return (
    <div className={cx('segmented', size === 'sm' && 'segmented-sm')} role="radiogroup" aria-label={label} id={id} onKeyDown={onKey}>
      {options.map((o, i) => {
        const Icon = o.icon;
        const on = o.value === value;
        return (
          <button
            key={o.value}
            ref={(el) => { refs.current[i] = el; }}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on || (idx === -1 && i === 0) ? 0 : -1}
            className={cx('segment', on && 'is-on')}
            onClick={() => onChange(o.value)}
            title={o.title}
          >
            {Icon && <Icon size={14} aria-hidden />}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

export function CopyButton({ text, label = 'Copy', size = 'sm', variant = 'ghost', iconOnly, className }: {
  text: string | (() => string);
  label?: string;
  size?: 'sm' | 'md';
  variant?: BtnVariant;
  iconOnly?: boolean;
  className?: string;
}) {
  const [done, setDone] = useState(false);
  const t = useRef<number>();
  useEffect(() => () => window.clearTimeout(t.current), []);
  const onClick = async () => {
    const ok = await copyText(typeof text === 'function' ? text() : text);
    if (ok) {
      setDone(true);
      window.clearTimeout(t.current);
      t.current = window.setTimeout(() => setDone(false), 1400);
    }
  };
  if (iconOnly) {
    return <IconButton icon={done ? Check : Copy} label={done ? 'Copied' : label} onClick={onClick} className={cx(done && 'is-done', className)} size={14} />;
  }
  return (
    <Button size={size} variant={variant} icon={done ? Check : Copy} onClick={onClick} className={className} aria-live="polite">
      {done ? 'Copied' : label}
    </Button>
  );
}

export function ProgressBar({ value, label, indeterminate }: { value?: number; label?: string; indeterminate?: boolean }) {
  const pct = value == null ? undefined : Math.round(Math.max(0, Math.min(1, value)) * 100);
  return (
    <div
      className={cx('progress', (indeterminate || pct == null) && 'is-indeterminate')}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct}
    >
      <div className="progress-fill" style={pct != null ? { width: `${pct}%` } : undefined} />
    </div>
  );
}

export function Meter({ value, label, tone }: { value: number; label: string; tone?: 'auto' }) {
  const pct = Math.max(0, Math.min(100, value));
  const level = tone === 'auto' ? (pct >= 90 ? 'danger' : pct >= 75 ? 'warn' : 'ok') : 'ok';
  return (
    <div className={cx('meter', `meter-${level}`)} role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
      <div className="meter-fill" style={{ width: `${pct}%` }} />
    </div>
  );
}

// ---------------------------------------------------------------- dialogs
const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

export function Modal({ open, onClose, title, children, footer, width = 520, description }: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const prevFocus = useRef<Element | null>(null);
  useLayoutEffect(() => {
    if (!open) return;
    prevFocus.current = document.activeElement;
    const el = ref.current;
    const first = el?.querySelector<HTMLElement>('[data-autofocus]') ?? el?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    return () => { (prevFocus.current as HTMLElement | null)?.focus?.(); };
  }, [open]);
  if (!open) return null;
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') { e.stopPropagation(); onClose(); }
    if (e.key === 'Tab' && ref.current) {
      const items = Array.from(ref.current.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (!items.length) return;
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  };
  return createPortal(
    <div className="overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal" role="dialog" aria-modal="true" aria-labelledby={titleId} ref={ref} style={{ maxWidth: width }} onKeyDown={onKey}>
        <header className="modal-head">
          <div>
            <h2 id={titleId} className="modal-title">{title}</h2>
            {description && <p className="modal-desc">{description}</p>}
          </div>
          <IconButton icon={X} label="Close" onClick={onClose} />
        </header>
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-foot">{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}

export function useConfirm() {
  const [state, setState] = useState<{ title: string; message?: ReactNode; confirm: string; danger?: boolean; resolve: (v: boolean) => void } | null>(null);
  const ask = useCallback((o: { title: string; message?: ReactNode; confirm?: string; danger?: boolean }) =>
    new Promise<boolean>((resolve) => setState({ confirm: 'Confirm', ...o, resolve })), []);
  const close = (v: boolean) => { state?.resolve(v); setState(null); };
  const node = (
    <Modal
      open={!!state}
      onClose={() => close(false)}
      title={state?.title}
      width={420}
      footer={
        <>
          <Button variant="ghost" onClick={() => close(false)}>Cancel</Button>
          <Button variant={state?.danger ? 'danger' : 'primary'} onClick={() => close(true)} data-autofocus>{state?.confirm}</Button>
        </>
      }
    >
      {state?.message && <p className="muted">{state.message}</p>}
    </Modal>
  );
  return [ask, node] as const;
}

// ---------------------------------------------------------------- menu
export function Menu({ trigger, children, align = 'start', label }: {
  trigger: (p: { ref: React.Ref<HTMLButtonElement>; onClick: () => void; 'aria-expanded': boolean; 'aria-haspopup': 'menu' }) => ReactNode;
  children: (close: () => void) => ReactNode;
  align?: 'start' | 'end';
  label: string;
}) {
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top?: number; bottom?: number; minWidth: number }>({ left: 0, top: 0, minWidth: 180 });
  const close = useCallback(() => { setOpen(false); btn.current?.focus(); }, []);
  useLayoutEffect(() => {
    if (!open || !btn.current) return;
    const r = btn.current.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    const left = align === 'end' ? Math.max(8, r.right - 220) : Math.max(8, Math.min(r.left, window.innerWidth - 228));
    if (below < 260) setPos({ left, bottom: window.innerHeight - r.top + 6, minWidth: Math.max(180, r.width) });
    else setPos({ left, top: r.bottom + 6, minWidth: Math.max(180, r.width) });
    requestAnimationFrame(() => menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus());
  }, [open, align]);
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => {
      if (!menu.current?.contains(e.target as Node) && !btn.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', h);
    return () => document.removeEventListener('mousedown', h);
  }, [open]);
  const onKey = (e: React.KeyboardEvent) => {
    const items = Array.from(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? []);
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'Escape') { e.preventDefault(); close(); }
    else if (e.key === 'ArrowDown') { e.preventDefault(); items[(i + 1) % items.length]?.focus(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); items[(i - 1 + items.length) % items.length]?.focus(); }
    else if (e.key === 'Tab') setOpen(false);
  };
  return (
    <>
      {trigger({ ref: btn, onClick: () => setOpen((o) => !o), 'aria-expanded': open, 'aria-haspopup': 'menu' })}
      {open && createPortal(
        <div className="menu" role="menu" aria-label={label} ref={menu} style={pos} onKeyDown={onKey}>
          {children(close)}
        </div>,
        document.body,
      )}
    </>
  );
}

export function MenuItem({ icon: Icon, children, onSelect, danger }: { icon?: LucideIcon; children: ReactNode; onSelect: () => void; danger?: boolean }) {
  return (
    <button type="button" role="menuitem" tabIndex={-1} className={cx('menu-item', danger && 'is-danger')} onClick={onSelect}>
      {Icon && <Icon size={15} aria-hidden />}
      <span>{children}</span>
    </button>
  );
}

export function EmptyState({ icon: Icon, title, children, action }: { icon?: LucideIcon; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      {Icon && <Icon size={20} aria-hidden className="empty-icon" />}
      <p className="empty-title">{title}</p>
      {children && <p className="empty-text">{children}</p>}
      {action}
    </div>
  );
}

export function Highlight({ text, ranges }: { text: string; ranges: [number, number][] }) {
  if (!ranges.length) return <>{text}</>;
  const out: ReactNode[] = [];
  let i = 0;
  ranges.forEach(([s, e], k) => {
    if (s > i) out.push(text.slice(i, s));
    out.push(<mark key={k}>{text.slice(s, e)}</mark>);
    i = e;
  });
  if (i < text.length) out.push(text.slice(i));
  return <>{out}</>;
}

export function PageHeader({ title, subtitle, icon: Icon, actions, eyebrow }: {
  title: ReactNode; subtitle?: ReactNode; icon?: LucideIcon; actions?: ReactNode; eyebrow?: ReactNode;
}) {
  return (
    <header className="page-head">
      <div className="page-head-main">
        {eyebrow && <div className="eyebrow">{eyebrow}</div>}
        <h1 className="page-title" tabIndex={-1} data-route-focus>
          {Icon && <Icon size={20} aria-hidden className="page-title-icon" />}
          {title}
        </h1>
        {subtitle && <p className="page-sub">{subtitle}</p>}
      </div>
      {actions && <div className="page-head-actions">{actions}</div>}
    </header>
  );
}

export function Section({ title, actions, children, id, description }: { title?: ReactNode; actions?: ReactNode; children: ReactNode; id?: string; description?: ReactNode }) {
  return (
    <section className="section" aria-labelledby={id ? `${id}-h` : undefined}>
      {(title || actions) && (
        <div className="section-head">
          <div>
            {title && <h2 className="section-title" id={id ? `${id}-h` : undefined}>{title}</h2>}
            {description && <p className="section-desc">{description}</p>}
          </div>
          {actions && <div className="section-actions">{actions}</div>}
        </div>
      )}
      {children}
    </section>
  );
}

export function Callout({ tone = 'info', icon: Icon, title, children, actions }: {
  tone?: 'info' | 'warn' | 'danger' | 'success';
  icon?: LucideIcon;
  title?: ReactNode;
  children?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className={cx('callout', `callout-${tone}`)} role={tone === 'danger' ? 'alert' : undefined}>
      {Icon && <Icon size={16} aria-hidden className="callout-icon" />}
      <div className="callout-body">
        {title && <div className="callout-title">{title}</div>}
        {children && <div className="callout-text">{children}</div>}
      </div>
      {actions && <div className="callout-actions">{actions}</div>}
    </div>
  );
}
