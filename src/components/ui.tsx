import type { MouseEvent, ReactNode } from 'react';
import type { AppErr } from '../lib/types';

export const MOD = typeof navigator !== 'undefined' && /Mac/.test(navigator.userAgent) ? '⌘' : 'Ctrl+';

export function Btn(p: {
  children: ReactNode; onClick?: (e: MouseEvent) => void; disabled?: boolean;
  variant?: 'primary' | 'ghost'; title?: string;
}) {
  return (
    <button
      type="button"
      className={`btn btn-${p.variant ?? 'primary'}`}
      disabled={p.disabled}
      title={p.title}
      onClick={(e) => !p.disabled && p.onClick?.(e)}
    >
      <span>{p.children}</span>
    </button>
  );
}

export function Segmented<T extends string>(p: {
  label: string; value: T; onChange: (v: T) => void;
  options: { value: T; label: string; disabled?: boolean; hint?: string }[];
}) {
  return (
    <div className="seg" role="radiogroup" aria-label={p.label}>
      {p.options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          className="chip"
          aria-checked={p.value === o.value}
          aria-disabled={o.disabled || undefined}
          disabled={o.disabled}
          title={o.hint}
          onClick={() => !o.disabled && p.onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Field(p: { label: string; children: ReactNode; note?: ReactNode }) {
  return (
    <section className="field">
      <h3 className="micro">({p.label})</h3>
      {p.children}
      {p.note && <p className="note">{p.note}</p>}
    </section>
  );
}

export function Pill(p: { tone?: 'info' | 'warn'; title?: string; children: ReactNode }) {
  return <span className={`pill ${p.tone ?? 'info'}`} title={p.title}>{p.children}</span>;
}

export function Meter({ value }: { value: number }) {
  const N = 40;
  const on = Math.round(Math.max(0, Math.min(1, value)) * N);
  return (
    <div className="meter" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(Math.max(0, Math.min(1, value)) * 100)}>
      {Array.from({ length: N }, (_, i) => (
        <i key={i} className={i < on ? `on b${Math.floor((i / N) * 4)}` : ''} />
      ))}
    </div>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return <kbd className="kbd">{children}</kbd>;
}

export function Banner({ error, onRetry }: { error: AppErr; onRetry?: () => void }) {
  return (
    <div className="banner" role="alert">
      <div className="banner-head">
        <code>{error.code}</code>
        {onRetry && error.code !== 'E_CANCELLED' && (
          <button type="button" className="textbtn" onClick={onRetry}>(Retry)</button>
        )}
      </div>
      <pre className="banner-msg">{error.msg}</pre>
      <p className="note">{error.action}</p>
    </div>
  );
}
