import { useCallback, useEffect, useState } from 'react';
import { UserRound, KeyRound, Plus, Trash2 } from 'lucide-react';
import type { NewToken, Policy, TokenRow } from '../lib/types';
import { useApp } from '../lib/app';
import { asServer } from '../lib/backend';
import { useRoute, navigate, href } from '../lib/router';
import { formatRelative, formatDateTime, formatBytes } from '../lib/format';
import { errorText } from '../lib/errors';
import { Badge, Button, Callout, EmptyState, IconButton, Meter, Modal, PageHeader, Section, Segmented, Switch, useConfirm } from '../components/ui';
import { toast } from '../components/toast';
import { PolicyEditor, cleanPolicy, emptyPolicy, summarizePolicy } from './admin/PolicyEditor';
import { TokenReveal } from './admin/TokenReveal';

export default function AccountPage() {
  const route = useRoute();
  const tab = route.query.get('tab') === 'tokens' ? 'tokens' : 'account';
  return (
    <div className="page">
      <PageHeader icon={UserRound} title="Account" />
      <div className="tabs-row">
        <Segmented<'account' | 'tokens'> label="Account section" value={tab} onChange={(v) => navigate(v === 'tokens' ? `${href.account}?tab=tokens` : href.account, { replace: true })}
          options={[{ value: 'account', label: 'Profile & password', icon: UserRound }, { value: 'tokens', label: 'API tokens', icon: KeyRound }]} />
      </div>
      {tab === 'account' ? <Profile /> : <Tokens />}
    </div>
  );
}

function Profile() {
  const { me, backend, signOut } = useApp();
  const server = asServer(backend)!;
  const [cur, setCur] = useState('');
  const [next, setNext] = useState('');
  const [next2, setNext2] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (next !== next2) return setErr('New passwords do not match');
    setBusy(true); setErr(null);
    try {
      const r = await server.changePassword(cur, next);
      toast.success('Password changed');
      setCur(''); setNext(''); setNext2('');
      if (r.relogin) await signOut();
    } catch (x) { setErr(errorText(x)); }
    setBusy(false);
  };
  if (!me) return null;
  const st = me.storage;
  return (
    <>
      <Section title="Profile">
        <div className="settings-card">
          <div className="setting-row"><span className="setting-label">Username</span><span>{me.user.username}</span></div>
          <div className="setting-row"><span className="setting-label">Role</span><Badge tone="outline">{me.user.role}</Badge></div>
          <div className="setting-row"><span className="setting-label">Capabilities</span><span className="kv-list">{me.permissions.length ? me.permissions.map((p) => <span className="kv-pill" key={p}>{p}</span>) : <span className="muted">none</span>}</span></div>
          {st && (
            <div className="setting-row">
              <span className="setting-label">Storage</span>
              <span className="storage-mini"><Meter value={st.quota ? (st.used / st.quota) * 100 : 0} label="Storage used" tone="auto" /><span className="muted small">{formatBytes(st.used)} of {formatBytes(st.quota)}</span></span>
            </div>
          )}
        </div>
      </Section>
      {me.user.via === 'session' && (
        <Section title="Change password">
          <form className="form-stack narrow" onSubmit={submit}>
            {err && <Callout tone="danger">{err}</Callout>}
            <label className="field"><span className="field-label">Current password</span><input className="input" type="password" value={cur} onChange={(e) => setCur(e.target.value)} autoComplete="current-password" required /></label>
            <label className="field"><span className="field-label">New password</span><input className="input" type="password" value={next} onChange={(e) => setNext(e.target.value)} autoComplete="new-password" minLength={10} required /><span className="field-desc">At least 10 characters. You'll be signed out afterwards.</span></label>
            <label className="field"><span className="field-label">Confirm new password</span><input className="input" type="password" value={next2} onChange={(e) => setNext2(e.target.value)} autoComplete="new-password" required /></label>
            <div><Button type="submit" variant="primary" loading={busy}>Change password</Button></div>
          </form>
        </Section>
      )}
    </>
  );
}

