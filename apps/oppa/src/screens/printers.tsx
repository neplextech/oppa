import {
  CirclePlus,
  EthernetPort,
  MonitorCog,
  MoreHorizontal,
  Printer,
  RefreshCw,
  Send,
  Trash2,
  Usb,
} from 'lucide-react';
import { useState } from 'react';

import { PageHeader } from '@/components/page';
import { Badge, Button, Card, FieldLabel, StatusDot, Toggle, inputClassName } from '@/components/ui';
import type { ManualPrinterInput, PrinterSummary } from '@/lib/types';
import { titleCase } from '@/lib/utils';

export function PrintersScreen({
  printers,
  busy,
  onRefresh,
  onConfigure,
  onAddManual,
  onRemove,
  onTest,
}: {
  printers: PrinterSummary[];
  busy: string | null;
  onRefresh: () => Promise<void>;
  onConfigure: (printerId: string, changes: { displayName?: string; enabled?: boolean }) => Promise<void>;
  onAddManual: (input: ManualPrinterInput) => Promise<void>;
  onRemove: (printerId: string) => Promise<void>;
  onTest: (printerId: string) => Promise<void>;
}) {
  const [showForm, setShowForm] = useState(false);
  const [input, setInput] = useState<ManualPrinterInput>({
    displayName: '',
    host: '',
    port: 9100,
  });

  return (
    <div className="mx-auto max-w-6xl">
      <PageHeader
        eyebrow="Local inventory"
        title="Printers"
        description="Enable the printers this agent may use. Logical routing stays in the connected application."
        action={
          <div className="flex gap-2">
            <Button variant="secondary" busy={busy === 'refresh-printers'} onClick={() => void onRefresh()}>
              <RefreshCw className="size-4" aria-hidden />
              Refresh
            </Button>
            <Button onClick={() => setShowForm((value) => !value)}>
              <CirclePlus className="size-4" aria-hidden />
              Network printer
            </Button>
          </div>
        }
      />

      {showForm ? (
        <Card className="mb-6 border-blue-200 bg-blue-50/40 p-5">
          <div className="grid gap-4 md:grid-cols-[1.2fr_1fr_0.5fr_auto] md:items-end">
            <div>
              <FieldLabel htmlFor="manual-name">Display name</FieldLabel>
              <input
                id="manual-name"
                className={inputClassName}
                placeholder="Counter printer"
                value={input.displayName}
                onChange={(event) => setInput((current) => ({ ...current, displayName: event.target.value }))}
              />
            </div>
            <div>
              <FieldLabel htmlFor="manual-host">Host or IP address</FieldLabel>
              <input
                id="manual-host"
                className={inputClassName}
                placeholder="192.168.1.82"
                value={input.host}
                onChange={(event) => setInput((current) => ({ ...current, host: event.target.value }))}
              />
            </div>
            <div>
              <FieldLabel htmlFor="manual-port">Port</FieldLabel>
              <input
                id="manual-port"
                className={inputClassName}
                min={1}
                max={65535}
                type="number"
                value={input.port}
                onChange={(event) =>
                  setInput((current) => ({
                    ...current,
                    port: Number(event.target.value),
                  }))
                }
              />
            </div>
            <Button
              busy={busy === 'add-manual-printer'}
              disabled={!input.displayName.trim() || !input.host.trim()}
              onClick={() => {
                void onAddManual(input).then(() => {
                  setShowForm(false);
                  setInput({ displayName: '', host: '', port: 9100 });
                });
              }}
            >
              Add printer
            </Button>
          </div>
          <p className="mt-3 text-[11px] text-slate-500">
            Raw TCP submissions use an explicit timeout. The default port is 9100.
          </p>
        </Card>
      ) : null}

      <Card className="overflow-hidden">
        <div className="hidden grid-cols-[minmax(220px,1.2fr)_0.8fr_0.8fr_0.55fr_150px] gap-4 border-b border-slate-100 bg-slate-50/70 px-5 py-3 text-[10px] font-bold tracking-[0.12em] text-slate-400 uppercase md:grid">
          <span>Printer</span>
          <span>Connection</span>
          <span>Capabilities</span>
          <span>Status</span>
          <span className="text-right">Actions</span>
        </div>
        <div className="divide-y divide-slate-100">
          {printers.map((printer) => (
            <PrinterRow
              key={printer.id}
              printer={printer}
              busy={busy}
              onConfigure={onConfigure}
              onRemove={onRemove}
              onTest={onTest}
            />
          ))}
        </div>
      </Card>
    </div>
  );
}

