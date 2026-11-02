import { useCallback, useEffect, useState } from 'react';
import { Plus, Pencil, KeyRound, Trash2, Bot, UserRound, Lock, MoreHorizontal, Ban, CircleCheck } from 'lucide-react';
import type { Policy, Role, UserRow } from '../../lib/types';
import { useApp } from '../../lib/app';
import { asServer } from '../../lib/backend';
import { formatBytes, formatRelative } from '../../lib/format';
import { errorText } from '../../lib/errors';
import { Badge, Button, Callout, Menu, MenuItem, Modal, Section, Segmented, Spinner, Switch, useConfirm } from '../../components/ui';
import { toast } from '../../components/toast';
import { PolicyEditor, cleanPolicy, summarizePolicy } from './PolicyEditor';
import { CreateTokenDialog } from '../Account';

type Kind = 'user' | 'admin' | 'agent';

function defaultPolicy(kind: Kind): Policy {
  if (kind === 'admin') return { allow: ['*'], deny: [], permissions: ['host_fs_read', 'host_fs_write', 'network', 'system_info', 'process_info'] };
  if (kind === 'user') return { allow: ['*'], deny: [], permissions: ['network'] };
  return { allow: [], deny: [], permissions: [] };
}

export function UsersPanel() {
  const { backend, me } = useApp();
  const server = asServer(backend)!;
  const [users, setUsers] = useState<UserRow[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<UserRow | null>(null);
  const [tokenFor, setTokenFor] = useState<UserRow | null>(null);
  const [pwFor, setPwFor] = useState<UserRow | null>(null);
  const [confirm, confirmNode] = useConfirm();

  const load = useCallback(async () => {
    try { setUsers(await server.adminUsers()); setErr(null); } catch (e) { setErr(errorText(e)); }
  }, [server]);
  useEffect(() => { void load(); }, [load]);

  const toggleDisabled = async (u: UserRow) => {
    try { await server.adminUpdateUser(u.id, { disabled: !u.disabled }); await load(); } catch (e) { toast.error(errorText(e)); }
  };
  const remove = async (u: UserRow) => {
    if (!(await confirm({ title: `Delete ${u.username}?`, message: 'Their files, jobs and tokens are removed. This cannot be undone.', confirm: 'Delete', danger: true }))) return;
    try { await server.adminDeleteUser(u.id); await load(); toast.success(`Deleted ${u.username}`); } catch (e) { toast.error(errorText(e)); }
  };

  return (
    <Section
      title="Users & agents"
      description="People sign in with a password. Agents (AI assistants, scripts) use API tokens and get only the tools you allow."
      actions={<Button size="sm" variant="primary" icon={Plus} onClick={() => setCreating(true)}>New user or agent</Button>}
    >
      {err && <Callout tone="danger">{err}</Callout>}
      {!users && !err && <Spinner />}
      {users && (
        <div className="table-wrap">
          <table className="table users-table">
            <thead><tr><th scope="col">Name</th><th scope="col">Role</th><th scope="col">Access</th><th scope="col">Last seen</th><th scope="col">Storage</th><th scope="col"><span className="sr-only">Actions</span></th></tr></thead>
            <tbody>
              {users.map((u) => {
                const self = u.id === me?.user.id;
                return (
                  <tr key={u.id} className={u.disabled ? 'is-dim' : undefined}>
                    <td>
                      <span className="user-cell">
                        <span className="avatar avatar-sm" aria-hidden>{u.kind === 'agent' ? <Bot size={13} /> : u.username.slice(0, 1).toUpperCase()}</span>
                        <span>
                          <span className="user-cell-name">{u.username}{self && <span className="muted small"> (you)</span>}</span>
                          {u.description && <span className="user-cell-desc">{u.description}</span>}
                        </span>
                      </span>
                    </td>
                    <td><Badge tone={u.role === 'owner' || u.role === 'admin' ? 'accent' : u.role === 'agent' ? 'outline' : 'neutral'}>{u.role}</Badge>{u.disabled && <> <Badge tone="danger">disabled</Badge></>}</td>
                    <td className="small">{summarizePolicy(u.policy)}{u.policy.permissions.length > 0 && <span className="muted"> · {u.policy.permissions.length} capabilit{u.policy.permissions.length === 1 ? 'y' : 'ies'}</span>}</td>
                    <td className="small">{u.last_seen_at ? formatRelative(u.last_seen_at) : <span className="muted">never</span>}</td>
                    <td className="small num">{formatBytes(u.storage_used ?? 0)}</td>
                    <td className="row-actions">
                      <Button size="sm" variant="ghost" icon={Pencil} onClick={() => setEditing(u)}>Edit</Button>
                      <Menu label={`Actions for ${u.username}`} align="end" trigger={(t) => (
                        <button className="icon-btn" aria-label={`More actions for ${u.username}`} {...t}><MoreHorizontal size={16} aria-hidden /></button>
                      )}>
                        {(close) => (
                          <>
                            <MenuItem icon={KeyRound} onSelect={() => { close(); setTokenFor(u); }}>Create API token</MenuItem>
                            {u.kind !== 'agent' && <MenuItem icon={Lock} onSelect={() => { close(); setPwFor(u); }}>Reset password</MenuItem>}
                            {!self && <MenuItem icon={u.disabled ? CircleCheck : Ban} onSelect={() => { close(); void toggleDisabled(u); }}>{u.disabled ? 'Enable' : 'Disable'}</MenuItem>}
                            {!self && <><div className="menu-sep" role="separator" /><MenuItem icon={Trash2} danger onSelect={() => { close(); void remove(u); }}>Delete</MenuItem></>}
                          </>
                        )}
                      </Menu>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <CreateUserDialog open={creating} onClose={() => setCreating(false)} onCreated={async (u) => { setCreating(false); await load(); if (u.kind === 'agent') setTokenFor(u); }} />
      <EditUserDialog user={editing} onClose={() => setEditing(null)} onSaved={async () => { setEditing(null); await load(); }} isSelf={editing?.id === me?.user.id} />
      <CreateTokenDialog open={!!tokenFor} forName={tokenFor?.username} title="New token" onClose={() => setTokenFor(null)} onCreate={(b) => server.adminCreateToken({ ...b, user_id: tokenFor!.id })} />
      <ResetPasswordDialog user={pwFor} onClose={() => setPwFor(null)} />
      {confirmNode}
    </Section>
  );
}

function CreateUserDialog({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: (u: UserRow) => void }) {
  const { backend, me } = useApp();
  const server = asServer(backend)!;
  const [kind, setKind] = useState<Kind>('agent');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [description, setDescription] = useState('');
  const [policy, setPolicy] = useState<Policy>(defaultPolicy('agent'));
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { if (open) { setKind('agent'); setUsername(''); setPassword(''); setDescription(''); setPolicy(defaultPolicy('agent')); setErr(null); } }, [open]);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true); setErr(null);
    try {
      const u = await server.adminCreateUser({
        username: username.trim(), role: kind as Role, description: description.trim(), policy: cleanPolicy(policy),
        ...(kind !== 'agent' ? { password } : {}),
      });
      toast.success(`Created ${u.username}`);
      onCreated(u);
    } catch (x) { setErr(errorText(x)); }
    setBusy(false);
  };
  return (
    <Modal open={open} onClose={onClose} title="New user or agent" width={680}
      footer={<><Button variant="ghost" onClick={onClose}>Cancel</Button><Button variant="primary" type="submit" form="new-user" loading={busy}>Create {kind}</Button></>}>
      <form id="new-user" className="form-stack" onSubmit={submit}>
        {err && <Callout tone="danger">{err}</Callout>}
        <Segmented<Kind> label="Account type" value={kind} onChange={(k) => { setKind(k); setPolicy(defaultPolicy(k)); }} options={[
          { value: 'agent', label: 'Agent', icon: Bot }, { value: 'user', label: 'User', icon: UserRound },
          ...(me?.user.role === 'owner' || me?.user.role === 'admin' ? [{ value: 'admin' as Kind, label: 'Admin', icon: UserRound }] : []),
        ]} />
        <p className="field-desc">{kind === 'agent' ? 'Agents authenticate with API tokens (you can create one next) and only see the tools you allow.' : kind === 'admin' ? 'Admins can manage users, models and settings.' : 'Users sign in with a password and can run the tools you allow.'}</p>
        <div className="form-grid">
          <label className="field"><span className="field-label">Username</span><input className="input" value={username} onChange={(e) => setUsername(e.target.value)} required data-autofocus autoComplete="off" /></label>
          {kind !== 'agent' && (
            <label className="field"><span className="field-label">Password</span><input className="input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} required minLength={10} autoComplete="new-password" /></label>
          )}
        </div>
        <label className="field"><span className="field-label">Description</span><input className="input" value={description} onChange={(e) => setDescription(e.target.value)} placeholder={kind === 'agent' ? 'e.g. Claude Desktop on Mischa’s laptop' : 'Optional'} /></label>
        <PolicyEditor value={policy} onChange={setPolicy} idPrefix="new" />
      </form>
    </Modal>
  );
}

function EditUserDialog({ user, onClose, onSaved, isSelf }: { user: UserRow | null; onClose: () => void; onSaved: () => void; isSelf: boolean }) {
  const { backend, me } = useApp();
  const server = asServer(backend)!;
  const [role, setRole] = useState<Role>('user');
  const [description, setDescription] = useState('');
  const [disabled, setDisabled] = useState(false);
  const [policy, setPolicy] = useState<Policy>(defaultPolicy('user'));
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    if (user) { setRole(user.role); setDescription(user.description); setDisabled(user.disabled); setPolicy({ ...user.policy }); setErr(null); }
  }, [user]);
  if (!user) return null;
  const save = async () => {
    setBusy(true); setErr(null);
    try {
      await server.adminUpdateUser(user.id, {
        ...(role !== user.role ? { role } : {}),
        description, policy: cleanPolicy(policy),
        ...(disabled !== user.disabled ? { disabled } : {}),
      });
      toast.success('Saved');
      onSaved();
    } catch (x) { setErr(errorText(x)); }
    setBusy(false);
  };
  const roles: Role[] = user.role === 'agent' ? ['agent'] : me?.user.role === 'owner' ? ['owner', 'admin', 'user'] : ['admin', 'user'];
  return (
    <Modal open onClose={onClose} title={`Edit ${user.username}`} width={680}
      footer={<><Button variant="ghost" onClick={onClose}>Cancel</Button><Button variant="primary" onClick={save} loading={busy}>Save changes</Button></>}>
      <div className="form-stack">
        {err && <Callout tone="danger">{err}</Callout>}
        <div className="form-grid">
          {user.role !== 'agent' && (
            <label className="field">
              <span className="field-label">Role</span>
              <div className="select-wrap">
                <select className="select" value={role} onChange={(e) => setRole(e.target.value as Role)} disabled={isSelf || (user.role === 'owner' && me?.user.role !== 'owner')}>
                  {roles.map((r) => <option key={r} value={r}>{r}</option>)}
                </select>
              </div>
            </label>
          )}
          <label className="field"><span className="field-label">Description</span><input className="input" value={description} onChange={(e) => setDescription(e.target.value)} /></label>
        </div>
        {!isSelf && (
          <div className="switch-row"><span className="field-label">Disabled</span><Switch checked={disabled} onChange={setDisabled} label="Disabled" /></div>
        )}
        <PolicyEditor value={policy} onChange={setPolicy} idPrefix={`edit-${user.id}`} />
      </div>
    </Modal>
  );
}

function ResetPasswordDialog({ user, onClose }: { user: UserRow | null; onClose: () => void }) {
  const { backend } = useApp();
  const server = asServer(backend)!;
  const [pw, setPw] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { setPw(''); setErr(null); }, [user]);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try { await server.adminSetPassword(user!.id, pw); toast.success('Password updated'); onClose(); } catch (x) { setErr(errorText(x)); }
    setBusy(false);
  };
  return (
    <Modal open={!!user} onClose={onClose} title={`Reset password for ${user?.username ?? ''}`} width={440}
      footer={<><Button variant="ghost" onClick={onClose}>Cancel</Button><Button variant="primary" type="submit" form="pw-form" loading={busy}>Set password</Button></>}>
      <form id="pw-form" className="form-stack" onSubmit={submit}>
        {err && <Callout tone="danger">{err}</Callout>}
        <label className="field"><span className="field-label">New password</span><input className="input" type="password" value={pw} onChange={(e) => setPw(e.target.value)} minLength={10} required data-autofocus autoComplete="new-password" /></label>
      </form>
    </Modal>
  );
}
