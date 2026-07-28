import { Activity, FileClock, MonitorCog, Printer, RotateCcw, ScrollText, Settings2, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

import type { ScreenId } from '@/components/app-shell';
import { cn } from '@/lib/utils';

interface Command {
  id: string;
  label: string;
  description?: string;
  icon: typeof Activity;
  shortcut?: string;
  action: () => void;
  group: string;
}

export function CommandMenu({
  onClose,
  onNavigate,
  onReconnect,
}: {
  onClose: () => void;
  onNavigate: (screen: ScreenId) => void;
  onReconnect: () => Promise<void>;
}) {
  const [query, setQuery] = useState('');
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const allCommands: Command[] = [
    {
      id: 'nav-overview',
      label: 'Overview',
      description: 'Operational status and recent activity',
      icon: Activity,
      shortcut: '⌘1',
      group: 'Navigate',
      action: () => onNavigate('overview'),
    },
    {
      id: 'nav-jobs',
      label: 'Jobs',
      description: 'Print job history and status',
      icon: FileClock,
      shortcut: '⌘2',
      group: 'Navigate',
      action: () => onNavigate('jobs'),
    },
    {
      id: 'nav-printers',
      label: 'Printers',
      description: 'Manage connected printers',
      icon: Printer,
      shortcut: '⌘3',
      group: 'Navigate',
      action: () => onNavigate('printers'),
    },
    {
      id: 'nav-virtual',
      label: 'Virtual Printer',
      description: 'Test with a software target',
      icon: MonitorCog,
      shortcut: '⌘4',
      group: 'Navigate',
      action: () => onNavigate('virtual'),
    },
    {
      id: 'nav-logs',
      label: 'Logs',
      description: 'Operational diagnostics',
      icon: ScrollText,
      shortcut: '⌘5',
      group: 'Navigate',
      action: () => onNavigate('diagnostics'),
    },
    {
      id: 'nav-settings',
      label: 'Settings',
      description: 'Preferences and product info',
      icon: Settings2,
      shortcut: '⌘,',
      group: 'Navigate',
      action: () => onNavigate('settings'),
    },
    {
      id: 'action-reconnect',
      label: 'Reconnect',
      description: 'Re-establish gateway connection',
      icon: RotateCcw,
      group: 'Actions',
      action: () => {
        void onReconnect();
      },
    },
  ];

  const filtered = query.trim()
    ? allCommands.filter(
        (c) =>
          c.label.toLowerCase().includes(query.toLowerCase()) ||
          c.description?.toLowerCase().includes(query.toLowerCase()),
      )
    : allCommands;

  const groups = filtered.reduce<Record<string, Command[]>>((acc, cmd) => {
    (acc[cmd.group] ??= []).push(cmd);
    return acc;
  }, {});

  const flatFiltered = Object.values(groups).flat();

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        onClose();
        return;
      }
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, flatFiltered.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const cmd = flatFiltered[activeIndex];
        if (cmd) {
          cmd.action();
          onClose();
        }
      }
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [flatFiltered, activeIndex, onClose]);

  let globalIndex = 0;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-20" onClick={onClose}>
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/20 backdrop-blur-sm dark:bg-black/40" />

      {/* Panel */}
      <div
        className="bg-popover border-border relative w-full max-w-md overflow-hidden rounded-2xl border shadow-2xl"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Command menu"
        aria-modal
      >
        {/* Search input */}
        <div className="border-border flex items-center gap-3 border-b px-4 py-3">
          <svg
            className="text-muted-foreground size-4 shrink-0"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
            aria-hidden
          >
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.35-4.35" />
          </svg>
          <input
            ref={inputRef}
            className="text-foreground placeholder:text-muted-foreground flex-1 bg-transparent text-sm outline-none"
            placeholder="Search commands…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Search commands"
            aria-autocomplete="list"
          />
          <button
            type="button"
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground rounded-md p-0.5 transition-colors"
            aria-label="Close"
          >
            <X className="size-4" />
          </button>
        </div>

        {/* Results */}
        <div className="max-h-80 overflow-y-auto py-2" role="listbox">
          {flatFiltered.length === 0 ? (
            <p className="text-muted-foreground px-4 py-8 text-center text-sm">No results for "{query}"</p>
          ) : (
            Object.entries(groups).map(([group, cmds]) => (
              <div key={group}>
                <p className="text-muted-foreground px-4 pt-2 pb-1 text-xs font-medium tracking-wider uppercase">
                  {group}
                </p>
                {cmds.map((cmd) => {
                  const idx = globalIndex++;
                  const Icon = cmd.icon;
                  const isActive = idx === activeIndex;
                  return (
                    <button
                      key={cmd.id}
                      type="button"
                      role="option"
                      aria-selected={isActive}
                      className={cn(
                        'flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm transition-colors',
                        isActive ? 'bg-accent text-accent-foreground' : 'text-foreground',
                      )}
                      onMouseEnter={() => setActiveIndex(idx)}
                      onClick={() => {
                        cmd.action();
                        onClose();
                      }}
                    >
                      <div
                        className={cn(
                          'flex size-7 shrink-0 items-center justify-center rounded-md',
                          isActive ? 'bg-primary/10' : 'bg-secondary',
                        )}
                      >
                        <Icon
                          className={cn('size-3.5', isActive ? 'text-primary' : 'text-muted-foreground')}
                          strokeWidth={1.75}
                          aria-hidden
                        />
                      </div>
                      <div className="min-w-0 flex-1">
                        <p className="font-medium">{cmd.label}</p>
                        {cmd.description && <p className="text-muted-foreground truncate text-xs">{cmd.description}</p>}
                      </div>
                      {cmd.shortcut && (
                        <kbd className="text-muted-foreground shrink-0 font-mono text-xs">{cmd.shortcut}</kbd>
                      )}
                    </button>
                  );
                })}
              </div>
            ))
          )}
        </div>

        <div className="border-border text-muted-foreground flex items-center gap-4 border-t px-4 py-2 text-xs">
          <span>
            <kbd className="font-mono">↑↓</kbd> navigate
          </span>
          <span>
            <kbd className="font-mono">↵</kbd> select
          </span>
          <span>
            <kbd className="font-mono">Esc</kbd> close
          </span>
        </div>
      </div>
    </div>
  );
}
