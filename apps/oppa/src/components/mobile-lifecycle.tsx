import { BatteryWarning, BellOff, ExternalLink, Info, LoaderCircle, Smartphone, Wifi } from 'lucide-react';
import { useCallback, useEffect, useState, type ReactNode } from 'react';

import { Toggle } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { agentClient } from '@/lib/agent-client';
import type { MobileLifecycleStatus } from '@/lib/types';
import { cn } from '@/lib/utils';

/** Loads lifecycle status and refreshes it whenever the app becomes visible again. */
export function useMobileLifecycle(enabled: boolean) {
  const [lifecycle, setLifecycle] = useState<MobileLifecycleStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    if (!enabled) return;
    try {
      setLifecycle(await agentClient.getMobileLifecycle());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Lifecycle status is unavailable.');
    }
  }, [enabled]);
  useEffect(() => {
    void refresh();
    // Returning from a system settings page should reflect the new state.
    const onVisible = () => {
      if (document.visibilityState === 'visible') void refresh();
    };
    document.addEventListener('visibilitychange', onVisible);
    window.addEventListener('focus', onVisible);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('focus', onVisible);
    };
  }, [refresh]);
  return { lifecycle, error, refresh };
}

function Row({ title, description, control }: { title: string; description: ReactNode; control?: ReactNode }) {
  return (
    <div className="border-border flex items-center justify-between gap-4 border-b py-4">
      <div className="min-w-0">
        <p className="text-sm font-medium">{title}</p>
        <div className="text-muted-foreground mt-1 text-xs leading-5">{description}</div>
      </div>
      {control && <div className="shrink-0">{control}</div>}
    </div>
  );
}

function StatePill({ tone, children }: { tone: 'ok' | 'warn' | 'muted'; children: ReactNode }) {
  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium',
        tone === 'ok' && 'border-[var(--color-connected)] text-[var(--color-connected)]',
        tone === 'warn' && 'border-[var(--color-warning)] text-[var(--color-warning)]',
        tone === 'muted' && 'border-border text-muted-foreground',
      )}
    >
      <span className="size-1.5 rounded-full bg-current" aria-hidden />
      {children}
    </span>
  );
}

/** Background-operation preferences and platform limitations for Android and iOS. */
export function MobileLifecycleSection({ productName, canQuit }: { productName: string; canQuit: boolean }) {
  const { lifecycle, error, refresh } = useMobileLifecycle(true);
  const [working, setWorking] = useState<string | null>(null);

  async function run(id: string, action: () => Promise<unknown>) {
    setWorking(id);
    try {
      await action();
    } finally {
      setWorking(null);
      await refresh();
    }
  }

  if (error) return <p className="text-muted-foreground py-4 text-xs">{error}</p>;
  if (!lifecycle) {
    return (
      <p className="text-muted-foreground flex items-center gap-2 py-4 text-xs">
        <LoaderCircle className="size-3.5 animate-spin" aria-hidden /> Checking background operation…
      </p>
    );
  }

  const android = lifecycle.platform === 'android';
  return (
    <div>
      {android ? (
        <>
          <Row
            title="Keep running in background"
            description={
              <span className="flex flex-wrap items-center gap-2">
                Keep receiving print jobs while you use other apps or the screen is locked.
                <StatePill tone={lifecycle.backgroundServiceRunning ? 'ok' : 'muted'}>
                  {lifecycle.backgroundServiceRunning ? 'Running' : 'Not running'}
                </StatePill>
              </span>
            }
            control={
              <Toggle
                label="Keep running in background"
                checked={lifecycle.settings.backgroundAgent}
                onChange={(enabled) =>
                  void run('background', () => agentClient.setBackgroundAgent(enabled)).catch(() => undefined)
                }
              />
            }
          />
          {lifecycle.notificationsGranted === false && (
            <Row
              title="Notifications are off"
              description="Android hides the agent's status notification and its Stop action until you allow notifications."
              control={
                <Button
                  size="sm"
                  variant="outline"
                  disabled={working !== null}
                  onClick={() =>
                    void run('notifications', async () => {
                      const granted = await agentClient.requestNotificationPermission();
                      if (!granted) await agentClient.openAppSettings();
                    }).catch(() => undefined)
                  }
                >
                  <BellOff aria-hidden />
                  Allow
                </Button>
              }
            />
          )}
          <Row
            title="Battery optimization"
            description={
              lifecycle.ignoringBatteryOptimizations
                ? 'Unrestricted. Android will not pause the agent to save battery.'
                : 'Android may pause the agent when the screen is off for a long time. Allow unrestricted battery use for reliable printing.'
            }
            control={
              lifecycle.ignoringBatteryOptimizations ? (
                <StatePill tone="ok">Unrestricted</StatePill>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={working !== null}
                  onClick={() =>
                    void run('battery', () => agentClient.requestBatteryExemption()).catch(() => undefined)
                  }
                >
                  <BatteryWarning aria-hidden />
                  Allow
                </Button>
              )
            }
          />
          {lifecycle.batteryGuidance && (
            <div className="border-border border-b py-4">
              <p className="text-sm font-medium">{lifecycle.batteryGuidance.vendor} battery manager</p>
              <p className="text-muted-foreground mt-1 text-xs leading-5">
                This device may stop background apps even when they are exempt. Also check:
              </p>
              <ol className="text-muted-foreground mt-2 list-decimal space-y-1 pl-5 text-xs leading-5">
                {lifecycle.batteryGuidance.steps.map((step) => (
                  <li key={step}>{step}</li>
                ))}
              </ol>
              <Button
                className="mt-3"
                size="sm"
                variant="ghost"
                onClick={() => void agentClient.openBatteryGuide().catch(() => undefined)}
              >
                Device-specific guide
                <ExternalLink aria-hidden />
              </Button>
            </div>
          )}
          {canQuit && (
            <Row
              title="Stop agent"
              description={`Disconnect and close ${productName}. Jobs wait on the server until you open it again.`}
              control={
                <Button
                  size="sm"
                  variant="destructive"
                  disabled={working !== null}
                  onClick={() => void agentClient.quitApplication().catch(() => undefined)}
                >
                  Stop
                </Button>
              }
            />
          )}
        </>
      ) : (
        <>
          <Row
            title="Keep screen awake"
            description="Prevent auto-lock while the agent is paired and on screen, so print jobs keep arriving."
            control={
              <Toggle
                label="Keep screen awake"
                checked={lifecycle.settings.keepAwake}
                onChange={(enabled) =>
                  void run('keep-awake', () => agentClient.setKeepAwake(enabled)).catch(() => undefined)
                }
              />
            }
          />
          <Row
            title="Local network access"
            description={
              lifecycle.localNetworkAccess === 'denied'
                ? `${productName} cannot reach printers on your network. Turn on Local Network for ${productName} in Settings.`
                : 'Required to print to network printers and to find them automatically.'
            }
            control={
              lifecycle.localNetworkAccess === 'denied' ? (
                <Button size="sm" variant="outline" onClick={() => void agentClient.openAppSettings()}>
                  <Wifi aria-hidden />
                  Settings
                </Button>
              ) : (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={working !== null}
                  onClick={() =>
                    void run('network', () => agentClient.requestLocalNetworkAccess()).catch(() => undefined)
                  }
                >
                  {working === 'network' ? 'Checking…' : 'Check'}
                </Button>
              )
            }
          />
        </>
      )}
      <LimitationsCallout limitations={lifecycle.limitations} ios={!android} />
    </div>
  );
}

