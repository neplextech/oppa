import { Activity, Command, FileClock, Globe2, MonitorCog, Printer, ScrollText, Settings2 } from 'lucide-react';
import type { ReactNode } from 'react';

import { CommandMenu } from '@/components/command-menu';
import type { AgentStatus } from '@/lib/types';
import { cn, titleCase } from '@/lib/utils';

export type ScreenId = 'overview' | 'printers' | 'virtual' | 'jobs' | 'diagnostics' | 'settings';

const ALL_NAV: Array<{ id: ScreenId; label: string; icon: typeof Activity; shortcut: string; devOnly?: boolean }> = [
  { id: 'overview', label: 'Overview', icon: Activity, shortcut: '1' },
  { id: 'jobs', label: 'Jobs', icon: FileClock, shortcut: '2' },
  { id: 'printers', label: 'Printers', icon: Printer, shortcut: '3' },
  { id: 'virtual', label: 'Virtual Printer', icon: MonitorCog, shortcut: '4', devOnly: true },
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
}) {
  return (
    <div className="bg-background text-foreground flex h-dvh flex-col overflow-hidden">
      {/* Title bar */}
      <TitleBar status={status} />

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar navigation — hidden until server is paired */}
        {paired && (
          <Sidebar
            activeScreen={activeScreen}
            onNavigate={onNavigate}
            onOpenCommand={onOpenCommand}
            developerMode={developerMode}
          />
        )}

        {/* Main workspace */}
        <main className="bg-background flex-1 overflow-x-hidden overflow-y-auto" id="main-content" tabIndex={-1}>
          {children}
        </main>
      </div>

      {commandOpen && <CommandMenu onClose={onCloseCommand} onNavigate={onNavigate} onReconnect={onReconnect} />}
    </div>
  );
}

function TitleBar({ status }: { status: AgentStatus }) {
  const agentId = status.agentId;
  const connected = status.connectionState === 'connected';
  const tone =
    status.connectionState === 'connected'
      ? 'connected'
      : status.connectionState === 'authentication_failed' || status.connectionState === 'credential_revoked'
        ? 'error'
        : status.connectionState === 'connecting' ||
            status.connectionState === 'authenticating' ||
            status.connectionState === 'pairing' ||
            status.connectionState === 'discovering'
          ? 'pending'
          : 'offline';

  return (
    <header
      data-tauri-drag-region
      className="border-sidebar-border bg-sidebar flex h-11 shrink-0 items-center border-b select-none"
    >
      {/* Space reserved for macOS traffic lights — no drag region here */}
      <div className="w-[72px] shrink-0" />

      {/* Product identity */}
      <div data-tauri-drag-region className="flex items-center gap-2 px-2">
        <img src="/oppa-icon.png" alt="" className="pointer-events-none size-5 shrink-0 rounded-md" draggable={false} />
        <span data-tauri-drag-region className="text-foreground/90 pointer-events-none text-sm font-semibold">
          {status.product.name}
        </span>
      </div>

      <div data-tauri-drag-region className="bg-border pointer-events-none mx-3 h-3.5 w-px" />

      {/* Connection state — non-interactive, all pointer-events-none so drag passes through */}
      <div data-tauri-drag-region className="text-muted-foreground pointer-events-none flex items-center gap-2 text-sm">
        <span
          className={cn('status-dot', `status-dot--${tone}`)}
          aria-label={`State: ${titleCase(status.connectionState)}`}
        />
        <span className={cn(connected ? 'text-foreground/70' : 'text-muted-foreground')}>
          {titleCase(status.connectionState)}
        </span>
        {status.connectedService ? (
          <>
            <span className="text-border/60">·</span>
            <span
              className="text-foreground/70 flex min-w-0 items-center gap-1"
              title={`${status.connectedService.name} · ${status.connectedService.gatewayUrl}`}
            >
              <Globe2 className="size-3" aria-hidden />
              <span className="max-w-44 truncate">{status.connectedService.name}</span>
            </span>
          </>
        ) : null}
        {agentId && (
          <>
            <span className="text-border/60">·</span>
            <span className="font-mono text-xs">{agentId}</span>
          </>
        )}
      </div>

      {/* Primary drag target — wide empty space in the middle */}
      <div data-tauri-drag-region className="flex-1 self-stretch" />

      <div className="flex items-center gap-2 px-4">
        {import.meta.env.DEV && (
          <span className="border-primary/30 text-primary bg-primary/10 rounded px-1.5 py-0.5 font-mono text-xs font-medium tracking-wide">
            DEV
          </span>
        )}
        <span className="text-muted-foreground/50 pointer-events-none font-mono text-xs">v{status.version}</span>
      </div>
    </header>
  );
}

