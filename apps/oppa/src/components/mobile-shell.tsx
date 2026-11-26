import { FileClock, LayoutDashboard, MonitorCog, Printer, RefreshCw, ScrollText, Settings2 } from 'lucide-react';
import type { ReactNode } from 'react';

import type { ScreenId } from '@/components/app-shell';
import type { AgentStatus, OpenPrinterConnectionState } from '@/lib/types';
import { cn } from '@/lib/utils';

const TABS: Array<{ id: ScreenId; label: string; icon: typeof LayoutDashboard; devOnly?: boolean }> = [
  { id: 'overview', label: 'Overview', icon: LayoutDashboard },
  { id: 'printers', label: 'Printers', icon: Printer },
  { id: 'jobs', label: 'Jobs', icon: FileClock },
  { id: 'virtual', label: 'Virtual', icon: MonitorCog, devOnly: true },
  { id: 'diagnostics', label: 'Logs', icon: ScrollText },
  { id: 'settings', label: 'Settings', icon: Settings2 },
];

const SCREEN_TITLES: Record<ScreenId, string> = {
  overview: 'Overview',
  printers: 'Printers',
  jobs: 'Print jobs',
  virtual: 'Virtual printers',
  diagnostics: 'Logs',
  settings: 'Settings',
};

function connectionSummary(state: OpenPrinterConnectionState): { label: string; tone: 'ok' | 'busy' | 'warn' } {
  switch (state) {
    case 'connected':
      return { label: 'Connected', tone: 'ok' };
    case 'connecting':
    case 'authenticating':
    case 'paired':
    case 'discovering':
    case 'pairing':
      return { label: 'Connecting', tone: 'busy' };
    case 'authentication_failed':
    case 'credential_revoked':
      return { label: 'Needs pairing', tone: 'warn' };
    default:
      return { label: 'Offline', tone: 'warn' };
  }
}

/**
 * Phone and tablet layout. Phones use a bottom tab bar; wider screens keep a
 * navigation rail. There is no window chrome: the system owns the status bar
 * and navigation bar, and safe-area insets keep content clear of both.
 */
export function MobileShell({
  activeScreen,
  onNavigate,
  status,
  developerMode,
  paired,
  onReconnect,
  children,
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  status: AgentStatus;
  developerMode: boolean;
  paired: boolean;
  onReconnect: () => Promise<void>;
  children: ReactNode;
}) {
  const tabs = TABS.filter((tab) => !tab.devOnly || developerMode);
  const connection = connectionSummary(status.connectionState);
  return (
    <div className="mobile-shell bg-background text-foreground">
      <header className="mobile-header">
        <img src="/oppa-icon.png" alt="" className="size-7 rounded-lg" draggable={false} />
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-[15px] leading-5 font-semibold">{SCREEN_TITLES[activeScreen]}</h1>
          <p className="text-muted-foreground truncate text-xs leading-4">
            {status.connectedService?.name ?? status.product.name}
          </p>
        </div>
        <div id="mobile-header-actions" className="mobile-header-actions" />
        {paired && (
          <button
            type="button"
            className={cn('mobile-connection', `mobile-connection--${connection.tone}`)}
            onClick={() => void onReconnect().catch(() => undefined)}
            aria-label={`${connection.label}. Reconnect`}
          >
            <span className="mobile-connection__dot" aria-hidden />
            <span className="mobile-connection__label">{connection.label}</span>
            {connection.tone !== 'ok' && <RefreshCw className="size-3.5" aria-hidden />}
          </button>
        )}
      </header>

      <div className="mobile-body">
        {paired && (
          <nav className="mobile-rail" aria-label="Primary navigation">
            {tabs.map((tab) => (
              <NavItem key={tab.id} tab={tab} active={activeScreen === tab.id} onNavigate={onNavigate} />
            ))}
          </nav>
        )}
        <main className="mobile-main" id="main-content" tabIndex={-1}>
          {children}
        </main>
      </div>

      {paired && (
        <nav className="mobile-tabbar" aria-label="Primary navigation">
          {tabs.map((tab) => (
            <NavItem key={tab.id} tab={tab} active={activeScreen === tab.id} onNavigate={onNavigate} />
          ))}
        </nav>
      )}
    </div>
  );
}

function NavItem({
  tab,
  active,
  onNavigate,
}: {
  tab: (typeof TABS)[number];
  active: boolean;
  onNavigate: (screen: ScreenId) => void;
}) {
  const Icon = tab.icon;
  return (
    <button
      type="button"
      className="mobile-nav-item"
      aria-current={active ? 'page' : undefined}
      onClick={() => onNavigate(tab.id)}
    >
      <span className="mobile-nav-item__icon">
        <Icon aria-hidden />
      </span>
      <span className="mobile-nav-item__label">{tab.label}</span>
    </button>
  );
}
