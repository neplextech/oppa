import { LoaderCircle } from 'lucide-react';
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react';

/**
 * Shared application-level UI primitives.
 * Thin wrappers and compositions over shadcn components.
 */
import { Switch } from '@/components/ui/switch';
import { cn } from '@/lib/utils';

/* ── Status dot ─────────────────────────────────────────────────── */

export type StatusTone = 'connected' | 'warning' | 'error' | 'pending' | 'offline' | 'neutral';

export function StatusDot({ tone, pulse }: { tone: StatusTone; pulse?: boolean }) {
  return (
    <span className="relative flex size-[6px] shrink-0" aria-hidden>
      {pulse && (
        <span
          className={cn(
            'absolute inline-flex size-full animate-ping rounded-full opacity-60',
            tone === 'connected' && 'bg-[var(--color-connected)]',
            tone === 'warning' && 'bg-[var(--color-warning)]',
            tone === 'error' && 'bg-[var(--color-error)]',
            tone === 'pending' && 'bg-[var(--color-pending)]',
            (tone === 'offline' || tone === 'neutral') && 'bg-muted-foreground',
          )}
        />
      )}
      <span
        className={cn(
          'relative inline-flex size-[6px] rounded-full',
          tone === 'connected' && 'bg-[var(--color-connected)]',
          tone === 'warning' && 'bg-[var(--color-warning)]',
          tone === 'error' && 'bg-[var(--color-error)]',
          tone === 'pending' && 'bg-[var(--color-pending)]',
          (tone === 'offline' || tone === 'neutral') && 'bg-muted-foreground/40',
        )}
      />
    </span>
  );
}

/* ── Job / operational state badge ──────────────────────────────── */

export function StateBadge({
  state,
  className,
}: {
  state: 'submitted' | 'queued' | 'received' | 'failed' | 'cancelled' | (string & {});
  className?: string;
}) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-medium tracking-wide uppercase',
        state === 'submitted' && 'bg-[var(--color-connected)]/10 text-[var(--color-connected)]',
        state === 'failed' && 'bg-[var(--color-error)]/10 text-[var(--color-error)]',
        state === 'queued' && 'bg-[var(--color-pending)]/10 text-[var(--color-pending)]',
        state === 'received' && 'bg-[var(--color-pending)]/10 text-[var(--color-pending)]',
        state === 'cancelled' && 'bg-muted text-muted-foreground',
        !['submitted', 'failed', 'queued', 'received', 'cancelled'].includes(state) && 'bg-muted text-muted-foreground',
        className,
      )}
    >
      {state}
    </span>
  );
}

/* ── Inline busy button (wraps any button element) ──────────────── */

export function BusyButton({
  children,
  busy,
  className,
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { busy?: boolean }) {
  return (
    <button
      className={cn(
        'inline-flex items-center gap-1.5 rounded border border-border bg-secondary px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-accent disabled:pointer-events-none disabled:opacity-50',
        className,
      )}
      disabled={disabled || busy}
      {...props}
    >
      {busy && <LoaderCircle className="size-3 animate-spin" aria-hidden />}
      {children}
    </button>
  );
}

/* ── Compact section header ──────────────────────────────────────── */

export function SectionHeader({
  title,
  description,
  action,
  className,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn('flex items-start justify-between gap-4', className)}>
      <div>
        <h2 className="text-muted-foreground text-[11px] font-semibold tracking-wider uppercase">{title}</h2>
        {description && <p className="text-muted-foreground/70 mt-0.5 text-xs">{description}</p>}
      </div>
      {action}
    </div>
  );
}

/* ── Screen wrapper ──────────────────────────────────────────────── */

export function ScreenContainer({ children, className }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('flex h-full flex-col', className)}>{children}</div>;
}

export function ScreenHeader({
  title,
  description,
  action,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="border-border flex shrink-0 items-start justify-between gap-4 border-b px-5 py-4">
      <div>
        <h1 className="text-foreground text-sm font-semibold">{title}</h1>
        {description && <p className="text-muted-foreground mt-0.5 text-xs">{description}</p>}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

/* ── Empty state ─────────────────────────────────────────────────── */

export function EmptyState({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-12 text-center">
      <p className="text-foreground/70 text-xs font-semibold">{title}</p>
      <p className="text-muted-foreground max-w-xs text-[11px] leading-5">{description}</p>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/* ── Inline error banner ─────────────────────────────────────────── */

export function ErrorBanner({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="flex items-start gap-3 border-b border-[var(--color-error)]/20 bg-[var(--color-error)]/5 px-5 py-3 text-xs">
      <span className="mt-0.5 size-[6px] shrink-0 rounded-full bg-[var(--color-error)]" />
      <span className="min-w-0 flex-1 text-[var(--color-error)]/90">{message}</span>
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="text-muted-foreground hover:text-foreground shrink-0 transition-colors"
        >
          Retry
        </button>
      )}
    </div>
  );
}

/* ── Definition list ─────────────────────────────────────────────── */

export function DefList({ children, className }: HTMLAttributes<HTMLDListElement>) {
  return <dl className={cn('grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-[11px]', className)}>{children}</dl>;
}

export function DefTerm({ children }: { children: ReactNode }) {
  return <dt className="text-muted-foreground/80 whitespace-nowrap">{children}</dt>;
}

export function DefValue({ children, className }: { children: ReactNode; className?: string }) {
  return <dd className={cn('text-foreground/90 font-mono truncate', className)}>{children}</dd>;
}

/* ── Toggle (switch-style) ───────────────────────────────────────── */

export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return <Switch size="sm" checked={checked} onCheckedChange={onChange} aria-label={label} />;
}

/* ── Field label ─────────────────────────────────────────────────── */

export function FieldLabel({ htmlFor, children }: { htmlFor: string; children: ReactNode }) {
  return (
    <label htmlFor={htmlFor} className="text-muted-foreground mb-1 block text-[11px] font-medium">
      {children}
    </label>
  );
}

export const inputClass =
  'h-8 w-full rounded border border-border bg-secondary px-2.5 text-xs text-foreground outline-none placeholder:text-muted-foreground/50 focus:border-primary/50 focus:ring-1 focus:ring-primary/20 transition disabled:opacity-50';
