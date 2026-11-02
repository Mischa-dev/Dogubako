import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { CircleAlert, CircleCheck, Info, X } from 'lucide-react';

export interface Toast { id: number; tone: 'info' | 'success' | 'error'; text: string }

let seq = 0;
let items: Toast[] = [];
const subs = new Set<(t: Toast[]) => void>();
const emit = () => subs.forEach((s) => s(items));

export function toast(text: string, tone: Toast['tone'] = 'info', ms = 3200) {
  const t = { id: ++seq, tone, text };
  items = [...items, t].slice(-4);
  emit();
  window.setTimeout(() => dismiss(t.id), ms);
}
toast.success = (t: string) => toast(t, 'success');
toast.error = (t: string) => toast(t, 'error', 6000);

function dismiss(id: number) {
  items = items.filter((x) => x.id !== id);
  emit();
}

export function Toaster() {
  const [list, setList] = useState<Toast[]>(items);
  useEffect(() => { subs.add(setList); return () => { subs.delete(setList); }; }, []);
  return createPortal(
    <div className="toaster" role="status" aria-live="polite">
      {list.map((t) => {
        const Icon = t.tone === 'success' ? CircleCheck : t.tone === 'error' ? CircleAlert : Info;
        return (
          <div key={t.id} className={`toast toast-${t.tone}`}>
            <Icon size={16} aria-hidden />
            <span>{t.text}</span>
            <button className="toast-x" onClick={() => dismiss(t.id)} aria-label="Dismiss"><X size={14} /></button>
          </div>
        );
      })}
    </div>,
    document.body,
  );
}
