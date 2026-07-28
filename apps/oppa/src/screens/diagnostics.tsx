import { CheckCircle2, Clipboard, Database, Download, RadioTower, RefreshCw } from 'lucide-react';
import { useState } from 'react';

import { PageHeader, SectionHeading } from '@/components/page';
import { Badge, Button, Card, StatusDot } from '@/components/ui';
import type { Diagnostics } from '@/lib/types';
import { formatRelativeTime, titleCase } from '@/lib/utils';

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
      `Agent version: ${diagnostics.agentVersion}`,
      `Product: ${diagnostics.productId}`,
      `Platform: ${diagnostics.platform}`,
      `Connection: ${diagnostics.connectionState}`,
      `Database: ${diagnostics.databaseHealthy ? 'healthy' : 'unhealthy'}`,
      `Migration: ${diagnostics.migrationVersion}`,
      '',
      ...diagnostics.logs.map(
        (entry) => `${entry.timestamp} ${entry.level.toUpperCase()} ${entry.target}: ${entry.message}`,
      ),
    ].join('\n');
    await navigator.clipboard.writeText(text);
  }

  async function exportDiagnostics() {
    setExportedPath(await onExport());
  }

  return (
    <div className="mx-auto max-w-6xl">
      <PageHeader
        eyebrow="Local health"
        title="Diagnostics"
        description="Sanitized operational information for troubleshooting. Credentials and full print payloads are never included."
        action={
          <div className="flex gap-2">
            <Button variant="secondary" onClick={() => void copyDiagnostics()}>
              <Clipboard className="size-4" aria-hidden />
              Copy
            </Button>
            <Button busy={busy === 'export-diagnostics'} onClick={() => void exportDiagnostics()}>
              <Download className="size-4" aria-hidden />
              Export bundle
            </Button>
          </div>
        }
      />

      {exportedPath ? (
        <Card className="mb-5 border-emerald-200 bg-emerald-50 px-4 py-3 text-xs text-emerald-800">
          <span className="font-semibold">Sanitized bundle exported:</span>{' '}
          <span className="font-mono select-all">{exportedPath}</span>
        </Card>
      ) : null}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {[
          ['Agent', `v${diagnostics.agentVersion}`, diagnostics.productId],
          ['Platform', diagnostics.platform, 'Local desktop'],
          [
            'Database',
            diagnostics.databaseHealthy ? 'Healthy' : 'Needs attention',
            `Migration ${diagnostics.migrationVersion}`,
          ],
          ['Connection', titleCase(diagnostics.connectionState), 'Gateway state'],
        ].map(([label, value, detail]) => (
          <Card key={label} className="p-5">
            <div className="text-[10px] font-bold tracking-[0.14em] text-slate-400 uppercase">{label}</div>
            <div className="mt-2 truncate text-base font-bold text-slate-900">{value}</div>
            <div className="mt-1 text-[11px] text-slate-400">{detail}</div>
          </Card>
        ))}
      </div>

      <div className="mt-6 grid gap-6 xl:grid-cols-[0.75fr_1.25fr]">
        <Card className="p-5">
          <SectionHeading
            title="Discovery providers"
            description="Most recent scan status"
            action={
              <Button
                variant="ghost"
                size="icon"
                busy={busy === 'reload'}
                onClick={() => void onReload()}
                title="Refresh diagnostics"
              >
                <RefreshCw className="size-4" aria-hidden />
              </Button>
            }
          />
          <div className="space-y-2">
            {diagnostics.discoveryProviders.map((provider) => (
              <div key={provider.name} className="flex items-start gap-3 rounded-lg border border-slate-100 p-3">
                <div className="mt-1">
                  <StatusDot tone={provider.available ? 'green' : 'red'} />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="text-xs font-semibold text-slate-700">{provider.name}</div>
                  <div className="mt-1 text-[11px] text-slate-400">{provider.detail}</div>
                </div>
                <span className="text-[10px] text-slate-400">{formatRelativeTime(provider.lastScanAt)}</span>
              </div>
            ))}
          </div>
          <div
            className={`mt-4 flex items-center gap-2 rounded-lg p-3 text-xs ${
              diagnostics.databaseHealthy ? 'bg-emerald-50 text-emerald-700' : 'bg-red-50 text-red-700'
            }`}
          >
            <Database className="size-4" aria-hidden />
            <span className="font-semibold">
              {diagnostics.databaseHealthy
                ? 'Local state passed its health check.'
                : 'Local state needs attention; review the logs below.'}
            </span>
          </div>
        </Card>

        <Card className="overflow-hidden">
          <div className="flex items-center justify-between border-b border-slate-100 p-5">
            <div>
              <h2 className="text-sm font-bold text-slate-900">Recent sanitized logs</h2>
              <p className="mt-1 text-xs text-slate-500">Bounded local operational history</p>
            </div>
            <Badge tone="success">
              <CheckCircle2 className="size-3" aria-hidden />
              Redaction active
            </Badge>
          </div>
          <div className="divide-y divide-slate-100 font-mono">
            {diagnostics.logs.map((entry, index) => (
              <div
                key={`${entry.timestamp}-${index}`}
                className="grid gap-2 px-5 py-3 text-[11px] sm:grid-cols-[72px_80px_1fr]"
              >
                <span className="text-slate-400">
                  {new Intl.DateTimeFormat(undefined, {
                    hour: '2-digit',
                    minute: '2-digit',
                    second: '2-digit',
                  }).format(new Date(entry.timestamp))}
                </span>
                <span
                  className={
                    entry.level === 'error'
                      ? 'text-red-600'
                      : entry.level === 'warn'
                        ? 'text-amber-600'
                        : 'text-blue-600'
                  }
                >
                  {entry.level.toUpperCase()} {entry.target}
                </span>
                <span className="leading-5 text-slate-600">{entry.message}</span>
              </div>
            ))}
          </div>
          <div className="flex items-center gap-2 border-t border-slate-100 bg-slate-50 px-5 py-3 text-[10px] text-slate-400">
            <RadioTower className="size-3" aria-hidden />
            Tokens, credentials, and complete document contents are excluded.
          </div>
        </Card>
      </div>
    </div>
  );
}
