import { memo, useId, useState } from 'react';
import { Eye, EyeOff, FolderOpen } from 'lucide-react';
import type { ParamSpec } from '../lib/types';
import { isSliderRange, type FileItem } from '../lib/schema';
import { Segmented, Switch, cx } from '../components/ui';
import { FileField } from './FileField';
import { useApp } from '../lib/app';

export type Setter = (name: string, v: unknown | ((prev: unknown) => unknown)) => void;

export interface FieldProps {
  param: ParamSpec;
  value: unknown;
  error?: string;
  onChange: Setter;
  /** Use mono font for text editors (dev/data tools). */
  mono?: boolean;
  compact?: boolean;
}

function FieldShell({ param, id, error, children, inline, extra, descId }: {
  param: ParamSpec; id: string; error?: string; children: React.ReactNode; inline?: boolean; extra?: React.ReactNode; descId: string;
}) {
  const asGroup = param.type === 'enum' || param.type === 'file';
  return (
    <div className={cx('field', inline && 'field-inline', error && 'has-error')} data-param={param.name}>
      <div className="field-head">
        {asGroup ? (
          <span className="field-label" id={`${id}-label`}>
            {param.label}
            {param.required && <span className="req" aria-hidden> *</span>}
          </span>
        ) : (
          <label className="field-label" htmlFor={id} id={`${id}-label`}>
            {param.label}
            {param.required && <span className="req" aria-hidden> *</span>}
          </label>
        )}
        {extra}
      </div>
      {inline ? (
        <>
          {children}
          {param.description && <p className="field-desc field-desc-inline" id={descId}>{param.description}</p>}
        </>
      ) : (
        <>
          {children}
          {param.description && <p className="field-desc" id={descId}>{param.description}</p>}
        </>
      )}
      {error && <p className="field-error" id={`${id}-err`} role="alert">{error}</p>}
    </div>
  );
}

