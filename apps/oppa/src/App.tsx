import { LoaderCircle } from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

import { AppShell, type ScreenId } from '@/components/app-shell';
import { DeepLinkDialog } from '@/components/deep-link-dialog';
import { ErrorBanner } from '@/components/ui';
import { useAgent } from '@/hooks/use-agent';
import { agentClient } from '@/lib/agent-client';
import type { DeepLinkPairing } from '@/lib/types';
import { DiagnosticsScreen } from '@/screens/diagnostics';
import { JobsScreen } from '@/screens/jobs';
import { OverviewScreen } from '@/screens/overview';
import { PrintersScreen } from '@/screens/printers';
import { SettingsScreen } from '@/screens/settings';
import { SetupScreen } from '@/screens/setup';
import { VirtualPrintersScreen } from '@/screens/virtual-printers';

import { playPrinterSound } from './lib/printer-sound';

export type Theme = 'system' | 'light' | 'dark';

function applyThemeClass(t: Theme) {
  const isDark = t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  document.documentElement.classList.toggle('dark', isDark);
}

// Apply saved theme before first render to prevent flash
const initialTheme = (localStorage.getItem('oppa-theme') as Theme | null) ?? 'system';
applyThemeClass(initialTheme);

const VALID_SCREENS = new Set<ScreenId>(['overview', 'printers', 'virtual', 'jobs', 'diagnostics', 'settings']);

const SCREEN_SHORTCUTS: Record<string, ScreenId> = {
  '1': 'overview',
  '2': 'jobs',
  '3': 'printers',
  '4': 'virtual',
  '5': 'diagnostics',
  ',': 'settings',
};

