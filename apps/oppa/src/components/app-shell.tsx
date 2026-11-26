import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  LayoutDashboard,
  Command,
  Copy,
  FileClock,
  PanelLeftClose,
  PanelLeftOpen,
  Server,
  Maximize2,
  Minus,
  MonitorCog,
  Printer,
  ScrollText,
  Settings2,
  X,
} from 'lucide-react';
import { useEffect, useState, type ReactNode } from 'react';

import { CommandMenu } from '@/components/command-menu';
import { MobileShell } from '@/components/mobile-shell';
import { Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '@/components/ui/tooltip';
import { WindowMenu } from '@/components/window-menu';
import type { AgentStatus } from '@/lib/types';
import { cn, titleCase } from '@/lib/utils';
import { isMacOSPlatform, isWindowsTauri } from '@/lib/window-platform';

export type ScreenId = 'overview' | 'printers' | 'virtual' | 'jobs' | 'diagnostics' | 'settings';

const ALL_NAV: Array<{
  id: ScreenId;
  label: string;
  icon: typeof LayoutDashboard;
  shortcut: string;
  devOnly?: boolean;
}> = [
  { id: 'overview', label: 'Overview', icon: LayoutDashboard, shortcut: '1' },
  { id: 'printers', label: 'Printers', icon: Printer, shortcut: '3' },
  { id: 'jobs', label: 'Print jobs', icon: FileClock, shortcut: '2' },
  { id: 'virtual', label: 'Virtual printers', icon: MonitorCog, shortcut: '4', devOnly: true },
  { id: 'diagnostics', label: 'Logs', icon: ScrollText, shortcut: '5' },
];

export function AppShell({
  activeScreen,
  onNavigate,
  status,
  developerMode,
  paired,
  commandOpen,
  onOpenCommand,
  onCloseCommand,
  onReconnect,
  children,
  contextualActions = [],
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  status: AgentStatus;
  developerMode: boolean;
  paired: boolean;
  commandOpen: boolean;
  onOpenCommand: () => void;
  onCloseCommand: () => void;
  onReconnect: () => Promise<void>;
  children: ReactNode;
  contextualActions?: Array<{ id: string; label: string; action: () => void }>;
}) {
  if (status.capabilities.mobile) {
    return (
      <MobileShell
        activeScreen={activeScreen}
        onNavigate={onNavigate}
        status={status}
        developerMode={developerMode}
        paired={paired}
        onReconnect={onReconnect}
      >
        {children}
      </MobileShell>
    );
  }
  return (
    <div className="bg-background text-foreground flex h-dvh flex-col overflow-hidden">
      {/* Title bar */}
      <TitleBar
        minimal
        status={status}
        paired={paired}
        developerMode={developerMode}
        onNavigate={onNavigate}
        onOpenCommand={onOpenCommand}
        onReconnect={onReconnect}
      />

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar navigation — hidden until server is paired */}
        {paired && (
          <Sidebar
            status={status}
            activeScreen={activeScreen}
            onNavigate={onNavigate}
            onOpenCommand={onOpenCommand}
            developerMode={developerMode}
          />
        )}

        {/* Main workspace */}
        <main className="bg-background min-w-0 flex-1 overflow-hidden" id="main-content" tabIndex={-1}>
          {children}
        </main>
      </div>

      <footer className="border-border text-muted-foreground flex h-7 shrink-0 items-center justify-between gap-4 border-t px-3 text-xs">
        <span>
          {status.activeErrors.length
            ? `${status.activeErrors.length} issue${status.activeErrors.length === 1 ? '' : 's'} need attention`
            : 'Agent running'}
        </span>
        <span className="tabular-nums">
          {status.pendingJobs} pending job{status.pendingJobs === 1 ? '' : 's'}
        </span>
      </footer>
      {commandOpen && (
        <CommandMenu
          developerMode={developerMode}
          contextualActions={contextualActions}
          onClose={onCloseCommand}
          onNavigate={onNavigate}
          onReconnect={onReconnect}
        />
      )}
    </div>
  );
}

