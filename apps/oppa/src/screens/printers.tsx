import {
  EthernetPort,
  MoreHorizontal,
  MonitorCog,
  PanelRightOpen,
  Plus,
  Power,
  Printer,
  RefreshCw,
  Send,
  Trash2,
  Usb,
} from 'lucide-react';
import { useState } from 'react';

import { EmptyState, FieldLabel, ScreenContainer, ScreenHeader, StatusDot, Toggle, inputClass } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { ManualPrinterInput, PrinterSummary } from '@/lib/types';
import { cn, titleCase } from '@/lib/utils';

function ConnectionIcon({ type }: { type: PrinterSummary['connectionType'] }) {
  const Icon = type === 'network' ? EthernetPort : type === 'virtual' ? MonitorCog : type === 'usb' ? Usb : Printer;
  return <Icon className="size-4" aria-hidden />;
}

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
  onConfigure: (id: string, changes: { displayName?: string; enabled?: boolean }) => Promise<void>;
  onAddManual: (input: ManualPrinterInput) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
}) {
  const [showForm, setShowForm] = useState(false);
  const [input, setInput] = useState<ManualPrinterInput>({
    displayName: '',
    host: '',
    port: 9100,
  });
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = printers.find((p) => p.id === selectedId) ?? null;

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Printers"
        description="Enable the local printers this agent may use."
        action={
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" disabled={busy === 'refresh-printers'} onClick={() => void onRefresh()}>
              {busy === 'refresh-printers' ? (
                <RefreshCw className="size-3.5 animate-spin" aria-hidden />
              ) : (
                <RefreshCw className="size-3.5" aria-hidden />
              )}
              Refresh
            </Button>
            <Button size="sm" onClick={() => setShowForm((v) => !v)}>
              <Plus className="size-3.5" aria-hidden />
              Add printer
            </Button>
          </div>
        }
      />

      {showForm && (
        <div className="border-border bg-card/60 shrink-0 border-b px-6 py-5">
          <p className="text-foreground mb-4 text-sm font-semibold">Add network printer</p>
          <div className="flex flex-wrap items-end gap-4">
            <div>
              <FieldLabel htmlFor="manual-name">Display name</FieldLabel>
              <input
                id="manual-name"
                className={cn(inputClass, 'w-48')}
                placeholder="Counter printer"
                value={input.displayName}
                onChange={(e) => setInput((c) => ({ ...c, displayName: e.target.value }))}
              />
            </div>
            <div>
              <FieldLabel htmlFor="manual-host">Host / IP</FieldLabel>
              <input
                id="manual-host"
                className={cn(inputClass, 'w-40')}
                placeholder="192.168.1.82"
                value={input.host}
                onChange={(e) => setInput((c) => ({ ...c, host: e.target.value }))}
              />
            </div>
            <div>
              <FieldLabel htmlFor="manual-port">Port</FieldLabel>
              <input
                id="manual-port"
                className={cn(inputClass, 'w-24')}
                type="number"
                min={1}
                max={65535}
                value={input.port}
                onChange={(e) => setInput((c) => ({ ...c, port: Number(e.target.value) }))}
              />
            </div>
            <div className="flex gap-2">
              <Button
                size="sm"
                disabled={!input.displayName.trim() || !input.host.trim() || busy === 'add-manual-printer'}
                onClick={() => {
                  void onAddManual(input).then(() => {
                    setShowForm(false);
                    setInput({ displayName: '', host: '', port: 9100 });
                  });
                }}
              >
                {busy === 'add-manual-printer' ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
                Add
              </Button>
              <Button variant="outline" size="sm" onClick={() => setShowForm(false)}>
                Cancel
              </Button>
            </div>
          </div>
          <p className="text-muted-foreground mt-3 text-xs">Default port 9100. Raw TCP with explicit timeout.</p>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        {/* Printer list */}
        <div className={cn('flex flex-col overflow-hidden', selected ? 'flex-1' : 'w-full')}>
          {printers.length === 0 ? (
            <EmptyState
              title="No printers discovered"
              description="Refresh discovery to scan for local system queues and USB printers, or add a network printer manually."
            />
          ) : (
            <>
              {/* Table header */}
              <div className="border-border text-muted-foreground grid shrink-0 grid-cols-[1fr_140px_100px_80px_48px] items-center border-b px-5 py-2.5 text-xs font-medium tracking-wide uppercase">
                <span>Printer</span>
                <span>Connection</span>
                <span>Status</span>
                <span>Enabled</span>
                <span />
              </div>

              <div className="flex-1 overflow-y-auto">
                {printers.map((printer) => (
                  <PrinterRow
                    key={printer.id}
                    printer={printer}
                    busy={busy}
                    selected={selectedId === printer.id}
                    onSelect={() => setSelectedId((prev) => (prev === printer.id ? null : printer.id))}
                    onOpenDetails={() => setSelectedId(printer.id)}
                    onConfigure={onConfigure}
                    onRemove={onRemove}
                    onTest={onTest}
                  />
                ))}
              </div>
            </>
          )}
        </div>

        {/* Detail pane */}
        {selected && (
          <div className="border-border w-72 shrink-0 overflow-y-auto border-l xl:w-80">
            <PrinterDetail
              printer={selected}
              busy={busy}
              onConfigure={onConfigure}
              onRemove={onRemove}
              onTest={onTest}
              onClose={() => setSelectedId(null)}
            />
          </div>
        )}
      </div>
    </ScreenContainer>
  );
}

function PrinterRow({
  printer,
  busy,
  selected,
  onSelect,
  onOpenDetails,
  onConfigure,
  onRemove,
  onTest,
}: {
  printer: PrinterSummary;
  busy: string | null;
  selected: boolean;
  onSelect: () => void;
  onOpenDetails: () => void;
  onConfigure: (id: string, changes: { displayName?: string; enabled?: boolean }) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
}) {
  return (
    <ContextMenu>
      <ContextMenuTrigger
        render={<div />}
        className={cn(
          'group grid cursor-pointer grid-cols-[1fr_140px_100px_80px_48px] items-center border-b border-border/50 px-5 py-3.5 transition-colors',
          selected ? 'bg-primary/5' : 'hover:bg-accent/30',
        )}
        onClick={onSelect}
        tabIndex={0}
        role="row"
        aria-selected={selected}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            onSelect();
          }
        }}
      >
        {/* Name + icon */}
        <div className="flex min-w-0 items-center gap-3">
          <div className="border-border bg-secondary text-muted-foreground flex size-8 shrink-0 items-center justify-center rounded-lg border">
            <ConnectionIcon type={printer.connectionType} />
          </div>
          <div className="min-w-0">
            <p className="text-foreground truncate text-sm font-medium">{printer.displayName}</p>
            <p className="text-muted-foreground truncate text-xs">{printer.sourceName}</p>
          </div>
        </div>

        {/* Connection */}
        <div className="text-sm">
          <p className="text-foreground/80">{titleCase(printer.connectionType)}</p>
          {printer.address && <p className="text-muted-foreground truncate font-mono text-xs">{printer.address}</p>}
        </div>

        {/* Status */}
        <div className="flex items-center gap-2 text-sm">
          <StatusDot tone={printer.available ? 'connected' : 'offline'} />
          <span className={printer.available ? 'text-foreground/80' : 'text-muted-foreground'}>
            {printer.available ? 'Ready' : 'Offline'}
          </span>
        </div>

        {/* Enable toggle */}
        <div onClick={(event) => event.stopPropagation()}>
          <Toggle
            checked={printer.enabled}
            label={`${printer.enabled ? 'Disable' : 'Enable'} ${printer.displayName}`}
            onChange={(enabled) => void onConfigure(printer.id, { enabled })}
          />
        </div>

        {/* Overflow menu */}
        <div onClick={(event) => event.stopPropagation()}>
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <button
                  type="button"
                  className="text-muted-foreground hover:text-foreground focus-visible:ring-ring flex size-8 items-center justify-center rounded-md opacity-0 transition-all group-hover:opacity-100 focus-visible:opacity-100 focus-visible:ring-1 focus-visible:outline-none"
                  aria-label={`Actions for ${printer.displayName}`}
                />
              }
            >
              <MoreHorizontal className="size-4" strokeWidth={1.75} aria-hidden />
            </DropdownMenuTrigger>
            <DropdownMenuContent side="bottom" align="end" sideOffset={4}>
              <DropdownMenuItem onClick={() => void onConfigure(printer.id, { enabled: !printer.enabled })}>
                {printer.enabled ? 'Disable' : 'Enable'}
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!printer.enabled || !printer.available || !!busy?.startsWith('test')}
                onClick={() => void onTest(printer.id)}
              >
                <Send className="size-3.5" aria-hidden />
                Test print
              </DropdownMenuItem>
              {printer.connectionType === 'network' && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem
                    variant="destructive"
                    disabled={busy === `remove-${printer.id}`}
                    onClick={() => void onRemove(printer.id)}
                  >
                    <Trash2 className="size-3.5" aria-hidden />
                    Remove
                  </DropdownMenuItem>
                </>
              )}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="min-w-52">
        <ContextMenuGroup>
          <ContextMenuItem onClick={onOpenDetails}>
            <PanelRightOpen className="size-4" aria-hidden />
            View details
          </ContextMenuItem>
          <ContextMenuItem onClick={() => void onConfigure(printer.id, { enabled: !printer.enabled })}>
            <Power className="size-4" aria-hidden />
            {printer.enabled ? 'Disable printer' : 'Enable printer'}
          </ContextMenuItem>
          <ContextMenuItem
            disabled={!printer.enabled || !printer.available || !!busy?.startsWith('test')}
            onClick={() => void onTest(printer.id)}
          >
            <Send className="size-4" aria-hidden />
            Send test print
          </ContextMenuItem>
        </ContextMenuGroup>
        {printer.connectionType === 'network' ? (
          <>
            <ContextMenuSeparator />
            <ContextMenuGroup>
              <ContextMenuItem
                variant="destructive"
                disabled={busy === `remove-${printer.id}`}
                onClick={() => void onRemove(printer.id)}
              >
                <Trash2 className="size-4" aria-hidden />
                Remove printer
              </ContextMenuItem>
            </ContextMenuGroup>
          </>
        ) : null}
      </ContextMenuContent>
    </ContextMenu>
  );
}

