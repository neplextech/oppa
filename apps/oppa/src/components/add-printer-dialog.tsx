import { LoaderCircle, Radar, Wifi } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Field, FieldGroup, FieldLabel, FieldError } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { agentClient } from '@/lib/agent-client';
import type { DiscoveredNetworkPrinter, ManualPrinterInput } from '@/lib/types';

/** Focused configuration for an explicitly compatible raw network printer. */
export function AddPrinterDialog({
  open,
  onOpenChange,
  onAdd,
  busy,
  networkDiscovery = false,
  systemQueues = true,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdd: (input: ManualPrinterInput) => Promise<void>;
  busy: boolean;
  /** Offer a DNS-SD scan for printer services (mobile). */
  networkDiscovery?: boolean;
  /** Whether operating-system print queues exist on this platform. */
  systemQueues?: boolean;
}) {
  const [input, setInput] = useState<ManualPrinterInput>({ displayName: '', host: '', port: 9100 });
  const [scan, setScan] = useState<{
    state: 'idle' | 'scanning' | 'done' | 'failed';
    printers: DiscoveredNetworkPrinter[];
    message?: string;
  }>({ state: 'idle', printers: [] });
  async function scanNetwork() {
    setScan({ state: 'scanning', printers: [] });
    try {
      const printers = await agentClient.discoverNetworkPrinters();
      setScan({ state: 'done', printers });
    } catch (cause) {
      setScan({
        state: 'failed',
        printers: [],
        message: cause instanceof Error ? cause.message : 'The network could not be scanned.',
      });
    }
  }
  const [verified, setVerified] = useState(false);
  const [error, setError] = useState<string | null>(null);
  function submit() {
    const host = input.host.trim();
    if (
      !input.displayName.trim() ||
      !host ||
      /[\s/?#@]/.test(host) ||
      host.includes('://') ||
      !Number.isInteger(input.port) ||
      input.port < 1 ||
      input.port > 65535
    ) {
      setError('Enter a name, a host without a URL scheme or path, and a port between 1 and 65535.');
      return;
    }
    setError(null);
    void onAdd({ ...input, host, displayName: input.displayName.trim() })
      .then(() => {
        onOpenChange(false);
        setInput({ displayName: '', host: '', port: 9100 });
        setVerified(false);
      })
      .catch((cause: unknown) =>
        setError(
          cause instanceof Error ? cause.message : 'Unable to add this printer. Check its address and try again.',
        ),
      );
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) {
          onOpenChange(next);
          setError(null);
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add network printer</DialogTitle>
          <DialogDescription>
            Connect directly to a printer that supports raw ESC/POS. A network address or port does not establish
            language compatibility.
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          {networkDiscovery && (
            <div className="border-border mb-5 rounded-xl border p-3">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="flex items-center gap-2 text-sm font-medium">
                    <Wifi className="text-primary size-4" aria-hidden /> Printers on this network
                  </p>
                  <p className="text-muted-foreground mt-0.5 text-xs leading-5">
                    Finds printers that advertise themselves. You can always enter an IP address below.
                  </p>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={busy || scan.state === 'scanning'}
                  onClick={() => void scanNetwork()}
                >
                  {scan.state === 'scanning' ? (
                    <LoaderCircle className="animate-spin" aria-hidden />
                  ) : (
                    <Radar aria-hidden />
                  )}
                  {scan.state === 'scanning' ? 'Scanning' : 'Scan'}
                </Button>
              </div>
              {scan.state === 'done' && scan.printers.length === 0 && (
                <p className="text-muted-foreground mt-3 text-xs leading-5">
                  No advertised printers were found. Enter the printer&apos;s IP address and port instead.
                </p>
              )}
              {scan.state === 'failed' && (
                <p className="mt-3 text-xs leading-5 text-[var(--color-error)]">{scan.message}</p>
              )}
              {scan.printers.length > 0 && (
                <ul className="mt-3 flex flex-col gap-1.5" aria-label="Discovered printers">
                  {scan.printers.map((printer) => (
                    <li key={`${printer.host}:${printer.port}`}>
                      <button
                        type="button"
                        disabled={printer.alreadyAdded}
                        className="border-border hover:bg-accent flex min-h-11 w-full items-center justify-between gap-3 rounded-lg border px-3 py-2 text-left disabled:opacity-60"
                        onClick={() => {
                          setInput({ displayName: printer.name, host: printer.host, port: printer.port });
                          setError(null);
                        }}
                      >
                        <span className="min-w-0">
                          <span className="block truncate text-sm font-medium">{printer.name}</span>
                          <span className="text-muted-foreground block truncate font-mono text-[11px]">
                            {printer.host}:{printer.port}
                            {printer.assumedPort ? ' · port assumed' : ''}
                          </span>
                        </span>
                        <span className="text-primary shrink-0 text-xs font-medium">
                          {printer.alreadyAdded ? 'Added' : 'Use'}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="manual-name">Display name</FieldLabel>
              <Input
                autoFocus={!networkDiscovery}
                id="manual-name"
                value={input.displayName}
                required
                maxLength={128}
                onChange={(e) => setInput({ ...input, displayName: e.target.value })}
              />
            </Field>
            <Field data-invalid={!!error}>
              <FieldLabel htmlFor="manual-host">Host name or IP address</FieldLabel>
              <Input
                id="manual-host"
                value={input.host}
                required
                spellCheck={false}
                autoCapitalize="none"
                autoCorrect="off"
                inputMode="url"
                aria-invalid={!!error}
                onChange={(e) => setInput({ ...input, host: e.target.value })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="manual-port">TCP port</FieldLabel>
              <Input
                id="manual-port"
                type="number"
                inputMode="numeric"
                min={1}
                max={65535}
                value={input.port}
                onChange={(e) => setInput({ ...input, port: Number(e.target.value) })}
              />
            </Field>
            <Field>
              <label className="flex items-start gap-2 text-xs leading-5">
                <input
                  className="accent-primary mt-1"
                  type="checkbox"
                  checked={verified}
                  onChange={(e) => setVerified(e.target.checked)}
                />
                I have verified that this printer supports ESC/POS.
              </label>
            </Field>
            {error && <FieldError role="alert">{error}</FieldError>}
          </FieldGroup>
          {systemQueues ? (
            <p className="text-muted-foreground my-4 text-xs leading-5">
              System-driver printing uses an installed operating-system queue. Add those printers through your system
              settings, then refresh discovery here.
            </p>
          ) : (
            <p className="text-muted-foreground my-4 text-xs leading-5">
              Use the printer&apos;s raw TCP port, usually 9100. USB and Bluetooth printers are not supported on this
              device yet.
            </p>
          )}
          <DialogFooter>
            <Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !verified}>
              {busy ? 'Adding…' : 'Add printer'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
