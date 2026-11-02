import { useMemo, useState } from 'react';
import { Plus, X, Search } from 'lucide-react';
import type { PermissionId, Policy } from '../../lib/types';
import { useApp } from '../../lib/app';
import { categoryIcon } from '../../lib/icons';
import { search } from '../../lib/search';
import { Segmented, cx } from '../../components/ui';

export const PERMISSIONS: { id: PermissionId; label: string; description: string }[] = [
  { id: 'network', label: 'Network access', description: 'Make outbound network requests (DNS, HTTP, TLS checks…)' },
  { id: 'system_info', label: 'System information', description: 'Read hardware and operating system information' },
  { id: 'process_info', label: 'Process list', description: 'List running processes on the host' },
  { id: 'host_fs_read', label: 'Read host files', description: 'Read files and folders on the host within allowed roots' },
  { id: 'host_fs_write', label: 'Modify host files', description: 'Modify files and folders on the host within allowed roots' },
];

export const emptyPolicy = (): Policy => ({ allow: [], deny: [], permissions: [] });

export function summarizePolicy(p: Policy | null | undefined): string {
  if (!p) return '—';
  if (p.allow.includes('*')) return p.deny.length ? `All tools except ${p.deny.length}` : 'All tools';
  if (!p.allow.length) return 'No tools';
  const cats = p.allow.filter((a) => a.startsWith('category:')).length;
  const rest = p.allow.length - cats;
  return [cats && `${cats} categor${cats === 1 ? 'y' : 'ies'}`, rest && `${rest} tool pattern${rest === 1 ? '' : 's'}`].filter(Boolean).join(', ');
}

