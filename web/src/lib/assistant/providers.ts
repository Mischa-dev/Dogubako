/**
 * Model providers for the natural-language mode. Every provider speaks the
 * OpenAI chat-completions protocol with tool calling; presets only fill in
 * the base URL and suggest models. Any model id can be typed, and "Load
 * models" asks the endpoint for its own list.
 */
export type ProviderId = 'openai' | 'anthropic' | 'openrouter' | 'ollama' | 'lmstudio' | 'server' | 'custom';

export interface ProviderPreset {
  id: ProviderId;
  name: string;
  /** One line under the name in the picker. */
  blurb: string;
  baseUrl: string;
  /** Needs an API key to work. */
  needsKey: boolean;
  /** Where to get a key. */
  keyUrl?: string;
  /** Runs on this machine or network; nothing leaves it. */
  local?: boolean;
  /** Suggested model ids, best first. The first is the default. */
  models: { id: string; label?: string; note?: string }[];
}

export const PROVIDERS: ProviderPreset[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    blurb: 'GPT models with your OpenAI API key',
    baseUrl: 'https://api.openai.com/v1',
    needsKey: true,
    keyUrl: 'https://platform.openai.com/api-keys',
    models: [{ id: 'gpt-6-luna', label: 'GPT-6 Luna', note: 'Fast and efficient' }],
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    blurb: 'Claude models through Anthropic’s OpenAI-compatible API',
    baseUrl: 'https://api.anthropic.com/v1',
    needsKey: true,
    keyUrl: 'https://console.anthropic.com/settings/keys',
    models: [
      { id: 'claude-sonnet-5-5', label: 'Claude Sonnet 5.5', note: 'Balanced' },
      { id: 'claude-haiku-5-5', label: 'Claude Haiku 5.5', note: 'Fastest' },
      { id: 'claude-opus-5-5', label: 'Claude Opus 5.5', note: 'Most capable' },
    ],
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    blurb: 'Hundreds of models behind one key',
    baseUrl: 'https://openrouter.ai/api/v1',
    needsKey: true,
    keyUrl: 'https://openrouter.ai/settings/keys',
    models: [{ id: 'openai/gpt-6-luna', label: 'GPT-6 Luna' }],
  },
  {
    id: 'ollama',
    name: 'Ollama',
    blurb: 'Open models running on your own machine',
    baseUrl: 'http://localhost:11434/v1',
    needsKey: false,
    local: true,
    models: [],
  },
  {
    id: 'lmstudio',
    name: 'LM Studio',
    blurb: 'Local models served by LM Studio',
    baseUrl: 'http://localhost:1234/v1',
    needsKey: false,
    local: true,
    models: [],
  },
  {
    id: 'custom',
    name: 'Custom',
    blurb: 'Any OpenAI-compatible endpoint',
    baseUrl: '',
    needsKey: false,
    models: [],
  },
];

/** Offered only when the server administrator configured an endpoint. */
export const SERVER_PROVIDER: ProviderPreset = {
  id: 'server',
  name: 'This server',
  blurb: 'The model your administrator set up; no key needed',
  baseUrl: '',
  needsKey: false,
  models: [],
};

export const providerById = (id: ProviderId): ProviderPreset =>
  id === 'server' ? SERVER_PROVIDER : PROVIDERS.find((p) => p.id === id) ?? PROVIDERS[PROVIDERS.length - 1];
