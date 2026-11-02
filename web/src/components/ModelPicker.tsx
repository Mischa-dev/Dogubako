import { useEffect, useMemo, useState } from 'react';
import { Check, ChevronDown, Cloud, ExternalLink, Eye, EyeOff, HardDrive, KeyRound, RefreshCw, Search, Server, Settings2, Sparkles, Zap } from 'lucide-react';
import { useApp } from '../lib/app';
import { href, navigate } from '../lib/router';
import { errorText } from '../lib/errors';
import { PROVIDERS, SERVER_PROVIDER, providerById, type ProviderId, type ProviderPreset } from '../lib/assistant/providers';
import {
  assistantSettings, modelLabel, providerConfig, selectModel, targetFor, updateProvider, type ConfirmMode,
} from '../lib/assistant/settings';
import { useAssistant } from '../lib/assistant/useAssistant';
import { Badge, Button, Callout, Menu, MenuItem, Segmented, Switch, cx } from './ui';

const providerIcon = (p: ProviderPreset) => (p.id === 'server' ? Server : p.local ? HardDrive : p.id === 'custom' ? Settings2 : Cloud);

/** Compact model switcher shown where requests are typed. */
export function ModelChip({ className }: { className?: string }) {
  const { settings, label, info } = useAssistant();
  const preset = providerById(settings.provider);
  const Icon = providerIcon(preset);
  const options = useMemo(() => {
    const seen = new Set<string>();
    const out: { provider: ProviderId; model: string }[] = [];
    const push = (provider: ProviderId, model: string) => {
      const k = `${provider}\u0000${model}`;
      if (!model || seen.has(k)) return;
      seen.add(k);
      out.push({ provider, model });
    };
    push(settings.provider, providerConfig(settings).model);
    settings.recent.forEach((r) => push(r.provider, r.model));
    preset.models.forEach((m) => push(settings.provider, m.id));
    return out.slice(0, 8);
  }, [settings, preset]);
  return (
    <Menu
      label="Model"
      align="end"
      trigger={(t) => (
        <button type="button" className={cx('model-chip', className)} {...t} title={`${preset.name} · ${label}`}>
          <Icon size={13} aria-hidden />
          <span className="model-chip-name">{label}</span>
          <ChevronDown size={12} aria-hidden />
        </button>
      )}
    >
      {(close) => (
        <>
          {options.map((o) => {
            const current = o.provider === settings.provider && o.model === providerConfig(settings).model;
            return (
              <MenuItem key={`${o.provider}:${o.model}`} icon={current ? Check : undefined} onSelect={() => { selectModel(o.provider, o.model); close(); }}>
                {modelLabel(o.provider, o.model)} <span className="muted small">· {providerById(o.provider).name}</span>
              </MenuItem>
            );
          })}
          {info?.server_provider && settings.provider !== 'server' && (
            <MenuItem icon={Server} onSelect={() => { assistantSettings.set((s) => ({ ...s, provider: 'server' })); close(); }}>This server’s model</MenuItem>
          )}
          <div className="menu-sep" role="separator" />
          <MenuItem icon={Settings2} onSelect={() => { close(); navigate(href.settings('assistant')); }}>Model settings…</MenuItem>
        </>
      )}
    </Menu>
  );
}

