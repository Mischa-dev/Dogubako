import { Store } from '../store';
import type { AssistantTarget } from '../backend';
import { PROVIDERS, providerById, type ProviderId } from './providers';

/** When to stop and ask before the model runs a tool. */
export type ConfirmMode = 'risky' | 'always';

export interface ProviderConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

export interface AssistantSettings {
  /** Natural-language mode is opt-in. */
  enabled: boolean;
  provider: ProviderId;
  /** Per-provider settings, so switching back and forth keeps each key and model. */
  configs: Partial<Record<ProviderId, ProviderConfig>>;
  /** Recently used models across providers, newest first, for the quick picker. */
  recent: { provider: ProviderId; model: string }[];
  confirm: ConfirmMode;
}

export const ASSISTANT_DEFAULT: AssistantSettings = {
  enabled: false,
  provider: 'openai',
  configs: {},
  recent: [],
  confirm: 'risky',
};

const isSettings = (v: any): v is AssistantSettings =>
  !!v && typeof v === 'object' && typeof v.enabled === 'boolean' && typeof v.provider === 'string' && v.configs && typeof v.configs === 'object';

/**
 * Saved on this device only (localStorage), like other preferences. API keys
 * never go to the Dogubako server's database; on a server they are sent along
 * with each request to the model endpoint and dropped.
 */
export const assistantSettings = new Store<AssistantSettings>('dgb.assistant', ASSISTANT_DEFAULT, isSettings);

export function providerConfig(s: AssistantSettings, id: ProviderId = s.provider): ProviderConfig {
  const preset = providerById(id);
  const c = s.configs[id];
  return {
    baseUrl: c?.baseUrl ?? preset.baseUrl,
    apiKey: c?.apiKey ?? '',
    model: c?.model ?? preset.models[0]?.id ?? '',
  };
}

export function updateProvider(id: ProviderId, patch: Partial<ProviderConfig>) {
  assistantSettings.set((s) => ({ ...s, configs: { ...s.configs, [id]: { ...providerConfig(s, id), ...patch } } }));
}

export function selectModel(provider: ProviderId, model: string) {
  assistantSettings.set((s) => ({
    ...s,
    provider,
    configs: { ...s.configs, [provider]: { ...providerConfig(s, provider), model } },
    recent: [{ provider, model }, ...s.recent.filter((r) => !(r.provider === provider && r.model === model))].slice(0, 6),
  }));
}

export function targetFor(s: AssistantSettings, id: ProviderId = s.provider): AssistantTarget {
  if (id === 'server') return { provider: 'server' };
  const c = providerConfig(s, id);
  return { base_url: c.baseUrl, api_key: c.apiKey || undefined };
}

/** What still needs filling in before a request can be sent, or null when ready. */
export function missingSetup(s: AssistantSettings, serverModel?: string | null): string | null {
  const preset = providerById(s.provider);
  const c = providerConfig(s, s.provider);
  if (s.provider !== 'server' && !c.baseUrl.trim()) return 'Enter the endpoint’s base URL';
  if (preset.needsKey && !c.apiKey.trim()) return `Add your ${preset.name} API key`;
  if (!c.model.trim() && !(s.provider === 'server' && serverModel)) return 'Choose a model';
  return null;
}

export const modelLabel = (provider: ProviderId, model: string): string =>
  providerById(provider).models.find((m) => m.id === model)?.label ?? model;

export { PROVIDERS };
