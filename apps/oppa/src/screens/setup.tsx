import { Clock, KeyRound, RefreshCw, RotateCcw, Search, Settings2, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';

import { ServerConfigurationForm } from '@/components/server-configuration-form';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { agentClient } from '@/lib/agent-client';
import type { AgentStatus, DiscoveredServiceSummary, OpenPrinterServerConfiguration, RecentServer } from '@/lib/types';

export function SetupScreen({
  status,
  discoveredService,
  busy,
  onDiscover,
  onPair,
  onForget,
  onSaveServerConfiguration,
  onResetServerConfiguration,
  onReconnect,
  onOpenSettings,
}: {
  status: AgentStatus;
  discoveredService: DiscoveredServiceSummary | null;
  busy: string | null;
  onDiscover: () => Promise<DiscoveredServiceSummary>;
  onPair: (code: string, agentName: string) => Promise<DiscoveredServiceSummary>;
  onForget: () => Promise<void>;
  onSaveServerConfiguration: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onResetServerConfiguration: () => Promise<void>;
  onReconnect: () => Promise<void>;
  onOpenSettings: () => void;
}) {
  const [showServerConfiguration, setShowServerConfiguration] = useState(false);
  const [pairingCode, setPairingCode] = useState('');
  const [agentName, setAgentName] = useState('Front Counter');
  const [recentServers, setRecentServers] = useState<RecentServer[]>([]);
  const [pendingSwitch, setPendingSwitch] = useState<RecentServer | null>(null);
  const [switching, setSwitching] = useState(false);
  const [deletingServerUrl, setDeletingServerUrl] = useState<string | null>(null);
  const pairing = status.connectionState === 'pairing' || busy === 'pair-server';

  useEffect(() => {
    void agentClient
      .listRecentServers()
      .then(setRecentServers)
      .catch(() => undefined);
  }, []);

  const canReconnect =
    status.connectionState === 'authentication_failed' || status.connectionState === 'credential_revoked';

  const handleDeleteRecentServer = async (serverUrl: string) => {
    const deleteRecentServer = agentClient.deleteRecentServer as (serverUrl: string) => Promise<void>;
    setDeletingServerUrl(serverUrl);
    try {
      await deleteRecentServer(serverUrl);
      setRecentServers((current) => current.filter((server) => server.serverUrl !== serverUrl));
    } finally {
      setDeletingServerUrl((current) => (current === serverUrl ? null : current));
    }
  };

  return (
    <>
      <main className="bg-background flex min-h-dvh flex-col">
        {/* Draggable titlebar area for custom window chrome */}
        <div data-tauri-drag-region className="h-11 w-full shrink-0" />

        <div className="flex flex-1 items-center justify-center p-6">
          <div className="border-border w-full max-w-3xl overflow-hidden rounded-lg border shadow-lg">
            <div className="grid md:grid-cols-[0.85fr_1.15fr]">
              {/* Left panel */}
              <div className="border-border bg-card flex flex-col justify-between border-r p-8">
                <div>
                  <img src="/oppa-icon.png" alt="" className="border-border size-11 rounded-xl border shadow-sm" />
                  <p className="text-primary mt-8 text-xs font-semibold tracking-widest uppercase">
                    {status.product.name}
                  </p>
                  <h1 className="text-foreground mt-3 text-xl leading-7 font-semibold tracking-tight">
                    Connect to your print server.
                  </h1>
                  <p className="text-muted-foreground mt-3 text-sm leading-6">
                    Pair this computer with an OpenPrinter server to start receiving and processing print jobs locally.
                  </p>
                </div>
                <div className="mt-8 flex items-center justify-end">
                  <button
                    type="button"
                    onClick={onOpenSettings}
                    className="text-muted-foreground/50 hover:text-muted-foreground rounded-md p-1.5 transition-colors"
                    aria-label="Open settings"
                    title="Settings"
                  >
                    <Settings2 className="size-4" aria-hidden />
                  </button>
                </div>
              </div>

              {/* Right panel */}
              <div className="bg-background min-w-0 p-8">
                <p className="text-muted-foreground/60 text-xs font-semibold tracking-widest uppercase">
                  Initial setup
                </p>
                <h2 className="text-foreground mt-4 text-lg font-semibold">Discover and pair</h2>
                <p className="text-muted-foreground mt-1.5 text-sm leading-6">
                  Confirm the server identity, then enter the temporary code created by its administrator.
                </p>

                <div className="mt-6 flex gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    className="flex-1"
                    disabled={busy === 'discover-server'}
                    onClick={() => void onDiscover()}
                  >
                    {busy === 'discover-server' ? (
                      <RefreshCw className="size-3.5 animate-spin" aria-hidden />
                    ) : (
                      <Search className="size-3.5" aria-hidden />
                    )}
                    Discover server
                  </Button>

                  {canReconnect && (
                    <Button
                      type="button"
                      variant="outline"
                      disabled={busy === 'reconnect'}
                      onClick={() => void onReconnect()}
                      title="Retry the gateway connection"
                    >
                      {busy === 'reconnect' ? (
                        <RefreshCw className="size-3.5 animate-spin" aria-hidden />
                      ) : (
                        <RotateCcw className="size-3.5" aria-hidden />
                      )}
                      Reconnect
                    </Button>
                  )}
                </div>

                {discoveredService ? (
                  <div className="border-border bg-card mt-5 rounded-lg border p-4">
                    <p className="text-foreground font-semibold">{discoveredService.name}</p>
                    <p className="text-muted-foreground mt-1 font-mono text-xs">{discoveredService.serverId}</p>
                    <div className="mt-4 grid gap-3">
                      <label className="space-y-1.5">
                        <span className="text-muted-foreground text-xs font-medium">Agent name</span>
                        <Input
                          value={agentName}
                          maxLength={128}
                          onChange={(event) => setAgentName(event.target.value)}
                        />
                      </label>
                      <label className="space-y-1.5">
                        <span className="text-muted-foreground text-xs font-medium">Pairing code</span>
                        <Input
                          value={pairingCode}
                          autoCapitalize="characters"
                          autoCorrect="off"
                          spellCheck={false}
                          placeholder="ABCD-EFGH"
                          onChange={(event) => setPairingCode(event.target.value.toUpperCase())}
                        />
                      </label>
                    </div>
                    <Button
                      className="mt-4 w-full"
                      disabled={pairing || pairingCode.trim().length < 4 || agentName.trim().length === 0}
                      onClick={() => void onPair(pairingCode.trim(), agentName.trim())}
                    >
                      {pairing ? <RefreshCw className="size-3.5 animate-spin" /> : <KeyRound className="size-3.5" />}
                      {pairing ? 'Pairing…' : 'Pair this computer'}
                    </Button>
                  </div>
                ) : null}

                <div className="border-border mt-5 border-t pt-4">
                  <button
                    type="button"
                    className="text-muted-foreground hover:text-foreground flex w-full items-center gap-2 text-left text-sm font-medium"
                    onClick={() => setShowServerConfiguration((current) => !current)}
                  >
                    <Settings2 className="size-4" aria-hidden />
                    OpenPrinter server
                    <span className="text-muted-foreground/50 ml-auto font-mono text-xs">
                      {showServerConfiguration ? 'Hide' : 'Configure'}
                    </span>
                  </button>
                  <p className="text-muted-foreground/60 mt-1 truncate pl-6 font-mono text-xs">
                    {status.serverConfiguration.serverUrl}
                  </p>
                  {showServerConfiguration ? (
                    <div className="mt-4">
                      <ServerConfigurationForm
                        compact
                        value={status.serverConfiguration}
                        saving={busy === 'server-configuration'}
                        resetting={busy === 'reset-server-configuration'}
                        onSave={onSaveServerConfiguration}
                        onReset={onResetServerConfiguration}
                      />
                    </div>
                  ) : null}
                </div>

                {status.activeErrors.length > 0 ? (
                  <div className="border-error/20 bg-error/5 mt-4 rounded-lg border p-4">
                    <p className="text-error text-sm font-semibold">Pairing needs attention</p>
                    {status.activeErrors.map((message) => (
                      <p key={message} className="text-error/70 mt-1.5 text-sm">
                        {message}
                      </p>
                    ))}
                  </div>
                ) : null}

                {status.agentId !== null ? (
                  <Button
                    type="button"
                    variant="outline"
                    className="mt-4 w-full"
                    disabled={busy === 'forget-server'}
                    onClick={() => void onForget()}
                  >
                    {busy === 'forget-server' ? <RefreshCw className="size-3.5 animate-spin" /> : null}
                    Forget server and pair again
                  </Button>
                ) : null}

                {recentServers.length > 0 ? (
                  <div className="border-border mt-5 border-t pt-4">
                    <p className="text-muted-foreground/60 flex items-center gap-1.5 text-xs font-semibold tracking-widest uppercase">
                      <Clock className="size-3" aria-hidden />
                      Recent servers
                    </p>
                    <div className="mt-2 space-y-1">
                      {recentServers.map((server) => (
                        <div
                          key={server.serverUrl}
                          className="hover:bg-accent flex items-center gap-2 rounded-md px-2 py-2 transition-colors"
                        >
                          <button
                            type="button"
                            className="min-w-0 flex-1 text-left"
                            onClick={() => setPendingSwitch(server)}
                          >
                            <div className="min-w-0">
                              {server.name ? (
                                <p className="text-foreground/80 truncate text-sm font-medium">{server.name}</p>
                              ) : null}
                              <p className="text-muted-foreground truncate font-mono text-xs">{server.serverUrl}</p>
                            </div>
                          </button>
                          <button
                            type="button"
                            className="text-muted-foreground hover:text-foreground disabled:opacity-40"
                            aria-label={`Delete recent server ${server.name ?? server.serverUrl}`}
                            disabled={deletingServerUrl === server.serverUrl}
                            onClick={() => void handleDeleteRecentServer(server.serverUrl)}
                          >
                            {deletingServerUrl === server.serverUrl ? (
                              <RefreshCw className="size-3.5 animate-spin" aria-hidden />
                            ) : (
                              <Trash2 className="size-3.5" aria-hidden />
                            )}
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
            </div>
          </div>
        </div>
      </main>

      <Dialog
        open={pendingSwitch !== null}
        onOpenChange={(open) => {
          if (!open) setPendingSwitch(null);
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Switch server?</DialogTitle>
            <DialogDescription>
              {pendingSwitch?.name ? (
                <>
                  Connect to <strong>{pendingSwitch.name}</strong> ({pendingSwitch?.serverUrl})?
                </>
              ) : (
                <>
                  Connect to <span className="font-mono text-xs">{pendingSwitch?.serverUrl}</span>?
                </>
              )}{' '}
              Your current configuration will be cleared and you will need to pair again.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={switching} onClick={() => setPendingSwitch(null)}>
              Cancel
            </Button>
            <Button
              disabled={switching}
              onClick={() => {
                if (!pendingSwitch) return;
                setSwitching(true);
                void agentClient
                  .applyRecentServer(pendingSwitch.serverUrl)
                  .then(() => setPendingSwitch(null))
                  .finally(() => setSwitching(false));
              }}
            >
              {switching ? <RefreshCw className="size-3.5 animate-spin" aria-hidden /> : null}
              Switch
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
