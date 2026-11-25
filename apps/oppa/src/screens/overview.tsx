import { ArrowRight, CheckCircle2, CircleAlert, LoaderCircle, MoreHorizontal, Printer, RefreshCw } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { EmptyState, ScreenContainer, ScreenHeader, StateBadge, StatusDot } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
} from '@/components/ui/dropdown-menu';
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
  const navigate = useNavigate();
  const enabled = printers.filter((p) => p.enabled);
  const available = enabled.filter((p) => p.available);
  const connected = status.connectionState === 'connected';
  const connecting = ['connecting', 'authenticating', 'paired'].includes(status.connectionState);
  const unavailable = enabled.length - available.length;
  const needsPairing = ['authentication_failed', 'credential_revoked'].includes(status.connectionState);
  const healthy = connected && enabled.length > 0 && !unavailable && !status.activeErrors.length;
  const Icon = healthy ? CheckCircle2 : connecting ? LoaderCircle : CircleAlert;
  const heading = needsPairing
    ? 'Pairing needs attention'
    : !connected
      ? connecting
        ? 'Connecting to your server'
        : 'Server disconnected'
      : !enabled.length
        ? 'Choose printers for this server'
        : unavailable
          ? 'Some printers are unavailable'
          : status.activeErrors.length
            ? 'The agent needs attention'
            : 'Ready for print jobs';
  const description = needsPairing
    ? 'Verify your server and pair this computer again.'
    : !connected
      ? 'The local agent is running. Incoming jobs resume when the connection is restored.'
      : !enabled.length
        ? 'Enable a local printer to allow your server to send it print jobs.'
        : unavailable
          ? `${unavailable} enabled printer${unavailable === 1 ? ' is' : 's are'} unavailable. Check the connection or system queue.`
          : 'All enabled printers are available.';
  const recent = [...jobs].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 6);
  return (
    <ScreenContainer>
      <ScreenHeader
        title="Overview"

        action={
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Overview actions" />}>
              <MoreHorizontal />
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuGroup>
                <DropdownMenuItem disabled={busy !== null} onClick={() => void onReconnect().catch(() => undefined)}>
                  <RefreshCw />
                  Reconnect server
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => void navigate('/diagnostics')}>Inspect logs</DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-6">
        <section className="flex flex-wrap items-start justify-between gap-4">
          <div className="flex min-w-0 items-start gap-3">
            <span
              className={
                healthy
                  ? 'bg-connected/10 flex size-10 shrink-0 items-center justify-center rounded-full'
                  : 'bg-warning/10 flex size-10 shrink-0 items-center justify-center rounded-full'
              }
            >
              <Icon
                className={
                  healthy
                    ? 'text-connected mt-1 size-5 shrink-0'
                    : connecting
                      ? 'text-muted-foreground mt-1 size-5 shrink-0 animate-spin'
                      : 'text-warning mt-1 size-5 shrink-0'
                }
                aria-hidden
              />
            </span>
            <div className="min-w-0">
              <h2 className="text-lg font-semibold">{heading}</h2>
              <p className="text-muted-foreground mt-1 max-w-xl text-sm leading-5">{description}</p>
            </div>
          </div>
          {connected ? (
            <Button variant="ghost" size="sm" onClick={onNavigatePrinters}>
              Manage
              <ArrowRight />
            </Button>
          ) : needsPairing ? (
            <Button size="sm" onClick={() => void navigate('/settings?category=Connection')}>
              Review connection
            </Button>
          ) : null}
        </section>
        {status.activeErrors.length > 0 && (
          <div role="alert" className="text-error mt-4 flex flex-col gap-1 text-sm">
            {status.activeErrors.map((error) => (
              <p className="break-words" key={error}>
                {error}
              </p>
            ))}
            <button className="mt-1 w-fit underline" onClick={() => void navigate('/diagnostics')}>
              Inspect logs
            </button>
          </div>
        )}
        <dl className="border-border my-6 grid grid-cols-3 gap-4 border-y py-4">
          {[
            ['Available printers', `${available.length} / ${enabled.length}`, 'Enabled for server access'],
            ['Pending jobs', status.pendingJobs, 'In the local agent'],
            ['Submitted records', jobs.filter((j) => j.state === 'submitted').length, 'In retained history'],
          ].map(([label, value, caption]) => (
            <div key={label}>
              <dt className="text-muted-foreground text-xs">{label}</dt>
              <dd className="mt-1 text-[28px] leading-9 font-semibold tabular-nums">{value}</dd>
              <p className="text-muted-foreground mt-1 text-xs">{caption}</p>
            </div>
          ))}
        </dl>
        <section>
          <div className="mb-2 flex items-center justify-between">
            <h2 className="text-sm font-semibold">Your printers</h2>
            <span className="text-muted-foreground text-xs">{enabled.length} enabled</span>
          </div>
          {!enabled.length ? (
            <EmptyState
              title="No printers enabled"
              description="Select which local printers your server may use."
              action={
                <Button size="sm" onClick={onNavigatePrinters}>
                  <Printer />
                  Choose printers
                </Button>
              }
            />
          ) : (
            enabled.slice(0, 8).map((printer) => (
              <button
                key={printer.id}
                onClick={() => void navigate(`/printers?selected=${encodeURIComponent(printer.id)}`)}
                className="hover:bg-accent flex w-full items-center gap-3 rounded-md py-2 text-left"
              >
                <span className="device-icon">
                  <Printer className="size-4" aria-hidden />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm">{printer.displayName}</p>
                  <p className="text-muted-foreground mt-1 text-xs">
                    {titleCase(printer.connectionType)} ·{' '}
                    {printer.submissionMode.type === 'driver' ? 'System driver' : 'ESC/POS'}
                  </p>
                </div>
                <StatusDot tone={printer.available ? 'connected' : 'warning'} />
                <span className="text-muted-foreground w-20 text-right text-xs">
                  {printer.available ? 'Available' : 'Unavailable'}
                </span>
              </button>
            ))
          )}
        </section>
        <section className="border-border mt-6 border-t pt-5">
          <div className="mb-2 flex items-center justify-between">
            <h2 className="text-sm font-semibold">Recent jobs</h2>
            <Button size="sm" variant="ghost" onClick={onNavigateJobs}>
              View all
              <ArrowRight />
            </Button>
          </div>
          {!recent.length ? (
            <EmptyState
              title="No print activity yet"
              description="Jobs appear here when the agent receives work from your server."
            />
          ) : (
            recent.map((job) => (
              <button
                key={job.id}
                onClick={() => void navigate(`/jobs?selected=${encodeURIComponent(job.id)}`)}
                className="hover:bg-accent flex w-full items-center gap-4 rounded-md py-2 text-left"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{job.printerName}</p>
                  {job.error && <p className="text-error mt-1 truncate text-xs">{job.error}</p>}
                </div>
                <StateBadge state={job.state} />
                <time
                  dateTime={job.updatedAt}
                  title={new Date(job.updatedAt).toLocaleString()}
                  className="text-muted-foreground w-20 text-right text-xs"
                >
                  {formatRelativeTime(job.updatedAt)}
                </time>
              </button>
            ))
          )}
        </section>
        <p className="text-muted-foreground mt-6 text-xs">
          Availability describes the local transport. Physical printing and paper status are not verified.
        </p>
      </div>
    </ScreenContainer>
  );
}