export function TitleBar({
  minimal = false,
  status,
  paired,
  developerMode,
  onNavigate,
  onOpenCommand,
  onReconnect,
}: {
  minimal?: boolean;
  status: AgentStatus;
  paired: boolean;
  developerMode: boolean;
  onNavigate?: (screen: ScreenId) => void;
  onOpenCommand?: () => void;
  onReconnect?: () => Promise<void>;
}) {
  const macOS = isMacOSPlatform();
  const windows = isWindowsTauri();

  return (
    <header
      data-tauri-drag-region
      className={cn(
        'border-sidebar-border relative flex shrink-0 items-center border-b select-none',
        minimal ? 'bg-background' : 'bg-sidebar',
        minimal && !windows ? 'h-10' : 'h-11',
        windows && 'oppa-titlebar--windows',
      )}
    >
      {/* Tauri places the macOS traffic lights over the title bar. */}
      {macOS && <div className="w-[72px] shrink-0" />}

      {minimal ? (
        <span
          data-tauri-drag-region
          className="text-muted-foreground pointer-events-none absolute left-1/2 -translate-x-1/2 text-sm"
        >
          {status.product.name}
        </span>
      ) : (
        <div data-tauri-drag-region className="flex items-center gap-2 px-2">
          <img src="/oppa-icon.png" alt="" className="size-5" />
          <span>{status.product.name}</span>
        </div>
      )}

      {!macOS && (
        <WindowMenu
          paired={paired}
          developerMode={developerMode}
          onNavigate={onNavigate}
          onOpenCommand={onOpenCommand}
          onReconnect={onReconnect}
        />
      )}

      {/* Primary drag target — wide empty space in the middle */}
      <div data-tauri-drag-region className="flex-1 self-stretch" />

      {!macOS && !windows && <WindowControls />}
    </header>
  );
}

function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const tauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  useEffect(() => {
    if (!tauri) return;

    const appWindow = getCurrentWindow();
    let disposed = false;

    const syncMaximized = () => {
      void appWindow
        .isMaximized()
        .then((value) => {
          if (!disposed) setMaximized(value);
        })
        .catch(() => undefined);
    };

    syncMaximized();
    const unlisten = appWindow.onResized(syncMaximized);

    return () => {
      disposed = true;
      void unlisten.then((stopListening) => stopListening());
    };
  }, [tauri]);

  if (!tauri) return null;

  const runWindowAction = (action: () => Promise<void>) => {
    void action().catch(() => undefined);
  };

  return (
    <div className="flex h-full shrink-0 items-stretch" role="group" aria-label="Window controls">
      <button
        type="button"
        data-no-drag
        onClick={() => runWindowAction(() => getCurrentWindow().minimize())}
        className="text-muted-foreground hover:bg-foreground/10 hover:text-foreground focus-visible:ring-ring flex h-full w-11 items-center justify-center transition-colors focus-visible:z-10 focus-visible:ring-2 focus-visible:outline-none"
        aria-label="Minimize window"
        title="Minimize"
      >
        <Minus className="size-4" strokeWidth={1.75} aria-hidden />
      </button>
      <button
        type="button"
        data-no-drag
        onClick={() =>
          runWindowAction(async () => {
            await getCurrentWindow().toggleMaximize();
            setMaximized(await getCurrentWindow().isMaximized());
          })
        }
        className="text-muted-foreground hover:bg-foreground/10 hover:text-foreground focus-visible:ring-ring flex h-full w-11 items-center justify-center transition-colors focus-visible:z-10 focus-visible:ring-2 focus-visible:outline-none"
        aria-label={maximized ? 'Restore window' : 'Maximize window'}
        title={maximized ? 'Restore' : 'Maximize'}
      >
        {maximized ? (
          <Copy className="size-3.5" strokeWidth={1.75} aria-hidden />
        ) : (
          <Maximize2 className="size-3.5" strokeWidth={1.75} aria-hidden />
        )}
      </button>
      <button
        type="button"
        data-no-drag
        onClick={() => runWindowAction(() => getCurrentWindow().close())}
        className="text-muted-foreground hover:bg-destructive hover:text-destructive-foreground focus-visible:ring-ring flex h-full w-11 items-center justify-center transition-colors focus-visible:z-10 focus-visible:ring-2 focus-visible:outline-none"
        aria-label="Close window"
        title="Close"
      >
        <X className="size-4" strokeWidth={1.75} aria-hidden />
      </button>
    </div>
  );
}