function PatternInput({ label, values, onChange, id }: { label: string; values: string[]; onChange: (v: string[]) => void; id: string }) {
  const { docs } = useApp();
  const [q, setQ] = useState('');
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const hits = useMemo(() => (q.trim() ? search(docs, q, { limit: 8 }).map((h) => h.item.id) : []), [q, docs]);
  const suggestions = useMemo(() => {
    const s = q.trim().toLowerCase();
    const list = [...hits];
    const raw = s && /^[a-z0-9._*-]+$/.test(s) ? s : null;
    // A literal pattern goes first only if it is a wildcard or an exact tool id; otherwise best matches lead.
    if (raw && (raw.includes('*') || hits.includes(raw))) list.unshift(raw);
    if (s && /^[a-z]+$/.test(s)) list.splice(raw?.includes('*') ? 1 : 0, 0, `${s}.*`);
    if (raw && !list.includes(raw)) list.push(raw);
    return [...new Set(list)].filter((x) => !values.includes(x)).slice(0, 8);
  }, [q, hits, values]);
  const add = (v: string) => {
    if (!v || values.includes(v)) return;
    onChange([...values, v]);
    setQ('');
    setActive(0);
  };
  return (
    <div className="pattern-input">
      <label className="field-label" htmlFor={id}>{label}</label>
      {values.length > 0 && (
        <ul className="pattern-chips" aria-label={label}>
          {values.map((v) => (
            <li key={v} className="pattern-chip mono">
              {v}
              <button type="button" onClick={() => onChange(values.filter((x) => x !== v))} aria-label={`Remove ${v}`}><X size={12} aria-hidden /></button>
            </li>
          ))}
        </ul>
      )}
      <div className="combo">
        <div className="input-group">
          <span className="input-prefix" aria-hidden><Search size={14} /></span>
          <input
            id={id}
            className="input mono"
            value={q}
            placeholder="Search tools or type a pattern (image.*, video.compress)"
            onChange={(e) => { setQ(e.target.value); setOpen(true); setActive(0); }}
            onFocus={() => setOpen(true)}
            onBlur={() => window.setTimeout(() => setOpen(false), 120)}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') { e.preventDefault(); setActive((a) => Math.min(suggestions.length - 1, a + 1)); }
              else if (e.key === 'ArrowUp') { e.preventDefault(); setActive((a) => Math.max(0, a - 1)); }
              else if (e.key === 'Enter') { e.preventDefault(); add(suggestions[active] ?? q.trim()); }
              else if (e.key === 'Escape') { setOpen(false); }
            }}
            role="combobox"
            aria-expanded={open && suggestions.length > 0}
            aria-controls={`${id}-list`}
            aria-activedescendant={open && suggestions[active] ? `${id}-opt-${active}` : undefined}
            autoComplete="off"
            spellCheck={false}
          />
        </div>
        {open && suggestions.length > 0 && (
          <ul className="combo-list" role="listbox" id={`${id}-list`}>
            {suggestions.map((s, i) => (
              <li key={s} id={`${id}-opt-${i}`} role="option" aria-selected={i === active} className={cx('combo-opt', i === active && 'is-active')}
                onMouseDown={(e) => { e.preventDefault(); add(s); }} onMouseMove={() => setActive(i)}>
                <Plus size={12} aria-hidden /> <span className="mono">{s}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

export function PolicyEditor({ value, onChange, idPrefix, compact }: { value: Policy; onChange: (p: Policy) => void; idPrefix: string; compact?: boolean }) {
  const { categories } = useApp();
  const all = value.allow.includes('*');
  const set = (patch: Partial<Policy>) => onChange({ ...value, ...patch });
  const catOn = (id: string) => all || value.allow.includes(`category:${id}`) || value.allow.includes(`${id}.*`);
  const toggleCat = (id: string) => {
    const has = value.allow.includes(`category:${id}`) || value.allow.includes(`${id}.*`);
    set({ allow: has ? value.allow.filter((a) => a !== `category:${id}` && a !== `${id}.*`) : [...value.allow, `category:${id}`] });
  };
  const explicit = value.allow.filter((a) => a !== '*' && !a.startsWith('category:') && !/^[a-z]+\.\*$/.test(a));
  const num = (v: string) => (v.trim() === '' ? null : Math.max(0, Number(v)));

  return (
    <div className="policy">
      <fieldset className="policy-block">
        <legend className="policy-legend">Allowed tools</legend>
        <Segmented<'all' | 'some'>
          size="sm"
          label="Tool access"
          value={all ? 'all' : 'some'}
          onChange={(v) => set({ allow: v === 'all' ? ['*'] : value.allow.filter((a) => a !== '*') })}
          options={[{ value: 'all', label: 'All tools' }, { value: 'some', label: 'Selected categories & tools' }]}
        />
        {!all && (
          <>
            <div className="cat-checks" role="group" aria-label="Allowed categories">
              {categories.map((c) => {
                const Icon = categoryIcon(c.id);
                return (
                  <label key={c.id} className={cx('cat-check', catOn(c.id) && 'is-on')}>
                    <input type="checkbox" checked={catOn(c.id)} onChange={() => toggleCat(c.id)} />
                    <Icon size={14} aria-hidden />
                    <span>{c.label}</span>
                    <span className="muted small">{c.count}</span>
                  </label>
                );
              })}
            </div>
            <PatternInput
              id={`${idPrefix}-allow`}
              label="Additional tools or patterns"
              values={explicit}
              onChange={(v) => set({ allow: [...value.allow.filter((a) => !explicit.includes(a)), ...v] })}
            />
          </>
        )}
        <PatternInput id={`${idPrefix}-deny`} label="Denied tools (override allowed)" values={value.deny} onChange={(v) => set({ deny: v })} />
      </fieldset>

      <fieldset className="policy-block">
        <legend className="policy-legend">Capabilities</legend>
        <div className="perm-list">
          {PERMISSIONS.map((p) => {
            const on = value.permissions.includes(p.id);
            return (
              <label key={p.id} className="perm">
                <input type="checkbox" checked={on} onChange={() => set({ permissions: on ? value.permissions.filter((x) => x !== p.id) : [...value.permissions, p.id] })} />
                <span className="perm-text"><span className="perm-label">{p.label}</span><span className="perm-desc">{p.description}</span></span>
              </label>
            );
          })}
        </div>
      </fieldset>

      {!compact && (
        <fieldset className="policy-block">
          <legend className="policy-legend">Limits</legend>
          <div className="limit-grid">
            <label className="field">
              <span className="field-label">Concurrent jobs</span>
              <input className="input input-num" type="number" min={0} value={value.max_concurrent_jobs ?? ''} placeholder="Default" onChange={(e) => set({ max_concurrent_jobs: num(e.target.value) })} />
            </label>
            <label className="field">
              <span className="field-label">Storage quota (MB)</span>
              <input className="input input-num" type="number" min={0} value={value.quota_mb ?? ''} placeholder="Default" onChange={(e) => set({ quota_mb: num(e.target.value) })} />
            </label>
            <label className="field">
              <span className="field-label">Max upload (MB)</span>
              <input className="input input-num" type="number" min={0} value={value.max_upload_mb ?? ''} placeholder="Default" onChange={(e) => set({ max_upload_mb: num(e.target.value) })} />
            </label>
            <label className="field">
              <span className="field-label">MCP tool discovery</span>
              <div className="select-wrap">
                <select className="select" value={value.mcp_discovery ?? ''} onChange={(e) => set({ mcp_discovery: (e.target.value || null) as Policy['mcp_discovery'] })}>
                  <option value="">Instance default</option>
                  <option value="full">Full (every tool listed)</option>
                  <option value="compact">Compact (search + run)</option>
                </select>
              </div>
            </label>
          </div>
        </fieldset>
      )}
    </div>
  );
}

/** Strip nulls so the server's Option<> fields deserialize cleanly. */
export function cleanPolicy(p: Policy): Policy {
  const out: any = { allow: p.allow, deny: p.deny, permissions: p.permissions };
  for (const k of ['max_concurrent_jobs', 'quota_mb', 'max_upload_mb', 'mcp_discovery'] as const) if (p[k] != null) out[k] = p[k];
  return out;
}
