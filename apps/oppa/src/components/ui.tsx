import { LoaderCircle } from 'lucide-react';
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react';

import { cn } from '@/lib/utils';

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';

export function Button({
  className,
  variant = 'primary',
  size = 'default',
  busy = false,
  children,
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: 'default' | 'sm' | 'icon';
  busy?: boolean;
}) {
  return (
    <button
      className={cn(
        'inline-flex items-center justify-center gap-2 rounded-lg text-sm font-semibold transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500/50 disabled:pointer-events-none disabled:opacity-50',
        variant === 'primary' && 'bg-blue-600 text-white shadow-sm hover:bg-blue-700',
        variant === 'secondary' && 'border border-slate-200 bg-white text-slate-700 shadow-sm hover:bg-slate-50',
        variant === 'ghost' && 'text-slate-600 hover:bg-slate-100 hover:text-slate-950',
        variant === 'danger' && 'bg-red-600 text-white hover:bg-red-700',
        size === 'default' && 'h-10 px-4',
        size === 'sm' && 'h-8 px-3 text-xs',
        size === 'icon' && 'size-9',
        className,
      )}
      disabled={disabled || busy}
      {...props}
    >
      {busy ? <LoaderCircle className="size-4 animate-spin" aria-hidden /> : null}
      {children}
    </button>
  );
}

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn('rounded-xl border border-slate-200/80 bg-white shadow-[0_1px_2px_rgba(15,23,42,0.03)]', className)}
      {...props}
    />
  );
}

export function Badge({
  tone = 'neutral',
  className,
  children,
}: {
  tone?: 'success' | 'warning' | 'danger' | 'info' | 'neutral';
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-semibold',
        tone === 'success' && 'bg-emerald-50 text-emerald-700 ring-1 ring-emerald-600/15',
        tone === 'warning' && 'bg-amber-50 text-amber-700 ring-1 ring-amber-600/15',
        tone === 'danger' && 'bg-red-50 text-red-700 ring-1 ring-red-600/15',
        tone === 'info' && 'bg-blue-50 text-blue-700 ring-1 ring-blue-600/15',
        tone === 'neutral' && 'bg-slate-100 text-slate-600 ring-1 ring-slate-500/10',
        className,
      )}
    >
      {children}
    </span>
  );
}

export function StatusDot({
  tone,
  pulse = false,
}: {
  tone: 'green' | 'amber' | 'red' | 'slate' | 'blue';
  pulse?: boolean;
}) {
  return (
    <span className="relative flex size-2" aria-hidden>
      {pulse ? (
        <span
          className={cn(
            'absolute inline-flex size-full animate-ping rounded-full opacity-60',
            tone === 'green' && 'bg-emerald-400',
            tone === 'amber' && 'bg-amber-400',
            tone === 'red' && 'bg-red-400',
            tone === 'blue' && 'bg-blue-400',
            tone === 'slate' && 'bg-slate-400',
          )}
        />
      ) : null}
      <span
        className={cn(
          'relative inline-flex size-2 rounded-full',
          tone === 'green' && 'bg-emerald-500',
          tone === 'amber' && 'bg-amber-500',
          tone === 'red' && 'bg-red-500',
          tone === 'blue' && 'bg-blue-500',
          tone === 'slate' && 'bg-slate-400',
        )}
      />
    </span>
  );
}

export function FieldLabel({ children, htmlFor }: { children: ReactNode; htmlFor: string }) {
  return (
    <label className="mb-1.5 block text-xs font-semibold text-slate-700" htmlFor={htmlFor}>
      {children}
    </label>
  );
}

export const inputClassName =
  'h-10 w-full rounded-lg border border-slate-200 bg-white px-3 text-sm text-slate-900 outline-none transition placeholder:text-slate-400 focus:border-blue-500 focus:ring-3 focus:ring-blue-500/10';

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={cn(
        'relative h-6 w-11 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500/50',
        checked ? 'bg-blue-600' : 'bg-slate-300',
      )}
      onClick={() => onChange(!checked)}
    >
      <span
        className={cn(
          'absolute top-0.5 size-5 rounded-full bg-white shadow-sm transition-transform',
          checked ? 'translate-x-5' : 'translate-x-0.5',
        )}
      />
    </button>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex min-h-64 flex-col items-center justify-center px-6 py-12 text-center">
      <div className="mb-4 flex size-11 items-center justify-center rounded-xl bg-slate-100 text-slate-500">{icon}</div>
      <h3 className="text-sm font-semibold text-slate-900">{title}</h3>
      <p className="mt-1 max-w-sm text-sm leading-6 text-slate-500">{description}</p>
      {action ? <div className="mt-5">{action}</div> : null}
    </div>
  );
}