function PrinterDetail({
  printer,
  busy,
  onConfigure,
  onRemove,
  onTest,
  onClose,
}: {
  printer: PrinterSummary;
  busy: string | null;
  onConfigure: (id: string, changes: { displayName?: string; enabled?: boolean }) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
  onClose: () => void;
}) {
  const [nameEdit, setNameEdit] = useState(printer.displayName);

  return (
    <>
      <div className="border-border flex items-center justify-between border-b px-5 py-4">
        <p className="text-foreground text-sm font-semibold">Printer details</p>
        <button
          type="button"
          onClick={onClose}
          className="text-muted-foreground hover:text-foreground rounded-md p-1 transition-colors"
          aria-label="Close"
        >
          <svg className="size-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} aria-hidden>
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </div>

      <div className="space-y-5 p-5">
        {/* Editable name */}
        <div>
          <FieldLabel htmlFor={`name-${printer.id}`}>Display name</FieldLabel>
          <input
            id={`name-${printer.id}`}
            className={inputClass}
            value={nameEdit}
            onChange={(e) => setNameEdit(e.target.value)}
            onBlur={() => {
              const name = nameEdit.trim();
              if (name && name !== printer.displayName) {
                void onConfigure(printer.id, { displayName: name });
              }
            }}
          />
        </div>

        {/* State */}
        <div>
          <p className="text-muted-foreground mb-2.5 text-xs font-semibold tracking-wide uppercase">Status</p>
          <div className="flex items-center gap-2.5 text-sm">
            <StatusDot tone={printer.available ? 'connected' : 'offline'} />
            <span className="text-foreground/80">{printer.available ? 'Available' : 'Unavailable'}</span>
            <span className="text-border">·</span>
            <span className="text-foreground/80">{printer.enabled ? 'Enabled' : 'Disabled'}</span>
          </div>
        </div>

        {/* Metadata */}
        <div>
          <p className="text-muted-foreground mb-2.5 text-xs font-semibold tracking-wide uppercase">Details</p>
          <div className="space-y-2">
            <Row label="Source">{printer.sourceName}</Row>
            <Row label="Connection">{titleCase(printer.connectionType)}</Row>
            {printer.address && <Row label="Address">{printer.address}</Row>}
            <Row label="ID">{printer.id}</Row>
          </div>
        </div>

        {/* Capabilities */}
        {printer.capabilities && (
          <div>
            <p className="text-muted-foreground mb-2.5 text-xs font-semibold tracking-wide uppercase">Capabilities</p>
            <div className="flex flex-wrap gap-1.5">
              {printer.capabilities.widths.map((w) => (
                <Cap key={w}>{w}mm</Cap>
              ))}
              {printer.capabilities.supportsCut && <Cap>cut</Cap>}
              {printer.capabilities.supportsQr && <Cap>qr</Cap>}
              {printer.capabilities.documentTypes.map((t) => (
                <Cap key={t}>{t}</Cap>
              ))}
            </div>
          </div>
        )}

        {/* Security boundary */}
        <div className="border-border bg-card/60 rounded-lg border px-4 py-3 text-xs leading-5">
          <p className="text-muted-foreground/80 mb-1 font-semibold">Security boundary</p>
          <p className="text-muted-foreground/70">
            Discovered locally · {printer.enabled ? 'Enabled for OPPA' : 'Not enabled'} ·{' '}
            {printer.enabled ? 'Eligible for job routing' : 'Not exposed to server'}
          </p>
        </div>

        {/* Actions */}
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={!printer.enabled || !printer.available || !!busy?.startsWith('test')}
            onClick={() => void onTest(printer.id)}
          >
            {busy === `test-${printer.id}` ? (
              <RefreshCw className="size-3.5 animate-spin" aria-hidden />
            ) : (
              <Send className="size-3.5" aria-hidden />
            )}
            Test print
          </Button>

          {printer.connectionType === 'network' && (
            <Button
              variant="destructive"
              size="sm"
              disabled={busy === `remove-${printer.id}`}
              onClick={() => void onRemove(printer.id)}
            >
              <Trash2 className="size-3.5" aria-hidden />
              Remove
            </Button>
          )}
        </div>
      </div>
    </>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="text-foreground/80 font-mono text-xs break-all">{children}</span>
    </div>
  );
}

function Cap({ children }: { children: React.ReactNode }) {
  return (
    <span className="border-border bg-secondary text-muted-foreground rounded border px-2 py-0.5 font-mono text-[11px] tracking-wide uppercase">
      {children}
    </span>
  );
}
