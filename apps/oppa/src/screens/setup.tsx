import { KeyRound, RefreshCw, Search, Settings2, ShieldCheck } from 'lucide-react';
import { useState } from 'react';

import { ServerConfigurationForm } from '@/components/server-configuration-form';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { AgentStatus, DiscoveredServiceSummary, OpenPrinterServerConfiguration } from '@/lib/types';

export function SetupScreen({
  status,
  discoveredService,
  busy,
  onDiscover,
  onPair,
  onForget,
  onSaveServerConfiguration,
  onResetServerConfiguration,
}: {
  status: AgentStatus;
  discoveredService: DiscoveredServiceSummary | null;
  busy: string | null;
  onDiscover: () => Promise<DiscoveredServiceSummary>;
  onPair: (code: string, agentName: string) => Promise<DiscoveredServiceSummary>;
  onForget: () => Promise<void>;
  onSaveServerConfiguration: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onResetServerConfiguration: () => Promise<void>;
}) {
  const [showServerConfiguration, setShowServerConfiguration] = useState(false);
  const [pairingCode, setPairingCode] = useState('');
  const [agentName, setAgentName] = useState('Front Counter');
  const pairing = status.connectionState === 'pairing' || busy === 'pair-server';

  return (
    <main className="bg-background flex min-h-dvh items-center justify-center p-6">
      <div className="border-border w-full max-w-3xl overflow-hidden rounded border">
        <div className="grid md:grid-cols-[0.85fr_1.15fr]">
          <div className="border-border bg-card flex flex-col justify-between border-r p-8">
            <div>
              <img src="/oppa-icon.png" alt="" className="border-border size-10 rounded-xl border shadow-sm" />
              <p className="text-primary mt-8 text-[10px] font-semibold tracking-widest uppercase">
                {status.product.name}
              </p>
              <h1 className="text-foreground mt-3 text-xl leading-7 font-semibold tracking-tight">
                Pair this computer with an OpenPrinter server.
              </h1>
              <p className="text-muted-foreground mt-3 text-[11px] leading-5">
                The private Ed25519 key stays in operating-system secure storage. Only its public key is sent to the
                server, and normal print payloads are not individually signed.
              </p>
              <div className="mt-8 space-y-2">
                {[
                  'One server base URL with automatic discovery',
                  'Short-lived, single-use pairing code',
                  'Private key never enters the web interface',
                  'Challenge authentication on every gateway connection',
                ].map((point) => (
                  <div key={point} className="text-muted-foreground flex items-start gap-2 text-[11px]">
                    <span className="mt-1.5 size-1 shrink-0 rounded-full bg-[var(--color-connected)]" />
                    {point}
                  </div>
                ))}
              </div>
            </div>
            <div className="text-muted-foreground/50 mt-8 flex items-center gap-2 text-[10px]">
              <ShieldCheck className="size-3 text-[var(--color-connected)]" aria-hidden />
              Pairing-code Ed25519 authentication
            </div>
          </div>

          <div className="bg-background p-8">
            <p className="text-muted-foreground/60 text-[10px] font-semibold tracking-widest uppercase">
              Initial setup
            </p>
            <h2 className="text-foreground mt-5 text-base font-semibold">Discover and pair</h2>
            <p className="text-muted-foreground mt-1.5 text-[11px] leading-5">
              Confirm the server identity, then enter the temporary code created by its administrator.
            </p>

            <Button
              type="button"
              variant="outline"
              className="mt-6 w-full"
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

            {discoveredService ? (
              <div className="border-border bg-card mt-4 rounded border p-3 text-[11px]">
                <p className="text-foreground font-semibold">{discoveredService.name}</p>
                <p className="text-muted-foreground mt-1 font-mono text-[9px]">{discoveredService.serverId}</p>
                <div className="mt-4 grid gap-3">
                  <label className="space-y-1">
                    <span className="text-muted-foreground text-[10px] font-medium">Agent name</span>
                    <Input value={agentName} maxLength={128} onChange={(event) => setAgentName(event.target.value)} />
                  </label>
                  <label className="space-y-1">
                    <span className="text-muted-foreground text-[10px] font-medium">Pairing code</span>
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
                className="text-muted-foreground hover:text-foreground flex w-full items-center gap-2 text-left text-[11px] font-medium"
                onClick={() => setShowServerConfiguration((current) => !current)}
              >
                <Settings2 className="size-3.5" aria-hidden />
                OpenPrinter server
                <span className="text-muted-foreground/50 ml-auto font-mono text-[9px]">
                  {showServerConfiguration ? 'Hide' : 'Configure'}
                </span>
              </button>
              <p className="text-muted-foreground/60 mt-1 truncate pl-5.5 font-mono text-[9px]">
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
              <div className="mt-4 rounded border border-[var(--color-error)]/20 bg-[var(--color-error)]/5 p-3">
                <p className="text-[11px] font-semibold text-[var(--color-error)]">Pairing needs attention</p>
                {status.activeErrors.map((message) => (
                  <p key={message} className="mt-1 text-[11px] text-[var(--color-error)]/70">
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
          </div>
        </div>
      </div>
    </main>
  );
}
