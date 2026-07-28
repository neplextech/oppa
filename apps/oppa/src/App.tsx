import { AlertTriangle, LoaderCircle, RefreshCw } from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';

import { AppShell, type ScreenId } from '@/components/app-shell';
import { Button, Card } from '@/components/ui';
import { useAgent } from '@/hooks/use-agent';
import { agentClient } from '@/lib/agent-client';
import { DiagnosticsScreen } from '@/screens/diagnostics';
import { JobsScreen } from '@/screens/jobs';
import { OverviewScreen } from '@/screens/overview';
import { PrintersScreen } from '@/screens/printers';
import { SettingsScreen } from '@/screens/settings';
import { SetupScreen } from '@/screens/setup';
import { VirtualPrintersScreen } from '@/screens/virtual-printers';

export default function App() {
  const [screen, setScreen] = useState<ScreenId>('overview');
  const { status, printers, jobs, diagnostics, loading, busy, error, actions } = useAgent();

  useEffect(() => {
    let disposed = false;
    let unsubscribe: () => void = () => undefined;
    void agentClient
      .subscribeNavigation(setScreen)
      .then((stop) => {
        if (disposed) {
          stop();
        } else {
          unsubscribe = stop;
        }
      })
      .catch((cause: unknown) => {
        console.error('Unable to subscribe to tray navigation.', cause);
      });
    return () => {
      disposed = true;
      unsubscribe();
    };
  }, []);

  if (!loading && error && (!status || !diagnostics)) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-[#f7f8fa] p-6">
        <Card className="w-full max-w-md border-red-200 bg-red-50 p-6">
          <AlertTriangle className="size-5 text-red-600" aria-hidden />
          <h1 className="mt-4 text-lg font-bold text-red-950">The local agent could not start</h1>
          <p className="mt-2 text-sm leading-6 text-red-700">{error}</p>
          <Button className="mt-5" onClick={() => void actions.reload()}>
            <RefreshCw className="size-4" aria-hidden />
            Retry startup
          </Button>
        </Card>
      </main>
    );
  }

  if (loading || !status || !diagnostics) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-[#f7f8fa]">
        <div className="flex items-center gap-3 text-sm font-semibold text-slate-500">
          <LoaderCircle className="size-4 animate-spin text-blue-600" aria-hidden />
          Starting the local agent…
        </div>
      </main>
    );
  }

  if (!status.configured || status.state === 'unconfigured' || status.state === 'authorizing') {
    return (
      <SetupScreen
        status={status}
        busy={busy === 'authorize'}
        onAuthorize={async () => {
          await actions.authorize();
        }}
      />
    );
  }

  let content: ReactNode;

  switch (screen) {
    case 'overview':
      content = (
        <OverviewScreen
          status={status}
          printers={printers}
          jobs={jobs}
          busy={busy}
          onNavigatePrinters={() => setScreen('printers')}
          onNavigateJobs={() => setScreen('jobs')}
          onReconnect={actions.reconnect}
        />
      );
      break;
    case 'printers':
      content = (
        <PrintersScreen
          printers={printers}
          busy={busy}
          onRefresh={actions.refreshPrinters}
          onConfigure={actions.configurePrinter}
          onAddManual={actions.addManualPrinter}
          onRemove={actions.removePrinter}
          onTest={actions.sendTestPrint}
        />
      );
      break;
    case 'virtual':
      content = (
        <VirtualPrintersScreen
          printers={printers}
          busy={busy}
          onCreate={actions.createVirtualPrinter}
          onUpdate={actions.updateVirtualPrinter}
          onClear={actions.clearVirtualHistory}
          onTest={actions.sendTestPrint}
        />
      );
      break;
    case 'jobs':
      content = <JobsScreen jobs={jobs} />;
      break;
    case 'diagnostics':
      content = (
        <DiagnosticsScreen
          diagnostics={diagnostics}
          busy={busy}
          onReload={actions.reload}
          onExport={actions.exportDiagnostics}
        />
      );
      break;
    case 'settings':
      content = (
        <SettingsScreen
          status={status}
          busy={busy}
          onStartOnLogin={actions.setStartOnLogin}
          onReconnect={actions.reconnect}
          onOpenProductLink={actions.openProductLink}
        />
      );
      break;
  }

  return (
    <AppShell activeScreen={screen} onNavigate={setScreen} status={status}>
      {error ? (
        <Card className="mx-auto mb-5 flex max-w-6xl items-start gap-3 border-red-200 bg-red-50 p-4">
          <AlertTriangle className="mt-0.5 size-4 text-red-600" aria-hidden />
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold text-red-900">Operation failed</div>
            <div className="mt-1 text-xs text-red-700">{error}</div>
          </div>
          <Button variant="ghost" size="sm" onClick={() => void actions.reload()}>
            <RefreshCw className="size-3.5" aria-hidden />
            Retry
          </Button>
        </Card>
      ) : null}
      {content}
    </AppShell>
  );
}