export default function App() {
  const location = useLocation();
  const navigate = useNavigate();

  // Derive active screen from hash URL path (/#/printers → printers)
  const rawPath = location.pathname.slice(1) as ScreenId;
  const screen: ScreenId = VALID_SCREENS.has(rawPath) ? rawPath : 'overview';

  function setScreen(s: ScreenId) {
    if (s !== 'settings') setSettingsFromSetup(false);
    void navigate('/' + s);
  }

  const [theme, setThemeState] = useState<Theme>(initialTheme);
  const [developerMode, setDeveloperModeState] = useState(() => localStorage.getItem('oppa-developer-mode') === 'true');
  const [commandOpen, setCommandOpen] = useState(false);
  const [settingsFromSetup, setSettingsFromSetup] = useState(false);
  const [pendingDeepLink, setPendingDeepLink] = useState<DeepLinkPairing | null>(null);
  const { status, printers, jobs, diagnostics, discoveredService, loading, busy, error, actions } = useAgent();

  function setTheme(t: Theme) {
    localStorage.setItem('oppa-theme', t);
    setThemeState(t);
    applyThemeClass(t);
  }

  function setDeveloperMode(enabled: boolean) {
    localStorage.setItem('oppa-developer-mode', String(enabled));
    setDeveloperModeState(enabled);
  }

  // Redirect bare root to overview
  useEffect(() => {
    if (!location.pathname || location.pathname === '/') {
      void navigate('/overview', { replace: true });
    }
  }, [location.pathname, navigate]);

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
      .subscribeNavigation((s) => void navigate('/' + s))
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
  }, [navigate]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: () => void = () => undefined;
    void agentClient
      .subscribePrinterSound(() => {
        playPrinterSound();
      })
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      })
      .catch((cause: unknown) => {
        console.error('Unable to subscribe to printer sound events.', cause);
      });
    return () => {
      disposed = true;
      unsubscribe();
    };
  }, []);

  // Subscribe to deep-link pair requests and check for one that arrived at launch.
  useEffect(() => {
    let disposed = false;
    let unsubscribe: () => void = () => undefined;

    void agentClient.getPendingDeepLink().then((pending) => {
      if (!disposed && pending) setPendingDeepLink(pending);
    });

    void agentClient
      .subscribeDeepLink((data) => {
        if (!disposed) setPendingDeepLink(data);
      })
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      })
      .catch((cause: unknown) => {
        console.error('Unable to subscribe to deep-link events.', cause);
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
      if (event.key === 'k') {
        event.preventDefault();
        setCommandOpen((v) => !v);
        return;
      }
      if (event.key === 'r') {
        event.preventDefault();
        window.location.reload();
        return;
      }
      const target = SCREEN_SHORTCUTS[event.key];
      if (target) {
        event.preventDefault();
        setScreen(target);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handleDeepLinkConfirm(agentName: string): Promise<void> {
    if (!pendingDeepLink || !status) return;
    if (status.agentId !== null) await actions.forgetServer();
    await actions.setServerConfiguration({ serverUrl: pendingDeepLink.serverUrl });
    await actions.pairServer(pendingDeepLink.pairKey, agentName);
    setPendingDeepLink(null);
  }

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

  const isSetupState =
    status.connectionState === 'idle' ||
    status.connectionState === 'discovering' ||
    status.connectionState === 'discovery_failed' ||
    status.connectionState === 'unpaired' ||
    status.connectionState === 'pairing' ||
    status.connectionState === 'authentication_failed' ||
    status.connectionState === 'credential_revoked';

  // Defined once here; status is non-null past the loading guard above.
  const deepLinkDialog = (
    <DeepLinkDialog
      open={pendingDeepLink !== null}
      pairing={pendingDeepLink}
      status={status}
      busy={busy}
      onConfirm={handleDeepLinkConfirm}
      onCancel={() => setPendingDeepLink(null)}
    />
  );

  if (isSetupState && !settingsFromSetup) {
    return (
      <>
        <SetupScreen
          status={status}
          discoveredService={discoveredService}
          busy={busy}
          onDiscover={actions.discoverServer}
          onPair={actions.pairServer}
          onForget={actions.forgetServer}
          onSaveServerConfiguration={actions.setServerConfiguration}
          onResetServerConfiguration={actions.resetServerConfiguration}
          onReconnect={actions.reconnect}
          onOpenSettings={() => {
            setSettingsFromSetup(true);
            void navigate('/settings');
          }}
        />
        {deepLinkDialog}
      </>
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
          developerMode={developerMode}
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
          developerMode={developerMode}
          onCreate={actions.createVirtualPrinter}
          onUpdate={actions.updateVirtualPrinter}
          onClear={actions.clearVirtualHistory}
          onDelete={actions.removePrinter}
          onTest={actions.sendTestPrint}
          onSetSound={actions.setVirtualPrinterSound}
        />
      );
      break;
    case 'jobs':
      content = <JobsScreen jobs={jobs} busy={busy} onClear={actions.clearJobs} />;
      break;
    case 'diagnostics':
      content = (
        <DiagnosticsScreen
          diagnostics={diagnostics}
          busy={busy}
          onReload={actions.reload}
          onExport={actions.exportDiagnostics}
          onClearLogs={actions.clearLogs}
        />
      );
      break;
    case 'settings':
      content = (
        <SettingsScreen
          status={status}
          busy={busy}
          theme={theme}
          developerMode={developerMode}
          fromSetup={settingsFromSetup}
          onThemeChange={setTheme}
          onDeveloperModeChange={setDeveloperMode}
          onStartOnLogin={actions.setStartOnLogin}
          onReconnect={actions.reconnect}
          onSaveServerConfiguration={actions.setServerConfiguration}
          onResetServerConfiguration={actions.resetServerConfiguration}
          onForgetServer={actions.forgetServer}
          onOpenProductLink={actions.openProductLink}
          onBackToSetup={() => setSettingsFromSetup(false)}
        />
      );
      break;
  }

  return (
    <>
      <AppShell
        activeScreen={screen}
        onNavigate={setScreen}
        status={status}
        developerMode={developerMode}
        paired={!isSetupState}
        commandOpen={commandOpen}
        onOpenCommand={() => setCommandOpen(true)}
        onCloseCommand={() => setCommandOpen(false)}
        onReconnect={actions.reconnect}
      >
        {error ? <ErrorBanner message={error} onRetry={() => void actions.reload()} /> : null}
        {content}
      </AppShell>
      {deepLinkDialog}
    </>
  );
}