function Sidebar({
  activeScreen,
  onNavigate,
  onOpenCommand,
  developerMode,
  status,
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  onOpenCommand: () => void;
  developerMode: boolean;
  status: AgentStatus;
}) {
  const [collapsed, setCollapsed] = useState(() => localStorage.getItem('oppa-sidebar-collapsed') === 'true');
  const modifier = isMacOSPlatform() ? '⌘' : 'Ctrl+';
  const items = ALL_NAV.filter((item) => !item.devOnly || developerMode);
  function navButton(
    label: string,
    icon: typeof LayoutDashboard,
    action: () => void,
    shortcut?: string,
    active = false,
  ) {
    const Icon = icon;
    return (
      <Tooltip key={label}>
        <TooltipTrigger
          render={
            <button
              type="button"
              className="sidebar-button"
              aria-label={label}
              aria-current={active ? 'page' : undefined}
              onClick={action}
            />
          }
        >
          <Icon aria-hidden />
          <span className="sidebar-label truncate">{label}</span>
          {shortcut && (
            <kbd className="ml-auto text-xs opacity-60">
              {modifier}
              {shortcut}
            </kbd>
          )}
        </TooltipTrigger>
        <TooltipContent side="right">
          {label}
          {shortcut ? ` · ${modifier}${shortcut}` : ''}
        </TooltipContent>
      </Tooltip>
    );
  }
  return (
    <TooltipProvider delay={450}>
      <nav className={cn('sidebar', collapsed && 'is-collapsed')} aria-label="Primary navigation">
        <div className="mb-4 flex items-center gap-2 px-2 py-2">
          <span className="sidebar-service text-primary bg-primary/10 flex size-7 shrink-0 items-center justify-center rounded-md">
            <Server className="size-4" />
          </span>
          <div className="sidebar-service min-w-0 flex-1">
            <p
              className="truncate text-sm font-medium"
              title={status.connectedService?.name ?? status.serverConfiguration.serverUrl}
            >
              {status.connectedService?.name ?? 'OpenPrinter server'}
            </p>
            <p className="text-muted-foreground mt-1 flex items-center gap-1.5 text-xs">
              <span
                className={cn(
                  'status-dot',
                  status.connectionState === 'connected' ? 'status-dot--connected' : 'status-dot--warning',
                )}
              />
              {titleCase(status.connectionState)}
            </p>
          </div>
          <button
            className="text-muted-foreground shrink-0 rounded p-1"
            aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            aria-expanded={!collapsed}
            title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            onClick={() => {
              localStorage.setItem('oppa-sidebar-collapsed', String(!collapsed));
              setCollapsed(!collapsed);
            }}
          >
            {collapsed ? <PanelLeftOpen className="size-4" /> : <PanelLeftClose className="size-4" />}
          </button>
        </div>

        <div className="flex flex-1 flex-col gap-1">
          {items
            .filter((item) => !item.devOnly)
            .map((item) =>
              navButton(item.label, item.icon, () => onNavigate(item.id), item.shortcut, activeScreen === item.id),
            )}
          {developerMode && (
            <>
              <p className="sidebar-label text-muted-foreground mt-4 mb-2 px-2 text-xs">Developer tools</p>
              {items
                .filter((item) => item.devOnly)
                .map((item) =>
                  navButton(item.label, item.icon, () => onNavigate(item.id), item.shortcut, activeScreen === item.id),
                )}
            </>
          )}
        </div>
        <div className="border-border flex flex-col gap-1 border-t pt-2">
          {navButton('Quick actions', Command, onOpenCommand, 'K')}
          {navButton('Settings', Settings2, () => onNavigate('settings'), ',', activeScreen === 'settings')}
        </div>
      </nav>
    </TooltipProvider>
  );
}