function Sidebar({
  activeScreen,
  onNavigate,
  onOpenCommand,
  developerMode,
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  onOpenCommand: () => void;
  developerMode: boolean;
}) {
  const visibleNav = ALL_NAV.filter((item) => !item.devOnly || developerMode);

  return (
    <nav
      className="border-sidebar-border bg-sidebar flex w-52 shrink-0 flex-col border-r select-none"
      aria-label="Primary navigation"
    >
      {/* Primary nav items */}
      <div className="flex-1 space-y-0.5 p-2 pt-3">
        {visibleNav.map((item) => {
          const Icon = item.icon;
          const active = activeScreen === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onNavigate(item.id)}
              aria-current={active ? 'page' : undefined}
              className={cn(
                'group relative flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                active
                  ? 'bg-primary/10 text-primary'
                  : 'text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
              )}
            >
              {active && <span className="bg-primary absolute top-1/2 left-0 h-5 w-0.5 -translate-y-1/2 rounded-r" />}
              <Icon
                className={cn(
                  'size-[18px] shrink-0',
                  active ? 'text-primary' : 'text-sidebar-foreground/50 group-hover:text-sidebar-accent-foreground',
                )}
                strokeWidth={active ? 2.25 : 1.75}
                aria-hidden
              />
              <span className="truncate">{item.label}</span>
              <kbd
                className={cn(
                  'ml-auto font-mono text-xs opacity-0 transition-opacity group-hover:opacity-100',
                  active ? 'opacity-40' : 'text-muted-foreground',
                )}
              >
                ⌘{item.shortcut}
              </kbd>
            </button>
          );
        })}
      </div>

      {/* Bottom section: command search + settings */}
      <div className="border-sidebar-border space-y-0.5 border-t p-2 pb-3">
        <button
          type="button"
          onClick={onOpenCommand}
          className="text-sidebar-foreground/60 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-sm transition-colors"
          aria-label="Open command menu (⌘K)"
        >
          <Command className="size-[18px] shrink-0" strokeWidth={1.75} aria-hidden />
          <span>Search & commands</span>
          <kbd className="text-muted-foreground ml-auto font-mono text-xs opacity-60">⌘K</kbd>
        </button>

        <button
          type="button"
          onClick={() => onNavigate('settings')}
          aria-current={activeScreen === 'settings' ? 'page' : undefined}
          className={cn(
            'group relative flex w-full items-center gap-3 rounded-md px-3 py-2.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
            activeScreen === 'settings'
              ? 'bg-primary/10 text-primary'
              : 'text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
          )}
        >
          {activeScreen === 'settings' && (
            <span className="bg-primary absolute top-1/2 left-0 h-5 w-0.5 -translate-y-1/2 rounded-r" />
          )}
          <Settings2
            className={cn(
              'size-[18px] shrink-0',
              activeScreen === 'settings'
                ? 'text-primary'
                : 'text-sidebar-foreground/50 group-hover:text-sidebar-accent-foreground',
            )}
            strokeWidth={activeScreen === 'settings' ? 2.25 : 1.75}
            aria-hidden
          />
          Settings
          <kbd className="text-muted-foreground ml-auto font-mono text-xs opacity-0 transition-opacity group-hover:opacity-100">
            ⌘,
          </kbd>
        </button>
      </div>
    </nav>
  );
}
