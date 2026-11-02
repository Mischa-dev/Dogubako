import { TriangleAlert } from 'lucide-react';
import type { NewToken } from '../../lib/types';
import { Callout, CopyButton, Segmented } from '../../components/ui';
import { useState } from 'react';

export function mcpConfig(origin: string, token: string) {
  return JSON.stringify({ mcpServers: { dogubako: { type: 'http', url: `${origin}/mcp`, headers: { Authorization: `Bearer ${token}` } } } }, null, 2);
}

export function curlExample(origin: string, token: string) {
  return `curl -H "Authorization: Bearer ${token}" \\\n  ${origin}/api/v1/tools\n\ncurl -X POST -H "Authorization: Bearer ${token}" \\\n  -H "Content-Type: application/json" \\\n  -d '{"params":{"text":"Hello"},"wait_seconds":30}' \\\n  ${origin}/api/v1/tools/text.case/run`;
}

export function TokenReveal({ token, showExamples = true }: { token: NewToken; showExamples?: boolean }) {
  const origin = location.origin;
  const [tab, setTab] = useState<'mcp' | 'curl'>('mcp');
  return (
    <div className="token-reveal">
      <Callout tone="warn" icon={TriangleAlert} title="Copy this token now">It won't be shown again. Treat it like a password.</Callout>
      <div className="token-box">
        <code className="mono" aria-label="New API token">{token.token}</code>
        <CopyButton text={token.token} variant="secondary" label="Copy token" />
      </div>
      {showExamples && (
        <div className="token-examples">
          <div className="token-examples-head">
            <Segmented size="sm" label="Example" value={tab} onChange={setTab} options={[{ value: 'mcp', label: 'MCP client config' }, { value: 'curl', label: 'curl' }]} />
            <CopyButton text={tab === 'mcp' ? mcpConfig(origin, token.token) : curlExample(origin, token.token)} />
          </div>
          <pre className="code-view mono small">{tab === 'mcp' ? mcpConfig(origin, token.token) : curlExample(origin, token.token)}</pre>
        </div>
      )}
    </div>
  );
}
