import {
  ClipboardCopy,
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
import {
  DEFAULT_PDF_PRINT_SETTINGS,
  MAX_TEST_PDF_BYTES,
  type ManualPrinterInput,
  type PdfCapabilities,
  type PdfPrintSettings,
  type PrinterConfigurationChanges,
  type PrinterSummary,
} from '@/lib/types';
import { cn, titleCase } from '@/lib/utils';

function ConnectionIcon({ type }: { type: PrinterSummary['connectionType'] }) {
  const Icon = type === 'network' ? EthernetPort : type === 'virtual' ? MonitorCog : type === 'usb' ? Usb : Printer;
  return <Icon className="size-4" aria-hidden />;
}

export function PrintersScreen({
  printers,
  busy,
  developerMode,
  onRefresh,
  onConfigure,
  onAddManual,
  onRemove,
  onTest,
  onTestPdf,
}: {
  printers: PrinterSummary[];
  busy: string | null;
  developerMode: boolean;
  onRefresh: () => Promise<void>;
  onConfigure: (id: string, changes: PrinterConfigurationChanges) => Promise<void>;
  onAddManual: (input: ManualPrinterInput) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
  onTestPdf: (id: string, file: File, settings: PdfPrintSettings) => Promise<void>;
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
          <p className="text-foreground mb-4 text-sm font-semibold">Add raw TCP printer</p>
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
          <p className="text-muted-foreground mt-3 text-xs">
            Sends ESC/POS directly to compatible receipt printers. Port 9100 alone does not identify a printer language.
          </p>
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
                    developerMode={developerMode}
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
              onTestPdf={onTestPdf}
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
  developerMode,
  onSelect,
  onOpenDetails,
  onConfigure,
  onRemove,
  onTest,
}: {
  printer: PrinterSummary;
  busy: string | null;
  selected: boolean;
  developerMode: boolean;
  onSelect: () => void;
  onOpenDetails: () => void;
  onConfigure: (id: string, changes: PrinterConfigurationChanges) => Promise<void>;
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
              {developerMode && (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onClick={() => void navigator.clipboard.writeText(printer.id)}>
                    <ClipboardCopy className="size-3.5" aria-hidden />
                    Copy ID
                  </DropdownMenuItem>
                </>
              )}
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
        {developerMode && (
          <>
            <ContextMenuSeparator />
            <ContextMenuGroup>
              <ContextMenuItem onClick={() => void navigator.clipboard.writeText(printer.id)}>
                <ClipboardCopy className="size-4" aria-hidden />
                Copy ID
              </ContextMenuItem>
            </ContextMenuGroup>
          </>
        )}
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
  onTestPdf,
  onClose,
}: {
  printer: PrinterSummary;
  busy: string | null;
  onConfigure: (id: string, changes: PrinterConfigurationChanges) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
  onTestPdf: (id: string, file: File, settings: PdfPrintSettings) => Promise<void>;
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
            {printer.connectionType === 'system_queue' && (
              <div className="pt-1">
                <FieldLabel htmlFor={`submission-mode-${printer.id}`}>Printing mode</FieldLabel>
                <select
                  id={`submission-mode-${printer.id}`}
                  className={inputClass}
                  value={printer.submissionMode.type}
                  disabled={busy === `printer-${printer.id}`}
                  onChange={(event) => {
                    const submissionMode =
                      event.target.value === 'raw'
                        ? { type: 'raw' as const, language: 'esc-pos' as const }
                        : { type: 'driver' as const };
                    void onConfigure(printer.id, { submissionMode });
                  }}
                >
                  <option value="driver">System driver</option>
                  <option value="raw">ESC/POS direct</option>
                </select>
                <p className="text-muted-foreground/70 mt-1 text-xs leading-4">
                  {printer.submissionMode.type === 'driver'
                    ? 'Prints a page through the installed printer driver.'
                    : 'Sends ESC/POS commands directly. Use only with a compatible receipt printer.'}
                </p>
              </div>
            )}
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
              {printer.capabilities.supportsSystemDriver && <Cap>system driver</Cap>}
              {printer.capabilities.supportsEscPos && <Cap>esc/pos</Cap>}
              {printer.capabilities.documentTypes.map((t) => (
                <Cap key={t}>{t}</Cap>
              ))}
            </div>
          </div>
        )}

        {/* PDF documents */}
        {printer.capabilities?.pdf && (
          <PdfTestSection printer={printer} pdf={printer.capabilities.pdf} busy={busy} onTestPdf={onTestPdf} />
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
    <span className="border-border bg-secondary text-muted-foreground rounded border px-2 py-0.5 font-mono text-xs tracking-wide uppercase">
      {children}
    </span>
  );
}

const PAPER_LABELS: Record<string, string> = { A4: 'A4', A5: 'A5', Letter: 'Letter', Legal: 'Legal' };
const PAGE_RANGE_PATTERN = /^\d{1,5}(-\d{1,5})?(,\d{1,5}(-\d{1,5})?)*$/;

function describeRoute(pdf: PdfCapabilities): string {
  return pdf.route === 'native' ? 'System PDF pipeline' : 'Rasterized by OPPA';
}

/** Shows what a printer accepts for PDFs and submits a local PDF test with explicit settings. */
function PdfTestSection({
  printer,
  pdf,
  busy,
  onTestPdf,
}: {
  printer: PrinterSummary;
  pdf: PdfCapabilities;
  busy: string | null;
  onTestPdf: (id: string, file: File, settings: PdfPrintSettings) => Promise<void>;
}) {
  const [file, setFile] = useState<File | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [settings, setSettings] = useState<PdfPrintSettings>(DEFAULT_PDF_PRINT_SETTINGS);
  const [formError, setFormError] = useState<string | null>(null);
  const paperOptions = pdf.paperSizes.filter((size) => size in PAPER_LABELS);
  const canPrint = printer.enabled && printer.available && !!file && busy === null;

  function update<Key extends keyof PdfPrintSettings>(key: Key, value: PdfPrintSettings[Key]) {
    setSettings((current) => ({ ...current, [key]: value }));
    setFormError(null);
  }

  function chooseFile(next: File | null) {
    setFileError(null);
    if (next && (next.size > MAX_TEST_PDF_BYTES || !next.name.toLowerCase().endsWith('.pdf'))) {
      setFile(null);
      setFileError('Choose a PDF file of 20 MB or less.');
      return;
    }
    setFile(next);
  }

  function submit() {
    if (!file) {
      return;
    }
    if (!Number.isInteger(settings.copies) || settings.copies < 1 || settings.copies > 99) {
      setFormError('Copies must be a whole number from 1 to 99.');
      return;
    }
    const ranges = settings.pageRanges.trim();
    if (ranges !== '' && !PAGE_RANGE_PATTERN.test(ranges)) {
      setFormError('Page ranges look like 1-3,5.');
      return;
    }
    if (settings.paper === 'custom') {
      const { customWidthMm: width, customHeightMm: height } = settings;
      if (![width, height].every((value) => Number.isInteger(value) && value >= 10 && value <= 1000)) {
        setFormError('Custom paper dimensions must be whole millimetres from 10 to 1000.');
        return;
      }
    }
    setFormError(null);
    // Failures are shown by the agent hook's error banner, so the rejection is not repeated here.
    onTestPdf(printer.id, file, settings).catch(() => undefined);
  }

  return (
    <div>
      <p className="text-muted-foreground mb-2.5 text-xs font-semibold tracking-wide uppercase">PDF documents</p>
      <div className="space-y-2">
        <Row label="Route">{describeRoute(pdf)}</Row>
        <Row label="Paper">
          {paperOptions.length > 0
            ? paperOptions.map((size) => PAPER_LABELS[size]).join(', ')
            : 'Driver default (not reported)'}
        </Row>
        <Row label="Custom sizes">{pdf.customPaper ? 'Available' : 'Not available'}</Row>
        <Row label="Duplex">{pdf.duplex ? 'Available' : 'Not available'}</Row>
      </div>

      <div className="border-border bg-card/60 mt-4 space-y-3 rounded-lg border p-3">
        <div>
          <FieldLabel htmlFor={`pdf-file-${printer.id}`}>PDF file</FieldLabel>
          <input
            id={`pdf-file-${printer.id}`}
            type="file"
            accept="application/pdf,.pdf"
            className="text-muted-foreground file:bg-secondary block w-full text-xs file:mr-2 file:rounded-md file:border-0 file:px-2 file:py-1 file:text-xs"
            onChange={(event) => chooseFile(event.target.files?.[0] ?? null)}
          />
          {fileError && <p className="text-destructive mt-1 text-xs">{fileError}</p>}
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div>
            <FieldLabel htmlFor={`pdf-copies-${printer.id}`}>Copies</FieldLabel>
            <input
              id={`pdf-copies-${printer.id}`}
              type="number"
              min={1}
              max={99}
              className={inputClass}
              value={settings.copies}
              onChange={(event) => update('copies', Number(event.target.value))}
            />
          </div>
          <div>
            <FieldLabel htmlFor={`pdf-ranges-${printer.id}`}>Pages</FieldLabel>
            <input
              id={`pdf-ranges-${printer.id}`}
              className={inputClass}
              placeholder="All, or 1-3,5"
              value={settings.pageRanges}
              onChange={(event) => update('pageRanges', event.target.value)}
            />
          </div>
        </div>

        <div>
          <FieldLabel htmlFor={`pdf-paper-${printer.id}`}>Paper</FieldLabel>
          <select
            id={`pdf-paper-${printer.id}`}
            className={inputClass}
            value={settings.paper}
            onChange={(event) => update('paper', event.target.value as PdfPrintSettings['paper'])}
          >
            <option value="default">Each page&apos;s own size</option>
            {paperOptions.map((size) => (
              <option key={size} value={size.toLowerCase()}>
                {PAPER_LABELS[size]}
              </option>
            ))}
            {pdf.customPaper && <option value="custom">Custom (labels)</option>}
          </select>
        </div>

        {settings.paper === 'custom' && (
          <div className="grid grid-cols-2 gap-2">
            <div>
              <FieldLabel htmlFor={`pdf-width-${printer.id}`}>Width (mm)</FieldLabel>
              <input
                id={`pdf-width-${printer.id}`}
                type="number"
                min={10}
                max={1000}
                className={inputClass}
                value={settings.customWidthMm}
                onChange={(event) => update('customWidthMm', Number(event.target.value))}
              />
            </div>
            <div>
              <FieldLabel htmlFor={`pdf-height-${printer.id}`}>Height (mm)</FieldLabel>
              <input
                id={`pdf-height-${printer.id}`}
                type="number"
                min={10}
                max={1000}
                className={inputClass}
                value={settings.customHeightMm}
                onChange={(event) => update('customHeightMm', Number(event.target.value))}
              />
            </div>
          </div>
        )}

        <div className="grid grid-cols-2 gap-2">
          <div>
            <FieldLabel htmlFor={`pdf-orientation-${printer.id}`}>Orientation</FieldLabel>
            <select
              id={`pdf-orientation-${printer.id}`}
              className={inputClass}
              value={settings.orientation}
              onChange={(event) => update('orientation', event.target.value as PdfPrintSettings['orientation'])}
            >
              <option value="auto">As document</option>
              <option value="portrait">Portrait</option>
              <option value="landscape">Landscape</option>
            </select>
          </div>
          <div>
            <FieldLabel htmlFor={`pdf-scaling-${printer.id}`}>Scaling</FieldLabel>
            <select
              id={`pdf-scaling-${printer.id}`}
              className={inputClass}
              value={settings.scaling}
              onChange={(event) => update('scaling', event.target.value as PdfPrintSettings['scaling'])}
            >
              <option value="actual">Actual size</option>
              <option value="fit">Fit to paper</option>
            </select>
          </div>
        </div>

        <div>
          <FieldLabel htmlFor={`pdf-duplex-${printer.id}`}>Sides</FieldLabel>
          <select
            id={`pdf-duplex-${printer.id}`}
            className={inputClass}
            value={settings.duplex}
            disabled={!pdf.duplex}
            onChange={(event) => update('duplex', event.target.value as PdfPrintSettings['duplex'])}
          >
            <option value="simplex">One-sided</option>
            {pdf.duplex && <option value="long-edge">Two-sided, long edge</option>}
            {pdf.duplex && <option value="short-edge">Two-sided, short edge</option>}
          </select>
        </div>

        {formError && <p className="text-destructive text-xs">{formError}</p>}

        <Button size="sm" disabled={!canPrint} onClick={submit}>
          {busy === `test-${printer.id}` ? (
            <RefreshCw className="size-3.5 animate-spin" aria-hidden />
          ) : (
            <Send className="size-3.5" aria-hidden />
          )}
          Print PDF test
        </Button>
        <p className="text-muted-foreground/70 text-xs leading-4">
          Submitted to the printer queue. Physical output is not verified.
        </p>
      </div>
    </div>
  );
}
