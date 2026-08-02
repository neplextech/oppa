import { RefreshCw, Search, Trash2, X } from 'lucide-react';
import { useMemo, useState } from 'react';

import { EmptyState, ScreenContainer, ScreenHeader, StateBadge } from '@/components/ui';
import { Button } from '@/components/ui/button';
import type { JobState, JobSummary } from '@/lib/types';
import { cn, formatRelativeTime } from '@/lib/utils';

const STATE_OPTIONS: Array<{ value: JobState | 'all'; label: string }> = [
  { value: 'all', label: 'All' },
  { value: 'queued', label: 'Queued' },
  { value: 'received', label: 'Received' },
  { value: 'submitted', label: 'Submitted' },
  { value: 'failed', label: 'Failed' },
  { value: 'cancelled', label: 'Cancelled' },
];

export function JobsScreen({
  jobs,
  busy,
  onClear,
}: {
  jobs: JobSummary[];
  busy: string | null;
  onClear: () => Promise<void>;
}) {
  const [query, setQuery] = useState('');
  const [stateFilter, setStateFilter] = useState<JobState | 'all'>('all');
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = useMemo(
    () =>
      jobs.filter(
        (job) =>
          (stateFilter === 'all' || job.state === stateFilter) &&
          (query === '' ||
            [job.id, job.printerName, job.idempotencyKey].some((v) => v.toLowerCase().includes(query.toLowerCase()))),
      ),
    [jobs, query, stateFilter],
  );

  const selected = useMemo(() => jobs.find((j) => j.id === selectedId) ?? null, [jobs, selectedId]);

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Jobs"
        description="Durable local delivery records. Submitted means the job was handed to the printer transport."
        action={
          <Button
            variant="outline"
            size="sm"
            disabled={busy === 'clear-jobs' || jobs.length === 0}
            onClick={() => void onClear()}
          >
            {busy === 'clear-jobs' ? (
              <RefreshCw className="size-3.5 animate-spin" aria-hidden />
            ) : (
              <Trash2 className="size-3.5" aria-hidden />
            )}
            Clear records
          </Button>
        }
      />

      {/* Toolbar */}
      <div className="border-border bg-card/30 flex shrink-0 items-center gap-3 border-b px-4 py-2.5">
        <div className="relative max-w-xs flex-1">
          <Search
            className="text-muted-foreground/50 absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2"
            aria-hidden
          />
          <input
            className="border-border bg-secondary text-foreground placeholder:text-muted-foreground/50 focus:border-primary/50 focus:ring-primary/20 h-8 w-full rounded border pr-7 pl-8 text-sm transition outline-none focus:ring-1"
            placeholder="Search by job ID, printer, or key…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Search jobs"
          />
          {query && (
            <button
              type="button"
              onClick={() => setQuery('')}
              className="text-muted-foreground/50 hover:text-foreground absolute top-1/2 right-1.5 -translate-y-1/2 rounded p-0.5 transition-colors"
              aria-label="Clear search"
            >
              <X className="size-3.5" aria-hidden />
            </button>
          )}
        </div>

        <div className="flex items-center gap-1">
          {STATE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setStateFilter(opt.value)}
              className={cn(
                'rounded px-2.5 py-1 text-xs font-medium transition-colors',
                stateFilter === opt.value
                  ? 'bg-secondary text-foreground'
                  : 'text-muted-foreground hover:text-foreground',
              )}
            >
              {opt.label}
            </button>
          ))}
        </div>

        <span className="text-muted-foreground/60 ml-auto text-xs">
          {filtered.length} {filtered.length === 1 ? 'job' : 'jobs'}
        </span>
      </div>

      <div className="flex flex-1 overflow-hidden">
        {/* Table */}
        <div className={cn('flex flex-col overflow-hidden', selected ? 'flex-1' : 'w-full')}>
          {filtered.length === 0 ? (
            <EmptyState
              title="No matching jobs"
              description={
                query || stateFilter !== 'all'
                  ? 'Try adjusting the search or state filter.'
                  : 'Jobs will appear here once the agent receives work from the server.'
              }
            />
          ) : (
            <div className="flex-1 overflow-y-auto">
              <table className="w-full text-sm">
                <thead className="bg-background/95 sticky top-0 z-10 backdrop-blur-sm">
                  <tr className="border-border border-b text-left">
                    <th className="text-muted-foreground px-5 py-2.5 text-xs font-medium">State</th>
                    <th className="text-muted-foreground px-3 py-2.5 text-xs font-medium">Job ID</th>
                    <th className="text-muted-foreground hidden px-3 py-2.5 text-xs font-medium lg:table-cell">
                      Printer
                    </th>
                    <th className="text-muted-foreground hidden px-3 py-2.5 text-xs font-medium xl:table-cell">
                      Attempts
                    </th>
                    <th className="text-muted-foreground px-3 py-2.5 text-right text-xs font-medium">Updated</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((job) => (
                    <tr
                      key={job.id}
                      className={cn(
                        'cursor-pointer border-b border-border/50 transition-colors',
                        selectedId === job.id ? 'bg-primary/5' : 'hover:bg-accent/30',
                      )}
                      onClick={() => setSelectedId((prev) => (prev === job.id ? null : job.id))}
                      tabIndex={0}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          setSelectedId((prev) => (prev === job.id ? null : job.id));
                        }
                      }}
                    >
                      <td className="px-5 py-3">
                        <StateBadge state={job.state} />
                      </td>
                      <td className="px-3 py-3">
                        <p className="text-foreground/90 font-mono text-xs">{job.id}</p>
                        <p className="text-muted-foreground/60 mt-0.5 max-w-[180px] truncate font-mono text-xs">
                          {job.idempotencyKey}
                        </p>
                      </td>
                      <td className="hidden px-3 py-3 lg:table-cell">
                        <span className="text-foreground/80">{job.printerName}</span>
                      </td>
                      <td className="hidden px-3 py-3 xl:table-cell">
                        <span className="text-muted-foreground">{job.attempts}</span>
                      </td>
                      <td className="text-muted-foreground px-3 py-3 text-right text-xs">
                        {formatRelativeTime(job.updatedAt)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        {/* Detail pane */}
        {selected && (
          <div className="border-border w-72 shrink-0 overflow-y-auto border-l xl:w-80">
            <JobDetail job={selected} onClose={() => setSelectedId(null)} />
          </div>
        )}
      </div>
    </ScreenContainer>
  );
}

