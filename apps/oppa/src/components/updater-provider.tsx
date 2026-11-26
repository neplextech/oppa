import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { Download, LoaderCircle, RotateCcw } from 'lucide-react';
import { type PropsWithChildren, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { MobileUpdaterProvider } from '@/components/mobile-updater-provider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { UpdaterContext } from '@/components/updater-context';
import { agentClient } from '@/lib/agent-client';
import { detectMobileOs } from '@/lib/platform';

/**
 * `stopped` means installation failed after the agent was already shut down,
 * so the only safe recovery is relaunching the currently installed version.
 */
type UpdatePhase = 'ready' | 'downloading' | 'installing' | 'failed' | 'stopped';

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

function errorMessage(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (typeof cause === 'string') return cause;
  return 'The update could not be installed.';
}

/**
 * Selects the update mechanism for this platform. Desktop uses the signed
 * Tauri updater; Android uses verified direct-download APKs; iOS has none.
 */
export function UpdaterProvider({ children }: PropsWithChildren) {
  const mobileOs = detectMobileOs();
  if (mobileOs) return <MobileUpdaterProvider os={mobileOs}>{children}</MobileUpdaterProvider>;
  return <DesktopUpdaterProvider>{children}</DesktopUpdaterProvider>;
}

function DesktopUpdaterProvider({ children }: PropsWithChildren) {
  const [isChecking, setIsChecking] = useState(false);
  const [lastResult, setLastResult] = useState<string | null>(null);
  const [update, setUpdate] = useState<Update | null>(null);
  const [phase, setPhase] = useState<UpdatePhase>('ready');
  const [downloaded, setDownloaded] = useState(0);
  const [total, setTotal] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const checking = useRef(false);
  const downloadedBytes = useRef(0);

  const checkForUpdates = useCallback(async () => {
    if (!isTauri()) {
      setLastResult('Update checks are available in the installed desktop app.');
      return;
    }
    if (checking.current) return;

    checking.current = true;
    setIsChecking(true);
    setLastResult(null);
    try {
      const available = await check({ allowDowngrades: false });
      if (!available) {
        setLastResult('OPPA is up to date.');
        return;
      }
      setUpdate(available);
      setPhase('ready');
      setDownloaded(0);
      setTotal(0);
      setError(null);
      downloadedBytes.current = 0;
      setLastResult(`OPPA ${available.version} is available.`);
    } catch {
      setLastResult('Unable to check for updates right now.');
    } finally {
      checking.current = false;
      setIsChecking(false);
    }
  }, []);

  useEffect(() => {
    if (isTauri()) {
      void checkForUpdates();
    }
  }, [checkForUpdates]);

  async function closeUpdate() {
    if (phase === 'downloading' || phase === 'installing' || phase === 'stopped') return;
    await update?.close();
    setUpdate(null);
    setPhase('ready');
    setError(null);
  }

  async function installUpdate() {
    if (!update) return;

    setPhase('downloading');
    setError(null);
    setDownloaded(0);
    setTotal(0);
    downloadedBytes.current = 0;
    let agentStopped = false;

    try {
      // Download while the agent keeps serving jobs, so a failed or slow
      // download never interrupts printing.
      await update.download((event) => {
        if (event.event === 'Started') {
          setTotal(event.data.contentLength ?? 0);
          return;
        }
        if (event.event === 'Progress') {
          downloadedBytes.current += event.data.chunkLength;
          setDownloaded(downloadedBytes.current);
        }
      });
      setPhase('installing');
      // Close the gateway connection before the installer can replace or
      // terminate the process. Durable jobs are recovered after relaunch.
      await agentClient.prepareForUpdate();
      agentStopped = true;
      await update.install();
      await relaunch();
    } catch (cause) {
      setError(
        agentStopped
          ? `${errorMessage(cause)} The agent is stopped; restart OPPA to resume printing.`
          : errorMessage(cause),
      );
      setPhase(agentStopped ? 'stopped' : 'failed');
    }
  }

  const percentage = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
  const context = useMemo(
    () => ({ isChecking, lastResult, checkForUpdates }),
    [checkForUpdates, isChecking, lastResult],
  );

  return (
    <UpdaterContext.Provider value={context}>
      {children}
      <Dialog
        open={Boolean(update)}
        onOpenChange={(open) => {
          if (!open) void closeUpdate();
        }}
      >
        <DialogContent showCloseButton={phase === 'ready' || phase === 'failed'}>
          <DialogHeader>
            <DialogTitle>
              {phase === 'failed' || phase === 'stopped' ? 'Update failed' : 'OPPA update available'}
            </DialogTitle>
            <DialogDescription>
              {update
                ? `Version ${update.version} is ready to replace ${update.currentVersion}.`
                : 'A newer OPPA release is available.'}
            </DialogDescription>
          </DialogHeader>

          {phase === 'ready' && update?.body ? (
            <div className="bg-secondary/60 text-muted-foreground max-h-40 overflow-y-auto rounded-xl p-4 text-xs leading-5 whitespace-pre-wrap">
              {update.body}
            </div>
          ) : null}

          {phase === 'downloading' ? (
            <div className="space-y-3">
              <div className="text-muted-foreground flex items-center gap-2 text-sm">
                <LoaderCircle className="size-4 animate-spin" aria-hidden />
                Downloading signed update…
              </div>
              <div className="bg-secondary h-2 overflow-hidden rounded-full">
                <div
                  className="bg-primary h-full rounded-full transition-[width]"
                  style={{ width: `${total > 0 ? percentage : 12}%` }}
                />
              </div>
              <p className="text-muted-foreground font-mono text-[10px]">
                {total > 0 ? `${percentage}%` : `${Math.round(downloaded / 1024)} KiB`}
              </p>
            </div>
          ) : null}

          {phase === 'installing' ? (
            <div className="text-muted-foreground flex items-center gap-2 text-sm">
              <LoaderCircle className="size-4 animate-spin" aria-hidden />
              Installing update and restarting OPPA…
            </div>
          ) : null}

          {phase === 'failed' || phase === 'stopped' ? (
            <div className="border-destructive/20 bg-destructive/5 text-destructive rounded-xl border p-4 text-xs leading-5">
              {error}
            </div>
          ) : null}

          <DialogFooter>
            {phase === 'ready' || phase === 'failed' ? (
              <Button variant="outline" onClick={() => void closeUpdate()}>
                Later
              </Button>
            ) : null}
            {phase === 'ready' ? (
              <Button onClick={() => void installUpdate()}>
                <Download className="size-4" aria-hidden />
                Download and install
              </Button>
            ) : null}
            {phase === 'failed' ? (
              <Button onClick={() => void installUpdate()}>
                <RotateCcw className="size-4" aria-hidden />
                Retry
              </Button>
            ) : null}
            {phase === 'stopped' ? (
              <Button onClick={() => void relaunch()}>
                <RotateCcw className="size-4" aria-hidden />
                Restart OPPA
              </Button>
            ) : null}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </UpdaterContext.Provider>
  );
}
