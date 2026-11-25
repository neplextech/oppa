import {
  ArrowLeft,
  Check,
  ChevronDown,
  KeyRound,
  LoaderCircle,
  Printer,
  RefreshCw,
  Server,
  Trash2,
} from 'lucide-react';
import { useEffect, useReducer, useRef, useState } from 'react';

import { TitleBar } from '@/components/app-shell';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Field, FieldError, FieldGroup, FieldLabel } from '@/components/ui/field';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { agentClient } from '@/lib/agent-client';
import { readSetupProgress, setupReducer } from '@/lib/onboarding';
import type { SetupState } from '@/lib/onboarding';
import type {
  AgentStatus,
  DeepLinkPairing,
  DiscoveredServiceSummary,
  OpenPrinterServerConfiguration,
  PrinterConfigurationChanges,
  PrinterSummary,
  RecentServer,
} from '@/lib/types';
import { titleCase } from '@/lib/utils';

function errorMessage(cause: unknown, fallback: string): string {
  return cause instanceof Error ? cause.message : fallback;
}

export function SetupScreen({
  status,
  printers,
  busy,
  deepLink,
  onDiscover,
  onPair,
  onForget,
  onSaveServerConfiguration,
  onReconnect,
  onRefreshPrinters,
  onConfigurePrinter,
  onOpenProductLink,
  onOpenSettings,
  onActivate,
  onComplete,
  onConsumeDeepLink,
}: {
  status: AgentStatus;
  printers: PrinterSummary[];
  busy: string | null;
  deepLink: DeepLinkPairing | null;
  onDiscover: () => Promise<DiscoveredServiceSummary>;
  onPair: (code: string, computerName: string) => Promise<DiscoveredServiceSummary>;
  onForget: () => Promise<void>;
  onSaveServerConfiguration: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onReconnect: () => Promise<void>;
  onRefreshPrinters: () => Promise<void>;
  onConfigurePrinter: (id: string, changes: PrinterConfigurationChanges) => Promise<void>;
  onOpenProductLink: (link: 'documentation' | 'support', url: string) => Promise<void>;
  onOpenSettings: () => void;
  onActivate: () => void;
  onComplete: () => void;
  onConsumeDeepLink: () => void;
}) {
  const restored = useRef(readSetupProgress(localStorage.getItem('oppa-setup-progress')));
  const [state, dispatch] = useReducer(setupReducer, undefined, (): SetupState => {
    const saved = restored.current;
    const canResumePrinters =
      status.agentId &&
      saved &&
      ['pair', 'printers'].includes(saved.step) &&
      saved.serverUrl === status.serverConfiguration.serverUrl &&
      !['authentication_failed', 'credential_revoked'].includes(status.connectionState);
    return {
      step: canResumePrinters ? 'printers' : 'connect',
      serverUrl: saved?.serverUrl ?? status.serverConfiguration.serverUrl,
      computerName: saved?.computerName ?? 'This computer',
      code: '',
      identity: null,
    };
  });
  const [working, setWorking] = useState(false);
  const [retryRequested, setRetryRequested] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recentServers, setRecentServers] = useState<RecentServer[]>([]);
  const [recentOpen, setRecentOpen] = useState(false);
  const [pendingSwitch, setPendingSwitch] = useState<{ serverUrl: string; code?: string } | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const initialized = useRef(false);
  const previousStep = useRef(state.step);
  const lastLink = useRef<DeepLinkPairing | null>(null);
  const locked = working || busy !== null;
  const stepNumber = state.step === 'connect' ? 1 : state.step === 'pair' ? 2 : 3;
  const needsPairingRecovery = ['authentication_failed', 'credential_revoked'].includes(status.connectionState);

  useEffect(() => {
    localStorage.setItem(
      'oppa-setup-progress',
      JSON.stringify({ step: state.step, serverUrl: state.serverUrl, computerName: state.computerName }),
    );
    if (previousStep.current !== state.step) {
      heading.current?.focus();
      previousStep.current = state.step;
    }
  }, [state.step, state.serverUrl, state.computerName]);

  useEffect(() => {
    if (retryRequested && status.agentId && status.connectionState === 'connected') {
      dispatch({ type: 'reconnected' });
      setRetryRequested(false);
    }
  }, [retryRequested, status.agentId, status.connectionState]);

  async function discover(serverUrl: string, code?: string) {
    setWorking(true);
    setError(null);
    onActivate();
    try {
      // Configuration changes invalidate credentials through the existing secure agent operation.
      if (status.agentId) await onForget();
      if (serverUrl !== status.serverConfiguration.serverUrl) await onSaveServerConfiguration({ serverUrl });
      if (code !== undefined) dispatch({ type: 'link', serverUrl, code });
      const identity = await onDiscover();
      dispatch({ type: 'discovered', identity, serverUrl });
      if (code !== undefined) onConsumeDeepLink();
    } catch (cause) {
      setError(errorMessage(cause, 'Unable to reach this server. Check the address and your network connection.'));
      if (code !== undefined) onConsumeDeepLink();
    } finally {
      setWorking(false);
    }
  }

  useEffect(() => {
    if (initialized.current) return;
    initialized.current = true;
    void agentClient
      .listRecentServers()
      .then(setRecentServers)
      .catch(() => undefined);
    const saved = restored.current;
    if (
      !deepLink &&
      !status.agentId &&
      saved?.step === 'pair' &&
      saved.serverUrl === status.serverConfiguration.serverUrl
    )
      void discover(saved.serverUrl);
    // Initial recovery runs once; live connection changes must not erase entered codes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!deepLink || lastLink.current === deepLink) return;
    lastLink.current = deepLink;
    if (status.agentId) setPendingSwitch({ serverUrl: deepLink.serverUrl, code: deepLink.pairKey });
    else void discover(deepLink.serverUrl, deepLink.pairKey);
    // Each validated link is consumed once; the code remains only in memory.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deepLink]);

  async function pair() {
    if (!state.identity) return;
    setWorking(true);
    setError(null);
    onActivate();
    try {
      // Discovery currently enables new queues. Re-pair with no printer access until the user chooses.
      for (const printer of printers.filter((item) => item.enabled)) {
        await onConfigurePrinter(printer.id, { enabled: false });
      }
      await onPair(state.code, state.computerName.trim());
      dispatch({ type: 'paired' });
    } catch (cause) {
      setError(errorMessage(cause, 'Pairing failed. Request a fresh code from your server and try again.'));
    } finally {
      setWorking(false);
    }
  }

  async function refresh() {
    setWorking(true);
    setError(null);
    try {
      const known = new Set(printers.map((printer) => printer.id));
      await onRefreshPrinters();
      const discovered = await agentClient.listPrinters();
      for (const printer of discovered.filter((item) => !known.has(item.id) && item.enabled)) {
        await onConfigurePrinter(printer.id, { enabled: false });
      }
    } catch (cause) {
      setError(errorMessage(cause, 'Printer discovery failed. Try refreshing again.'));
    } finally {
      setWorking(false);
    }
  }

  function finish() {
    localStorage.removeItem('oppa-setup-progress');
    localStorage.removeItem('oppa-onboarding-active');
    onComplete();
  }

  function connect(serverUrl = state.serverUrl) {
    if (status.agentId) setPendingSwitch({ serverUrl });
    else void discover(serverUrl.trim());
  }

  const StepIcon = state.step === 'connect' ? Server : state.step === 'pair' ? KeyRound : Printer;
  return (
    <div className="onboarding bg-background text-foreground">
      <TitleBar
        minimal
        status={status}
        paired={false}
        developerMode={false}
        onNavigate={(screen) => {
          if (screen === 'settings') onOpenSettings();
        }}
        onReconnect={onReconnect}
      />
      <main className="onboarding-stage">
        <section className="onboarding-column" aria-labelledby="setup-heading">
          <div
            className="onboarding-progress"
            role="progressbar"
            aria-label="Setup progress"
            aria-valuemin={1}
            aria-valuemax={3}
            aria-valuenow={stepNumber}
          >
            {[1, 2, 3].map((step) => (
              <span key={step} data-complete={step <= stepNumber} />
            ))}
          </div>
          <p className="onboarding-step-label">Step {stepNumber} of 3</p>
          <header className="shrink-0">
            <div className="onboarding-symbol">
              <StepIcon className="size-4" aria-hidden />
            </div>
            <h1 id="setup-heading" ref={heading} tabIndex={-1} className="onboarding-title outline-none">
              {state.step === 'connect'
                ? 'Connect to a server'
                : state.step === 'pair'
                  ? 'Pair this computer'
                  : 'Choose printers'}
            </h1>
            <p className="onboarding-description">
              {state.step === 'connect'
                ? 'Enter the address of your OpenPrinter server to get started.'
                : state.step === 'pair'
                  ? 'Enter a one-time code from your server administrator.'
                  : 'Select the printers your server can access.'}
            </p>
          </header>
          {state.step === 'connect' && (
            <form
              className="onboarding-content"
              onSubmit={(e) => {
                e.preventDefault();
                connect();
              }}
            >
              <FieldGroup>
                <Field data-invalid={!!error}>
                  <FieldLabel htmlFor="setup-server">Server address</FieldLabel>
                  <Input
                    autoFocus
                    id="setup-server"
                    type="url"
                    autoComplete="url"
                    spellCheck={false}
                    value={state.serverUrl}
                    aria-invalid={!!error}
                    aria-describedby={error ? 'setup-error' : undefined}
                    disabled={locked}
                    onChange={(e) => {
                      dispatch({ type: 'address', value: e.target.value });
                      setError(null);
                    }}
                    required
                    maxLength={2048}
                  />
                  {error && (
                    <FieldError id="setup-error" role="alert" className="onboarding-error">
                      {error}
                    </FieldError>
                  )}
                  {!error && needsPairingRecovery && (
                    <p className="text-warning text-xs leading-4">
                      {status.connectionState === 'credential_revoked'
                        ? 'This computer’s access was revoked. Continue to verify the server and pair with a new code.'
                        : 'Authentication failed. Continue to pair again, or retry the connection below.'}
                    </p>
                  )}
                </Field>
              </FieldGroup>
              <div className="onboarding-actions">
                <Button type="submit" disabled={locked || !state.serverUrl.trim()} className="w-full">
                  {working && <LoaderCircle className="animate-spin" />}
                  {working ? 'Connecting…' : 'Continue'}
                </Button>
                {status.connectionState === 'authentication_failed' && (
                  <Button
                    variant="ghost"
                    disabled={locked}
                    onClick={() => {
                      setWorking(true);
                      setRetryRequested(true);
                      void onReconnect()
                        .catch((cause: unknown) =>
                          setError(errorMessage(cause, 'Unable to reconnect. Pair again with a new code.')),
                        )
                        .finally(() => setWorking(false));
                    }}
                  >
                    Retry existing connection
                  </Button>
                )}
              </div>
              {recentServers.length > 0 && (
                <div className="mt-4">
                  <button
                    type="button"
                    className="text-muted-foreground flex items-center gap-2 text-xs"
                    aria-expanded={recentOpen}
                    onClick={() => setRecentOpen(!recentOpen)}
                  >
                    Recently connected servers
                    <ChevronDown className="size-3" />
                  </button>
                  {recentOpen && (
                    <div className="mt-2 max-h-28 overflow-y-auto">
                      {recentServers.map((server) => (
                        <div key={server.serverUrl} className="border-border flex items-center gap-2 border-b py-2">
                          <button
                            type="button"
                            disabled={locked}
                            className="min-w-0 flex-1 text-left"
                            onClick={() => {
                              dispatch({ type: 'address', value: server.serverUrl });
                              setRecentOpen(false);
                            }}
                          >
                            <p className="truncate text-sm">{server.name ?? server.serverUrl}</p>
                            {server.name && (
                              <p className="text-muted-foreground truncate text-xs">{server.serverUrl}</p>
                            )}
                          </button>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`Remove recent server ${server.name ?? server.serverUrl}`}
                            disabled={locked}
                            onClick={() => {
                              setWorking(true);
                              void agentClient
                                .deleteRecentServer(server.serverUrl)
                                .then(() =>
                                  setRecentServers((current) =>
                                    current.filter((item) => item.serverUrl !== server.serverUrl),
                                  ),
                                )
                                .catch((cause: unknown) =>
                                  setError(errorMessage(cause, 'Unable to remove this server.')),
                                )
                                .finally(() => setWorking(false));
                            }}
                          >
                            <Trash2 />
                          </Button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}
            </form>
          )}
          {state.step === 'pair' && (
            <form
              className="onboarding-content"
              onSubmit={(e) => {
                e.preventDefault();
                void pair();
              }}
            >
              <div className="onboarding-identity">
                <p className="truncate text-sm font-medium">{state.identity?.name ?? 'Verifying server identity…'}</p>
                <p className="text-muted-foreground truncate text-xs" data-selectable title={state.serverUrl}>
                  {state.serverUrl}
                </p>
                {state.identity && (
                  <p className="text-muted-foreground truncate text-xs" title={state.identity.serverId}>
                    Server ID · {state.identity.serverId}
                  </p>
                )}
              </div>
              <FieldGroup className="gap-3">
                <Field>
                  <FieldLabel htmlFor="setup-name">Computer name</FieldLabel>
                  <Input
                    id="setup-name"
                    autoComplete="off"
                    value={state.computerName}
                    disabled={locked}
                    maxLength={128}
                    required
                    onChange={(e) => dispatch({ type: 'name', value: e.target.value })}
                  />
                </Field>
                <Field data-invalid={!!error}>
                  <FieldLabel htmlFor="setup-code">Pairing code</FieldLabel>
                  <Input
                    autoFocus
                    id="setup-code"
                    autoComplete="off"
                    autoCapitalize="characters"
                    spellCheck={false}
                    value={state.code}
                    disabled={locked}
                    maxLength={256}
                    aria-invalid={!!error}
                    aria-describedby={error ? 'setup-pair-error' : undefined}
                    onChange={(e) => {
                      dispatch({ type: 'code', value: e.target.value });
                      setError(null);
                    }}
                    required
                    minLength={4}
                  />
                  {error && (
                    <FieldError id="setup-pair-error" role="alert" className="onboarding-error">
                      {error} Request a new code if the previous one expired.
                    </FieldError>
                  )}
                </Field>
              </FieldGroup>
              <div className="onboarding-actions">
                <Button
                  type="submit"
                  disabled={locked || !state.identity || state.code.length < 4 || !state.computerName.trim()}
                >
                  {working && <LoaderCircle className="animate-spin" />}
                  {working ? 'Pairing…' : 'Pair computer'}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={locked}
                  onClick={() => {
                    dispatch({ type: 'back' });
                    setError(null);
                  }}
                >
                  <ArrowLeft />
                  Back
                </Button>
              </div>
            </form>
          )}
          {state.step === 'printers' && (
            <div className="onboarding-content flex min-h-0 flex-col">
              <div className="mb-2 flex shrink-0 items-center justify-between">
                <p className="text-muted-foreground text-xs">{printers.length} discovered</p>
                <Button variant="ghost" size="sm" disabled={locked} onClick={() => void refresh()}>
                  <RefreshCw className={working ? 'animate-spin' : undefined} />
                  Refresh
                </Button>
              </div>
              <div className="onboarding-printers" aria-label="Discovered printers">
                {printers.length ? (
                  printers.map((printer) => (
                    <div key={printer.id} className="border-border flex items-center gap-3 border-b py-3">
                      <Printer className="text-muted-foreground size-4 shrink-0" aria-hidden />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-medium">{printer.displayName}</p>
                        <p className="text-muted-foreground truncate text-xs">
                          {titleCase(printer.connectionType)} · {printer.available ? 'Available' : 'Unavailable'}
                        </p>
                      </div>
                      <Switch
                        size="sm"
                        checked={printer.enabled}
                        disabled={locked}
                        aria-label={`Allow server access to ${printer.displayName}`}
                        onCheckedChange={(enabled) => {
                          setWorking(true);
                          setError(null);
                          void onConfigurePrinter(printer.id, { enabled })
                            .catch((cause: unknown) =>
                              setError(errorMessage(cause, 'Unable to update printer access.')),
                            )
                            .finally(() => setWorking(false));
                        }}
                      />
                    </div>
                  ))
                ) : (
                  <div className="text-muted-foreground py-6 text-sm leading-5">
                    No printers discovered. Connect a printer and refresh, or configure printers later.
                  </div>
                )}
              </div>
              {error && (
                <p role="alert" className="onboarding-error mt-3">
                  {error}
                </p>
              )}
              {status.connectionState !== 'connected' && (
                <p className="text-muted-foreground mt-3 text-xs">
                  {needsPairingRecovery
                    ? 'The server requires pairing again.'
                    : 'Pairing saved. The agent will connect to the server in the background.'}
                  {needsPairingRecovery && (
                    <button className="text-primary ml-1" onClick={() => dispatch({ type: 'back' })}>
                      Pair again
                    </button>
                  )}
                </p>
              )}
              <div className="onboarding-actions">
                <Button disabled={locked} onClick={finish}>
                  <Check />
                  Finish setup
                </Button>
                <Button variant="ghost" disabled={locked} onClick={finish}>
                  Configure printers later
                </Button>
              </div>
            </div>
          )}
        </section>
      </main>
      <footer className="onboarding-footer">
        <button onClick={() => void onOpenProductLink('support', status.product.supportUrl)}>Help</button>
        <button onClick={() => void onOpenProductLink('documentation', status.product.documentationUrl)}>
          Documentation
        </button>
      </footer>
      <Dialog
        open={pendingSwitch !== null}
        onOpenChange={(open) => {
          if (!open && !locked) {
            setPendingSwitch(null);
            onConsumeDeepLink();
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Replace this computer’s pairing?</DialogTitle>
            <DialogDescription>
              Continuing disconnects the current server and removes its saved credential. Verify the new server before
              pairing again.
            </DialogDescription>
          </DialogHeader>
          <p className="text-muted-foreground text-xs break-all">{pendingSwitch?.serverUrl}</p>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={locked}
              onClick={() => {
                setPendingSwitch(null);
                onConsumeDeepLink();
              }}
            >
              Cancel
            </Button>
            <Button
              disabled={locked}
              onClick={() => {
                if (pendingSwitch) {
                  const next = pendingSwitch;
                  setPendingSwitch(null);
                  void discover(next.serverUrl, next.code);
                }
              }}
            >
              Continue
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
