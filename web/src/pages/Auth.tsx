import { useState } from 'react';
import { ArrowRight, Eye, EyeOff, KeyRound } from 'lucide-react';
import type { Backend } from '../lib/backend';
import { ApiError, asServer } from '../lib/backend';
import type { Status } from '../lib/types';
import { LogoMark } from '../components/Logo';
import { Button, Callout } from '../components/ui';

function PasswordInput({ id, value, onChange, autoComplete, invalid }: { id: string; value: string; onChange: (v: string) => void; autoComplete: string; invalid?: boolean }) {
  const [show, setShow] = useState(false);
  return (
    <div className="input-group">
      <input id={id} className="input" type={show ? 'text' : 'password'} value={value} onChange={(e) => onChange(e.target.value)} autoComplete={autoComplete} required aria-invalid={invalid || undefined} />
      <button type="button" className="input-addon" onClick={() => setShow((s) => !s)} aria-label={show ? 'Hide password' : 'Show password'} aria-pressed={show}>
        {show ? <EyeOff size={15} aria-hidden /> : <Eye size={15} aria-hidden />}
      </button>
    </div>
  );
}

function AuthFrame({ title, subtitle, children }: { title: string; subtitle: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="auth">
      <div className="auth-card">
        <div className="auth-brand"><LogoMark size={32} /><span>Dogubako</span></div>
        <h1 className="auth-title" tabIndex={-1}>{title}</h1>
        <p className="auth-sub">{subtitle}</p>
        {children}
      </div>
      <p className="auth-foot">Open-source, local-first toolbox · AGPL-3.0-or-later</p>
    </div>
  );
}

export function LoginPage({ backend, status, onDone }: { backend: Backend; status: Status; onDone: () => void }) {
  const [u, setU] = useState('');
  const [p, setP] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      await asServer(backend)!.login(u.trim(), p);
      onDone();
    } catch (x) {
      setErr(x instanceof ApiError ? x.message : 'Sign-in failed');
      setBusy(false);
    }
  };
  return (
    <AuthFrame title="Sign in" subtitle={<>to <strong>{status.instance_name}</strong></>}>
      <form className="form-stack" onSubmit={submit}>
        {err && <Callout tone="danger">{err}</Callout>}
        <div className="field">
          <label className="field-label" htmlFor="login-user">Username</label>
          <input id="login-user" className="input" value={u} onChange={(e) => setU(e.target.value)} autoComplete="username" autoFocus required />
        </div>
        <div className="field">
          <label className="field-label" htmlFor="login-pass">Password</label>
          <PasswordInput id="login-pass" value={p} onChange={setP} autoComplete="current-password" invalid={!!err} />
        </div>
        <Button type="submit" variant="primary" loading={busy} iconRight={ArrowRight} className="btn-block">Sign in</Button>
      </form>
    </AuthFrame>
  );
}

export function SetupPage({ backend, status, onDone }: { backend: Backend; status: Status; onDone: () => void }) {
  const [token, setToken] = useState('');
  const [u, setU] = useState('');
  const [p, setP] = useState('');
  const [p2, setP2] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const mismatch = p2.length > 0 && p !== p2;
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (p !== p2) return setErr('Passwords do not match');
    setBusy(true);
    setErr(null);
    try {
      await asServer(backend)!.setup(token.trim(), u.trim(), p);
      onDone();
    } catch (x) {
      setErr(x instanceof ApiError ? x.message : 'Setup failed');
      setBusy(false);
    }
  };
  return (
    <AuthFrame title="Set up this instance" subtitle={<>Create the owner account for <strong>{status.instance_name}</strong>.</>}>
      <form className="form-stack" onSubmit={submit}>
        {err && <Callout tone="danger">{err}</Callout>}
        <div className="field">
          <label className="field-label" htmlFor="setup-token">Setup token</label>
          <div className="input-group">
            <span className="input-prefix" aria-hidden><KeyRound size={14} /></span>
            <input id="setup-token" className="input mono" value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" spellCheck={false} required autoFocus />
          </div>
          <p className="field-desc">Printed in the server log when it started without any accounts.</p>
        </div>
        <div className="field">
          <label className="field-label" htmlFor="setup-user">Owner username</label>
          <input id="setup-user" className="input" value={u} onChange={(e) => setU(e.target.value)} autoComplete="username" required />
        </div>
        <div className="field">
          <label className="field-label" htmlFor="setup-pass">Password</label>
          <PasswordInput id="setup-pass" value={p} onChange={setP} autoComplete="new-password" />
          <p className="field-desc">At least 10 characters.</p>
        </div>
        <div className="field">
          <label className="field-label" htmlFor="setup-pass2">Confirm password</label>
          <PasswordInput id="setup-pass2" value={p2} onChange={setP2} autoComplete="new-password" invalid={mismatch} />
          {mismatch && <p className="field-error">Passwords do not match</p>}
        </div>
        <Button type="submit" variant="primary" loading={busy} iconRight={ArrowRight} className="btn-block">Create owner account</Button>
      </form>
    </AuthFrame>
  );
}