export function TokenTable({ tokens, onRevoke, showOwner }: { tokens: TokenRow[]; onRevoke: (t: TokenRow) => void; showOwner?: boolean }) {
  return (
    <div className="table-wrap">
      <table className="table">
        <thead><tr><th scope="col">Name</th>{showOwner && <th scope="col">Owner</th>}<th scope="col">Token</th><th scope="col">Scope</th><th scope="col">Last used</th><th scope="col">Expires</th><th scope="col"><span className="sr-only">Actions</span></th></tr></thead>
        <tbody>
          {tokens.map((t) => (
            <tr key={t.id} className={t.revoked ? 'is-dim' : undefined}>
              <td>{t.name}{t.revoked && <> <Badge tone="danger">revoked</Badge></>}</td>
              {showOwner && <td>{t.username}</td>}
              <td className="mono small">{t.prefix}…</td>
              <td className="small">{t.scope ? summarizePolicy(t.scope) : <span className="muted">Full account</span>}</td>
              <td className="small" title={formatDateTime(t.last_used_at)}>{t.last_used_at ? formatRelative(t.last_used_at) : <span className="muted">never</span>}</td>
              <td className="small">{t.expires_at ? formatDateTime(t.expires_at) : <span className="muted">never</span>}</td>
              <td className="row-actions">{!t.revoked && <IconButton icon={Trash2} label={`Revoke ${t.name}`} onClick={() => onRevoke(t)} />}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function CreateTokenDialog({ open, onClose, onCreate, title = 'New API token', forName }: {
  open: boolean;
  onClose: () => void;
  onCreate: (body: { name: string; expires_days?: number; scope?: Policy }) => Promise<NewToken>;
  title?: string;
  forName?: string;
}) {
  const [name, setName] = useState('');
  const [days, setDays] = useState('90');
  const [scoped, setScoped] = useState(false);
  const [scope, setScope] = useState<Policy>(emptyPolicy());
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [created, setCreated] = useState<NewToken | null>(null);
  useEffect(() => { if (open) { setName(''); setDays('90'); setScoped(false); setScope(emptyPolicy()); setErr(null); setCreated(null); } }, [open]);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true); setErr(null);
    try {
      setCreated(await onCreate({ name: name.trim(), expires_days: days ? Number(days) : undefined, scope: scoped ? cleanPolicy(scope) : undefined }));
    } catch (x) { setErr(errorText(x)); }
    setBusy(false);
  };
  return (
    <Modal open={open} onClose={onClose} title={created ? 'Token created' : title} description={!created && forName ? <>For <strong>{forName}</strong></> : undefined} width={640}
      footer={created ? <Button variant="primary" onClick={onClose}>Done</Button> : (
        <>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button variant="primary" type="submit" form="token-form" loading={busy} disabled={!name.trim()}>Create token</Button>
        </>
      )}>
      {created ? <TokenReveal token={created} /> : (
        <form id="token-form" className="form-stack" onSubmit={submit}>
          {err && <Callout tone="danger">{err}</Callout>}
          <label className="field"><span className="field-label">Name</span><input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. Claude Desktop, CI pipeline" required data-autofocus maxLength={100} /></label>
          <label className="field">
            <span className="field-label">Expires</span>
            <div className="select-wrap">
              <select className="select" value={days} onChange={(e) => setDays(e.target.value)}>
                <option value="7">In 7 days</option><option value="30">In 30 days</option><option value="90">In 90 days</option><option value="365">In 1 year</option><option value="">Never</option>
              </select>
            </div>
          </label>
          <div className="switch-row">
            <span className="field-label">Restrict this token</span>
            <Switch checked={scoped} onChange={setScoped} label="Restrict this token" />
          </div>
          <p className="field-desc">Without restrictions the token can do everything the account can. A scope can only narrow access.</p>
          {scoped && <PolicyEditor value={scope} onChange={setScope} idPrefix="tok" compact />}
        </form>
      )}
    </Modal>
  );
}

function Tokens() {
  const { backend, me } = useApp();
  const server = asServer(backend)!;
  const [tokens, setTokens] = useState<TokenRow[] | null>(null);
  const [creating, setCreating] = useState(false);
  const [confirm, confirmNode] = useConfirm();
  const load = useCallback(async () => { try { setTokens(await server.myTokens()); } catch (e) { toast.error(errorText(e)); } }, [server]);
  useEffect(() => { void load(); }, [load]);
  const revoke = async (t: TokenRow) => {
    if (!(await confirm({ title: `Revoke “${t.name}”?`, message: 'Apps using it will lose access immediately.', confirm: 'Revoke', danger: true }))) return;
    try { await server.revokeToken(t.id); await load(); } catch (e) { toast.error(errorText(e)); }
  };
  return (
    <Section
      title="API tokens"
      description="Use tokens to call the HTTP API or connect MCP clients (e.g. AI assistants) to this instance."
      actions={me?.user.via !== 'token' && <Button size="sm" variant="primary" icon={Plus} onClick={() => setCreating(true)}>New token</Button>}
    >
      {tokens && tokens.length === 0 && <EmptyState icon={KeyRound} title="No tokens">Create one to use the API or MCP.</EmptyState>}
      {tokens && tokens.length > 0 && <TokenTable tokens={tokens} onRevoke={revoke} />}
      <CreateTokenDialog open={creating} onClose={() => { setCreating(false); void load(); }} onCreate={(b) => server.createToken(b)} />
      {confirmNode}
    </Section>
  );
}
