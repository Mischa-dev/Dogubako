import { useCallback, useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { createBackend, asServer, type Backend } from './lib/backend';
import { AppProvider, normalizeTool } from './lib/app';
import type { Category, Me, Status, ToolInfo } from './lib/types';
import { Shell } from './Shell';
import { LoginPage, SetupPage } from './pages/Auth';
import { Button, Spinner } from './components/ui';
import { LogoMark } from './components/Logo';
import { Toaster } from './components/toast';

type Phase =
  | { k: 'loading' }
  | { k: 'error'; message: string }
  | { k: 'setup'; backend: Backend; status: Status }
  | { k: 'login'; backend: Backend; status: Status }
  | { k: 'ready'; backend: Backend; status: Status; me: Me | null; tools: ToolInfo[]; categories: Category[] | null };

export function App() {
  const [phase, setPhase] = useState<Phase>({ k: 'loading' });
  const [backend, setBackend] = useState<Backend | null>(null);

  const boot = useCallback(async (b?: Backend) => {
    try {
      const be = b ?? backend ?? (await createBackend());
      if (!backend) setBackend(be);
      const status = await be.getStatus();
      if (be.mode === 'server' && status.setup_required) return setPhase({ k: 'setup', backend: be, status });
      const me = await be.getMe();
      if (be.mode === 'server' && status.auth === 'required' && !me) return setPhase({ k: 'login', backend: be, status });
      const [tools, categories] = await Promise.all([be.listTools(), be.listCategories().catch(() => null)]);
      document.title = be.mode === 'server' && status.instance_name && status.instance_name !== 'Dogubako' ? `${status.instance_name} · Dogubako` : 'Dogubako';
      setPhase({ k: 'ready', backend: be, status, me, tools: tools.map(normalizeTool), categories });
    } catch (e) {
      setPhase({ k: 'error', message: e instanceof Error ? e.message : String(e) });
    }
  }, [backend]);

  useEffect(() => { void boot(); /* eslint-disable-next-line */ }, []);

  useEffect(() => {
    const s = backend && asServer(backend);
    if (!s) return;
    return s.onUnauthorized(() => {
      setPhase((p) => (p.k === 'ready' && p.status.auth === 'required' ? { k: 'login', backend: p.backend, status: p.status } : p));
    });
  }, [backend]);

  let body: React.ReactNode;
  switch (phase.k) {
    case 'loading':
      body = (
        <div className="boot" aria-busy="true">
          <LogoMark size={36} />
          <Spinner size={18} />
          <span className="sr-only">Loading Dogubako…</span>
        </div>
      );
      break;
    case 'error':
      body = (
        <div className="boot">
          <LogoMark size={36} />
          <div className="boot-error" role="alert">
            <h1>Can't reach Dogubako</h1>
            <p>{phase.message}</p>
            <Button icon={RefreshCw} onClick={() => { setPhase({ k: 'loading' }); void boot(); }}>Try again</Button>
          </div>
        </div>
      );
      break;
    case 'setup':
      body = <SetupPage backend={phase.backend} status={phase.status} onDone={() => boot(phase.backend)} />;
      break;
    case 'login':
      body = <LoginPage backend={phase.backend} status={phase.status} onDone={() => boot(phase.backend)} />;
      break;
    case 'ready':
      body = (
        <AppProvider
          backend={phase.backend}
          status={phase.status}
          me={phase.me}
          tools={phase.tools}
          categories={phase.categories}
          onSignedOut={() => setPhase({ k: 'login', backend: phase.backend, status: phase.status })}
        >
          <Shell />
        </AppProvider>
      );
  }
  return (
    <>
      {body}
      <Toaster />
    </>
  );
}
