import type { ToolInfo } from '../lib/types';

export function tool(p: Partial<ToolInfo> & { id: string; name: string }): ToolInfo {
  return {
    description: '', category: 'text', keywords: [], params: [], output: { kind: 'text' }, platforms: ['linux'], requires: [],
    permissions: [], timeout_secs: 60, reports_progress: false, execution: 'native', version: '1', destructive: false, source: 'builtin',
    availability: { available: true, platform_supported: true }, ...p,
  } as ToolInfo;
}