/** Settings → Natural language. */
export function AssistantSettingsPanel() {
  const { backend, isServer } = useApp();
  const { settings, info, missing } = useAssistant();
  const providers = info?.server_provider ? [SERVER_PROVIDER, ...PROVIDERS] : PROVIDERS;
  const preset = providerById(settings.provider);
  const cfg = providerConfig(settings);
  const [showKey, setShowKey] = useState(false);
  const [models, setModels] = useState<{ id: string; owned_by?: string }[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [filter, setFilter] = useState('');
  const [test, setTest] = useState<{ state: 'idle' | 'running' | 'ok' | 'error'; text?: string }>({ state: 'idle' });

  // A different provider has a different model list.
  useEffect(() => { setModels(null); setLoadErr(null); setFilter(''); setTest({ state: 'idle' }); }, [settings.provider]);

  const disabled = info !== null && !info.enabled;
  const localOnServer = isServer && preset.local && info && !info.private_network;

  const loadModels = async () => {
    setLoading(true);
    setLoadErr(null);
    try {
      setModels(await backend.assistantModels(targetFor(assistantSettings.get())));
    } catch (e) {
      setLoadErr(errorText(e));
    } finally {
      setLoading(false);
    }
  };

  const runTest = async () => {
    setTest({ state: 'running' });
    const t0 = performance.now();
    try {
      const s = assistantSettings.get();
      const res = await backend.assistantChat(targetFor(s), {
        model: providerConfig(s).model,
        messages: [{ role: 'user', content: 'Reply with the single word OK.' }],
      });
      const said = String(res?.choices?.[0]?.message?.content ?? '').trim();
      setTest({ state: 'ok', text: `Connected in ${((performance.now() - t0) / 1000).toFixed(1)} s${said ? ` · the model said “${said.slice(0, 40)}”` : ''}` });
    } catch (e) {
      setTest({ state: 'error', text: errorText(e) });
    }
  };

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return (models ?? []).filter((m) => !q || m.id.toLowerCase().includes(q)).slice(0, 60);
  }, [models, filter]);

  return (
    <>
      <div className="settings-card">
        <div className="setting-row">
          <div className="setting-text">
            <label className="setting-label" htmlFor="nl-on">Do things in plain words</label>
            <span className="setting-desc">
              Type what you want, like “shrink these photos and remove the location”, in the search or after dropping a file. A model picks the tools and runs them here.
            </span>
          </div>
          <div className="setting-control">
            <Switch id="nl-on" checked={settings.enabled && !disabled} disabled={disabled} onChange={(v) => assistantSettings.set((s) => ({ ...s, enabled: v }))} />
          </div>
        </div>
      </div>
      {disabled && <Callout tone="info">The administrator turned natural-language mode off on this server.</Callout>}

      <h3 className="nl-heading">Model</h3>
      <div className="provider-grid" role="radiogroup" aria-label="Model provider">
        {providers.map((p) => {
          const Icon = providerIcon(p);
          const on = p.id === settings.provider;
          return (
            <button
              key={p.id}
              type="button"
              role="radio"
              aria-checked={on}
              className={cx('provider-card', on && 'is-on')}
              onClick={() => assistantSettings.set((s) => ({ ...s, provider: p.id }))}
            >
              <span className="provider-card-top">
                <Icon size={15} aria-hidden />
                <span className="provider-name">{p.name}</span>
                {p.local && <Badge tone="success">Local</Badge>}
              </span>
              <span className="provider-blurb">{p.blurb}</span>
            </button>
          );
        })}
      </div>

      <div className="settings-card nl-fields">
        {settings.provider !== 'server' && (
          <div className="nl-field">
            <label className="setting-label" htmlFor="nl-url">Base URL</label>
            <div className="nl-input-row">
              <input
                id="nl-url"
                className="input mono"
                value={cfg.baseUrl}
                placeholder="https://example.com/v1"
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => updateProvider(settings.provider, { baseUrl: e.target.value })}
              />
              {preset.baseUrl && cfg.baseUrl !== preset.baseUrl && (
                <Button size="sm" variant="ghost" onClick={() => updateProvider(settings.provider, { baseUrl: preset.baseUrl })}>Reset</Button>
              )}
            </div>
            <span className="setting-desc">Any endpoint that speaks the OpenAI chat-completions API with tool calling. It usually ends in <span className="mono">/v1</span>.</span>
            {localOnServer && (
              <Callout tone="warn">
                This address is resolved on the Dogubako server, not on your computer, and the server only reaches public addresses. Use the server’s own address for {preset.name}, or ask the administrator to set <span className="mono">DOGUBAKO_ASSISTANT_ALLOW_PRIVATE=1</span>.
              </Callout>
            )}
          </div>
        )}

        {settings.provider !== 'server' && (
          <div className="nl-field">
            <label className="setting-label" htmlFor="nl-key">API key{!preset.needsKey && <span className="muted"> (optional)</span>}</label>
            <div className="nl-input-row">
              <input
                id="nl-key"
                className="input mono"
                type={showKey ? 'text' : 'password'}
                value={cfg.apiKey}
                placeholder={preset.needsKey ? 'sk-…' : 'Not needed for most local servers'}
                spellCheck={false}
                autoComplete="off"
                onChange={(e) => updateProvider(settings.provider, { apiKey: e.target.value })}
              />
              <Button size="sm" variant="ghost" icon={showKey ? EyeOff : Eye} onClick={() => setShowKey((v) => !v)} aria-label={showKey ? 'Hide key' : 'Show key'} />
            </div>
            <span className="setting-desc">
              Saved in this browser only{isServer ? ', sent with each request to the model, never stored on the server, and forgotten when you sign out' : ''}.
              {preset.keyUrl && <> <a className="link-sm" href={preset.keyUrl} target="_blank" rel="noopener noreferrer"><KeyRound size={12} aria-hidden /> Get a key <ExternalLink size={11} aria-hidden /></a></>}
            </span>
          </div>
        )}

        <div className="nl-field">
          <label className="setting-label" htmlFor="nl-model">Model</label>
          <div className="nl-input-row">
            <input
              id="nl-model"
              className="input mono"
              value={cfg.model}
              placeholder={settings.provider === 'server' && info?.server_provider?.model ? info.server_provider.model : 'Model id, e.g. gpt-6-luna'}
              spellCheck={false}
              autoComplete="off"
              disabled={settings.provider === 'server' && !!info?.server_provider?.model}
              onChange={(e) => updateProvider(settings.provider, { model: e.target.value })}
              onBlur={(e) => { if (e.target.value.trim()) selectModel(settings.provider, e.target.value.trim()); }}
            />
            {!(settings.provider === 'server' && info?.server_provider?.model) && (
              <Button size="sm" variant="secondary" icon={RefreshCw} loading={loading} onClick={() => void loadModels()}>Load models</Button>
            )}
          </div>
          {preset.models.length > 0 && (
            <div className="model-suggest" role="list" aria-label="Suggested models">
              {preset.models.map((m) => (
                <button key={m.id} type="button" role="listitem" className={cx('model-pill', cfg.model === m.id && 'is-on')} onClick={() => selectModel(settings.provider, m.id)}>
                  {cfg.model === m.id ? <Check size={12} aria-hidden /> : <Sparkles size={12} aria-hidden />}
                  {m.label ?? m.id}
                  {m.note && <span className="muted">{m.note}</span>}
                </button>
              ))}
            </div>
          )}
          {loadErr && <span className="field-error">{loadErr}</span>}
          {models && (
            <div className="model-browser">
              <div className="model-browser-search">
                <Search size={13} aria-hidden />
                <input className="input input-bare" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder={`Filter ${models.length} models`} aria-label="Filter models" />
              </div>
              <ul className="model-browser-list">
                {shown.map((m) => (
                  <li key={m.id}>
                    <button type="button" className={cx('model-browser-item', cfg.model === m.id && 'is-on')} onClick={() => selectModel(settings.provider, m.id)}>
                      <span className="mono">{m.id}</span>
                      {m.owned_by && <span className="muted small">{m.owned_by}</span>}
                      {cfg.model === m.id && <Check size={13} aria-hidden />}
                    </button>
                  </li>
                ))}
                {shown.length === 0 && <li className="muted small model-browser-empty">{models.length ? 'No match. You can still type any model id above.' : 'The endpoint listed no models.'}</li>}
              </ul>
            </div>
          )}
          <span className="setting-desc">Pick a model that supports tool calling. You can type any id, including your own fine-tunes.</span>
        </div>

        <div className="nl-test">
          <Button size="sm" variant="secondary" icon={Zap} loading={test.state === 'running'} disabled={!!missing} onClick={() => void runTest()}>Test connection</Button>
          {missing && <span className="muted small">{missing}.</span>}
          {test.state === 'ok' && <span className="small ok-text"><Check size={13} aria-hidden /> {test.text}</span>}
          {test.state === 'error' && <span className="field-error">{test.text}</span>}
        </div>
      </div>

      <h3 className="nl-heading">Safety</h3>
      <div className="settings-card">
        <div className="setting-row">
          <div className="setting-text">
            <span className="setting-label">Ask before running</span>
            <span className="setting-desc">Tools that use the internet, write to folders or change files always wait for your OK. Choose “Every step” to approve each run.</span>
          </div>
          <div className="setting-control">
            <Segmented<ConfirmMode> label="Ask before running" value={settings.confirm} onChange={(v) => assistantSettings.set((s) => ({ ...s, confirm: v }))}
              options={[{ value: 'risky', label: 'Risky steps' }, { value: 'always', label: 'Every step' }]} />
          </div>
        </div>
      </div>
      <p className="muted small nl-privacy">
        {preset.local
          ? 'With a local model, nothing leaves your machine.'
          : `Your request, file names and tool results (like extracted text) go to ${settings.provider === 'server' ? 'the model your administrator chose' : preset.name}. The files themselves are processed ${isServer ? 'by this server' : 'on this device'} and are not uploaded to the model.`}
      </p>
    </>
  );
}
