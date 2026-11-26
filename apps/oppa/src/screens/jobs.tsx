import { MoreHorizontal, Trash2, X } from 'lucide-react';
import { useState } from 'react';

import { EmptyState, ScreenContainer, ScreenHeader, StateBadge } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuGroup,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ConfirmAction, KeyValue, MasterDetail, SearchField, Toolbar } from '@/components/workspace';
import type { JobSummary } from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';
import { listNavigation, useSelection } from '@/lib/workspace';

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
  const [filter, setFilter] = useState('all');
  const [order, setOrder] = useState('newest');
  const [confirm, setConfirm] = useState(false);
  const [selectedId, select] = useSelection('oppa-job-selection');
  const selected = jobs.find((job) => job.id === selectedId);
  const filtered = jobs
    .filter(
      (job) =>
        (filter === 'all' || job.state === filter) &&
        [job.printerName, job.id, job.idempotencyKey, job.error ?? ''].some((value) =>
          value.toLowerCase().includes(query.toLowerCase()),
        ),
    )
    .sort((a, b) =>
      order === 'newest' ? b.updatedAt.localeCompare(a.updatedAt) : a.updatedAt.localeCompare(b.updatedAt),
    );
  return (
    <ScreenContainer>
      <ScreenHeader
        title="Print jobs"
        description="Durable local delivery history."
        action={
          <DropdownMenu>
            <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Job history actions" />}>
              <MoreHorizontal />
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuGroup>
                <DropdownMenuItem
                  variant="destructive"
                  disabled={busy !== null || jobs.length === 0}
                  onClick={() => setConfirm(true)}
                >
                  <Trash2 />
                  Clear records…
                </DropdownMenuItem>
              </DropdownMenuGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        }
      />
      <Toolbar>
        <SearchField label="Search print jobs" value={query} onChange={setQuery} />
        <select aria-label="Delivery state" value={filter} onChange={(e) => setFilter(e.target.value)}>
          {['all', 'queued', 'received', 'submitted', 'failed', 'cancelled'].map((state) => (
            <option key={state} value={state}>
              {state === 'all' ? 'All states' : titleCase(state)}
            </option>
          ))}
        </select>
        <select aria-label="Sort jobs" value={order} onChange={(e) => setOrder(e.target.value)}>
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
        </select>
        <span className="text-muted-foreground text-xs">{filtered.length} records</span>
      </Toolbar>
      <MasterDetail
        storageKey="oppa-job-inspector-width"
        master={
          !filtered.length ? (
            <EmptyState
              title="No matching print jobs"
              description={
                jobs.length
                  ? 'Adjust the search or delivery-state filter.'
                  : 'Jobs appear here when your server sends work to this agent.'
              }
            />
          ) : (
            <div className="min-h-0 flex-1 overflow-auto">
              <table className="history-table" aria-label="Print job history">
                <thead>
                  <tr>
                    <th>Printer</th>
                    <th className="w-28">Delivery</th>
                    <th className="w-24">Updated</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((job, index) => (
                    <ContextMenu key={job.id}>
                      <ContextMenuTrigger
                        render={<tr />}
                        className="history-row"
                        data-row-id={job.id}
                        tabIndex={
                          job.id === selectedId || (!filtered.some((item) => item.id === selectedId) && index === 0)
                            ? 0
                            : -1
                        }
                        aria-selected={job.id === selectedId}
                        onClick={() => select(job.id)}
                        onKeyDown={(event) => listNavigation(event, select)}
                      >
                        <td>
                          <p className="truncate font-medium">{job.printerName}</p>
                          {job.error ? (
                            <p className="text-error mt-1 line-clamp-2 text-xs" title={job.error}>
                              {job.error}
                            </p>
                          ) : job.attempts > 1 ? (
                            <p className="text-muted-foreground mt-1 text-xs">{job.attempts} attempts</p>
                          ) : null}
                        </td>
                        <td>
                          <StateBadge state={job.state} />
                        </td>
                        <td>
                          <time
                            className="text-muted-foreground text-xs"
                            dateTime={job.updatedAt}
                            title={new Date(job.updatedAt).toLocaleString()}
                          >
                            {formatRelativeTime(job.updatedAt)}
                          </time>
                        </td>
                      </ContextMenuTrigger>
                      <ContextMenuContent>
                        <ContextMenuGroup>
                          <ContextMenuItem onClick={() => select(job.id)}>Inspect job</ContextMenuItem>
                          <ContextMenuItem onClick={() => void navigator.clipboard.writeText(job.id)}>
                            Copy job ID
                          </ContextMenuItem>
                        </ContextMenuGroup>
                      </ContextMenuContent>
                    </ContextMenu>
                  ))}
                </tbody>
              </table>
            </div>
          )
        }
        detail={selected ? <JobInspector job={selected} onClose={() => select(null)} /> : null}
      />
      <div className="border-border text-muted-foreground shrink-0 border-t px-6 py-2 text-xs">
        Submitted means accepted by the local transport. Physical printing is not confirmed.
      </div>
      <ConfirmAction
        open={confirm}
        onOpenChange={setConfirm}
        title="Clear local job records?"
        description="This removes retained history from this device. It does not cancel jobs, clear the durable delivery queue, or change server delivery policies."
        busy={busy !== null}
        action={async () => {
          await onClear();
          select(null);
        }}
      />
    </ScreenContainer>
  );
}
function JobInspector({ job, onClose }: { job: JobSummary; onClose: () => void }) {
  return (
    <>
      <div className="inspector-heading">
        <h2 className="text-sm font-semibold">Job details</h2>
        <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label="Close job inspector">
          <X />
        </Button>
      </div>
      <section className="inspector-section">
        <StateBadge state={job.state} />
        <p className="mt-3 text-sm font-medium">{job.printerName}</p>
        {job.error && (
          <p data-selectable role="alert" className="text-error mt-3 text-sm leading-5 break-words">
            {job.error}
          </p>
        )}
      </section>
      <section className="inspector-section">
        <h3>Recorded lifecycle</h3>
        <ol className="border-border ml-1 flex flex-col gap-5 border-l pl-4 text-sm">
          <li>
            <p>Received by local agent</p>
            <time className="text-muted-foreground mt-1 block text-xs" dateTime={job.receivedAt}>
              {new Date(job.receivedAt).toLocaleString()}
            </time>
          </li>
          <li>
            <p>Latest recorded state · {titleCase(job.state)}</p>
            <time className="text-muted-foreground mt-1 block text-xs" dateTime={job.updatedAt}>
              {new Date(job.updatedAt).toLocaleString()}
            </time>
          </li>
        </ol>
        <p className="text-muted-foreground mt-4 text-xs leading-5">
          Intermediate stage timestamps are not recorded. Submission confirms transport acceptance, not physical output.
        </p>
        <dl className="mt-3">
          <KeyValue label="Attempts" value={String(job.attempts)} />
        </dl>
      </section>
      <section className="inspector-section">
        <h3>Technical identifiers</h3>
        <dl className="flex flex-col gap-3">
          <KeyValue label="Job ID" value={job.id} copy />
          <KeyValue label="Idempotency key" value={job.idempotencyKey} copy />
          <KeyValue label="Printer ID" value={job.printerId} copy />
        </dl>
      </section>
    </>
  );
}
