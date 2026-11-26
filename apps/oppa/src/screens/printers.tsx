import { EthernetPort, MonitorCog, MoreHorizontal, Plus, Printer, RefreshCw, Send, Trash2, Usb, X } from 'lucide-react';
import { useState } from 'react';

import { AddPrinterDialog } from '@/components/add-printer-dialog';
import { PdfTestSection } from '@/components/printer-pdf-test';
import { EmptyState, ScreenContainer, ScreenHeader, StateBadge, StatusDot, Toggle, inputClass } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { ConfirmAction, KeyValue, MasterDetail, SearchField, Toolbar } from '@/components/workspace';
import type { PlatformCapabilities } from '@/lib/platform';
import type {
  JobSummary,
  ManualPrinterInput,
  PdfPrintSettings,
  PrinterConfigurationChanges,
  PrinterSummary,
} from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';
import { listNavigation, useSelection } from '@/lib/workspace';

interface PrinterScreenProps {
  capabilities: PlatformCapabilities;
  printers: PrinterSummary[];
  jobs: JobSummary[];
  busy: string | null;
  developerMode: boolean;
  onRefresh: () => Promise<void>;
  onConfigure: (id: string, changes: PrinterConfigurationChanges) => Promise<void>;
  onAddManual: (input: ManualPrinterInput) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
  onTestPdf: (id: string, file: File, settings: PdfPrintSettings) => Promise<void>;
}

function DeviceIcon({ printer }: { printer: PrinterSummary }) {
  const Icon =
    printer.connectionType === 'network'
      ? EthernetPort
      : printer.connectionType === 'virtual'
        ? MonitorCog
        : printer.connectionType === 'usb'
          ? Usb
          : Printer;
  return <Icon className="size-4" aria-hidden />;
}

