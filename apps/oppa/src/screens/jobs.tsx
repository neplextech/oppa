import { AlertTriangle, CheckCircle2, Clock3, FileClock, Search } from 'lucide-react';
import { useMemo, useState } from 'react';

import { PageHeader } from '@/components/page';
import { Badge, Card, EmptyState, inputClassName } from '@/components/ui';
import type { JobState, JobSummary } from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';

export function JobsScreen({ jobs }: { jobs: JobSummary[] }) {
  const [query, setQuery] = useState('');
  const [state, setState] = useState<JobState | 'all'>('all');
  const filtered = useMemo(
    () =>
      jobs.filter(
        (job) =>
          (state === 'all' || job.state === state) &&
          [job.id, job.printerName, job.idempotencyKey].some((value) =>
            value.toLowerCase().includes(query.toLowerCase()),
          ),
      ),
    [jobs, query, state],
  );

  return (
    <div className="mx-auto max-w-6xl">
      <PageHeader
        eyebrow="Local history"
        title="Jobs"
        description="Durable local delivery records. Submitted means the agent handed the job to a printer transport; it does not prove physical output."
      />

      <Card className="mb-4 flex flex-col gap-3 p-3 sm:flex-row">
        <label className="relative flex-1">
          <Search className="absolute top-3 left-3 size-4 text-slate-400" aria-hidden />
          <span className="sr-only">Search jobs</span>
          <input
            className={`${inputClassName} pl-9`}
            placeholder="Search job, idempotency key, or printer"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <select
          aria-label="Filter by job state"
          className={`${inputClassName} sm:w-44`}
          value={state}
          onChange={(event) => setState(event.target.value as JobState | 'all')}
        >
          <option value="all">All states</option>
          <option value="queued">Queued</option>
          <option value="received">Received</option>
          <option value="submitted">Submitted</option>
          <option value="failed">Failed</option>
          <option value="cancelled">Cancelled</option>
        </select>
      </Card>

      <Card className="overflow-hidden">
        {filtered.length === 0 ? (
          <EmptyState
            icon={<FileClock className="size-5" aria-hidden />}
            title="No matching jobs"
            description="Try a different search or delivery state."
          />
        ) : (
          <div className="divide-y divide-slate-100">
            {filtered.map((job) => (
              <div
                key={job.id}
                className="grid gap-3 px-5 py-4 md:grid-cols-[1.1fr_0.9fr_0.7fr_0.45fr] md:items-center"
              >
                <div className="flex min-w-0 gap-3">
                  <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-slate-100">
                    {job.state === 'submitted' ? (
                      <CheckCircle2 className="size-4 text-emerald-600" aria-hidden />
                    ) : job.state === 'failed' ? (
                      <AlertTriangle className="size-4 text-red-500" aria-hidden />
                    ) : (
                      <Clock3 className="size-4 text-amber-500" aria-hidden />
                    )}
                  </div>
                  <div className="min-w-0">
                    <div className="truncate text-sm font-semibold text-slate-800">{job.id}</div>
                    <div className="mt-0.5 truncate font-mono text-[10px] text-slate-400">{job.idempotencyKey}</div>
                  </div>
                </div>
                <div>
                  <div className="text-xs font-semibold text-slate-600">{job.printerName}</div>
                  <div className="mt-0.5 text-[11px] text-slate-400">
                    {job.attempts} {job.attempts === 1 ? 'attempt' : 'attempts'}
                  </div>
                </div>
                <div className="text-[11px] text-slate-500">
                  <div>{formatRelativeTime(job.updatedAt)}</div>
                  {job.error ? <div className="mt-1 text-red-500">{job.error}</div> : null}
                </div>
                <div className="md:text-right">
                  <Badge
                    tone={
                      job.state === 'submitted'
                        ? 'success'
                        : job.state === 'failed'
                          ? 'danger'
                          : job.state === 'cancelled'
                            ? 'neutral'
                            : 'warning'
                    }
                  >
                    {titleCase(job.state)}
                  </Badge>
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
