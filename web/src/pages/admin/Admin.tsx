import { Shield, Users, KeyRound, ScrollText, Server, BookOpen } from 'lucide-react';
import { href } from '../../lib/router';
import { PageHeader, cx } from '../../components/ui';
import { UsersPanel } from './Users';
import { AuditPanel } from './Audit';
import { InstancePanel } from './Instance';
import { ApiPanel } from './Api';
import { AllTokensPanel } from './AllTokens';

const TABS = [
  { id: 'users', label: 'Users & agents', icon: Users },
  { id: 'tokens', label: 'Tokens', icon: KeyRound },
  { id: 'audit', label: 'Audit log', icon: ScrollText },
  { id: 'instance', label: 'Instance', icon: Server },
  { id: 'api', label: 'API', icon: BookOpen },
] as const;

export default function AdminPage({ section = 'users' }: { section?: string }) {
  const cur = TABS.find((t) => t.id === section) ?? TABS[0];
  return (
    <div className="page page-wide">
      <PageHeader icon={Shield} title="Admin" subtitle="Manage who can use this instance and what they can do." />
      <nav className="tabs" aria-label="Admin sections">
        {TABS.map((t) => (
          <a key={t.id} href={href.admin(t.id)} className={cx('tab', cur.id === t.id && 'is-active')} aria-current={cur.id === t.id ? 'page' : undefined}>
            <t.icon size={15} aria-hidden />{t.label}
          </a>
        ))}
      </nav>
      {cur.id === 'users' && <UsersPanel />}
      {cur.id === 'tokens' && <AllTokensPanel />}
      {cur.id === 'audit' && <AuditPanel />}
      {cur.id === 'instance' && <InstancePanel />}
      {cur.id === 'api' && <ApiPanel />}
    </div>
  );
}
