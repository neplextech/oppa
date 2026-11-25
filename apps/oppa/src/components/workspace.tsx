import { Copy, Search } from 'lucide-react';
import { useEffect, useRef, useState, type ReactNode } from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';

/** Persistent inspector with pointer and keyboard resizing; narrow workspaces show the inspector alone. */
export function MasterDetail({
  storageKey,
  master,
  detail,
}: {
  storageKey: string;
  master: ReactNode;
  detail: ReactNode;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(() => {
    const saved = Number(localStorage.getItem(storageKey));
    return Number.isFinite(saved) && saved >= 260 && saved <= 480 ? saved : 320;
  });
  const hasDetail = Boolean(detail);
  const hadDetail = useRef(false);
  useEffect(() => {
    if (hasDetail && host.current && host.current.clientWidth <= 650) {
      const heading = host.current.querySelector<HTMLElement>('aside h2');
      if (heading) {
        heading.tabIndex = -1;
        heading.focus();
      }
    } else if (!hasDetail && hadDetail.current) {
      host.current?.querySelector<HTMLElement>('[data-row-id][tabindex="0"]')?.focus();
    }
    hadDetail.current = hasDetail;
  }, [hasDetail]);
  function resize(value: number) {
    const next = Math.round(Math.max(260, Math.min(480, (host.current?.clientWidth ?? 900) - 280, value)));
    setWidth(next);
    localStorage.setItem(storageKey, String(next));
  }
  return (
    <div ref={host} className="workspace-split min-h-0 min-w-0 flex-1">
      <div className={detail ? 'master-detail has-inspector' : 'master-detail'}>
        <div className="master-content">{master}</div>
        {detail && (
          <>
            <div
              role="separator"
              tabIndex={0}
              aria-label="Resize inspector"
              aria-orientation="vertical"
              aria-valuemin={260}
              aria-valuemax={480}
              aria-valuenow={width}
              className="inspector-divider"
              onKeyDown={(event) => {
                if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
                  event.preventDefault();
                  resize(width + (event.key === 'ArrowLeft' ? 20 : -20));
                }
              }}
              onPointerDown={(event) => event.currentTarget.setPointerCapture(event.pointerId)}
              onPointerMove={(event) => {
                if (event.currentTarget.hasPointerCapture(event.pointerId))
                  resize((host.current?.getBoundingClientRect().right ?? 0) - event.clientX);
              }}
              onPointerUp={(event) => event.currentTarget.releasePointerCapture(event.pointerId)}
            />
            <aside className="inspector-content" style={{ width }}>
              {detail}
            </aside>
          </>
        )}
      </div>
    </div>
  );
}

/** Search and filter actions share a compact wrapping toolbar. */
export function Toolbar({ children }: { children: ReactNode }) {
  return <div className="workspace-toolbar">{children}</div>;
}

/** Labeled search input with the approved control geometry. */
export function SearchField({
  value,
  onChange,
  label,
}: {
  value: string;
  onChange: (value: string) => void;
  label: string;
}) {
  return (
    <div className="relative min-w-40 flex-1">
      <Search className="text-muted-foreground pointer-events-none absolute top-2.5 left-3 size-4" aria-hidden />
      <Input
        aria-label={label}
        placeholder={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="pl-9"
      />
    </div>
  );
}

/** Readable metadata with optional explicit clipboard feedback. */
export function KeyValue({ label, value, copy = false }: { label: string; value: string; copy?: boolean }) {
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState(false);
  return (
    <div className="relative flex min-w-0 items-start justify-between gap-3 py-1">
      <div className="min-w-0">
        <dt className="text-muted-foreground text-xs">{label}</dt>
        <dd className={copy ? 'mt-1 font-mono text-xs break-all' : 'mt-1 text-sm break-words'}>{value}</dd>
      </div>
      {copy && (
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Copy ${label}`}
          title={error ? 'Copy failed; select the value to copy' : copied ? 'Copied' : `Copy ${label}`}
          onClick={() => {
            void navigator.clipboard
              .writeText(value)
              .then(() => {
                setCopied(true);
                setError(false);
              })
              .catch(() => setError(true));
          }}
        >
          <Copy />
        </Button>
      )}
      <span className="sr-only" role="status">
        {error ? 'Unable to copy' : copied ? `${label} copied` : ''}
      </span>
    </div>
  );
}

/** Destructive operations use the shared modal focus trap and remain open on errors. */
export function ConfirmAction({
  open,
  onOpenChange,
  title,
  description,
  action,
  busy,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  action: () => Promise<void>;
  busy: boolean;
}) {
  const [error, setError] = useState<string | null>(null);
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => {
        if (!busy) {
          setError(null);
          onOpenChange(value);
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {error && (
          <p className="text-error text-sm" role="alert">
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            variant="destructive"
            disabled={busy}
            onClick={() => {
              void action()
                .then(() => onOpenChange(false))
                .catch((cause: unknown) =>
                  setError(cause instanceof Error ? cause.message : 'The operation failed. Try again.'),
                );
            }}
          >
            {busy ? 'Working…' : 'Confirm'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