function toHex6(v: string): string {
  let s = v.trim().replace(/^#/, '');
  if (/^[0-9a-f]{3,4}$/i.test(s)) s = s.slice(0, 3).split('').map((c) => c + c).join('');
  if (/^[0-9a-f]{8}$/i.test(s)) s = s.slice(0, 6);
  return /^[0-9a-f]{6}$/i.test(s) ? `#${s.toLowerCase()}` : '#000000';
}

export const Field = memo(function Field({ param: p, value, error, onChange, mono }: FieldProps) {
  const id = `f-${useId().replace(/:/g, '')}-${p.name}`;
  const descId = `${id}-desc`;
  const describedBy = [p.description ? descId : '', error ? `${id}-err` : ''].filter(Boolean).join(' ') || undefined;
  const set = (v: unknown) => onChange(p.name, v);
  const common = { id, 'aria-describedby': describedBy, 'aria-invalid': error ? true : undefined, 'aria-required': p.required || undefined };
  const [reveal, setReveal] = useState(false);
  const { backend } = useApp();

  switch (p.type) {
    case 'boolean':
      return (
        <div className={cx('field field-switch', error && 'has-error')} data-param={p.name}>
          <div className="switch-row">
            <label className="field-label" htmlFor={id}>{p.label}</label>
            <Switch id={id} checked={!!value} onChange={set} describedBy={describedBy} />
          </div>
          {p.description && <p className="field-desc" id={descId}>{p.description}</p>}
          {error && <p className="field-error" id={`${id}-err`}>{error}</p>}
        </div>
      );

    case 'enum': {
      if (p.multi) {
        const arr = Array.isArray(value) ? (value as string[]) : [];
        return (
          <FieldShell param={p} id={id} error={error} descId={descId}>
            <div className="chips" role="group" aria-labelledby={`${id}-label`} aria-describedby={describedBy}>
              {p.options.map((o) => {
                const on = arr.includes(o.value);
                return (
                  <label key={o.value} className={cx('chip', on && 'is-on')}>
                    <input type="checkbox" checked={on} onChange={() => set(on ? arr.filter((x) => x !== o.value) : [...arr, o.value])} />
                    <span>{o.label}</span>
                  </label>
                );
              })}
            </div>
          </FieldShell>
        );
      }
      const short = p.options.length <= 4 && p.options.every((o) => o.label.length <= 22);
      const allowEmpty = !p.required && p.default == null;
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          {short && !allowEmpty ? (
            <Segmented label={p.label} id={id} value={String(value ?? '')} onChange={set} options={p.options.map((o) => ({ value: o.value, label: o.label }))} />
          ) : (
            <div className="select-wrap">
              <select className="select" {...common} aria-labelledby={`${id}-label`} value={String(value ?? '')} onChange={(e) => set(e.target.value)}>
                {allowEmpty && <option value="">—</option>}
                {p.options.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </div>
          )}
        </FieldShell>
      );
    }

    case 'integer':
    case 'number': {
      const slider = isSliderRange(p);
      const span = (p.max ?? 100) - (p.min ?? 0);
      const step = p.type === 'integer' ? 1 : p.type === 'number' && p.step ? p.step : span <= 2 ? 0.01 : span <= 20 ? 0.1 : 1;
      const num = value === '' || value == null ? '' : String(value);
      const onNum = (s: string) => set(s === '' ? '' : s.match(/^-?\d*\.?\d*(e-?\d+)?$/i) && !isNaN(Number(s)) ? Number(s) : s);
      const placeholder = p.min != null && p.max != null ? `${p.min}–${p.max}` : p.min != null ? `≥ ${p.min}` : p.max != null ? `≤ ${p.max}` : undefined;
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          <div className={cx('num-row', slider && 'has-slider')}>
            {slider && (
              <input
                type="range"
                className="range"
                min={p.min}
                max={p.max}
                step={step}
                value={num === '' || isNaN(Number(num)) ? (p.default as number) ?? p.min : Number(num)}
                onChange={(e) => set(Number(e.target.value))}
                aria-label={`${p.label} slider`}
                tabIndex={-1}
                style={{ ['--pct' as any]: `${(((Number(num || p.min) - p.min!) / span) * 100).toFixed(1)}%` }}
              />
            )}
            <input
              {...common}
              className="input input-num"
              type="number"
              inputMode={p.type === 'integer' ? 'numeric' : 'decimal'}
              min={p.min}
              max={p.max}
              step={step}
              value={num}
              placeholder={placeholder}
              onChange={(e) => onNum(e.target.value)}
            />
          </div>
        </FieldShell>
      );
    }

    case 'color': {
      const s = String(value ?? '');
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          <div className="color-row">
            <input type="color" className="color-swatch" value={toHex6(s || '#000000')} onChange={(e) => set(e.target.value)} aria-label={`${p.label} picker`} />
            <input {...common} className="input mono" value={s} onChange={(e) => set(e.target.value)} placeholder="#rrggbb" spellCheck={false} maxLength={9} />
          </div>
        </FieldShell>
      );
    }

    case 'secret':
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          <div className="input-group">
            <input {...common} className="input mono" type={reveal ? 'text' : 'password'} value={String(value ?? '')} onChange={(e) => set(e.target.value)} autoComplete="off" spellCheck={false} />
            <button type="button" className="input-addon" onClick={() => setReveal((r) => !r)} aria-label={reveal ? 'Hide value' : 'Show value'} aria-pressed={reveal}>
              {reveal ? <EyeOff size={15} aria-hidden /> : <Eye size={15} aria-hidden />}
            </button>
          </div>
        </FieldShell>
      );

    case 'json': {
      const s = String(value ?? '');
      let ok: boolean | null = null;
      if (s.trim()) { try { JSON.parse(s); ok = true; } catch { ok = false; } }
      return (
        <FieldShell
          param={p}
          id={id}
          error={error}
          descId={descId}
          extra={<span className={cx('syntax-tag', ok === false && 'is-bad', ok && 'is-ok')}>{ok === false ? 'invalid JSON' : 'JSON'}</span>}
        >
          <textarea {...common} className="textarea mono code" rows={5} value={s} onChange={(e) => set(e.target.value)} spellCheck={false} placeholder="[ ]" />
        </FieldShell>
      );
    }

    case 'path': {
      const s = String(value ?? '');
      const pick = backend.pickPath;
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          <div className="input-group">
            <input {...common} className="input mono" value={s} onChange={(e) => set(e.target.value)} placeholder={p.kind === 'dir' ? '/path/to/folder' : '/path/to/file'} spellCheck={false} />
            {pick && (
              <button type="button" className="input-addon input-addon-text" onClick={async () => { const r = await pick.call(backend, p.kind); if (r) set(r); }}>
                <FolderOpen size={14} aria-hidden /> Browse
              </button>
            )}
          </div>
          {backend.mode === 'server' && <p className="field-desc">A path on the server. Only folders the administrator allowed can be used.</p>}
        </FieldShell>
      );
    }

    case 'file':
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          <FileField param={p} items={(value as FileItem[]) ?? []} onChange={onChange} labelledBy={`${id}-label`} describedBy={describedBy} invalid={!!error} />
        </FieldShell>
      );

    case 'text':
    default: {
      const t = p.type === 'text' ? p : null;
      const s = String(value ?? '');
      const isMono = mono || !!t?.syntax;
      if (t?.multiline) {
        const onDrop = async (e: React.DragEvent<HTMLTextAreaElement>) => {
          const f = e.dataTransfer.files?.[0];
          if (f && f.size < 5_000_000) {
            e.preventDefault();
            set(await f.text());
          }
        };
        return (
          <FieldShell
            param={p}
            id={id}
            error={error}
            descId={descId}
            extra={
              <span className="field-meta">
                {t.syntax && <span className="syntax-tag">{t.syntax}</span>}
                {s.length > 0 && <span className="char-count">{s.length.toLocaleString()}{t.max_len ? ` / ${t.max_len.toLocaleString()}` : ''}</span>}
              </span>
            }
          >
            <textarea
              {...common}
              className={cx('textarea', isMono && 'mono code')}
              rows={s.split('\n').length > 8 ? 14 : 8}
              value={s}
              placeholder={t.placeholder ?? (isMono ? undefined : 'Type or paste text, or drop a text file')}
              onChange={(e) => set(e.target.value)}
              onDrop={onDrop}
              spellCheck={!isMono}
              maxLength={t.max_len}
            />
          </FieldShell>
        );
      }
      return (
        <FieldShell param={p} id={id} error={error} descId={descId}>
          <input {...common} className={cx('input', isMono && 'mono')} value={s} placeholder={t?.placeholder} onChange={(e) => set(e.target.value)} maxLength={t?.max_len} spellCheck={!isMono} />
        </FieldShell>
      );
    }
  }
});
