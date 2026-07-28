import { ArrowRight, CheckCircle2, LockKeyhole, Printer, ShieldCheck } from 'lucide-react';

import { Card, Button } from '@/components/ui';
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
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-[#f7f8fa] p-6">
      <div className="absolute inset-0 bg-[radial-gradient(circle_at_top_left,rgba(59,130,246,0.08),transparent_35%),radial-gradient(circle_at_bottom_right,rgba(15,23,42,0.07),transparent_30%)]" />
      <div className="relative grid w-full max-w-4xl overflow-hidden rounded-3xl border border-slate-200 bg-white shadow-[0_28px_80px_rgba(15,23,42,0.12)] md:grid-cols-[0.9fr_1.1fr]">
        <section className="flex flex-col justify-between bg-slate-950 p-8 text-white md:p-10">
          <div>
            <div className="flex size-11 items-center justify-center rounded-2xl bg-white/10 ring-1 ring-white/15">
              <Printer className="size-5" aria-hidden />
            </div>
            <p className="mt-10 text-xs font-bold tracking-[0.2em] text-blue-300 uppercase">{status.product.name}</p>
            <h1 className="mt-3 text-3xl leading-tight font-bold tracking-[-0.035em]">
              Connect cloud applications to the printers on this computer.
            </h1>
            <p className="mt-4 text-sm leading-6 text-slate-300">
              {status.product.name} discovers local printers, receives authorized jobs, and records each job before
              submission.
            </p>
          </div>
          <div className="mt-12 flex items-center gap-2 text-xs text-slate-400">
            <ShieldCheck className="size-4 text-emerald-400" aria-hidden />
            No arbitrary files, scripts, or shell commands
          </div>
        </section>

        <section className="p-8 md:p-10">
          <div className="flex items-center gap-2 text-xs font-bold tracking-[0.16em] text-blue-600 uppercase">
            <span className="flex size-5 items-center justify-center rounded-full bg-blue-50">1</span>
            Initial setup
          </div>
          <h2 className="mt-5 text-2xl font-bold tracking-[-0.025em] text-slate-950">Connect your account</h2>
          <p className="mt-2 text-sm leading-6 text-slate-500">
            Your browser will open the authorization page configured by this product build. The agent never sees your
            password.
          </p>

          <div className="mt-7 space-y-4">
            {[
              ['Authorize in your browser', 'Choose the account and approve this agent.'],
              ['Discover printers', 'Review system, network, and virtual printers.'],
              ['Stay connected', `${status.product.name} continues safely in the background.`],
            ].map(([title, description], index) => (
              <div key={title} className="flex gap-3">
                <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-emerald-500" aria-hidden />
                <div>
                  <div className="text-sm font-semibold text-slate-800">
                    {index + 1}. {title}
                  </div>
                  <div className="mt-0.5 text-xs leading-5 text-slate-500">{description}</div>
                </div>
              </div>
            ))}
          </div>

          <Button className="mt-8 h-11 w-full" busy={busy} disabled={authorizing} onClick={() => void onAuthorize()}>
            <LockKeyhole className="size-4" aria-hidden />
            {authorizing ? 'Waiting for browser authorization…' : 'Connect account'}
            <ArrowRight className="ml-auto size-4" aria-hidden />
          </Button>
          <p className="mt-4 text-center text-[11px] leading-5 text-slate-400">
            Authorization expires quickly and is accepted only on this computer.
          </p>
          {status.activeErrors.length > 0 ? (
            <Card className="mt-5 border-red-200 bg-red-50 p-3 text-xs leading-5 text-red-700">
              <div className="font-bold text-red-900">Authorization needs attention</div>
              <ul className="mt-1 list-disc space-y-1 pl-4">
                {status.activeErrors.map((message) => (
                  <li key={message}>{message}</li>
                ))}
              </ul>
            </Card>
          ) : null}
        </section>
      </div>
    </main>
  );
}
