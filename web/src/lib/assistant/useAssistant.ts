import { useEffect, useState } from 'react';
import { useApp } from '../app';
import type { AssistantInfo, Backend } from '../backend';
import { useStore } from '../store';
import { assistantSettings, missingSetup, modelLabel, providerConfig, type AssistantSettings } from './settings';
import { providerById } from './providers';

const infoCache = new WeakMap<Backend, Promise<AssistantInfo>>();
const OFF: AssistantInfo = { enabled: false, server_provider: null, private_network: false };

export function assistantInfo(b: Backend): Promise<AssistantInfo> {
  let p = infoCache.get(b);
  if (!p) {
    p = b.assistantInfo().catch(() => OFF);
    infoCache.set(b, p);
  }
  return p;
}

export interface AssistantState {
  settings: AssistantSettings;
  info: AssistantInfo | null;
  /** The person switched natural-language mode on and the host allows it. */
  on: boolean;
  /** What still needs setting up, or null when requests can be sent. */
  missing: string | null;
  /** The chosen model, as shown in pickers ("GPT-6 Luna"). */
  label: string;
  model: string;
}

export function useAssistant(): AssistantState {
  const { backend } = useApp();
  const settings = useStore(assistantSettings);
  const [info, setInfo] = useState<AssistantInfo | null>(null);
  useEffect(() => {
    let live = true;
    void assistantInfo(backend).then((i) => { if (live) setInfo(i); });
    return () => { live = false; };
  }, [backend]);
  const serverModel = info?.server_provider?.model ?? null;
  const cfg = providerConfig(settings);
  const model = cfg.model || (settings.provider === 'server' ? serverModel ?? '' : '');
  return {
    settings,
    info,
    on: settings.enabled && info?.enabled !== false,
    missing: missingSetup(settings, serverModel),
    label: model ? modelLabel(settings.provider, model) : providerById(settings.provider).name,
    model,
  };
}

/** Words that read like a request rather than a tool name. */
export const looksLikeRequest = (q: string) => q.trim().split(/\s+/).filter(Boolean).length >= 3;
