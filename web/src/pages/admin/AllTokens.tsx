import { useCallback, useEffect, useState } from 'react';
import type { TokenRow } from '../../lib/types';
import { useApp } from '../../lib/app';
import { asServer } from '../../lib/backend';
import { errorText } from '../../lib/errors';
import { EmptyState, Section, Segmented, Spinner, useConfirm } from '../../components/ui';
import { toast } from '../../components/toast';
import { TokenTable } from '../Account';
import { KeyRound } from 'lucide-react';

export function AllTokensPanel() {
  const { backend } = useApp();
  const server = asServer(backend)!;
  const [tokens, setTokens] = useState<TokenRow[] | null>(null);
  const [show, setShow] = useState<'active' | 'all'>('active');
  const [confirm, confirmNode] = useConfirm();
  const load = useCallback(async () => { try { setTokens(await server.adminTokens()); } catch (e) { toast.error(errorText(e)); } }, [server]);
  useEffect(() => { void load(); }, [load]);
  const revoke = async (t: TokenRow) => {
    if (!(await confirm({ title: `Revoke “${t.name}” (${t.username})?`, confirm: 'Revoke', danger: true }))) return;
    try { await server.revokeToken(t.id); await load(); } catch (e) { toast.error(errorText(e)); }
  };
  const list = (tokens ?? []).filter((t) => show === 'all' || !t.revoked);
  return (
    <Section title="All API tokens" description="Every token on this instance. Create tokens for a user or agent from the Users tab."
      actions={<Segmented size="sm" label="Filter" value={show} onChange={setShow} options={[{ value: 'active', label: 'Active' }, { value: 'all', label: 'All' }]} />}>
      {!tokens && <Spinner />}
      {tokens && list.length === 0 && <EmptyState icon={KeyRound} title="No tokens" />}
      {list.length > 0 && <TokenTable tokens={list} onRevoke={revoke} showOwner />}
      {confirmNode}
    </Section>
  );
}
