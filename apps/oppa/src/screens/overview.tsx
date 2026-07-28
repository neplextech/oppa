import { AlertTriangle, ArrowRight, CheckCircle2, Clock3, Cloud, Printer, RefreshCw, Send } from 'lucide-react';
import type { ReactNode } from 'react';

import { PageHeader, SectionHeading } from '@/components/page';
import { Badge, Button, Card, StatusDot } from '@/components/ui';
import type { AgentStatus, JobSummary, PrinterSummary } from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';

export function OverviewScreen({
  status,
  printers,
  jobs,
  busy,
  onNavigatePrinters,
  onNavigateJobs,
  onReconnect,
}: {
  status: AgentStatus;
  printers: PrinterSummary[];
  jobs: JobSummary[];
  busy: string | null;
  onNavigatePrinters: () => void;
  onNavigateJobs: () => void;
  onReconnect: () => Promise<void>;
}) {
  const enabled = printers.filter((printer) => printer.enabled);
  const online = enabled.filter((printer) => printer.available);
  const recent = jobs.slice(0, 4);

  return (
    <div className="mx-auto max-w-6xl">
      <PageHeader
        eyebrow="Agent overview"
        title="Everything is ready to print"
        description={`${status.product.name} is connected, your configured printers are synchronized, and incoming jobs are processed in the background.`}
        action={
          <Button variant="secondary" busy={busy === 'reconnect'} onClick={() => void onReconnect()}>
            <RefreshCw className="size-4" aria-hidden />
            Reconnect
          </Button>
        }
      />

      {status.activeErrors.length > 0 ? (
        <Card className="mb-6 flex items-start gap-3 border-amber-200 bg-amber-50 p-4">
          <AlertTriangle className="mt-0.5 size-4 text-amber-600" aria-hidden />
          <div>
            <div className="text-sm font-semibold text-amber-900">Action required</div>
            <ul className="mt-1 space-y-1 text-xs text-amber-700">
              {status.activeErrors.map((message) => (
                <li key={message}>{message}</li>
              ))}
            </ul>
          </div>
        </Card>
      ) : null}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <Metric
          icon={<Cloud className="size-4" aria-hidden />}
          label="Gateway"
          value={titleCase(status.gatewayState)}
          detail={`Connected ${formatRelativeTime(status.lastConnectionAt)}`}
          tone="blue"
        />
        <Metric
          icon={<Printer className="size-4" aria-hidden />}
          label="Printers online"
          value={`${online.length} / ${enabled.length}`}
          detail={`${printers.length} discovered in total`}
          tone="emerald"
        />
        <Metric
          icon={<Clock3 className="size-4" aria-hidden />}
          label="Pending jobs"
          value={String(status.pendingJobs)}
          detail="Durably stored on this device"
          tone="amber"
        />
        <Metric
          icon={<Send className="size-4" aria-hidden />}
          label="Submitted today"
          value={String(jobs.filter((job) => job.state === 'submitted').length)}
          detail="Submission, not physical completion"
          tone="slate"
        />
      </div>

      <div className="mt-7 grid gap-6 xl:grid-cols-[1.25fr_0.75fr]">
        <Card className="p-5">
          <SectionHeading
            title="Recent jobs"
            description="Latest local delivery state"
            action={
              <Button variant="ghost" size="sm" onClick={onNavigateJobs}>
                View all
                <ArrowRight className="size-3.5" aria-hidden />
              </Button>
            }
          />
          <div className="divide-y divide-slate-100">
            {recent.map((job) => (
              <div key={job.id} className="flex items-center gap-3 py-3.5">
                <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-slate-100 text-slate-500">
                  {job.state === 'submitted' ? (
                    <CheckCircle2 className="size-4 text-emerald-600" aria-hidden />
                  ) : job.state === 'failed' ? (
                    <AlertTriangle className="size-4 text-red-500" aria-hidden />
                  ) : (
                    <Clock3 className="size-4 text-amber-500" aria-hidden />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-semibold text-slate-800">{job.id}</div>
                  <div className="mt-0.5 truncate text-xs text-slate-400">
                    {job.printerName} · {formatRelativeTime(job.updatedAt)}
                  </div>
                </div>
                <Badge tone={job.state === 'submitted' ? 'success' : job.state === 'failed' ? 'danger' : 'warning'}>
                  {titleCase(job.state)}
                </Badge>
              </div>
            ))}
          </div>
        </Card>

        <Card className="overflow-hidden">
          <div className="border-b border-slate-100 p-5">
            <SectionHeading
              title="Printer health"
              description="Enabled local targets"
              action={
                <Button variant="ghost" size="sm" onClick={onNavigatePrinters}>
                  Manage
                </Button>
              }
            />
          </div>
          <div className="space-y-1 p-3">
            {enabled.slice(0, 5).map((printer) => (
              <div key={printer.id} className="flex items-center gap-3 rounded-lg px-3 py-2.5 hover:bg-slate-50">
                <StatusDot tone={printer.available ? 'green' : 'red'} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-semibold text-slate-700">{printer.displayName}</div>
                  <div className="truncate text-[11px] text-slate-400">{titleCase(printer.connectionType)}</div>
                </div>
                <span className="text-[11px] font-medium text-slate-400">
                  {printer.available ? 'Ready' : 'Unavailable'}
                </span>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  );
}

function Metric({
  icon,
  label,
  value,
  detail,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  detail: string;
  tone: 'blue' | 'emerald' | 'amber' | 'slate';
}) {
  return (
    <Card className="p-5">
      <div
        className={`flex size-8 items-center justify-center rounded-lg ${
          tone === 'blue'
            ? 'bg-blue-50 text-blue-600'
            : tone === 'emerald'
              ? 'bg-emerald-50 text-emerald-600'
              : tone === 'amber'
                ? 'bg-amber-50 text-amber-600'
                : 'bg-slate-100 text-slate-600'
        }`}
      >
        {icon}
      </div>
      <div className="mt-4 text-xs font-semibold text-slate-500">{label}</div>
      <div className="mt-1 text-2xl font-bold tracking-[-0.025em] text-slate-950">{value}</div>
      <div className="mt-1 text-[11px] leading-4 text-slate-400">{detail}</div>
    </Card>
  );
}
