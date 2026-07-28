import { Activity, BriefcaseBusiness, FileClock, MonitorCog, Printer, ScrollText, Settings2 } from 'lucide-react';
import type { ReactNode } from 'react';

import type { AgentStatus } from '@/lib/types';
import { cn, titleCase } from '@/lib/utils';

import { StatusDot } from './ui';

export type ScreenId = 'overview' | 'printers' | 'virtual' | 'jobs' | 'diagnostics' | 'settings';

const navigation: Array<{
  id: ScreenId;
  label: string;
  icon: typeof Activity;
}> = [
  { id: 'overview', label: 'Overview', icon: Activity },
  { id: 'printers', label: 'Printers', icon: Printer },
  { id: 'virtual', label: 'Virtual Printers', icon: MonitorCog },
  { id: 'jobs', label: 'Jobs', icon: FileClock },
  { id: 'diagnostics', label: 'Diagnostics', icon: ScrollText },
  { id: 'settings', label: 'Settings', icon: Settings2 },
];

export function AppShell({
  activeScreen,
  onNavigate,
  status,
  children,
}: {
  activeScreen: ScreenId;
  onNavigate: (screen: ScreenId) => void;
  status: AgentStatus;
  children: ReactNode;
}) {
  const connected = status.state === 'connected';

  return (
    <div className="flex min-h-screen bg-[#f7f8fa] text-slate-950">
      <aside className="fixed inset-y-0 left-0 z-20 hidden w-60 flex-col border-r border-slate-200 bg-white lg:flex">
        <div className="flex h-20 items-center gap-3 border-b border-slate-100 px-5">
          <div className="flex size-9 items-center justify-center rounded-xl bg-slate-950 text-white shadow-sm">
            <Printer className="size-[18px]" strokeWidth={2.2} aria-hidden />
          </div>
          <div>
            <div className="text-sm font-bold tracking-[-0.01em]">{status.product.name}</div>
            <div className="text-[11px] font-medium text-slate-400">Printer agent</div>
          </div>
        </div>

        <nav className="flex-1 space-y-1 px-3 py-5" aria-label="Primary">
          {navigation.map((item) => {
            const Icon = item.icon;
            const active = activeScreen === item.id;
            return (
              <button
                key={item.id}
                className={cn(
                  'flex h-10 w-full items-center gap-3 rounded-lg px-3 text-left text-sm font-medium transition-colors',
                  active
                    ? 'bg-slate-950 text-white shadow-sm'
                    : 'text-slate-500 hover:bg-slate-100 hover:text-slate-950',
                )}
                onClick={() => onNavigate(item.id)}
                type="button"
              >
                <Icon className="size-4" strokeWidth={2} aria-hidden />
                {item.label}
              </button>
            );
          })}
        </nav>

        <div className="p-3">
          <div className="rounded-xl border border-slate-200 bg-slate-50 p-3.5">
            <div className="flex items-center gap-2 text-xs font-semibold text-slate-700">
              <StatusDot tone={connected ? 'green' : 'amber'} pulse={connected} />
              {titleCase(status.state)}
            </div>
            <p className="mt-2 truncate text-[11px] text-slate-400">{status.agentId ?? 'Account not connected'}</p>
          </div>
          <div className="mt-3 flex items-center gap-2 px-2 text-[10px] font-medium text-slate-400">
            <BriefcaseBusiness className="size-3" aria-hidden />
            {status.product.id} · v{status.version}
          </div>
        </div>
      </aside>

      <div className="flex min-h-screen min-w-0 flex-1 flex-col lg:pl-60">
        <header className="sticky top-0 z-10 flex h-16 items-center justify-between border-b border-slate-200/80 bg-white/90 px-5 backdrop-blur lg:px-8">
          <div className="flex min-w-0 items-center gap-3 lg:hidden">
            <div className="flex shrink-0 items-center gap-2">
              <div className="flex size-8 items-center justify-center rounded-lg bg-slate-950 text-white">
                <Printer className="size-4" aria-hidden />
              </div>
              <span className="hidden text-sm font-bold sm:inline">{status.product.name}</span>
            </div>
            <label>
              <span className="sr-only">Current screen</span>
              <select
                aria-label="Current screen"
                className="max-w-44 rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-xs font-semibold text-slate-700 outline-none focus:border-blue-500 focus:ring-3 focus:ring-blue-500/10"
                value={activeScreen}
                onChange={(event) => onNavigate(event.target.value as ScreenId)}
              >
                {navigation.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="hidden text-xs font-medium text-slate-400 lg:block">Open Printer Proxy Agent</div>
          <div className="flex items-center gap-2 rounded-full border border-slate-200 bg-white px-3 py-1.5 text-xs font-semibold text-slate-600 shadow-sm">
            <StatusDot tone={connected ? 'green' : 'amber'} pulse={connected} />
            Gateway {status.gatewayState}
          </div>
        </header>

        <main className="flex-1 px-5 py-7 lg:px-8 lg:py-8">{children}</main>

        <nav
          className="sticky bottom-0 z-20 grid grid-cols-6 border-t border-slate-200 bg-white px-1 py-2 lg:hidden"
          aria-label="Primary"
        >
          {navigation.map((item) => {
            const Icon = item.icon;
            const active = item.id === activeScreen;
            return (
              <button
                key={item.id}
                className={cn(
                  'flex min-w-0 flex-col items-center gap-1 rounded-lg py-1.5 text-[9px] font-semibold',
                  active ? 'text-blue-600' : 'text-slate-400',
                )}
                onClick={() => onNavigate(item.id)}
                type="button"
              >
                <Icon className="size-4" aria-hidden />
                <span className="max-w-full truncate">{item.label.replace(' Printers', '')}</span>
              </button>
            );
          })}
        </nav>
      </div>
    </div>
  );
}