export function PrintersScreen(props: PrinterScreenProps) {
  const { printers, busy, onRefresh, onConfigure, onAddManual, onRemove, onTest } = props;
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const [showAdd, setShowAdd] = useState(false);
  const [removeId, setRemoveId] = useState<string | null>(null);
  const [selectedId, select] = useSelection('oppa-printer-selection');
  const selected = printers.find((p) => p.id === selectedId);
  const filtered = printers.filter(
    (p) =>
      [p.displayName, p.sourceName, p.address ?? ''].some((value) =>
        value.toLowerCase().includes(query.toLowerCase()),
      ) &&
      (filter === 'all' || (filter === 'enabled' ? p.enabled : filter === 'disabled' ? !p.enabled : !p.available)),
  );
  return (
    <ScreenContainer>
      <ScreenHeader
        title="Printers"
        description="Choose and configure the devices this server may use."
        action={
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={busy !== null}
              onClick={() => void onRefresh().catch(() => undefined)}
            >
              <RefreshCw className={busy === 'refresh-printers' ? 'animate-spin' : undefined} />
              Refresh
            </Button>
            <Button size="sm" onClick={() => setShowAdd(true)}>
              <Plus />
              Add printer
            </Button>
          </div>
        }
      />
      <Toolbar>
        <SearchField label="Search printers" value={query} onChange={setQuery} />
        <select aria-label="Filter printers" value={filter} onChange={(e) => setFilter(e.target.value)}>
          <option value="all">All printers</option>
          <option value="enabled">Enabled</option>
          <option value="disabled">Disabled</option>
          <option value="unavailable">Unavailable</option>
        </select>
        <span className="text-muted-foreground text-xs">{filtered.length} devices</span>
      </Toolbar>
      <MasterDetail
        storageKey="oppa-printer-inspector-width"
        master={
          filtered.length ? (
            <div role="list" aria-label="Printers">
              {filtered.map((printer) => (
                <ContextMenu key={printer.id}>
                  <ContextMenuTrigger
                    render={<div />}
                    className="device-row"
                    role="listitem"
                    tabIndex={
                      selectedId === printer.id ||
                      (!filtered.some((item) => item.id === selectedId) && filtered[0]?.id === printer.id)
                        ? 0
                        : -1
                    }
                    data-row-id={printer.id}
                    aria-selected={selectedId === printer.id}
                    onClick={() => select(printer.id)}
                    onKeyDown={(e) => listNavigation(e, select)}
                  >
                    <span className="device-icon">
                      <DeviceIcon printer={printer} />
                    </span>
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium">{printer.displayName}</p>
                      <p className="text-muted-foreground mt-1 flex items-center gap-2 text-xs">
                        <span className="truncate">{titleCase(printer.connectionType)}</span>
                        <StatusDot tone={printer.available ? 'connected' : 'warning'} />
                        <span>{printer.available ? 'Available' : 'Unavailable'}</span>
                      </p>
                    </div>
                    <span className="text-muted-foreground text-xs">{printer.enabled ? 'Enabled' : 'Disabled'}</span>
                    <span onClick={(e) => e.stopPropagation()}>
                      <Toggle
                        disabled={busy !== null}
                        checked={printer.enabled}
                        label={`Allow server access to ${printer.displayName}`}
                        onChange={(enabled) => {
                          void onConfigure(printer.id, { enabled }).catch(() => undefined);
                        }}
                      />
                    </span>
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <ContextMenuGroup>
                      <ContextMenuItem onClick={() => select(printer.id)}>Inspect printer</ContextMenuItem>
                      <ContextMenuItem
                        disabled={!printer.enabled || !printer.available || busy !== null}
                        onClick={() => void onTest(printer.id).catch(() => undefined)}
                      >
                        <Send />
                        Send test print
                      </ContextMenuItem>
                      <ContextMenuItem
                        disabled={busy !== null}
                        onClick={() =>
                          void onConfigure(printer.id, { enabled: !printer.enabled }).catch(() => undefined)
                        }
                      >
                        {printer.enabled ? 'Disable server access' : 'Enable server access'}
                      </ContextMenuItem>
                      {printer.connectionType === 'network' && (
                        <ContextMenuItem variant="destructive" onClick={() => setRemoveId(printer.id)}>
                          <Trash2 />
                          Remove printer…
                        </ContextMenuItem>
                      )}
                    </ContextMenuGroup>
                  </ContextMenuContent>
                </ContextMenu>
              ))}
            </div>
          ) : (
            <EmptyState
              title={printers.length ? 'No matching printers' : 'No printers discovered'}
              description={
                printers.length
                  ? 'Try a different search or filter.'
                  : props.capabilities.systemPrintQueues
                    ? 'Refresh discovery for installed queues or add an ESC/POS network printer.'
                    : 'Add an ESC/POS network printer by IP address, or scan this network for printers.'
              }
              action={
                <Button size="sm" variant="outline" onClick={() => void onRefresh().catch(() => undefined)}>
                  Refresh discovery
                </Button>
              }
            />
          )
        }
        detail={
          selected ? (
            <PrinterInspector
              key={selected.id}
              {...props}
              printer={selected}
              onClose={() => select(null)}
              onRequestRemove={() => setRemoveId(selected.id)}
            />
          ) : null
        }
      />
      <AddPrinterDialog
        open={showAdd}
        onOpenChange={setShowAdd}
        onAdd={onAddManual}
        busy={busy !== null}
        networkDiscovery={props.capabilities.networkServiceDiscovery}
        systemQueues={props.capabilities.systemPrintQueues}
      />
      <ConfirmAction
        open={removeId !== null}
        onOpenChange={(open) => {
          if (!open) setRemoveId(null);
        }}
        title="Remove network printer?"
        description="This removes the local device configuration. It does not cancel jobs already submitted to the transport."
        busy={busy !== null}
        action={async () => {
          if (removeId) {
            await onRemove(removeId);
            if (selectedId === removeId) select(null);
            setRemoveId(null);
          }
        }}
      />
    </ScreenContainer>
  );
}

