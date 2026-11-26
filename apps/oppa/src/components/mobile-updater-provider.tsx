import { Download, LoaderCircle, ShieldCheck } from 'lucide-react';
import { type PropsWithChildren, useCallback, useEffect, useMemo, useRef, useState } from 'react';

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
import type { MobileUpdate } from '@/lib/types';

type Phase = 'ready' | 'downloading' | 'permission' | 'handed-off' | 'failed';

function formatSize(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Android: checks the direct-download release channel and hands a verified
 * APK to the system installer, which always asks the user to approve it.
 * iOS: no in-app updater; updates come from the distribution channel.
 */
export function MobileUpdaterProvider({ os, children }: PropsWithChildren<{ os: 'android' | 'ios' }>) {
  const [isChecking, setIsChecking] = useState(false);
  const [lastResult, setLastResult] = useState<string | null>(null);
  const [update, setUpdate] = useState<MobileUpdate | null>(null);
  const [phase, setPhase] = useState<Phase>('ready');
  const [error, setError] = useState<string | null>(null);
  const checking = useRef(false);

  const checkForUpdates = useCallback(async () => {
    if (os !== 'android') {
      setLastResult('Updates for this device are installed from its distribution channel.');
      return;
    }
    if (checking.current) return;
    checking.current = true;
    setIsChecking(true);
    setLastResult(null);
    try {
      const available = await agentClient.checkMobileUpdate();
      if (!available) {
        setLastResult('OPPA is up to date.');
        return;
      }
      setUpdate(available);
      setPhase('ready');
      setError(null);
      setLastResult(`OPPA ${available.version} is available.`);
    } catch {
      setLastResult('Unable to check for updates right now.');
    } finally {
      checking.current = false;
      setIsChecking(false);
    }
  }, [os]);

  useEffect(() => {
    if (os === 'android' && '__TAURI_INTERNALS__' in window) void checkForUpdates();
  }, [checkForUpdates, os]);

  async function install() {
    setPhase('downloading');
    setError(null);
    try {
      const outcome = await agentClient.installMobileUpdate();
      setPhase(outcome === 'permission_required' ? 'permission' : 'handed-off');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The update could not be installed.');
      setPhase('failed');
    }
  }

  const context = useMemo(
    () => ({ isChecking, lastResult, checkForUpdates }),
    [checkForUpdates, isChecking, lastResult],
  );
  const busy = phase === 'downloading';

  return (
    <UpdaterContext.Provider value={context}>
      {children}
      <Dialog
        open={Boolean(update)}
        onOpenChange={(open) => {
          if (!open && !busy) setUpdate(null);
        }}
      >
        <DialogContent showCloseButton={!busy}>
          <DialogHeader>
            <DialogTitle>{phase === 'failed' ? 'Update failed' : 'OPPA update available'}</DialogTitle>
            <DialogDescription>
              {update
                ? `Version ${update.version} (${formatSize(update.sizeBytes)}) can replace ${update.currentVersion}. Your pairing, printers, and history are kept.`
                : null}
            </DialogDescription>
          </DialogHeader>
          {phase === 'ready' && update?.notes ? (
            <div className="bg-secondary/60 text-muted-foreground max-h-40 overflow-y-auto rounded-xl p-4 text-xs leading-5 whitespace-pre-wrap">
              {update.notes}
            </div>
          ) : null}
          {phase === 'downloading' ? (
            <div className="text-muted-foreground flex items-center gap-2 text-sm">
              <LoaderCircle className="size-4 animate-spin" aria-hidden />
              Downloading and verifying the signed APK…
            </div>
          ) : null}
          {phase === 'permission' ? (
            <p className="text-muted-foreground text-sm leading-6">
              Allow <strong className="text-foreground">Install unknown apps</strong> for OPPA in the settings page that
              opened, then return here and tap Install again.
            </p>
          ) : null}
          {phase === 'handed-off' ? (
            <p className="text-muted-foreground flex gap-2 text-sm leading-6">
              <ShieldCheck className="text-primary mt-1 size-4 shrink-0" aria-hidden />
              The update was verified against this app&apos;s signing key. Confirm the installation in the Android
              installer.
            </p>
          ) : null}
          {phase === 'failed' ? (
            <div className="border-destructive/20 bg-destructive/5 text-destructive rounded-xl border p-4 text-xs leading-5">
              {error}
            </div>
          ) : null}
          <DialogFooter>
            {!busy ? (
              <Button variant="outline" onClick={() => setUpdate(null)}>
                Later
              </Button>
            ) : null}
            {phase !== 'handed-off' ? (
              <Button disabled={busy} onClick={() => void install()}>
                <Download className="size-4" aria-hidden />
                {phase === 'failed' ? 'Retry' : 'Install'}
              </Button>
            ) : null}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </UpdaterContext.Provider>
  );
}