function LimitationsCallout({ limitations, ios }: { limitations: string[]; ios: boolean }) {
  if (limitations.length === 0) return null;
  return (
    <div
      className={cn(
        'mt-4 rounded-xl border p-4',
        ios ? 'bg-card border-[var(--color-warning)]' : 'border-border bg-secondary',
      )}
    >
      <p className="flex items-center gap-2 text-sm font-medium">
        {ios ? <Smartphone className="size-4" aria-hidden /> : <Info className="size-4" aria-hidden />}
        {ios ? 'How printing works on iPhone and iPad' : 'How background printing works'}
      </p>
      <ul className="text-muted-foreground mt-2 space-y-2 text-xs leading-5">
        {limitations.map((item) => (
          <li key={item}>{item}</li>
        ))}
      </ul>
    </div>
  );
}

/** Compact Overview notice for lifecycle conditions that affect delivery. */
export function MobileLifecycleNotice({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { lifecycle } = useMobileLifecycle(true);
  if (!lifecycle) return null;
  let message: string | null = null;
  const webView = androidWebViewMajor();
  if (webView !== null && webView < MODERN_WEBVIEW) {
    message = `Android System WebView ${webView} is outdated, so some colors may look flat. Update it from your app store or device settings.`;
  } else if (lifecycle.platform === 'ios') {
    message = 'Keep OPPA open on screen to receive print jobs. iOS pauses the agent in the background.';
  } else if (!lifecycle.settings.backgroundAgent) {
    message = 'Background printing is off. Jobs arrive only while OPPA is open.';
  } else if (lifecycle.ignoringBatteryOptimizations === false) {
    message = 'Battery optimization may pause printing while the screen is off.';
  } else if (lifecycle.notificationsGranted === false) {
    message = 'Notifications are off, so the agent status notification is hidden.';
  }
  if (!message) return null;
  return (
    <button
      type="button"
      onClick={onOpenSettings}
      className="border-border bg-card hover:bg-accent flex w-full items-start gap-3 rounded-xl border p-3 text-left transition-colors"
    >
      <Info className="text-primary mt-0.5 size-4 shrink-0" aria-hidden />
      <span className="text-xs leading-5">
        {message} <span className="text-primary font-medium">Review settings</span>
      </span>
    </button>
  );
}

/** Oldest Chromium WebView with every CSS feature the interface uses (color-mix, oklch). */
const MODERN_WEBVIEW = 111;

/** Chromium major version of the Android System WebView, or `null` elsewhere. */
function androidWebViewMajor(): number | null {
  if (!/Android/.test(navigator.userAgent)) return null;
  const match = /Chrome\/(\d+)/.exec(navigator.userAgent);
  return match ? Number(match[1]) : null;
}
