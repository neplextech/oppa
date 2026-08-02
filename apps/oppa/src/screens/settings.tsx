import { getTauriVersion } from '@tauri-apps/api/app';
import {
  ArrowLeft,
  Code2,
  Download,
  ExternalLink,
  Globe2,
  LockKeyhole,
  Monitor,
  Moon,
  Power,
  RotateCcw,
  Sun,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';

import type { Theme } from '@/App';
import { ServerConfigurationForm } from '@/components/server-configuration-form';
import { ScreenContainer, ScreenHeader, SectionHeader, Toggle } from '@/components/ui';
import { Button } from '@/components/ui/button';
import { useUpdater } from '@/components/updater-context';
import type { AgentStatus, OpenPrinterServerConfiguration } from '@/lib/types';
import { cn } from '@/lib/utils';

const THEMES: Array<{ value: Theme; label: string; icon: typeof Sun }> = [
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
  { value: 'system', label: 'System', icon: Monitor },
];

export function SettingsScreen({
  status,
  busy,
  theme,
  developerMode,
  fromSetup,
  onThemeChange,
  onDeveloperModeChange,
  onStartOnLogin,
  onReconnect,
  onSaveServerConfiguration,
  onResetServerConfiguration,
  onForgetServer,
  onOpenProductLink,
  onBackToSetup,
}: {
  status: AgentStatus;
  busy: string | null;
  theme: Theme;
  developerMode: boolean;
  fromSetup?: boolean;
  onThemeChange: (t: Theme) => void;
  onDeveloperModeChange: (enabled: boolean) => void;
  onStartOnLogin: (enabled: boolean) => Promise<void>;
  onReconnect: () => Promise<void>;
  onSaveServerConfiguration: (input: OpenPrinterServerConfiguration) => Promise<void>;
  onResetServerConfiguration: () => Promise<void>;
  onForgetServer: () => Promise<void>;
  onOpenProductLink: (link: 'documentation' | 'support', browserFallbackUrl: string) => Promise<void>;
  onBackToSetup?: () => void;
}) {
  const { checkForUpdates, isChecking, lastResult } = useUpdater();
  const [tauriVersion, setTauriVersion] = useState<string | null>(null);

  useEffect(() => {
    if ('__TAURI_INTERNALS__' in window) {
      getTauriVersion()
        .then((v) => setTauriVersion(v))
        .catch(() => undefined);
    }
  }, []);

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Settings"
        description="Agent preferences and product configuration."
        action={
          fromSetup && onBackToSetup ? (
            <Button variant="ghost" size="sm" onClick={onBackToSetup}>
              <ArrowLeft className="size-3.5" aria-hidden />
              Back to pairing
            </Button>
          ) : undefined
        }
      />

      <div className="flex flex-1 justify-center overflow-y-auto">
      <div className="w-full max-w-2xl space-y-8 p-8">
        {/* Appearance */}
        <Section title="Appearance" description="Interface color scheme">
          <div className="bg-card/50 px-5 py-4">
            <p className="text-foreground mb-3 text-sm font-medium">Color scheme</p>
            <div className="flex gap-3">
              {THEMES.map(({ value, label, icon: Icon }) => (
                <button
                  key={value}
                  type="button"
                  onClick={() => onThemeChange(value)}
                  className={cn(
                    'flex flex-1 flex-col items-center gap-2 rounded-xl border px-3 py-4 text-xs font-medium transition-all',
                    theme === value
                      ? 'border-primary bg-primary/5 text-primary'
                      : 'border-border bg-secondary/50 text-muted-foreground hover:border-border/80 hover:bg-secondary hover:text-foreground',
                  )}
                  aria-pressed={theme === value}
                >
                  <Icon className="size-5" strokeWidth={1.75} aria-hidden />
                  {label}
                </button>
              ))}
            </div>
          </div>
        </Section>

        {/* OpenPrinter service */}
        <Section title="OpenPrinter service" description="Discovered identity and paired credential">
          {status.connectedService ? (
            <div className="bg-card/30 flex items-start gap-4 px-5 py-4">
              <div className="border-border bg-primary/10 text-primary flex size-9 shrink-0 items-center justify-center rounded-lg border">
                <Globe2 className="size-4" aria-hidden />
              </div>
              <div className="min-w-0">
                <p className="text-foreground text-sm font-semibold">{status.connectedService.name}</p>
                <p className="text-muted-foreground mt-0.5 text-xs">
                  Connected service · server {status.connectedService.serverVersion}
                </p>
                <p className="text-muted-foreground/60 mt-1 truncate font-mono text-xs">
                  {status.connectedService.gatewayUrl}
                </p>
              </div>
            </div>
          ) : (
            <div className="bg-card/30 flex items-center gap-3 px-5 py-3">
              <Globe2 className="text-muted-foreground size-4" aria-hidden />
              <p className="text-muted-foreground text-xs">
                Service identity appears here after the gateway completes its handshake.
              </p>
            </div>
          )}
          <ServerConfigurationForm
            value={status.serverConfiguration}
            saving={busy === 'server-configuration'}
            resetting={busy === 'reset-server-configuration'}
            onSave={onSaveServerConfiguration}
            onReset={onResetServerConfiguration}
          />
          <p className="border-border/50 text-muted-foreground/70 border-t px-5 py-3 text-xs leading-4">
            Changing the server URL will unpair this computer and require pairing again.
          </p>
          <div className="border-border/50 border-t px-5 py-3">
            <Button
              variant="outline"
              size="sm"
              disabled={busy === 'forget-server'}
              onClick={() => void onForgetServer()}
            >
              <LockKeyhole className="size-3.5" aria-hidden />
              {busy === 'forget-server' ? 'Forgetting…' : 'Forget server'}
            </Button>
          </div>
        </Section>

        {/* Startup */}
        <Section title="Startup" description="Desktop startup and connectivity">
          <SettingRow
            icon={<Power className="size-4" aria-hidden />}
            title="Start on login"
            description={`Launch ${status.product.name} automatically after signing in to this computer.`}
            control={
              <Toggle
                checked={status.startOnLogin}
                label={`Start ${status.product.name} on login`}
                onChange={(enabled) => void onStartOnLogin(enabled)}
              />
            }
          />
          <SettingRow
            icon={<RotateCcw className="size-4" aria-hidden />}
            title="Reconnect gateway"
            description="Close the active transport and establish a fresh connection."
            control={
              <Button variant="outline" size="sm" disabled={busy === 'reconnect'} onClick={() => void onReconnect()}>
                Reconnect
              </Button>
            }
          />
          <SettingRow
            icon={<Download className="size-4" aria-hidden />}
            title="Software updates"
            description={lastResult ?? 'Check GitHub Releases for a signed stable OPPA update.'}
            control={
              <Button variant="outline" size="sm" disabled={isChecking} onClick={() => void checkForUpdates()}>
                {isChecking ? 'Checking…' : 'Check now'}
              </Button>
            }
          />
        </Section>

        {/* Product */}
        <Section title="Product" description="Application identity and compiled capabilities">
          <div className="bg-card/50 p-5">
            <div className="flex items-start gap-4">
              <div className="border-border bg-secondary flex size-10 shrink-0 items-center justify-center rounded-lg border">
                <LockKeyhole className="text-muted-foreground size-4" aria-hidden />
              </div>
              <div className="min-w-0 flex-1">
                <p className="text-foreground font-semibold">{status.product.name}</p>
                <p className="text-muted-foreground font-mono text-xs">{status.product.id}</p>
                <p className="text-foreground/70 mt-2 text-sm leading-5">{status.product.description}</p>
                <div className="mt-3 flex items-center gap-4">
                  <ProductLink
                    label="Documentation"
                    onClick={() => void onOpenProductLink('documentation', status.product.documentationUrl)}
                  />
                  <ProductLink
                    label="Support"
                    onClick={() => void onOpenProductLink('support', status.product.supportUrl)}
                  />
                </div>
              </div>
            </div>
          </div>
          <div className="bg-card/30 border-border/50 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 border-t px-5 py-4">
            <AboutRow label="App version" value={`v${status.version}`} mono />
            {tauriVersion && <AboutRow label="Tauri" value={`v${tauriVersion}`} mono />}
            <AboutRow label="Platform" value={status.platform} />
            {import.meta.env.VITE_APP_IDENTIFIER && (
              <AboutRow label="Identifier" value={import.meta.env.VITE_APP_IDENTIFIER as string} mono />
            )}
            {status.agentId && <AboutRow label="Agent ID" value={status.agentId} mono />}
            <AboutRow label="Build" value={import.meta.env.DEV ? 'Development' : 'Release'} />
          </div>
        </Section>

        {/* Developer */}
        <Section title="Developer" description="Tools for integration testing and diagnostics">
          <SettingRow
            icon={<Code2 className="size-4" aria-hidden />}
            title="Developer Mode"
            description="Reveals virtual printer controls and enables Copy ID in context menus."
            control={<Toggle checked={developerMode} label="Enable developer mode" onChange={onDeveloperModeChange} />}
          />
        </Section>

      </div>
      </div>
    </ScreenContainer>
  );
}

function Section({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <div>
      <SectionHeader title={title} description={description} className="mb-3" />
      <div className="border-border divide-border divide-y overflow-hidden rounded-xl border">{children}</div>
    </div>
  );
}

function SettingRow({
  icon,
  title,
  description,
  control,
}: {
  icon: ReactNode;
  title: string;
  description: string | ReactNode;
  control: ReactNode;
}) {
  return (
    <div className="bg-card/30 flex items-center gap-4 px-5 py-4">
      <div className="border-border bg-secondary text-muted-foreground flex size-9 shrink-0 items-center justify-center rounded-lg border">
        {icon}
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-foreground text-sm font-medium">{title}</p>
        <p className="text-muted-foreground mt-0.5 text-xs leading-5">{description}</p>
      </div>
      {control}
    </div>
  );
}

function AboutRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <>
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className={`text-foreground/80 text-xs ${mono ? 'font-mono' : ''}`}>{value}</span>
    </>
  );
}

function ProductLink({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="text-primary hover:text-primary/80 inline-flex items-center gap-1 text-sm transition-colors"
    >
      {label}
      <ExternalLink className="size-3" aria-hidden />
    </button>
  );
}
