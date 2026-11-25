import { MoreHorizontal, Plus, Send, Settings2, Trash2, Volume2 } from 'lucide-react';
import { useEffect, useState } from 'react';

import { DocumentWorkspace } from '@/components/document-workspace';
import { EmptyState, ScreenContainer, ScreenHeader, Toggle, inputClass } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { ConfirmAction, Toolbar } from '@/components/workspace';
import { playPrinterSound } from '@/lib/printer-sound';
import { isVirtualPrinter } from '@/lib/types';
import type {
  PrinterSummary,
  VirtualPrinterInput,
  VirtualPrinterMode,
  VirtualPrinterProfile,
  VirtualPrinterSummary,
} from '@/lib/types';
import { useSelection } from '@/lib/workspace';

function soundPreference(id: string): boolean {
  const current = localStorage.getItem(`oppa-sound-${id}`);
  if (current !== null) return current === 'true';
  try {
    const previous: unknown = JSON.parse(localStorage.getItem('oppa-virtual-printer-sounds') ?? '{}');
    return (
      !!previous && typeof previous === 'object' && id in previous && (previous as Record<string, unknown>)[id] === true
    );
  } catch {
    return false;
  }
}
interface VirtualProps {
  printers: PrinterSummary[];
  busy: string | null;
  developerMode: boolean;
  onCreate: (input: VirtualPrinterInput) => Promise<void>;
  onUpdate: (id: string, mode: VirtualPrinterMode, delayMs: number) => Promise<void>;
  onClear: (id: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onTest: (id: string) => Promise<void>;
  onSetSound: (id: string, enabled: boolean) => Promise<void>;
}
export function VirtualPrintersScreen(props: VirtualProps) {
  const { printers, busy, onCreate, onClear, onDelete, onTest } = props;
  const virtual = printers.filter(isVirtualPrinter);
  const [selectedId, select] = useSelection('oppa-virtual-selection');
  const selected = virtual.find((p) => p.id === selectedId) ?? virtual[0];
  useEffect(() => {
    for (const printer of virtual)
      if (soundPreference(printer.id)) void props.onSetSound(printer.id, true).catch(() => undefined);
    // Restore previously enabled targets once on entering the developer workspace.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [create, setCreate] = useState(false);
  const [simulation, setSimulation] = useState(false);
  const [confirm, setConfirm] = useState<'clear' | 'delete' | null>(null);
  return (
    <ScreenContainer>
      <ScreenHeader
        title="Virtual printers"
        description="Capture and inspect print output without physical hardware."
        action={
          <Button size="sm" onClick={() => setCreate(true)}>
            <Plus />
            Create printer
          </Button>
        }
      />
      {!virtual.length ? (
        <EmptyState
          title="No virtual printers"
          description="Create a thermal receipt or office page target to inspect local rendering."
          action={
            <Button size="sm" onClick={() => setCreate(true)}>
              Create virtual printer
            </Button>
          }
        />
      ) : (
        selected && (
          <>
            <Toolbar>
              <select aria-label="Virtual printer" value={selected.id} onChange={(e) => select(e.target.value)}>
                {virtual.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.displayName}
                  </option>
                ))}
              </select>
              <span className="text-muted-foreground min-w-0 flex-1 truncate text-xs">
                {selected.virtualProfile?.type === 'esc-pos-receipt'
                  ? `${selected.virtualProfile.widthMm} mm · ESC/POS`
                  : 'Office driver pages'}
              </span>
              <Button size="sm" variant="outline" onClick={() => setSimulation(true)}>
                <Settings2 />
                Simulation
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busy !== null || !selected.available || !selected.enabled}
                onClick={() => void onTest(selected.id).catch(() => undefined)}
              >
                <Send />
                {busy === `test-${selected.id}` ? 'Submitting…' : 'Test print'}
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={<Button size="icon-sm" variant="ghost" aria-label="Virtual printer actions" />}
                >
                  <MoreHorizontal />
                </DropdownMenuTrigger>
                <DropdownMenuContent>
                  <DropdownMenuGroup>
                    <DropdownMenuItem
                      disabled={!selected.history.length || busy !== null}
                      onClick={() => setConfirm('clear')}
                    >
                      <Trash2 />
                      Clear captures…
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      variant="destructive"
                      disabled={busy !== null}
                      onClick={() => setConfirm('delete')}
                    >
                      Delete virtual printer…
                    </DropdownMenuItem>
                  </DropdownMenuGroup>
                </DropdownMenuContent>
              </DropdownMenu>
            </Toolbar>
            <DocumentWorkspace key={selected.id} printer={selected} />
            <SimulationDialog
              key={selected.id + 'simulation'}
              {...props}
              printer={selected}
              open={simulation}
              onOpenChange={setSimulation}
            />
            <ConfirmAction
              open={confirm !== null}
              onOpenChange={(open) => {
                if (!open) setConfirm(null);
              }}
              title={confirm === 'delete' ? 'Delete virtual printer?' : 'Clear captured output?'}
              description={
                confirm === 'delete'
                  ? 'This removes the virtual target and its captured output.'
                  : 'Captured documents for this target will be removed from local memory.'
              }
              busy={busy !== null}
              action={async () => {
                if (confirm === 'delete') {
                  await onDelete(selected.id);
                  select(null);
                } else await onClear(selected.id);
              }}
            />
          </>
        )
      )}
      <CreateVirtualDialog open={create} onOpenChange={setCreate} onCreate={onCreate} busy={busy !== null} />
    </ScreenContainer>
  );
}
function CreateVirtualDialog({
  open,
  onOpenChange,
  onCreate,
  busy,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreate: (input: VirtualPrinterInput) => Promise<void>;
  busy: boolean;
}) {
  const [name, setName] = useState('Virtual receipt');
  const [profile, setProfile] = useState('thermal-80');
  const [error, setError] = useState<string | null>(null);
  function submit() {
    const definition: VirtualPrinterProfile =
      profile === 'office'
        ? { type: 'system-driver-page', pageWidthMm: 210, pageHeightMm: 297, dpi: 300 }
        : { type: 'esc-pos-receipt', widthMm: profile === 'thermal-58' ? 58 : 80 };
    void onCreate({ displayName: name.trim(), profile: definition })
      .then(() => {
        onOpenChange(false);
        setError(null);
      })
      .catch((cause: unknown) =>
        setError(cause instanceof Error ? cause.message : 'Unable to create this virtual printer.'),
      );
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create virtual printer</DialogTitle>
          <DialogDescription>
            Capture receipt commands or system-driver pages locally. No physical device is used.
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="virtual-name">Display name</FieldLabel>
              <Input
                autoFocus
                id="virtual-name"
                value={name}
                required
                maxLength={128}
                onChange={(e) => setName(e.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="virtual-profile">Output profile</FieldLabel>
              <select
                id="virtual-profile"
                className={inputClass}
                value={profile}
                onChange={(e) => setProfile(e.target.value)}
              >
                <option value="thermal-80">Thermal receipt · 80 mm</option>
                <option value="thermal-58">Thermal receipt · 58 mm</option>
                <option value="office">Office page · A4, 300 DPI</option>
              </select>
            </Field>
            {error && (
              <p role="alert" className="text-error text-sm">
                {error}
              </p>
            )}
          </FieldGroup>
          <DialogFooter className="mt-6">
            <Button variant="outline" disabled={busy} onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || !name.trim()}>
              {busy ? 'Creating…' : 'Create printer'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
function SimulationDialog({
  printer,
  open,
  onOpenChange,
  busy,
  onUpdate,
  onSetSound,
}: VirtualProps & { printer: VirtualPrinterSummary; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [mode, setMode] = useState(printer.mode);
  const [delay, setDelay] = useState(printer.delayMs);
  const [sound, setSound] = useState(() => soundPreference(printer.id));
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (sound) void onSetSound(printer.id, true).catch(() => undefined);
  }, [sound, onSetSound, printer.id]);
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Simulation</DialogTitle>
          <DialogDescription>Control how {printer.displayName} responds to submitted jobs.</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            setError(null);
            void onUpdate(printer.id, mode, delay)
              .then(() => onOpenChange(false))
              .catch((cause: unknown) =>
                setError(cause instanceof Error ? cause.message : 'Unable to update simulation.'),
              );
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="simulation-mode">Submission behavior</FieldLabel>
              <select
                id="simulation-mode"
                className={inputClass}
                value={mode}
                onChange={(e) => setMode(e.target.value as VirtualPrinterMode)}
              >
                <option value="always_succeed">Always succeed</option>
                <option value="fail_next">Fail next job, then reset</option>
                <option value="always_fail">Always fail</option>
                <option value="delay">Delay submission</option>
                <option value="offline">Unavailable</option>
              </select>
            </Field>
            <Field>
              <FieldLabel htmlFor="simulation-delay">Delay in milliseconds</FieldLabel>
              <Input
                id="simulation-delay"
                type="number"
                min={0}
                max={30000}
                step={100}
                disabled={mode !== 'delay'}
                value={delay}
                onChange={(e) => setDelay(Number(e.target.value))}
              />
            </Field>
            <Field>
              <div className="flex items-center justify-between gap-4">
                <div>
                  <p className="text-sm">Printer sound</p>
                  <p className="text-muted-foreground mt-1 text-xs">Sound enforces at least 3.5 seconds of delay.</p>
                </div>
                <Toggle
                  checked={sound}
                  label="Enable printer sound"
                  onChange={(enabled) => {
                    void onSetSound(printer.id, enabled)
                      .then(() => {
                        setSound(enabled);
                        localStorage.setItem(`oppa-sound-${printer.id}`, String(enabled));
                      })
                      .catch(() => setError('Unable to update sound.'));
                  }}
                />
              </div>
              <Button type="button" size="sm" variant="ghost" onClick={playPrinterSound}>
                <Volume2 />
                Preview sound
              </Button>
            </Field>
            {error && (
              <p role="alert" className="text-error text-sm">
                {error}
              </p>
            )}
          </FieldGroup>
          <DialogFooter className="mt-6">
            <Button variant="outline" disabled={busy !== null} onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy !== null}>
              Apply simulation
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