function JobDetail({ job, onClose }: { job: JobSummary; onClose: () => void }) {
  const lifecycle: Array<{ label: string; done: boolean; active: boolean }> = [
    { label: 'received', done: true, active: job.state === 'received' },
    {
      label: 'persisted',
      done: !['queued'].includes(job.state),
      active: false,
    },
    {
      label: 'submitted',
      done: job.state === 'submitted',
      active: job.state === 'submitted',
    },
  ];

  return (
    <>
      <div className="border-border flex items-center justify-between border-b px-4 py-3">
        <p className="text-muted-foreground text-xs font-semibold tracking-wider uppercase">Job detail</p>
        <button
          type="button"
          onClick={onClose}
          className="text-muted-foreground hover:text-foreground rounded p-0.5 transition-colors"
          aria-label="Close detail"
        >
          <X className="size-3.5" />
        </button>
      </div>

      <div className="space-y-4 p-4 text-sm">
        {/* State */}
        <div>
          <StateBadge state={job.state} />
          {job.error && (
            <p className="mt-2 rounded border border-[var(--color-error)]/20 bg-[var(--color-error)]/5 px-2 py-1.5 font-mono text-xs leading-4 text-[var(--color-error)]/80">
              {job.error}
            </p>
          )}
        </div>

        {/* Identifiers */}
        <Section title="Identifiers">
          <KV label="Job ID">{job.id}</KV>
          <KV label="Idempotency key">{job.idempotencyKey}</KV>
          <KV label="Printer">{job.printerName}</KV>
          <KV label="Printer ID">{job.printerId}</KV>
        </Section>

        {/* Timing */}
        <Section title="Timing">
          <KV label="Received">{formatRelativeTime(job.receivedAt)}</KV>
          <KV label="Last update">{formatRelativeTime(job.updatedAt)}</KV>
          <KV label="Attempts">{String(job.attempts)}</KV>
        </Section>

        {/* Lifecycle */}
        <Section title="Lifecycle">
          <div className="space-y-2 pt-1">
            {lifecycle.map((step) => (
              <div key={step.label} className="flex items-center gap-2">
                <span
                  className={cn(
                    'size-1.5 rounded-full',
                    step.active
                      ? 'bg-[var(--color-connected)]'
                      : step.done
                        ? 'bg-[var(--color-connected)]/40'
                        : 'bg-muted',
                  )}
                />
                <span
                  className={cn(
                    'font-mono text-xs uppercase tracking-wide',
                    step.done ? 'text-foreground/70' : 'text-muted-foreground/40',
                  )}
                >
                  {step.label}
                </span>
              </div>
            ))}
          </div>
        </Section>
      </div>
    </>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="text-muted-foreground/50 mb-2 text-xs font-semibold tracking-wider uppercase">{title}</p>
      <div className="space-y-1.5">{children}</div>
    </div>
  );
}

function KV({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-muted-foreground/60 text-xs">{label}</span>
      <span className="text-foreground/80 font-mono text-xs break-all">{children}</span>
    </div>
  );
}
