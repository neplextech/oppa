import { CirclePlus, FileDown, MonitorCog, Send, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';

import { PageHeader, SectionHeading } from '@/components/page';
import { Badge, Button, Card, EmptyState, FieldLabel, inputClassName } from '@/components/ui';
import { isVirtualPrinter } from '@/lib/types';
import type { PrinterSummary, VirtualPrinterInput, VirtualPrinterMode } from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';

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
  onUpdate: (printerId: string, mode: VirtualPrinterMode, delayMs: number) => Promise<void>;
  onClear: (printerId: string) => Promise<void>;
  onTest: (printerId: string) => Promise<void>;
}) {
  const virtualPrinters = printers.filter(isVirtualPrinter);
  const [selectedId, setSelectedId] = useState(virtualPrinters[0]?.id ?? '');
  const [showCreate, setShowCreate] = useState(false);
  const [input, setInput] = useState<VirtualPrinterInput>({
    displayName: 'Receipt preview',
    width: 80,
  });

  const selected = useMemo(
    () => virtualPrinters.find((printer) => printer.id === selectedId) ?? virtualPrinters[0],
    [selectedId, virtualPrinters],
  );

  return (
    <div className="mx-auto max-w-6xl">
      <PageHeader
        eyebrow="Development tools"
        title="Virtual printers"
        description="Exercise the complete delivery path without physical hardware, and inspect rendered output safely."
        action={
          <Button onClick={() => setShowCreate((value) => !value)}>
            <CirclePlus className="size-4" aria-hidden />
            Create virtual printer
          </Button>
        }
      />

      {showCreate ? (
        <Card className="mb-6 grid gap-4 border-blue-200 bg-blue-50/40 p-5 sm:grid-cols-[1fr_160px_auto] sm:items-end">
          <div>
            <FieldLabel htmlFor="virtual-name">Display name</FieldLabel>
            <input
              id="virtual-name"
              className={inputClassName}
              value={input.displayName}
              onChange={(event) => setInput((current) => ({ ...current, displayName: event.target.value }))}
            />
          </div>
          <div>
            <FieldLabel htmlFor="virtual-width">Receipt width</FieldLabel>
            <select
              id="virtual-width"
              className={inputClassName}
              value={input.width}
              onChange={(event) =>
                setInput((current) => ({
                  ...current,
                  width: Number(event.target.value) as 58 | 80,
                }))
              }
            >
              <option value={58}>58 mm</option>
              <option value={80}>80 mm</option>
            </select>
          </div>
          <Button
            busy={busy === 'create-virtual-printer'}
            disabled={!input.displayName.trim()}
            onClick={() => {
              void onCreate(input).then(() => setShowCreate(false));
            }}
          >
            Create
          </Button>
        </Card>
      ) : null}

      {virtualPrinters.length === 0 ? (
        <Card>
          <EmptyState
            icon={<MonitorCog className="size-5" aria-hidden />}
            title="No virtual printers"
            description="Create one to test server integrations and receipt rendering without printer hardware."
            action={<Button onClick={() => setShowCreate(true)}>Create virtual printer</Button>}
          />
        </Card>
      ) : (
        <div className="grid gap-6 xl:grid-cols-[280px_1fr]">
          <Card className="h-fit p-3">
            <div className="px-2 pt-1 pb-2 text-[10px] font-bold tracking-[0.14em] text-slate-400 uppercase">
              Virtual targets
            </div>
            <div className="space-y-1">
              {virtualPrinters.map((printer) => (
                <button
                  key={printer.id}
                  className={`flex w-full items-center gap-3 rounded-lg px-3 py-3 text-left transition ${
                    selected?.id === printer.id ? 'bg-slate-950 text-white' : 'hover:bg-slate-100'
                  }`}
                  onClick={() => setSelectedId(printer.id)}
                  type="button"
                >
                  <MonitorCog className="size-4 shrink-0" aria-hidden />
                  <div className="min-w-0">
                    <div className="truncate text-sm font-semibold">{printer.displayName}</div>
                    <div className="mt-0.5 text-[11px] text-slate-400">{printer.history.length} captured outputs</div>
                  </div>
                </button>
              ))}
            </div>
          </Card>

          {selected ? (
            <div className="space-y-6">
              <Card className="p-5">
                <SectionHeading
                  title={selected.displayName}
                  description="Submission simulation"
                  action={
                    <Button
                      variant="secondary"
                      busy={busy === `test-${selected.id}`}
                      onClick={() => void onTest(selected.id)}
                    >
                      <Send className="size-4" aria-hidden />
                      Test print
                    </Button>
                  }
                />
                <div className="grid gap-4 sm:grid-cols-2">
                  <div>
                    <FieldLabel htmlFor="simulation-mode">Simulation mode</FieldLabel>
                    <select
                      id="simulation-mode"
                      className={inputClassName}
                      value={selected.mode}
                      onChange={(event) => {
                        const mode = event.target.value as VirtualPrinterMode;
                        void onUpdate(selected.id, mode, selected.delayMs);
                      }}
                    >
                      <option value="always_succeed">Always succeed</option>
                      <option value="fail_next">Fail next job</option>
                      <option value="always_fail">Always fail</option>
                      <option value="delay">Delay submission</option>
                      <option value="offline">Offline</option>
                    </select>
                  </div>
                  <div>
                    <FieldLabel htmlFor="simulation-delay">Delay in milliseconds</FieldLabel>
                    <input
                      id="simulation-delay"
                      className={inputClassName}
                      disabled={selected.mode !== 'delay'}
                      min={0}
                      max={30_000}
                      type="number"
                      value={selected.delayMs}
                      onChange={(event) => void onUpdate(selected.id, selected.mode, Number(event.target.value))}
                    />
                  </div>
                </div>
              </Card>

              <Card className="overflow-hidden">
                <div className="flex items-center justify-between border-b border-slate-100 p-5">
                  <div>
                    <h2 className="text-sm font-bold text-slate-900">Captured output</h2>
                    <p className="mt-1 text-xs text-slate-500">Structured previews and raw byte metadata</p>
                  </div>
                  <Button
                    variant="ghost"
                    size="sm"
                    busy={busy === `clear-${selected.id}`}
                    disabled={selected.history.length === 0}
                    onClick={() => void onClear(selected.id)}
                  >
                    <Trash2 className="size-3.5" aria-hidden />
                    Clear
                  </Button>
                </div>
                {selected.history.length === 0 ? (
                  <EmptyState
                    icon={<FileDown className="size-5" aria-hidden />}
                    title="No output yet"
                    description="Send a test print or deliver a job from the example server."
                  />
                ) : (
                  <div className="divide-y divide-slate-100">
                    {selected.history.map((output) => (
                      <div key={output.id} className="grid gap-4 p-5 lg:grid-cols-[1fr_260px]">
                        <div>
                          <div className="flex items-center gap-2">
                            <div className="text-sm font-semibold text-slate-800">{output.jobId}</div>
                            <Badge tone="info">{titleCase(output.format)}</Badge>
                          </div>
                          <div className="mt-1 text-[11px] text-slate-400">
                            {formatRelativeTime(output.createdAt)} · {output.byteLength} bytes
                          </div>
                        </div>
                        <pre className="max-h-48 overflow-auto rounded-lg bg-slate-950 p-4 font-mono text-[10px] leading-4 text-slate-200">
                          {output.preview}
                        </pre>
                      </div>
                    ))}
                  </div>
                )}
              </Card>
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}
