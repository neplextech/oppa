import { Clipboard, Database, Download, RefreshCw, ShieldCheck } from 'lucide-react';
import { useState } from 'react';

import { ScreenContainer, ScreenHeader, SectionHeader, StatusDot } from '@/components/ui';
import { Button } from '@/components/ui/button';
import type { Diagnostics } from '@/lib/types';
import { cn, formatRelativeTime, titleCase } from '@/lib/utils';

export function DiagnosticsScreen({
  diagnostics,
  busy,
  onReload,
  onExport,
}: {
  diagnostics: Diagnostics;
  busy: string | null;
  onReload: () => Promise<void>;
  onExport: () => Promise<string>;
}) {
  const [exportedPath, setExportedPath] = useState<string | null>(null);

  async function copyDiagnostics() {
    const text = [
      `Agent: v${diagnostics.agentVersion}`,
      `Product: ${diagnostics.productId}`,
      `Platform: ${diagnostics.platform}`,
      `Connection: ${diagnostics.connectionState}`,
      `Database: ${diagnostics.databaseHealthy ? 'healthy' : 'unhealthy'}`,
      `Migration: ${diagnostics.migrationVersion}`,
      '',
      ...diagnostics.logs.map((e) => `${e.timestamp} ${e.level.toUpperCase()} ${e.target}: ${e.message}`),
    ].join('\n');
    await navigator.clipboard.writeText(text);
  }

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Logs"
        description="Sanitized operational diagnostics. Credentials and full print payloads are never included."
        action={
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="sm" disabled={busy === 'reload'} onClick={() => void onReload()}>
              {busy === 'reload' ? (
                <RefreshCw className="size-3 animate-spin" aria-hidden />
              ) : (
                <RefreshCw className="size-3" aria-hidden />
              )}
              Refresh
            </Button>
            <Button variant="outline" size="sm" onClick={() => void copyDiagnostics()}>
              <Clipboard className="size-3" aria-hidden />
              Copy
            </Button>
            <Button
              size="sm"
              disabled={busy === 'export-diagnostics'}
              onClick={() => {
                void onExport().then((exportData) => {
                  setExportedPath(exportData);
                });
              }}
            >
              {busy === 'export-diagnostics' ? (
                <RefreshCw className="size-3 animate-spin" aria-hidden />
              ) : (
                <Download className="size-3" aria-hidden />
              )}
              Export bundle
            </Button>
          </div>
        }
      />

      {exportedPath && (
        <div className="flex shrink-0 items-center gap-2 border-b border-[var(--color-connected)]/20 bg-[var(--color-connected)]/5 px-5 py-2.5 text-[11px]">
          <span className="text-[var(--color-connected)]">Bundle exported:</span>
          <span className="text-muted-foreground font-mono select-all">{exportedPath}</span>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        {/* Left: system info + providers */}
        <div className="border-border bg-card/30 w-56 shrink-0 overflow-y-auto border-r xl:w-64">
          {/* System info */}
          <div className="border-border border-b px-4 py-3">
            <SectionHeader title="System" className="mb-3" />
            <div className="space-y-2 text-[11px]">
              <InfoRow label="Agent" value={`v${diagnostics.agentVersion}`} />
              <InfoRow label="Product" value={diagnostics.productId} />
              <InfoRow label="Platform" value={diagnostics.platform} />
              <InfoRow label="Connection" value={titleCase(diagnostics.connectionState)} />
              <InfoRow label="Migration" value={String(diagnostics.migrationVersion)} />
            </div>
          </div>

          {/* Database */}
          <div className="border-border border-b px-4 py-3">
            <div className="flex items-center gap-2 text-[11px]">
              <Database className="text-muted-foreground size-3.5 shrink-0" aria-hidden />
              <span className="text-muted-foreground font-medium">Database</span>
              <span
                className={cn(
                  'ml-auto font-mono',
                  diagnostics.databaseHealthy ? 'text-[var(--color-connected)]' : 'text-[var(--color-error)]',
                )}
              >
                {diagnostics.databaseHealthy ? 'healthy' : 'unhealthy'}
              </span>
            </div>
          </div>

          {/* Discovery providers */}
          <div className="px-4 py-3">
            <SectionHeader title="Discovery" className="mb-3" />
            <div className="space-y-2.5">
              {diagnostics.discoveryProviders.map((provider) => (
                <div key={provider.name} className="text-[11px]">
                  <div className="flex items-center gap-1.5">
                    <StatusDot tone={provider.available ? 'connected' : 'error'} />
                    <span className="text-foreground/80 font-medium">{provider.name}</span>
                  </div>
                  <p className="text-muted-foreground/60 mt-0.5 ml-4 text-[10px] leading-3.5">{provider.detail}</p>
                  <p className="text-muted-foreground/40 ml-4 text-[10px]">{formatRelativeTime(provider.lastScanAt)}</p>
                </div>
              ))}
            </div>
          </div>

          {/* Redaction notice */}
          <div className="border-border mt-auto border-t px-4 py-3">
            <div className="text-muted-foreground/60 flex items-start gap-1.5 text-[10px]">
              <ShieldCheck className="mt-0.5 size-3 shrink-0" aria-hidden />
              <p>Private keys, pairing codes, signatures, and document contents are excluded.</p>
            </div>
          </div>
        </div>

        {/* Right: log stream */}
        <div className="flex flex-1 flex-col overflow-hidden">
          <div className="border-border flex shrink-0 items-center justify-between border-b px-5 py-2">
            <SectionHeader title="Log stream" description="Bounded local operational history" />
            <span className="text-muted-foreground/50 font-mono text-[10px]">{diagnostics.logs.length} entries</span>
          </div>

          <div className="flex-1 overflow-y-auto">
            {diagnostics.logs.length === 0 ? (
              <div className="text-muted-foreground/50 flex items-center justify-center py-12 text-[11px]">
                No log entries
              </div>
            ) : (
              <table className="w-full font-mono text-[10px]">
                <thead className="bg-background/95 sticky top-0 backdrop-blur-sm">
                  <tr className="border-border border-b text-left">
                    <th className="text-muted-foreground/60 px-5 py-2 font-medium whitespace-nowrap">Time</th>
                    <th className="text-muted-foreground/60 px-3 py-2 font-medium">Level</th>
                    <th className="text-muted-foreground/60 px-3 py-2 font-medium">Target</th>
                    <th className="text-muted-foreground/60 px-3 py-2 font-medium">Message</th>
                  </tr>
                </thead>
                <tbody>
                  {diagnostics.logs.map((entry, index) => (
                    <tr
                      key={`${entry.timestamp}-${index}`}
                      className="border-border/30 hover:bg-accent/20 border-b transition-colors"
                    >
                      <td className="text-muted-foreground/50 px-5 py-1.5 whitespace-nowrap">
                        {new Intl.DateTimeFormat(undefined, {
                          hour: '2-digit',
                          minute: '2-digit',
                          second: '2-digit',
                        }).format(new Date(entry.timestamp))}
                      </td>
                      <td className="px-3 py-1.5">
                        <span
                          className={cn(
                            'uppercase tracking-wide',
                            entry.level === 'error'
                              ? 'text-[var(--color-error)]'
                              : entry.level === 'warn'
                                ? 'text-[var(--color-warning)]'
                                : 'text-muted-foreground/60',
                          )}
                        >
                          {entry.level}
                        </span>
                      </td>
                      <td className="text-muted-foreground/60 px-3 py-1.5 whitespace-nowrap">{entry.target}</td>
                      <td className="text-foreground/70 px-3 py-1.5 leading-4">{entry.message}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>
      </div>
    </ScreenContainer>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-muted-foreground/60 shrink-0">{label}</span>
      <span className="text-foreground/80 truncate text-right font-mono">{value}</span>
    </div>
  );
}