function PrinterRow({
  printer,
  busy,
  onConfigure,
  onRemove,
  onTest,
}: {
  printer: PrinterSummary;
  busy: string | null;
  onConfigure: (printerId: string, changes: { displayName?: string; enabled?: boolean }) => Promise<void>;
  onRemove: (printerId: string) => Promise<void>;
  onTest: (printerId: string) => Promise<void>;
}) {
  const Icon =
    printer.connectionType === 'network'
      ? EthernetPort
      : printer.connectionType === 'virtual'
        ? MonitorCog
        : printer.connectionType === 'usb'
          ? Usb
          : Printer;

  return (
    <div className="grid gap-4 px-5 py-4 md:grid-cols-[minmax(220px,1.2fr)_0.8fr_0.8fr_0.55fr_150px] md:items-center">
      <div className="flex min-w-0 items-center gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-slate-100 text-slate-500">
          <Icon className="size-4" aria-hidden />
        </div>
        <div className="min-w-0">
          <input
            aria-label={`Display name for ${printer.displayName}`}
            className="block w-full truncate rounded border border-transparent bg-transparent p-0 text-sm font-semibold text-slate-800 outline-none hover:border-slate-200 focus:border-blue-400 focus:px-1.5"
            defaultValue={printer.displayName}
            onBlur={(event) => {
              const displayName = event.target.value.trim();
              if (displayName && displayName !== printer.displayName) {
                void onConfigure(printer.id, { displayName });
              }
            }}
          />
          <div className="mt-0.5 truncate text-[11px] text-slate-400">{printer.sourceName}</div>
        </div>
      </div>
      <div>
        <div className="text-xs font-semibold text-slate-600">{titleCase(printer.connectionType)}</div>
        <div className="mt-0.5 truncate text-[11px] text-slate-400">{printer.address ?? 'Local agent'}</div>
      </div>
      <div className="flex flex-wrap gap-1">
        {printer.capabilities?.widths.map((width) => (
          <Badge key={width}>{width} mm</Badge>
        ))}
        {printer.capabilities?.supportsCut ? <Badge>Cut</Badge> : null}
        {!printer.capabilities ? <span className="text-xs text-slate-400">Unknown</span> : null}
      </div>
      <div className="flex items-center gap-2 text-xs font-semibold text-slate-600">
        <StatusDot tone={printer.available ? 'green' : 'red'} />
        {printer.available ? 'Ready' : 'Unavailable'}
      </div>
      <div className="flex items-center justify-end gap-1">
        <Toggle
          checked={printer.enabled}
          label={`${printer.enabled ? 'Disable' : 'Enable'} ${printer.displayName}`}
          onChange={(enabled) => void onConfigure(printer.id, { enabled })}
        />
        <Button
          variant="ghost"
          size="icon"
          busy={busy === `test-${printer.id}`}
          disabled={!printer.enabled || !printer.available}
          onClick={() => void onTest(printer.id)}
          title="Send test print"
        >
          <Send className="size-4" aria-hidden />
        </Button>
        {printer.connectionType === 'network' ? (
          <Button
            variant="ghost"
            size="icon"
            busy={busy === `remove-${printer.id}`}
            onClick={() => void onRemove(printer.id)}
            title="Remove printer"
          >
            <Trash2 className="size-4 text-red-500" aria-hidden />
          </Button>
        ) : (
          <Button variant="ghost" size="icon" disabled title="More actions">
            <MoreHorizontal className="size-4" aria-hidden />
          </Button>
        )}
      </div>
    </div>
  );
}
