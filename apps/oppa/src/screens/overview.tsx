import { ArrowRight, RefreshCw, TriangleAlert } from 'lucide-react';

import {
  DefList,
  DefTerm,
  DefValue,
  EmptyState,
  ScreenContainer,
  ScreenHeader,
  SectionHeader,
  StateBadge,
  StatusDot,
} from '@/components/ui';
import { Button } from '@/components/ui/button';
import type { AgentStatus, JobSummary, PrinterSummary } from '@/lib/types';
import { cn, formatRelativeTime, titleCase } from '@/lib/utils';

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
  const enabled = printers.filter((p) => p.enabled);
  const online = enabled.filter((p) => p.available);
  const recent = jobs.slice(0, 12);
  const hasErrors = status.activeErrors.length > 0;

  const agentTone =
    status.connectionState === 'connected'
      ? 'connected'
      : status.connectionState === 'authentication_failed' || status.connectionState === 'credential_revoked'
        ? 'error'
        : 'pending';

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Overview"
        description="Operational status and recent activity"
        action={
          <Button variant="outline" size="sm" disabled={busy === 'reconnect'} onClick={() => void onReconnect()}>
            {busy === 'reconnect' ? (
              <RefreshCw className="size-3 animate-spin" aria-hidden />
            ) : (
              <RefreshCw className="size-3" aria-hidden />
            )}
            Reconnect
          </Button>
        }
      />

      {/* Active errors */}
      {hasErrors && (
        <div className="flex shrink-0 items-start gap-3 border-b border-[var(--color-warning)]/20 bg-[var(--color-warning)]/5 px-5 py-3">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-[var(--color-warning)]" aria-hidden />
          <div className="min-w-0 flex-1">
            <p className="text-xs font-semibold text-[var(--color-warning)]">Action required</p>
            <ul className="mt-1 space-y-0.5 text-xs text-[var(--color-warning)]/70">
              {status.activeErrors.map((msg) => (
                <li key={msg}>{msg}</li>
              ))}
            </ul>
          </div>
        </div>
      )}

      {/* Status strip */}
      <div className="border-border bg-card/50 shrink-0 border-b px-5 py-3">
        <div className="flex flex-wrap items-center gap-x-8 gap-y-2">
          <StatusItem label="Agent">
            <StatusDot
              tone={agentTone}
              pulse={status.connectionState === 'connecting' || status.connectionState === 'authenticating'}
            />
            <span className={cn('ml-1', agentTone === 'connected' ? 'text-[var(--color-connected)]' : '')}>
              {titleCase(status.connectionState)}
            </span>
          </StatusItem>
          <StatusItem label="Printers">
            <span>
              {online.length}/{enabled.length} online
            </span>
          </StatusItem>
          <StatusItem label="Queue">
            <span className={cn(status.pendingJobs > 0 ? 'text-[var(--color-pending)]' : 'text-muted-foreground')}>
              {status.pendingJobs} pending
            </span>
          </StatusItem>
          <StatusItem label="Connected">
            <span>{formatRelativeTime(status.lastConnectionAt)}</span>
          </StatusItem>
          {status.agentId && (
            <StatusItem label="Agent ID">
              <span className="font-mono text-xs">{status.agentId}</span>
            </StatusItem>
          )}
        </div>
      </div>

      {/* Main body */}
      <div className="flex flex-1 overflow-hidden">
        {/* Recent jobs */}
        <div className="border-border flex flex-1 flex-col overflow-hidden border-r">
          <div className="border-border flex shrink-0 items-center justify-between border-b px-5 py-3">
            <SectionHeader title="Recent jobs" />
            <button
              type="button"
              onClick={onNavigateJobs}
              className="text-muted-foreground hover:text-foreground flex items-center gap-1 text-xs transition-colors"
            >
              All jobs
              <ArrowRight className="size-3" aria-hidden />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto">
            {recent.length === 0 ? (
              <EmptyState
                title="No jobs yet"
                description="Jobs will appear here once the agent receives work from the server."
              />
            ) : (
              <table className="w-full text-xs">
                <thead className="bg-background/95 sticky top-0 backdrop-blur-sm">
                  <tr className="border-border border-b text-left">
                    <th className="text-muted-foreground px-5 py-2 font-medium">State</th>
                    <th className="text-muted-foreground px-3 py-2 font-medium">Job ID</th>
                    <th className="text-muted-foreground hidden px-3 py-2 font-medium md:table-cell">Printer</th>
                    <th className="text-muted-foreground px-3 py-2 text-right font-medium">Received</th>
                  </tr>
                </thead>
                <tbody>
                  {recent.map((job) => (
                    <JobRow key={job.id} job={job} />
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>

        {/* Agent health */}
        <div className="w-64 shrink-0 overflow-y-auto xl:w-72">
          <div className="border-border flex items-center justify-between border-b px-4 py-3">
            <SectionHeader title="Health" />
            <button
              type="button"
              onClick={onNavigatePrinters}
              className="text-muted-foreground hover:text-foreground text-xs transition-colors"
            >
              Manage
            </button>
          </div>

          <div className="space-y-4 px-4 py-3">
            {/* System health */}
            <div>
              <p className="text-muted-foreground/60 mb-2 text-xs font-semibold tracking-wider uppercase">System</p>
              <DefList>
                <DefTerm>State</DefTerm>
                <DefValue className={agentTone === 'connected' ? 'text-[var(--color-connected)]' : ''}>
                  {titleCase(status.connectionState)}
                </DefValue>
                <DefTerm>Product</DefTerm>
                <DefValue>{status.product.id}</DefValue>
                <DefTerm>Version</DefTerm>
                <DefValue>v{status.version}</DefValue>
                <DefTerm>Platform</DefTerm>
                <DefValue>{status.platform}</DefValue>
              </DefList>
            </div>

            {/* Printer health */}
            <div>
              <p className="text-muted-foreground/60 mb-2 text-xs font-semibold tracking-wider uppercase">
                Printers
              </p>
              {enabled.length === 0 ? (
                <p className="text-muted-foreground/60 text-xs">No printers enabled.</p>
              ) : (
                <div className="space-y-1">
                  {enabled.slice(0, 6).map((printer) => (
                    <div key={printer.id} className="flex items-center gap-2 text-xs">
                      <StatusDot tone={printer.available ? 'connected' : 'error'} />
                      <span className="text-foreground/80 flex-1 truncate">{printer.displayName}</span>
                      <span className="text-muted-foreground/60 shrink-0">
                        {printer.available ? 'ready' : 'offline'}
                      </span>
                    </div>
                  ))}
                  {enabled.length > 6 && (
                    <p className="text-muted-foreground/50 text-xs">+{enabled.length - 6} more</p>
                  )}
                </div>
              )}
            </div>

            {/* Queue stats */}
            <div>
              <p className="text-muted-foreground/60 mb-2 text-xs font-semibold tracking-wider uppercase">Queue</p>
              <DefList>
                <DefTerm>Pending</DefTerm>
                <DefValue>{status.pendingJobs}</DefValue>
                <DefTerm>Total</DefTerm>
                <DefValue>{jobs.length}</DefValue>
                <DefTerm>Failed</DefTerm>
                <DefValue
                  className={jobs.filter((j) => j.state === 'failed').length > 0 ? 'text-[var(--color-error)]' : ''}
                >
                  {jobs.filter((j) => j.state === 'failed').length}
                </DefValue>
              </DefList>
            </div>
          </div>
        </div>
      </div>
    </ScreenContainer>
  );
}

function StatusItem({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-1.5 text-xs">
      <span className="text-muted-foreground/60">{label}</span>
      <span className="text-foreground/80 flex items-center gap-1 font-mono">{children}</span>
    </div>
  );
}

function JobRow({ job }: { job: JobSummary }) {
  return (
    <tr className="border-border/50 hover:bg-accent/30 border-b transition-colors">
      <td className="px-5 py-2">
        <StateBadge state={job.state} />
      </td>
      <td className="px-3 py-2">
        <span className="text-foreground/80 font-mono text-xs">{job.id}</span>
        {job.error && <p className="mt-0.5 truncate text-xs text-[var(--color-error)]/70">{job.error}</p>}
      </td>
      <td className="hidden px-3 py-2 md:table-cell">
        <span className="text-foreground/70 truncate">{job.printerName}</span>
      </td>
      <td className="text-muted-foreground px-3 py-2 text-right">{formatRelativeTime(job.updatedAt)}</td>
    </tr>
  );
}
