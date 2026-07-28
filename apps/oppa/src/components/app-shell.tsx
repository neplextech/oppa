import { Activity, Command, FileClock, MonitorCog, Printer, ScrollText, Settings2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { useState } from 'react';

import { CommandMenu } from '@/components/command-menu';
import type { AgentStatus } from '@/lib/types';
import { cn, titleCase } from '@/lib/utils';

export type ScreenId = 'overview' | 'printers' | 'virtual' | 'jobs' | 'diagnostics' | 'settings';

const primaryNav: Array<{ id: ScreenId; label: string; icon: typeof Activity; shortcut: string }> = [
  { id: 'overview', label: 'Overview', icon: Activity, shortcut: '1' },
  { id: 'jobs', label: 'Jobs', icon: FileClock, shortcut: '2' },
  { id: 'printers', label: 'Printers', icon: Printer, shortcut: '3' },
  { id: 'virtual', label: 'Virtual Printer', icon: MonitorCog, shortcut: '4' },
  { id: 'diagnostics', label: 'Logs', icon: ScrollText, shortcut: '5' },
];

export function AppShell({
  activeScreen,
  onNavigate,
  status,
  onReconnect,
  children,
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  status: AgentStatus;
  onReconnect: () => Promise<void>;
  children: ReactNode;
}) {
  const [commandOpen, setCommandOpen] = useState(false);

  function openCommand() {
    setCommandOpen(true);
  }

  return (
    <div className="bg-background text-foreground flex h-dvh flex-col overflow-hidden">
      {/* Title bar */}
      <TitleBar status={status} />

      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar navigation */}
        <Sidebar activeScreen={activeScreen} onNavigate={onNavigate} onOpenCommand={openCommand} />

        {/* Main workspace */}
        <main className="bg-background flex-1 overflow-x-hidden overflow-y-auto" id="main-content" tabIndex={-1}>
          {children}
        </main>
      </div>

      {commandOpen && (
        <CommandMenu onClose={() => setCommandOpen(false)} onNavigate={onNavigate} onReconnect={onReconnect} />
      )}
    </div>
  );
}

function TitleBar({ status }: { status: AgentStatus }) {
  const agentId = status.agentId;
  const connected = status.state === 'connected';
  const tone =
    status.state === 'connected'
      ? 'connected'
      : status.state === 'degraded'
        ? 'warning'
        : status.state === 'disconnected' || status.state === 'shutting_down'
          ? 'error'
          : status.state === 'connecting' || status.state === 'authorizing'
            ? 'pending'
            : 'offline';

  return (
    <header
      data-tauri-drag-region
      className="border-sidebar-border bg-sidebar flex h-10 shrink-0 items-center border-b select-none"
    >
      {/* macOS traffic-light space */}
      <div className="w-[72px] shrink-0" data-no-drag />

      {/* Product identity */}
      <div className="flex items-center gap-2 px-2">
        <div className="bg-primary flex size-5 shrink-0 items-center justify-center rounded-md">
          <Printer className="text-primary-foreground size-3" strokeWidth={2.5} aria-hidden />
        </div>
        <span className="text-foreground/90 text-sm font-semibold">{status.product.name}</span>
      </div>

      <div className="bg-border mx-3 h-3.5 w-px" />

      {/* Connection state */}
      <div className="text-muted-foreground flex items-center gap-2 text-xs" data-no-drag>
        <span className={cn('status-dot', `status-dot--${tone}`)} aria-label={`State: ${titleCase(status.state)}`} />
        <span className={cn(connected ? 'text-foreground/70' : 'text-muted-foreground')}>
          {titleCase(status.state)}
        </span>
        {agentId && (
          <>
            <span className="text-border/60">·</span>
            <span className="font-mono text-[11px]">{agentId}</span>
          </>
        )}
      </div>

      <div className="flex-1" />

      <div className="text-muted-foreground/50 px-4 font-mono text-[11px]" data-no-drag>
        v{status.version}
      </div>
    </header>
  );
}

function Sidebar({
  activeScreen,
  onNavigate,
  onOpenCommand,
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  onOpenCommand: () => void;
}) {
  return (
    <nav
      className="border-sidebar-border bg-sidebar flex w-52 shrink-0 flex-col border-r"
      aria-label="Primary navigation"
    >
      {/* Primary nav items */}
      <div className="flex-1 space-y-0.5 p-2 pt-3">
        {primaryNav.map((item) => {
          const Icon = item.icon;
          const active = activeScreen === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onNavigate(item.id)}
              aria-current={active ? 'page' : undefined}
              className={cn(
                'group relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
                active
                  ? 'bg-primary/10 text-primary'
                  : 'text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground',
              )}
            >
              {active && <span className="bg-primary absolute top-1/2 left-0 h-5 w-0.5 -translate-y-1/2 rounded-r" />}
              <Icon
                className={cn(
                  'size-4 shrink-0',
                  active ? 'text-primary' : 'text-sidebar-foreground/50 group-hover:text-sidebar-accent-foreground',
                )}
                strokeWidth={active ? 2.25 : 1.75}
                aria-hidden
              />
              <span className="truncate">{item.label}</span>
              <kbd
                className={cn(
                  'ml-auto font-mono text-[10px] opacity-0 transition-opacity group-hover:opacity-100',
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
          className="text-sidebar-foreground/60 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors"
          aria-label="Open command menu (⌘K)"
        >
          <Command className="size-4 shrink-0" strokeWidth={1.75} aria-hidden />
          <span>Search & commands</span>
          <kbd className="text-muted-foreground ml-auto font-mono text-[10px] opacity-60">⌘K</kbd>
        </button>

        <button
          type="button"
          onClick={() => onNavigate('settings')}
          aria-current={activeScreen === 'settings' ? 'page' : undefined}
          className={cn(
            'group relative flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
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
              'size-4 shrink-0',
              activeScreen === 'settings'
                ? 'text-primary'
                : 'text-sidebar-foreground/50 group-hover:text-sidebar-accent-foreground',
            )}
            strokeWidth={activeScreen === 'settings' ? 2.25 : 1.75}
            aria-hidden
          />
          Settings
          <kbd className="text-muted-foreground ml-auto font-mono text-[10px] opacity-0 transition-opacity group-hover:opacity-100">
            ⌘,
          </kbd>
        </button>
      </div>
    </nav>
  );
}
