import { Copy, Download, MoreHorizontal, RefreshCw, Trash2 } from 'lucide-react';
import { useState } from 'react';

import { EmptyState, ScreenContainer, ScreenHeader, StatusDot } from '@/components/ui';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ConfirmAction, KeyValue, SearchField, Toolbar } from '@/components/workspace';
import type { Diagnostics } from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';

export function DiagnosticsScreen({
  diagnostics,
  busy,
  onReload,
  onExport,
  canExportFile = true,
  onClearLogs,
}: {
  diagnostics: Diagnostics;
  busy: string | null;
  onReload: () => Promise<void>;
  onExport: () => Promise<string>;
  /** Whether the platform can write a diagnostics file the user can find. */
  canExportFile?: boolean;
  onClearLogs: () => Promise<void>;
}) {
  const [query, setQuery] = useState('');
  const [severity, setSeverity] = useState('all');
  const [clear, setClear] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const logs = diagnostics.logs.filter(
    (entry) =>
      (severity === 'all' || entry.level === severity) &&
      `${entry.target} ${entry.message}`.toLowerCase().includes(query.toLowerCase()),
  );
  const copy = (value: string) => {
    void navigator.clipboard
      .writeText(value)
      .then(() => setNotice('Copied sanitized diagnostics.'))
      .catch(() => setNotice('Unable to copy. Select the entry text and copy it manually.'));
  };
  return (
    <ScreenContainer>
      <ScreenHeader
        title="Logs"
        description="Sanitized local diagnostics. Credentials and print payloads are excluded."
        action={
          <div className="flex gap-2">
            <Button variant="outline" size="sm" disabled={busy !== null} onClick={() => void onReload()}>
              <RefreshCw />
              Refresh
            </Button>
            <Button variant="outline" size="sm" onClick={() => copy(JSON.stringify(diagnostics, null, 2))}>
              <Copy />
              Copy
            </Button>
            {canExportFile && (
              <Button
                size="sm"
                disabled={busy !== null}
                onClick={() =>
                  void onExport()
                    .then((path) => setNotice(`Diagnostics: ${path}`))
                    .catch((cause: unknown) =>
                      setNotice(cause instanceof Error ? cause.message : 'Unable to export diagnostics.'),
                    )
                }
              >
                <Download />
                Export
              </Button>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger render={<Button variant="ghost" size="icon-sm" aria-label="Log actions" />}>
                <MoreHorizontal />
              </DropdownMenuTrigger>
              <DropdownMenuContent>
                <DropdownMenuGroup>
                  <DropdownMenuItem
                    variant="destructive"
                    disabled={busy !== null || !diagnostics.logs.length}
                    onClick={() => setClear(true)}
                  >
                    <Trash2 />
                    Clear logs…
                  </DropdownMenuItem>
                </DropdownMenuGroup>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        }
      />
      {notice && (
        <p role="status" className="border-border text-muted-foreground shrink-0 border-b px-6 py-2 text-xs break-all">
          {notice}
        </p>
      )}
      <details className="border-border shrink-0 border-b">
        <summary className="text-muted-foreground cursor-pointer px-6 py-3 text-xs">
          System & discovery · {diagnostics.databaseHealthy ? 'Database healthy' : 'Database needs attention'} ·{' '}
          {diagnostics.discoveryProviders.filter((p) => p.available).length} providers available
        </summary>
        <div className="grid max-h-64 gap-6 overflow-auto px-6 pb-4 md:grid-cols-2">
          <section>
            <h2 className="mb-3 text-sm font-semibold">System</h2>
            <dl className="grid grid-cols-2 gap-3">
              <KeyValue label="Version" value={diagnostics.agentVersion} />
              <KeyValue label="Platform" value={diagnostics.platform} />
              <KeyValue label="Product" value={diagnostics.productId} />
              <KeyValue label="Connection" value={titleCase(diagnostics.connectionState)} />
            </dl>
          </section>
          <section>
            <h2 className="mb-3 text-sm font-semibold">Discovery providers</h2>
            {diagnostics.discoveryProviders.map((provider) => (
              <div key={provider.name} className="mb-3 flex items-start gap-2">
                <span className="mt-1.5">
                  <StatusDot tone={provider.available ? 'connected' : 'warning'} />
                </span>
                <div>
                  <p className="text-sm">{provider.name}</p>
                  <p className="text-muted-foreground mt-1 text-xs">
                    {provider.detail} · {formatRelativeTime(provider.lastScanAt)}
                  </p>
                </div>
              </div>
            ))}
          </section>
        </div>
      </details>
      <Toolbar>
        <SearchField label="Search logs" value={query} onChange={setQuery} />
        <select aria-label="Log severity" value={severity} onChange={(e) => setSeverity(e.target.value)}>
          <option value="all">All severities</option>
          <option value="error">Error</option>
          <option value="warn">Warning</option>
          <option value="info">Info</option>
        </select>
        <span className="text-muted-foreground text-xs">{logs.length} entries</span>
      </Toolbar>
      <div className="min-h-0 flex-1 overflow-auto">
        {!logs.length ? (
          <EmptyState
            title="No matching logs"
            description={
              diagnostics.logs.length
                ? 'Adjust the search or severity filter.'
                : 'Operational events will appear here as the agent runs.'
            }
          />
        ) : (
          logs.map((entry, index) => (
            <details className="log-entry" key={`${entry.timestamp}-${entry.target}-${index}`}>
              <summary>
                <time
                  className="text-muted-foreground text-xs tabular-nums"
                  title={new Date(entry.timestamp).toLocaleString()}
                >
                  {new Date(entry.timestamp).toLocaleTimeString(undefined, {
                    hour: '2-digit',
                    minute: '2-digit',
                    second: '2-digit',
                  })}
                </time>
                <span
                  className={
                    entry.level === 'error'
                      ? 'text-error text-xs'
                      : entry.level === 'warn'
                        ? 'text-warning text-xs'
                        : 'text-muted-foreground text-xs'
                  }
                >
                  {entry.level === 'warn' ? 'Warning' : titleCase(entry.level)}
                </span>
                <span className="text-muted-foreground truncate font-mono text-xs">{entry.target}</span>
                <span className="truncate font-mono text-xs">{entry.message}</span>
              </summary>
              <div className="bg-card px-6 py-4">
                <p className="mb-3 font-mono text-xs leading-6 break-all whitespace-pre-wrap">{entry.message}</p>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => copy(`${entry.timestamp} [${entry.level}] ${entry.target}: ${entry.message}`)}
                >
                  <Copy />
                  Copy entry
                </Button>
              </div>
            </details>
          ))
        )}
      </div>
      <ConfirmAction
        open={clear}
        onOpenChange={setClear}
        title="Clear local logs?"
        description="The bounded local diagnostic history will be removed. Job delivery records are unaffected."
        busy={busy !== null}
        action={onClearLogs}
      />
    </ScreenContainer>
  );
}