function PrinterInspector({
  printer,
  jobs,
  busy,
  onConfigure,
  onTest,
  onTestPdf,
  onClose,
  onRequestRemove,
}: PrinterScreenProps & { printer: PrinterSummary; onClose: () => void; onRequestRemove: () => void }) {
  const [name, setName] = useState(printer.displayName);
  const [error, setError] = useState<string | null>(null);
  return (
    <>
      <div className="inspector-heading">
        <h2 className="truncate text-sm font-semibold">{printer.displayName}</h2>
        <div className="flex gap-1">
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Printer actions" />}>
              <MoreHorizontal />
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuGroup>
                <DropdownMenuItem onClick={() => void navigator.clipboard.writeText(printer.id)}>
                  Copy printer ID
                </DropdownMenuItem>
                {printer.connectionType === 'network' && (
                  <DropdownMenuItem variant="destructive" onClick={onRequestRemove}>
                    Remove printer…
                  </DropdownMenuItem>
                )}
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button variant="ghost" size="icon-sm" aria-label="Close printer inspector" onClick={onClose}>
            <X />
          </Button>
        </div>
      </div>
      <section className="inspector-section">
        <div className="mb-3 flex items-center gap-2">
          <StatusDot tone={printer.available ? 'connected' : 'warning'} />
          <span>{printer.available ? 'Available to local transport' : 'Unavailable to local transport'}</span>
        </div>
        <Button
          size="sm"
          disabled={!printer.enabled || !printer.available || busy !== null}
          onClick={() => void onTest(printer.id).catch(() => undefined)}
        >
          <Send />
          {busy === `test-${printer.id}` ? 'Submitting…' : 'Send test print'}
        </Button>
        <p className="text-muted-foreground mt-3 text-xs">
          Transport acceptance does not verify physical printing or paper status.
        </p>
      </section>
      <section className="inspector-section">
        <h3>Device & access</h3>
        <label className="text-muted-foreground text-xs" htmlFor="printer-name">
          Display name
        </label>
        <form
          className="mt-2 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            setError(null);
            void onConfigure(printer.id, { displayName: name.trim() }).catch((cause: unknown) =>
              setError(cause instanceof Error ? cause.message : 'Unable to save the name.'),
            );
          }}
        >
          <Input id="printer-name" value={name} maxLength={128} onChange={(e) => setName(e.target.value)} />
          <Button
            type="submit"
            variant="outline"
            size="sm"
            disabled={busy !== null || !name.trim() || name.trim() === printer.displayName}
          >
            Save
          </Button>
        </form>
        {error && (
          <p role="alert" className="text-error mt-2 text-xs">
            {error}
          </p>
        )}
        <div className="mt-4 flex items-center justify-between gap-3">
          <div>
            <p className="text-sm">Server access</p>
            <p className="text-muted-foreground mt-1 text-xs">
              {printer.enabled ? 'May receive jobs from this server.' : 'Hidden from server job routing.'}
            </p>
          </div>
          <Toggle
            disabled={busy !== null}
            label="Enable server access"
            checked={printer.enabled}
            onChange={(enabled) => void onConfigure(printer.id, { enabled }).catch(() => undefined)}
          />
        </div>
      </section>
      <section className="inspector-section">
        <h3>Connection</h3>
        <dl className="flex flex-col gap-2">
          <KeyValue label="Source" value={printer.sourceName} />
          <KeyValue label="Connection type" value={titleCase(printer.connectionType)} />
          {printer.address && <KeyValue label="Address" value={printer.address} copy />}
          <KeyValue label="Printer ID" value={printer.id} copy />
        </dl>
      </section>
      <section className="inspector-section">
        <h3>Print mode</h3>
        {printer.connectionType === 'system_queue' ? (
          <select
            aria-label="Print mode"
            className={inputClass}
            value={printer.submissionMode.type}
            disabled={busy !== null}
            onChange={(e) =>
              void onConfigure(printer.id, {
                submissionMode: e.target.value === 'raw' ? { type: 'raw', language: 'esc-pos' } : { type: 'driver' },
              }).catch(() => undefined)
            }
          >
            <option value="driver">System driver</option>
            <option value="raw">Raw ESC/POS</option>
          </select>
        ) : (
          <p>{printer.submissionMode.type === 'driver' ? 'System driver' : 'Raw ESC/POS'}</p>
        )}
        <p className="text-muted-foreground mt-3 text-xs leading-5">
          {printer.submissionMode.type === 'driver'
            ? 'Pages are submitted through the installed system driver. This does not establish raw ESC/POS support.'
            : 'Raw commands require an explicitly compatible ESC/POS printer. The connection method alone does not establish compatibility.'}
        </p>
      </section>
      {printer.capabilities && (
        <section className="inspector-section">
          <h3>Paper & capabilities</h3>
          <dl className="flex flex-col gap-2">
            <KeyValue
              label="Reported receipt widths"
              value={
                printer.capabilities.widths.length
                  ? printer.capabilities.widths.map((width) => `${width} mm`).join(', ')
                  : 'Not reported'
              }
            />
            <KeyValue label="Document formats" value={printer.capabilities.documentTypes.join(', ')} />
            <KeyValue
              label="Receipt operations"
              value={
                [printer.capabilities.supportsCut ? 'Cut' : null, printer.capabilities.supportsQr ? 'QR codes' : null]
                  .filter(Boolean)
                  .join(', ') || 'Not reported'
              }
            />
          </dl>
        </section>
      )}
      {printer.capabilities?.pdf && (
        <section className="inspector-section">
          <PdfTestSection printer={printer} pdf={printer.capabilities.pdf} busy={busy} onTestPdf={onTestPdf} />
        </section>
      )}
      <section className="inspector-section">
        <h3>Recent activity & tests</h3>
        {jobs
          .filter((job) => job.printerId === printer.id)
          .slice(0, 5)
          .map((job) => (
            <div key={job.id} className="border-border flex items-start justify-between gap-3 border-b py-2">
              <div className="min-w-0">
                <p className="text-muted-foreground text-xs">{formatRelativeTime(job.updatedAt)}</p>
                {job.error && <p className="text-error mt-1 text-xs break-words">{job.error}</p>}
              </div>
              <StateBadge state={job.state} />
            </div>
          ))}
        {!jobs.some((job) => job.printerId === printer.id) && (
          <p className="text-muted-foreground text-xs">No retained activity for this device.</p>
        )}
      </section>
    </>
  );
}
