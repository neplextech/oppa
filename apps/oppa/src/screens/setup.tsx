import { ArrowRight, LockKeyhole, RefreshCw, Settings2, ShieldCheck } from 'lucide-react';
import { useState } from 'react';

import { ServerConfigurationForm } from '@/components/server-configuration-form';
import { Button } from '@/components/ui/button';
import type { AgentStatus, OpenPrinterServerConfiguration } from '@/lib/types';

export function SetupScreen({
  status,
  busy,
  onAuthorize,
  onSaveServerConfiguration,
  onResetServerConfiguration,
}: {
  status: AgentStatus;
  busy: string | null;
  onAuthorize: () => Promise<void>;
  onSaveServerConfiguration: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onResetServerConfiguration: () => Promise<void>;
}) {
  const authorizing = status.state === 'authorizing';
  const [showServerConfiguration, setShowServerConfiguration] = useState(false);

  return (
    <main className="bg-background flex min-h-dvh items-center justify-center p-6">
      <div className="border-border w-full max-w-3xl overflow-hidden rounded border">
        <div className="grid md:grid-cols-[0.85fr_1.15fr]">
          {/* Left panel */}
          <div className="border-border bg-card flex flex-col justify-between border-r p-8">
            <div>
              <img
                src="/oppa-icon.png"
                alt=""
                className="border-border size-10 rounded-xl border shadow-sm"
                draggable={false}
              />

              <p className="text-primary mt-8 text-[10px] font-semibold tracking-widest uppercase">
                {status.product.name}
              </p>

              <h1 className="text-foreground mt-3 text-xl leading-7 font-semibold tracking-tight">
                Connect cloud applications to the printers on this computer.
              </h1>

              <p className="text-muted-foreground mt-3 text-[11px] leading-5">
                {status.product.name} discovers local printers, receives authorized print jobs from a connected server,
                and records each delivery before submission.
              </p>

              <div className="mt-8 space-y-2">
                {[
                  'No arbitrary file access or shell commands',
                  'Credentials stay in OS secure storage',
                  'All jobs persisted before acknowledgement',
                  'Explicit timeouts on all network operations',
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
              OpenPrinter protocol enforced
            </div>
          </div>

          {/* Right panel */}
          <div className="bg-background p-8">
            <div className="text-muted-foreground/60 flex items-center gap-2 text-[10px] font-semibold tracking-widest uppercase">
              <span className="border-border bg-card flex size-4 items-center justify-center rounded border text-[9px] font-bold">
                1
              </span>
              Initial setup
            </div>

            <h2 className="text-foreground mt-5 text-base font-semibold">Connect your account</h2>
            <p className="text-muted-foreground mt-1.5 text-[11px] leading-5">
              Your browser will open the authorization page for the selected OpenPrinter service. The agent never sees
              your password.
            </p>

            <div className="mt-6 space-y-3">
              {(
                [
                  ['Open browser', 'Choose your account and approve this agent.'],
                  ['Discover printers', 'Review system, USB, network, and virtual printers.'],
                  ['Stay connected', `${status.product.name} continues safely in the background.`],
                ] as const
              ).map(([title, description], i) => (
                <div key={title} className="flex gap-3 text-[11px]">
                  <div className="border-border bg-card text-muted-foreground mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full border text-[9px] font-medium">
                    {i + 1}
                  </div>
                  <div>
                    <p className="text-foreground/90 font-medium">{title}</p>
                    <p className="text-muted-foreground/70">{description}</p>
                  </div>
                </div>
              ))}
            </div>

            <Button
              className="mt-8 w-full"
              disabled={authorizing || busy === 'authorize'}
              onClick={() => void onAuthorize()}
            >
              {busy === 'authorize' ? (
                <RefreshCw className="size-3.5 animate-spin" aria-hidden />
              ) : (
                <LockKeyhole className="size-3.5" aria-hidden />
              )}
              {authorizing ? 'Waiting for browser authorization…' : 'Connect account'}
              {!authorizing && busy !== 'authorize' && <ArrowRight className="ml-auto size-3.5" aria-hidden />}
            </Button>

            <p className="text-muted-foreground/50 mt-3 text-center text-[10px]">
              Authorization expires quickly and is accepted only on this computer.
            </p>

            <div className="border-border mt-5 border-t pt-4">
              <button
                type="button"
                className="text-muted-foreground hover:text-foreground flex w-full items-center gap-2 text-left text-[11px] font-medium transition-colors"
                aria-expanded={showServerConfiguration}
                onClick={() => setShowServerConfiguration((current) => !current)}
              >
                <Settings2 className="size-3.5" aria-hidden />
                Use a different OpenPrinter service
                <span className="text-muted-foreground/50 ml-auto font-mono text-[9px]">
                  {showServerConfiguration ? 'Hide' : 'Configure'}
                </span>
              </button>
              <p className="text-muted-foreground/60 mt-1 truncate pl-5.5 font-mono text-[9px]">
                {status.serverConfiguration.gatewayUrl}
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

            {status.activeErrors.length > 0 && (
              <div className="mt-4 rounded border border-[var(--color-error)]/20 bg-[var(--color-error)]/5 p-3">
                <p className="text-[11px] font-semibold text-[var(--color-error)]">Authorization needs attention</p>
                <ul className="mt-1.5 space-y-1 text-[11px] text-[var(--color-error)]/70">
                  {status.activeErrors.map((msg) => (
                    <li key={msg}>{msg}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        </div>
      </div>
    </main>
  );
}
