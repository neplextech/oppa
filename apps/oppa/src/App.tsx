import { LoaderCircle } from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';

import { AppShell, type ScreenId } from '@/components/app-shell';
import { ErrorBanner } from '@/components/ui';
import { useAgent } from '@/hooks/use-agent';
import { agentClient } from '@/lib/agent-client';
import { DiagnosticsScreen } from '@/screens/diagnostics';
import { JobsScreen } from '@/screens/jobs';
import { OverviewScreen } from '@/screens/overview';
import { PrintersScreen } from '@/screens/printers';
import { SettingsScreen } from '@/screens/settings';
import { SetupScreen } from '@/screens/setup';
import { VirtualPrintersScreen } from '@/screens/virtual-printers';

export type Theme = 'system' | 'light' | 'dark';

function applyThemeClass(t: Theme) {
  const isDark = t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', isDark);
}

// Apply saved theme before first render to prevent flash
const initialTheme = (localStorage.getItem('oppa-theme') as Theme | null) ?? 'system';
applyThemeClass(initialTheme);

const SCREEN_SHORTCUTS: Record<string, ScreenId> = {
  '1': 'overview',
  '2': 'jobs',
  '3': 'printers',
  '4': 'virtual',
  '5': 'diagnostics',
  ',': 'settings',
};

export default function App() {
  const [screen, setScreen] = useState<ScreenId>('overview');
  const [theme, setThemeState] = useState<Theme>(initialTheme);
  const { status, printers, jobs, diagnostics, loading, busy, error, actions } = useAgent();

  function setTheme(t: Theme) {
    localStorage.setItem('oppa-theme', t);
    setThemeState(t);
    applyThemeClass(t);
  }

  // Re-apply when system preference changes (while theme === 'system')
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => {
      if ((localStorage.getItem('oppa-theme') ?? 'system') === 'system') {
        applyThemeClass('system');
      }
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: () => void = () => undefined;
    void agentClient
      .subscribeNavigation(setScreen)
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      })
      .catch((cause: unknown) => {
        console.error('Unable to subscribe to tray navigation.', cause);
      });
    return () => {
      disposed = true;
      unsubscribe();
    };
  }, []);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (!event.metaKey && !event.ctrlKey) return;
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
      const target = SCREEN_SHORTCUTS[event.key];
      if (target) {
        event.preventDefault();
        setScreen(target);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  if (!loading && error && (!status || !diagnostics)) {
    return (
      <main className="bg-background flex min-h-dvh items-center justify-center p-6">
        <div className="w-full max-w-sm rounded-xl border border-[var(--color-error)]/20 bg-[var(--color-error)]/5 p-6">
          <img src="/oppa-icon.png" alt="" className="size-11 rounded-xl shadow-sm" draggable={false} />
          <h1 className="text-foreground mt-4 text-base font-semibold">Agent failed to start</h1>
          <p className="mt-2 text-sm leading-5 text-[var(--color-error)]/80">{error}</p>
          <button
            type="button"
            className="border-border bg-secondary text-foreground hover:bg-accent mt-5 inline-flex items-center gap-1.5 rounded-lg border px-3 py-2 text-sm font-medium transition-colors"
            onClick={() => void actions.reload()}
          >
            Retry startup
          </button>
        </div>
      </main>
    );
  }

  if (loading || !status || !diagnostics) {
    return (
      <main className="bg-background flex min-h-dvh items-center justify-center">
        <div className="text-muted-foreground flex items-center gap-3 text-sm">
          <img src="/oppa-icon.png" alt="" className="size-8 rounded-lg shadow-sm" draggable={false} />
          <LoaderCircle className="text-primary size-4 animate-spin" aria-hidden />
          Starting local agent…
        </div>
      </main>
    );
  }

  if (!status.configured || status.state === 'unconfigured' || status.state === 'authorizing') {
    return (
      <SetupScreen
        status={status}
        busy={busy}
        onAuthorize={async () => {
          await actions.authorize();
        }}
        onSaveServerConfiguration={actions.setServerConfiguration}
        onResetServerConfiguration={actions.resetServerConfiguration}
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
          theme={theme}
          onThemeChange={setTheme}
          onStartOnLogin={actions.setStartOnLogin}
          onReconnect={actions.reconnect}
          onSaveServerConfiguration={actions.setServerConfiguration}
          onResetServerConfiguration={actions.resetServerConfiguration}
          onOpenProductLink={actions.openProductLink}
        />
      );
      break;
  }

  return (
    <AppShell activeScreen={screen} onNavigate={setScreen} status={status} onReconnect={actions.reconnect}>
      {error ? <ErrorBanner message={error} onRetry={() => void actions.reload()} /> : null}
      {content}
    </AppShell>
  );
}
