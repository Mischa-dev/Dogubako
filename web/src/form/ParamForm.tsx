import { memo, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import type { ParamSpec } from '../lib/types';
import { isVisible, type FormErrors, type FormValues } from '../lib/schema';
import { Field, type Setter } from './Field';

export const ParamForm = memo(function ParamForm({ params, values, errors, onChange, mono, exclude, idPrefix = 'form' }: {
  params: ParamSpec[];
  values: FormValues;
  errors: FormErrors;
  onChange: Setter;
  mono?: boolean;
  exclude?: string[];
  idPrefix?: string;
}) {
  const visible = params.filter((p) => !exclude?.includes(p.name) && isVisible(p, values, params));
  const main = visible.filter((p) => !p.advanced);
  const adv = visible.filter((p) => p.advanced);
  const advHasError = adv.some((p) => errors[p.name]);
  const changed = adv.filter((p) => p.type !== 'file' && JSON.stringify(values[p.name] ?? '') !== JSON.stringify(p.default ?? (p.type === 'boolean' ? false : ''))).length;
  const [open, setOpen] = useState(false);
  const isOpen = open || advHasError;

  if (!params.length) return null;
  return (
    <div className="param-form">
      {main.map((p) => (
        <Field key={p.name} param={p} value={values[p.name]} error={errors[p.name]} onChange={onChange} mono={mono} />
      ))}
      {adv.length > 0 && (
        <div className="advanced">
          <button
            type="button"
            className="advanced-toggle"
            aria-expanded={isOpen}
            aria-controls={`${idPrefix}-adv`}
            onClick={() => setOpen((o) => !o)}
          >
            <ChevronRight size={14} aria-hidden className="chev" />
            Advanced options
            <span className="advanced-count">{adv.length}{changed ? ` · ${changed} changed` : ''}</span>
          </button>
          {isOpen && (
            <div className="advanced-body" id={`${idPrefix}-adv`}>
              {adv.map((p) => (
                <Field key={p.name} param={p} value={values[p.name]} error={errors[p.name]} onChange={onChange} mono={mono} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
});
