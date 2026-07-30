import { MonitorCog, Plus, RefreshCw, Send, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';

import { EmptyState, FieldLabel, ScreenContainer, ScreenHeader, inputClass } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { isVirtualPrinter } from '@/lib/types';
import type { PrinterSummary, VirtualPrinterInput, VirtualPrinterMode } from '@/lib/types';
import { cn, formatRelativeTime } from '@/lib/utils';

const MODES: Array<{ value: VirtualPrinterMode; label: string; description: string }> = [
  { value: 'always_succeed', label: 'Always succeed', description: 'All jobs complete immediately' },
  { value: 'fail_next', label: 'Fail next job', description: 'Next job fails, then resets' },
  { value: 'always_fail', label: 'Always fail', description: 'All jobs fail permanently' },
  { value: 'delay', label: 'Delay submission', description: 'Jobs succeed after configured delay' },
  { value: 'offline', label: 'Offline', description: 'Printer is unavailable' },
];

export function VirtualPrintersScreen({
  printers,
  busy,
  onCreate,
  onUpdate,
  onClear,
  onTest,
}: {
  printers: PrinterSummary[];
  busy: string | null;
  onCreate: (input: VirtualPrinterInput) => Promise<void>;
  onUpdate: (id: string, mode: VirtualPrinterMode, delayMs: number) => Promise<void>;
  onClear: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
}) {
  const virtualPrinters = printers.filter(isVirtualPrinter);
  const [selectedId, setSelectedId] = useState(virtualPrinters[0]?.id ?? '');
  const [showCreate, setShowCreate] = useState(false);
  const [input, setInput] = useState<VirtualPrinterInput>({ displayName: 'Receipt preview', width: 80 });

  const selected = useMemo(
    () => virtualPrinters.find((p) => p.id === selectedId) ?? virtualPrinters[0],
    [selectedId, virtualPrinters],
  );

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Virtual Printer"
        description="Simulate print delivery without hardware. Exercise the complete protocol lifecycle."
        action={
          <Button size="sm" onClick={() => setShowCreate((v) => !v)}>
            <Plus className="size-3" aria-hidden />
            Create
          </Button>
        }
      />

      {showCreate && (
        <div className="border-border bg-card/50 shrink-0 border-b px-5 py-4">
          <p className="text-muted-foreground mb-3 text-[11px] font-semibold">New virtual printer</p>
          <div className="flex flex-wrap items-end gap-3">
            <div>
              <FieldLabel htmlFor="virtual-name">Display name</FieldLabel>
              <input
                id="virtual-name"
                className={cn(inputClass, 'w-48')}
                value={input.displayName}
                onChange={(e) => setInput((c) => ({ ...c, displayName: e.target.value }))}
              />
            </div>
            <div>
              <FieldLabel htmlFor="virtual-width">Receipt width</FieldLabel>
              <select
                id="virtual-width"
                className={cn(inputClass, 'w-24')}
                value={input.width}
                onChange={(e) => setInput((c) => ({ ...c, width: Number(e.target.value) as 58 | 80 }))}
              >
                <option value={58}>58 mm</option>
                <option value={80}>80 mm</option>
              </select>
            </div>
            <div className="flex gap-2">
              <Button
                size="sm"
                disabled={!input.displayName.trim() || busy === 'create-virtual-printer'}
                onClick={() => void onCreate(input).then(() => setShowCreate(false))}
              >
                {busy === 'create-virtual-printer' ? <RefreshCw className="size-3 animate-spin" aria-hidden /> : null}
                Create
              </Button>
              <Button variant="outline" size="sm" onClick={() => setShowCreate(false)}>
                Cancel
              </Button>
            </div>
          </div>
        </div>
      )}

      {virtualPrinters.length === 0 ? (
        <EmptyState
          title="No virtual printers"
          description="Create a virtual printer to test server integrations and inspect rendered output without physical hardware."
          action={
            <Button size="sm" onClick={() => setShowCreate(true)}>
              <Plus className="size-3" aria-hidden />
              Create virtual printer
            </Button>
          }
        />
      ) : (
        <div className="flex flex-1 overflow-hidden">
          {/* Sidebar list */}
          <div className="border-border bg-card/30 w-48 shrink-0 overflow-y-auto border-r xl:w-56">
            <div className="px-3 pt-3 pb-1">
              <p className="text-muted-foreground/50 text-[10px] font-semibold tracking-wider uppercase">
                Virtual targets
              </p>
            </div>
            <div className="space-y-0.5 p-1.5">
              {virtualPrinters.map((printer) => (
                <ContextMenu key={printer.id}>
                  <ContextMenuTrigger
                    render={<button type="button" />}
                    onClick={() => setSelectedId(printer.id)}
                    className={cn(
                      'flex w-full items-center gap-2.5 rounded px-2.5 py-2 text-left text-[11px] transition-colors',
                      selected?.id === printer.id
                        ? 'bg-primary/10 text-primary'
                        : 'text-muted-foreground hover:bg-accent hover:text-foreground',
                    )}
                  >
                    <MonitorCog className="size-3.5 shrink-0" aria-hidden />
                    <div className="min-w-0">
                      <p className="truncate font-medium">{printer.displayName}</p>
                      <p className="text-muted-foreground/60 text-[10px]">{printer.history.length} captured</p>
                    </div>
                  </ContextMenuTrigger>
                  <ContextMenuContent className="min-w-48">
                    <ContextMenuGroup>
                      <ContextMenuItem onClick={() => setSelectedId(printer.id)}>
                        <MonitorCog className="size-4" aria-hidden />
                        Open printer
                      </ContextMenuItem>
                      <ContextMenuItem disabled={busy === `test-${printer.id}`} onClick={() => void onTest(printer.id)}>
                        <Send className="size-4" aria-hidden />
                        Send test print
                      </ContextMenuItem>
                    </ContextMenuGroup>
                    <ContextMenuSeparator />
                    <ContextMenuGroup>
                      <ContextMenuItem
                        disabled={printer.history.length === 0 || busy === `clear-${printer.id}`}
                        onClick={() => void onClear(printer.id)}
                      >
                        <Trash2 className="size-4" aria-hidden />
                        Clear captured output
                      </ContextMenuItem>
                    </ContextMenuGroup>
                  </ContextMenuContent>
                </ContextMenu>
              ))}
            </div>
          </div>

          {/* Detail */}
          {selected && (
            <div className="flex flex-1 flex-col overflow-hidden">
              {/* Controls */}
              <div className="border-border shrink-0 border-b px-5 py-4">
                <div className="mb-3 flex items-center justify-between">
                  <div>
                    <p className="text-foreground text-sm font-semibold">{selected.displayName}</p>
                    <p className="text-muted-foreground font-mono text-[11px]">{selected.id}</p>
                  </div>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy === `test-${selected.id}`}
                    onClick={() => void onTest(selected.id)}
                  >
                    {busy === `test-${selected.id}` ? (
                      <RefreshCw className="size-3 animate-spin" aria-hidden />
                    ) : (
                      <Send className="size-3" aria-hidden />
                    )}
                    Test print
                  </Button>
                </div>

                {/* Simulation mode */}
                <div className="grid gap-3 sm:grid-cols-2">
                  <div>
                    <FieldLabel htmlFor={`mode-${selected.id}`}>Simulation mode</FieldLabel>
                    <select
                      id={`mode-${selected.id}`}
                      className={inputClass}
                      value={selected.mode}
                      onChange={(e) => {
                        const mode = e.target.value as VirtualPrinterMode;
                        void onUpdate(selected.id, mode, selected.delayMs);
                      }}
                    >
                      {MODES.map((m) => (
                        <option key={m.value} value={m.value}>
                          {m.label}
                        </option>
                      ))}
                    </select>
                    <p className="text-muted-foreground/60 mt-1 text-[10px]">
                      {MODES.find((m) => m.value === selected.mode)?.description}
                    </p>
                  </div>
                  <div>
                    <FieldLabel htmlFor={`delay-${selected.id}`}>Delay (ms)</FieldLabel>
                    <input
                      id={`delay-${selected.id}`}
                      className={inputClass}
                      type="number"
                      min={0}
                      max={30_000}
                      disabled={selected.mode !== 'delay'}
                      value={selected.delayMs}
                      onChange={(e) => void onUpdate(selected.id, selected.mode, Number(e.target.value))}
                    />
                    <p className="text-muted-foreground/60 mt-1 text-[10px]">Active only in delay mode.</p>
                  </div>
                </div>
              </div>

              {/* Captured output */}
              <div className="border-border flex shrink-0 items-center justify-between border-b px-5 py-3">
                <p className="text-muted-foreground text-[11px] font-semibold tracking-wider uppercase">
                  Captured output
                </p>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={selected.history.length === 0 || busy === `clear-${selected.id}`}
                  onClick={() => void onClear(selected.id)}
                >
                  <Trash2 className="size-3" aria-hidden />
                  Clear
                </Button>
              </div>

              <div className="flex-1 overflow-y-auto">
                {selected.history.length === 0 ? (
                  <EmptyState
                    title="No output captured"
                    description="Send a test print or deliver a job from the example server."
                  />
                ) : (
                  <div>
                    {selected.history.map((output) => (
                      <div
                        key={output.id}
                        className="border-border/50 grid gap-4 border-b px-5 py-4 lg:grid-cols-[1fr_auto]"
                      >
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="text-foreground/80 font-mono text-[11px]">{output.jobId}</span>
                            <span className="border-border bg-secondary text-muted-foreground rounded border px-1.5 py-0.5 font-mono text-[9px] tracking-wide uppercase">
                              {output.format}
                            </span>
                          </div>
                          <p className="text-muted-foreground/60 mt-0.5 text-[10px]">
                            {formatRelativeTime(output.createdAt)} · {output.byteLength} bytes
                          </p>
                        </div>
                        <pre className="border-border text-muted-foreground max-h-36 w-full overflow-auto rounded border bg-[oklch(0.07_0_0)] p-3 font-mono text-[10px] leading-4 lg:w-64">
                          {output.preview}
                        </pre>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </ScreenContainer>
  );
}
