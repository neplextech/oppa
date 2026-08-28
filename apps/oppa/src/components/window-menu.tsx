import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  AlignCenter,
  ClipboardPaste,
  Command,
  Copy,
  FilePlus2,
  Fullscreen,
  ListChecks,
  Maximize2,
  Minimize2,
  RefreshCw,
  Redo2,
  Scissors,
  Settings2,
  Undo2,
  X,
} from 'lucide-react';
import { useEffect, useState } from 'react';

import type { ScreenId } from '@/components/app-shell';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { isTauriEnvironment } from '@/lib/window-platform';

type WindowMenuProps = {
  paired: boolean;
  developerMode: boolean;
  onNavigate?: (screen: ScreenId) => void;
  onOpenCommand?: () => void;
  onReconnect?: () => Promise<void>;
};

const triggerClassName =
  'text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-popup-open:bg-sidebar-accent data-popup-open:text-sidebar-accent-foreground h-7 rounded-md px-2 text-xs font-medium transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none';

export function WindowMenu({ paired, developerMode, onNavigate, onOpenCommand, onReconnect }: WindowMenuProps) {
  const tauri = isTauriEnvironment();
  const [maximized, setMaximized] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);

  useEffect(() => {
    if (!tauri) return;

    const appWindow = getCurrentWindow();
    let disposed = false;

    const syncWindowState = async () => {
      const [isMaximized, isFullscreen] = await Promise.all([appWindow.isMaximized(), appWindow.isFullscreen()]);
      if (!disposed) {
        setMaximized(isMaximized);
        setFullscreen(isFullscreen);
      }
    };

    void syncWindowState().catch(() => undefined);
    const unlisten = appWindow.onResized(() => {
      void syncWindowState().catch(() => undefined);
    });

    return () => {
      disposed = true;
      void unlisten.then((stopListening) => stopListening());
    };
  }, [tauri]);

  const runWindowAction = (action: () => Promise<void>) => {
    if (!tauri) return;
    void action().catch(() => undefined);
  };

  const navigate = (screen: ScreenId) => {
    if (screen === 'settings' || paired) onNavigate?.(screen);
  };

  const runEditCommand = (command: string) => {
    document.execCommand(command);
  };

  const reconnect = () => {
    if (onReconnect) void onReconnect().catch(() => undefined);
  };

  return (
    <nav data-no-drag className="flex shrink-0 items-center gap-0.5" aria-label="Application menu">
      <DropdownMenu>
        <DropdownMenuTrigger render={<button type="button" data-no-drag className={triggerClassName} />}>
          File
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-56" sideOffset={6}>
          <DropdownMenuGroup>
            <DropdownMenuItem disabled={!paired} onClick={() => navigate('printers')}>
              <FilePlus2 aria-hidden />
              Add Network Printer…
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => runWindowAction(() => getCurrentWindow().close())} disabled={!tauri}>
              <X aria-hidden />
              Close Window
              <DropdownMenuShortcut>Ctrl W</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem
              variant="destructive"
              onClick={() => void invoke('quit_application').catch(() => undefined)}
              disabled={!tauri}
            >
              Quit OPPA
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger render={<button type="button" data-no-drag className={triggerClassName} />}>
          View
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-56" sideOffset={6}>
          <DropdownMenuGroup>
            <DropdownMenuItem disabled={!paired || !onNavigate} onClick={() => navigate('overview')}>
              Overview
              <DropdownMenuShortcut>Ctrl 1</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem disabled={!paired || !onNavigate} onClick={() => navigate('jobs')}>
              Jobs
              <DropdownMenuShortcut>Ctrl 2</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem disabled={!paired || !onNavigate} onClick={() => navigate('printers')}>
              Printers
              <DropdownMenuShortcut>Ctrl 3</DropdownMenuShortcut>
            </DropdownMenuItem>
            {developerMode && (
              <DropdownMenuItem disabled={!paired || !onNavigate} onClick={() => navigate('virtual')}>
                Virtual Printer
                <DropdownMenuShortcut>Ctrl 4</DropdownMenuShortcut>
              </DropdownMenuItem>
            )}
            <DropdownMenuItem disabled={!paired || !onNavigate} onClick={() => navigate('diagnostics')}>
              Logs
              <DropdownMenuShortcut>Ctrl 5</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={!onNavigate} onClick={() => navigate('settings')}>
              <Settings2 aria-hidden />
              Settings
              <DropdownMenuShortcut>Ctrl ,</DropdownMenuShortcut>
            </DropdownMenuItem>
            {onOpenCommand && (
              <DropdownMenuItem onClick={onOpenCommand}>
                <Command aria-hidden />
                Search &amp; commands
                <DropdownMenuShortcut>Ctrl K</DropdownMenuShortcut>
              </DropdownMenuItem>
            )}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger render={<button type="button" data-no-drag className={triggerClassName} />}>
          Edit
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-52" sideOffset={6}>
          <DropdownMenuGroup>
            <DropdownMenuItem onClick={() => runEditCommand('undo')}>
              <Undo2 aria-hidden />
              Undo
              <DropdownMenuShortcut>Ctrl Z</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => runEditCommand('redo')}>
              <Redo2 aria-hidden />
              Redo
              <DropdownMenuShortcut>Ctrl Y</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => runEditCommand('cut')}>
              <Scissors aria-hidden />
              Cut
              <DropdownMenuShortcut>Ctrl X</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => runEditCommand('copy')}>
              <Copy aria-hidden />
              Copy
              <DropdownMenuShortcut>Ctrl C</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => runEditCommand('paste')}>
              <ClipboardPaste aria-hidden />
              Paste
              <DropdownMenuShortcut>Ctrl V</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={() => runEditCommand('selectAll')}>
              <ListChecks aria-hidden />
              Select All
              <DropdownMenuShortcut>Ctrl A</DropdownMenuShortcut>
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger render={<button type="button" data-no-drag className={triggerClassName} />}>
          Agent
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-48" sideOffset={6}>
          <DropdownMenuGroup>
            <DropdownMenuItem disabled={!onReconnect} onClick={reconnect}>
              <RefreshCw aria-hidden />
              Reconnect
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>

      <DropdownMenu>
        <DropdownMenuTrigger render={<button type="button" data-no-drag className={triggerClassName} />}>
          Window
        </DropdownMenuTrigger>
        <DropdownMenuContent className="min-w-56" sideOffset={6}>
          <DropdownMenuGroup>
            <DropdownMenuItem disabled={!tauri} onClick={() => runWindowAction(() => getCurrentWindow().minimize())}>
              <Minimize2 aria-hidden />
              Minimize
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!tauri}
              onClick={() =>
                runWindowAction(async () => {
                  const appWindow = getCurrentWindow();
                  await appWindow.toggleMaximize();
                  setMaximized(await appWindow.isMaximized());
                })
              }
            >
              {maximized ? <Copy aria-hidden /> : <Maximize2 aria-hidden />}
              {maximized ? 'Restore' : 'Maximize'}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={!tauri}
              onClick={() =>
                runWindowAction(async () => {
                  const appWindow = getCurrentWindow();
                  await appWindow.setFullscreen(!fullscreen);
                  setFullscreen(await appWindow.isFullscreen());
                })
              }
            >
              <Fullscreen aria-hidden />
              {fullscreen ? 'Exit Fullscreen' : 'Fullscreen'}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem disabled={!tauri} onClick={() => runWindowAction(() => getCurrentWindow().center())}>
              <AlignCenter aria-hidden />
              Center Window
            </DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </nav>
  );
}

export type { WindowMenuProps };
