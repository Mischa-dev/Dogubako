import { useMemo, useState } from 'react';
import { Download, WrapText } from 'lucide-react';
import { extForMime } from '../lib/renderers';
import { downloadText } from '../lib/clipboard';
import { Button, CopyButton, IconButton, Segmented } from '../components/ui';
import { JsonTree } from './JsonTree';

type View = 'preview' | 'source' | 'tree';

const baseMime = (m?: string) => (m ?? 'text/plain').split(';')[0].trim().toLowerCase();

export function TextOutput({ text, mime, filename = 'output', live }: { text: string; mime?: string; filename?: string; live?: boolean }) {
  const m = baseMime(mime);
  const json = useMemo(() => {
    if (m !== 'application/json') return undefined;
    try { return JSON.parse(text); } catch { return undefined; }
  }, [text, m]);
  const previewable = m === 'text/html' || m === 'text/markdown' || m === 'image/svg+xml';
  const options: { value: View; label: string }[] = [];
  if (previewable) options.push({ value: 'preview', label: 'Preview' });
  if (json !== undefined && typeof json === 'object' && json !== null) options.push({ value: 'tree', label: 'Tree' });
  if (options.length) options.push({ value: 'source', label: 'Source' });
  const [view, setView] = useState<View>(previewable ? 'preview' : 'source');
  const [wrap, setWrap] = useState(true);
  const effective = options.some((o) => o.value === view) ? view : 'source';
  const ext = extForMime(m);
  const lines = useMemo(() => (text.length < 400_000 ? text.split('\n').length : 0), [text]);

  // Markdown tools return rendered, sanitized HTML as text/html; raw markdown is shown as source.
  return (
    <div className="output-block">
      <div className="output-head">
        <span className="output-title">
          Output
          <span className="muted small"> · {m === 'text/plain' ? 'text' : m.replace(/^(text|application|image)\//, '')}{lines ? ` · ${lines.toLocaleString()} line${lines === 1 ? '' : 's'}` : ''}</span>
        </span>
        <span className="output-actions">
          {options.length > 1 && <Segmented size="sm" label="Output view" value={effective} onChange={setView} options={options} />}
          {effective === 'source' && <IconButton icon={WrapText} label={wrap ? 'Disable line wrap' : 'Wrap lines'} active={wrap} onClick={() => setWrap((w) => !w)} size={14} />}
          <CopyButton text={text} />
          <Button size="sm" variant="ghost" icon={Download} onClick={() => downloadText(text, `${filename}.${ext}`, m)}>.{ext}</Button>
        </span>
      </div>
      {effective === 'preview' && m === 'image/svg+xml' && (
        <div className="svg-preview checker"><img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(text)}`} alt="SVG output preview" /></div>
      )}
      {effective === 'preview' && (m === 'text/html' || m === 'text/markdown') && (
        <iframe className="html-preview" sandbox="" srcDoc={htmlDoc(text, m)} title="Rendered output preview" />
      )}
      {effective === 'tree' && <div className="tree-wrap"><JsonTree value={json} /></div>}
      {effective === 'source' && (
        <pre className={`code-view mono${wrap ? ' wrap' : ''}`} tabIndex={0} aria-label="Output text" aria-live={live ? 'polite' : undefined}>{text || <span className="muted">(empty)</span>}</pre>
      )}
    </div>
  );
}

function htmlDoc(body: string, mime: string): string {
  const content = mime === 'text/markdown' ? `<pre>${body.replace(/&/g, '&amp;').replace(/</g, '&lt;')}</pre>` : body;
  const dark = document.documentElement.dataset.theme === 'dark';
  return `<!doctype html><html><head><meta charset="utf-8"><style>
  :root{color-scheme:${dark ? 'dark' : 'light'}}
  body{font:14px/1.6 -apple-system,"SF Pro Text","Segoe UI Variable",system-ui,Inter,Roboto,sans-serif;margin:16px 20px;color:${dark ? '#e6e6e8' : '#1b1d22'};background:${dark ? '#1c1d21' : '#fff'}}
  pre,code{font-family:ui-monospace,"SF Mono","JetBrains Mono",Menlo,Consolas,monospace;font-size:12.5px}
  pre{background:${dark ? '#26272c' : '#f4f4f2'};padding:10px 12px;border-radius:6px;overflow:auto}
  table{border-collapse:collapse}td,th{border:1px solid ${dark ? '#3a3b40' : '#ddd'};padding:4px 8px}
  img{max-width:100%}a{color:${dark ? '#8fb0ff' : '#2f5fd0'}}blockquote{margin:0;padding-left:12px;border-left:3px solid ${dark ? '#3a3b40' : '#ddd'};color:${dark ? '#a6a8ae' : '#5d6068'}}
  </style></head><body>${content}</body></html>`;
}
