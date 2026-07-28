import { ArrowRight, LockKeyhole, Printer, RefreshCw, ShieldCheck } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type { AgentStatus } from '@/lib/types';

export function SetupScreen({
  status,
  busy,
  onAuthorize,
}: {
  status: AgentStatus;
  busy: boolean;
  onAuthorize: () => Promise<void>;
}) {
  const authorizing = status.state === 'authorizing';

  return (
    <main className="bg-background flex min-h-dvh items-center justify-center p-6">
      <div className="border-border w-full max-w-3xl overflow-hidden rounded border">
        <div className="grid md:grid-cols-[0.85fr_1.15fr]">
          {/* Left panel */}
          <div className="border-border bg-card flex flex-col justify-between border-r p-8">
            <div>
              <div className="border-border bg-primary/10 flex size-9 items-center justify-center rounded border">
                <Printer className="text-primary size-4" aria-hidden />
              </div>

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
              Your browser will open the authorization page configured for this product build. The agent never sees your
              password.
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

            <Button className="mt-8 w-full" disabled={authorizing || busy} onClick={() => void onAuthorize()}>
              {busy ? (
                <RefreshCw className="size-3.5 animate-spin" aria-hidden />
              ) : (
                <LockKeyhole className="size-3.5" aria-hidden />
              )}
              {authorizing ? 'Waiting for browser authorization…' : 'Connect account'}
              {!authorizing && !busy && <ArrowRight className="ml-auto size-3.5" aria-hidden />}
            </Button>

            <p className="text-muted-foreground/50 mt-3 text-center text-[10px]">
              Authorization expires quickly and is accepted only on this computer.
            </p>

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
