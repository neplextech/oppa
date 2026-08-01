import { RotateCcw, Save } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import type { OpenPrinterServerConfiguration } from '@/lib/types';

function messageFrom(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === 'string') return cause;
  if (cause && typeof cause === 'object' && 'message' in cause && typeof cause.message === 'string') {
    return cause.message;
  }
  return 'The server configuration could not be saved.';
}

export function ServerConfigurationForm({
  value,
  saving,
  resetting,
  onSave,
  onReset,
  compact = false,
}: {
  value: OpenPrinterServerConfiguration;
  saving: boolean;
  resetting: boolean;
  onSave: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onReset: () => Promise<void>;
  compact?: boolean;
}) {
  const [input, setInput] = useState(value);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setInput(value);
  }, [value]);

  const dirty = useMemo(() => input.serverUrl !== value.serverUrl, [input, value]);
  const busy = saving || resetting;

  async function submit() {
    setError(null);
    try {
      await onSave(input);
    } catch (cause) {
      setError(messageFrom(cause));
    }
  }

  async function reset() {
    setError(null);
    try {
      await onReset();
    } catch (cause) {
      setError(messageFrom(cause));
    }
  }

  return (
    <form
      className={compact ? 'space-y-4' : 'space-y-5 p-5'}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
    >
      <FieldGroup className={compact ? 'gap-3' : undefined}>
        <Field>
          <FieldLabel htmlFor="server-url">Server URL</FieldLabel>
          <Input
            id="server-url"
            type="url"
            inputMode="url"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            required
            disabled={busy}
            value={input.serverUrl}
            onChange={(event) => setInput({ serverUrl: event.target.value })}
          />
          {!compact ? <FieldDescription>Discovery supplies the pairing and gateway endpoints.</FieldDescription> : null}
        </Field>
      </FieldGroup>

      {error ? <FieldError role="alert">{error}</FieldError> : null}

      <div className="flex flex-wrap items-center gap-2">
        <Button type="submit" size="sm" disabled={!dirty || busy}>
          <Save className="size-3.5" aria-hidden />
          {saving ? 'Saving…' : 'Save service'}
        </Button>
        <Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => void reset()}>
          <RotateCcw className="size-3.5" aria-hidden />
          {resetting ? 'Restoring…' : 'Restore default'}
        </Button>
      </div>
    </form>
  );
}
