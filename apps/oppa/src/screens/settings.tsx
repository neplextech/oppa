import {
  Download,
  ExternalLink,
  Globe2,
  LockKeyhole,
  Monitor,
  Moon,
  Power,
  RotateCcw,
  ShieldCheck,
  Sun,
} from 'lucide-react';
import type { ReactNode } from 'react';

import type { Theme } from '@/App';
import { ServerConfigurationForm } from '@/components/server-configuration-form';
import {
  ScreenContainer,
  ScreenHeader,
  SectionHeader,
  Toggle,
} from '@/components/ui';
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
  onThemeChange,
  onStartOnLogin,
  onReconnect,
  onSaveServerConfiguration,
  onResetServerConfiguration,
  onOpenProductLink,
}: {
  status: AgentStatus;
  busy: string | null;
  theme: Theme;
  onThemeChange: (t: Theme) => void;
  onStartOnLogin: (enabled: boolean) => Promise<void>;
  onReconnect: () => Promise<void>;
  onSaveServerConfiguration: (
    input: OpenPrinterServerConfiguration,
  ) => Promise<void>;
  onResetServerConfiguration: () => Promise<void>;
  onOpenProductLink: (
    link: 'documentation' | 'support',
    browserFallbackUrl: string,
  ) => Promise<void>;
}) {
  const { checkForUpdates, isChecking, lastResult } = useUpdater();

  return (
    <ScreenContainer>
      <ScreenHeader
        title="Settings"
        description="Agent preferences and product configuration."
      />

      <div className="max-w-2xl flex-1 space-y-8 overflow-y-auto p-6">
        {/* Appearance */}
        <Section title="Appearance" description="Interface color scheme">
          <div className="bg-card/50 px-5 py-4">
            <p className="text-foreground mb-3 text-sm font-medium">
              Color scheme
            </p>
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
        <Section
          title="OpenPrinter service"
          description="Authorization and gateway endpoints"
        >
          {status.connectedService ? (
            <div className="bg-card/30 flex items-start gap-4 px-5 py-4">
              <div className="border-border bg-primary/10 text-primary flex size-9 shrink-0 items-center justify-center rounded-lg border">
                <Globe2 className="size-4" aria-hidden />
              </div>
              <div className="min-w-0">
                <p className="text-foreground text-sm font-semibold">
                  {status.connectedService.name}
                </p>
                <p className="text-muted-foreground mt-0.5 text-xs">
                  Connected service · server{' '}
                  {status.connectedService.serverVersion}
                </p>
                <p className="text-muted-foreground/60 mt-1 truncate font-mono text-[10px]">
                  {status.connectedService.gatewayUrl}
                </p>
              </div>
            </div>
          ) : (
            <div className="bg-card/30 flex items-center gap-3 px-5 py-3">
              <Globe2 className="text-muted-foreground size-4" aria-hidden />
              <p className="text-muted-foreground text-xs">
                Service identity appears here after the gateway completes its
                handshake.
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
            Changing services removes the current authorization before
            reconnecting, so credentials are never reused with a different
            gateway.
          </p>
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
              <Button
                variant="outline"
                size="sm"
                disabled={busy === 'reconnect'}
                onClick={() => void onReconnect()}
              >
                Reconnect
              </Button>
            }
          />
          <SettingRow
            icon={<Download className="size-4" aria-hidden />}
            title="Software updates"
            description={
              lastResult ??
              'Check GitHub Releases for a signed stable OPPA update.'
            }
            control={
              <Button
                variant="outline"
                size="sm"
                disabled={isChecking}
                onClick={() => void checkForUpdates()}
              >
                {isChecking ? 'Checking…' : 'Check now'}
              </Button>
            }
          />
        </Section>

        {/* Product */}
        <Section
          title="Product"
          description="Application identity and compiled capabilities"
        >
          <div className="bg-card/50 p-5">
            <div className="flex items-start gap-4">
              <div className="border-border bg-secondary flex size-10 shrink-0 items-center justify-center rounded-lg border">
                <LockKeyhole
                  className="text-muted-foreground size-4"
                  aria-hidden
                />
              </div>
              <div className="min-w-0 flex-1">
                <p className="text-foreground font-semibold">
                  {status.product.name}
                </p>
                <p className="text-muted-foreground font-mono text-xs">
                  {status.product.id}
                </p>
                <p className="text-foreground/70 mt-2 text-sm leading-5">
                  {status.product.description}
                </p>
                <div className="mt-3 flex items-center gap-4">
                  <ProductLink
                    label="Documentation"
                    onClick={() =>
                      void onOpenProductLink(
                        'documentation',
                        status.product.documentationUrl,
                      )
                    }
                  />
                  <ProductLink
                    label="Support"
                    onClick={() =>
                      void onOpenProductLink(
                        'support',
                        status.product.supportUrl,
                      )
                    }
                  />
                </div>
              </div>
            </div>
          </div>
          <div className="bg-card/30 border-border/50 flex items-center gap-2 border-t px-5 py-3">
            <span className="text-muted-foreground font-mono text-xs">
              v{status.version}
            </span>
            <span className="text-border">·</span>
            <span className="text-muted-foreground text-xs">
              {status.platform}
            </span>
            {status.agentId && (
              <>
                <span className="text-border">·</span>
                <span className="text-muted-foreground truncate font-mono text-xs">
                  {status.agentId}
                </span>
              </>
            )}
          </div>
        </Section>

        {/* Security */}
        <Section title="Security">
          <div className="flex items-start gap-4 px-5 py-4">
            <ShieldCheck
              className="mt-0.5 size-5 shrink-0 text-[var(--color-connected)]"
              aria-hidden
            />
            <div>
              <p className="text-sm font-semibold text-[var(--color-connected)]">
                Security boundary active
              </p>
              <p className="mt-1.5 text-sm leading-5 text-[var(--color-connected)]/70">
                The connected server can send only versioned OpenPrinter
                messages. It cannot run scripts, browse local files, or proxy
                arbitrary network requests.
              </p>
            </div>
          </div>
        </Section>
      </div>
    </ScreenContainer>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div>
      <SectionHeader title={title} description={description} className="mb-3" />
      <div className="border-border divide-border divide-y overflow-hidden rounded-xl border">
        {children}
      </div>
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
  description: string;
  control: ReactNode;
}) {
  return (
    <div className="bg-card/30 flex items-center gap-4 px-5 py-4">
      <div className="border-border bg-secondary text-muted-foreground flex size-8 shrink-0 items-center justify-center rounded-lg border">
        {icon}
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-foreground text-sm font-medium">{title}</p>
        <p className="text-muted-foreground mt-0.5 text-xs leading-4">
          {description}
        </p>
      </div>
      {control}
    </div>
  );
}

function ProductLink({
  label,
  onClick,
}: {
  label: string;
  onClick: () => void;
}) {
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
