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
import type { ManualPrinterInput } from '@/lib/types';

/** Focused configuration for an explicitly compatible raw network printer. */
export function AddPrinterDialog({
  open,
  onOpenChange,
  onAdd,
  busy,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdd: (input: ManualPrinterInput) => Promise<void>;
  busy: boolean;
}) {
  const [input, setInput] = useState<ManualPrinterInput>({ displayName: '', host: '', port: 9100 });
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
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="manual-name">Display name</FieldLabel>
              <Input
                autoFocus
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
                aria-invalid={!!error}
                onChange={(e) => setInput({ ...input, host: e.target.value })}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="manual-port">TCP port</FieldLabel>
              <Input
                id="manual-port"
                type="number"
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
          <p className="text-muted-foreground my-4 text-xs leading-5">
            System-driver printing uses an installed operating-system queue. Add those printers through your system
            settings, then refresh discovery here.
          </p>
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
